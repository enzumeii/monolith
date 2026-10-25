use crate::{
    config::{CronImplementation, SchedulerConfig},
    error::AppError,
    state::AppState,
};
use std::time::Duration;
use tokio::{sync::watch, task::JoinHandle};

#[allow(async_fn_in_trait)]
pub trait CronRuntime {
    async fn tick(&self, state: &AppState) -> Result<(), AppError>;
}

pub struct VpsCron;
pub struct VercelCron;

pub struct CronTasks {
    scheduler: Option<JoinHandle<()>>,
    retention: Option<JoinHandle<()>>,
    checkout: Option<JoinHandle<()>>,
    checkout_shutdown: Option<watch::Sender<bool>>,
}

impl CronRuntime for VpsCron {
    async fn tick(&self, state: &AppState) -> Result<(), AppError> {
        super::run_once(state).await.map(|_| ())
    }
}

impl CronRuntime for VercelCron {
    async fn tick(&self, state: &AppState) -> Result<(), AppError> {
        super::invoke(state).await
    }
}

impl CronTasks {
    #[must_use]
    pub fn start(state: &AppState) -> Self {
        match state.settings.scheduler.implementation {
            CronImplementation::Vps => Self::vps(state, &state.settings.scheduler),
            CronImplementation::Vercel => Self {
                scheduler: None,
                retention: None,
                checkout: None,
                checkout_shutdown: None,
            },
        }
    }

    pub async fn shutdown(mut self) {
        if let Some(stop) = &self.checkout_shutdown {
            let _ = stop.send(true);
        }
        for task in [&self.scheduler, &self.retention].into_iter().flatten() {
            task.abort();
        }
        for task in [self.scheduler.take(), self.retention.take()]
            .into_iter()
            .flatten()
        {
            let _ = task.await;
        }
        if let Some(task) = self.checkout.take() {
            let _ = task.await;
        }
        tracing::info!("Cron runtime tasks stopped");
    }

    fn vps(state: &AppState, settings: &SchedulerConfig) -> Self {
        let scheduler = state.settings.scheduler.enabled.then(|| {
            let state = state.clone();
            let period = Duration::from_secs(settings.poll_interval_secs);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(period);
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    interval.tick().await;
                    if let Err(failure) = VpsCron.tick(&state).await {
                        crate::error::log_background_error("cron invocation", &failure);
                    }
                }
            })
        });
        let pool = state.pool.clone();
        let period = Duration::from_secs(settings.retention_interval_secs);
        let retention = Some(tokio::spawn(async move {
            let mut interval = tokio::time::interval(period);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                if let Err(failure) = crate::repositories::auth::cleanup_refresh_tokens(&pool).await
                {
                    crate::error::log_background_error("refresh token retention", &failure);
                }
            }
        }));
        let (checkout, checkout_shutdown) = super::checkout::start(state);
        Self {
            scheduler,
            retention,
            checkout,
            checkout_shutdown,
        }
    }
}

impl Drop for CronTasks {
    fn drop(&mut self) {
        for task in [&self.scheduler, &self.retention, &self.checkout]
            .into_iter()
            .flatten()
        {
            task.abort();
        }
    }
}
