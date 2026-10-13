use axum::Router;
use std::{
    future::{Future, IntoFuture},
    io,
    time::Duration,
};
use tokio::{net::TcpListener, sync::oneshot};

pub async fn shutdown_signal() -> io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {result=tokio::signal::ctrl_c()=>result, _=terminate.recv()=>Ok(())}
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}
pub async fn serve_until(
    listener: TcpListener,
    router: Router,
    signal: impl Future<Output = io::Result<()>>,
    drain: Duration,
) -> io::Result<()> {
    let (stop, stopped) = oneshot::channel();
    let server = axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = stopped.await;
        })
        .into_future();
    tokio::pin!(server);
    tokio::select! {
        result=&mut server=>result,
        result=signal=>{
            result?;let _=stop.send(());
            tokio::time::timeout(drain,server).await.map_err(|_|io::Error::new(io::ErrorKind::TimedOut,"API shutdown drain exceeded deadline"))?
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::get;
    use std::sync::Arc;
    #[tokio::test]
    async fn shutdown_drains_inflight_and_refuses_new_connections() {
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let start = started.clone();
        let finish = release.clone();
        let router = Router::new().route(
            "/",
            get(move || {
                let started = start.clone();
                let release = finish.clone();
                async move {
                    started.notify_one();
                    release.notified().await;
                    "done"
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let addr = listener.local_addr().expect("address");
        let (tx, rx) = oneshot::channel();
        let server = tokio::spawn(serve_until(
            listener,
            router,
            async {
                rx.await.expect("signal");
                Ok(())
            },
            Duration::from_secs(2),
        ));
        let client = tokio::spawn(async move {
            reqwest::get(format!("http://{addr}/"))
                .await
                .expect("request")
                .text()
                .await
                .expect("body")
        });
        started.notified().await;
        tx.send(()).expect("shutdown");
        tokio::time::timeout(Duration::from_secs(1), async {
            while TcpListener::bind(addr).await.is_err() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("listener closed");
        release.notify_one();
        assert_eq!(client.await.expect("client"), "done");
        server.await.expect("server").expect("drained");
    }
    #[tokio::test]
    async fn shutdown_deadline_bounds_a_stuck_request() {
        let started = Arc::new(tokio::sync::Notify::new());
        let start = started.clone();
        let router = Router::new().route(
            "/",
            get(move || {
                let started = start.clone();
                async move {
                    started.notify_one();
                    std::future::pending::<&str>().await
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let addr = listener.local_addr().expect("address");
        let (tx, rx) = oneshot::channel();
        let server = tokio::spawn(serve_until(
            listener,
            router,
            async {
                rx.await.expect("signal");
                Ok(())
            },
            Duration::from_millis(30),
        ));
        let client = tokio::spawn(async move { reqwest::get(format!("http://{addr}/")).await });
        started.notified().await;
        tx.send(()).expect("shutdown");
        let error = tokio::time::timeout(Duration::from_secs(1), server)
            .await
            .expect("bounded shutdown")
            .expect("server")
            .expect_err("drain timeout");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        client.abort();
    }
}
