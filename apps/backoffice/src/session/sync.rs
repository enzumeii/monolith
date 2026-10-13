#[cfg(target_arch = "wasm32")]
use dioxus::prelude::*;

pub(super) struct CookieLock {
    #[cfg(target_arch = "wasm32")]
    eval: document::Eval,
}

// Native rendering never performs browser cookie coordination.
#[cfg_attr(not(target_arch = "wasm32"), allow(clippy::unused_async))]
pub(super) async fn lock() -> crate::api::ApiResult<CookieLock> {
    #[cfg(target_arch = "wasm32")]
    {
        let eval = document::eval(
            r"
            if (!navigator.locks) { dioxus.send('unsupported'); return; }
            try {
                await navigator.locks.request('backoffice.cookie', async()=>{
                    dioxus.send('locked');
                    await dioxus.recv();
                });
            } catch (_) { dioxus.send('failed'); }
        ",
        );
        let mut guard = CookieLock { eval };
        let result = guard
            .eval
            .recv::<String>()
            .await
            .map_err(|_| crate::api::ApiError::new("could not coordinate authentication"))?;
        if result != "locked" {
            return Err(crate::api::ApiError::new(
                "authentication requires browser Web Locks support",
            ));
        }
        Ok(guard)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        Ok(CookieLock {})
    }
}

pub(super) fn notify() {
    #[cfg(target_arch = "wasm32")]
    let _ = document::eval(
        "const channel=new BroadcastChannel('backoffice.session');channel.postMessage('changed');channel.close();",
    );
}

pub(super) fn install() {
    #[cfg(target_arch = "wasm32")]
    spawn(async move {
        let mut channel = document::eval(
            r"
            localStorage.removeItem('backoffice.session');
            const bus=new BroadcastChannel('backoffice.session');
            bus.onmessage=()=>dioxus.send('changed');
            window.addEventListener('focus',()=>dioxus.send('changed'));
            await new Promise(()=>{});
        ",
        );
        while channel.recv::<String>().await.is_ok() {
            let _ = super::restore().await;
        }
    });
}

impl Drop for CookieLock {
    fn drop(&mut self) {
        #[cfg(target_arch = "wasm32")]
        let _ = self.eval.send(());
    }
}
