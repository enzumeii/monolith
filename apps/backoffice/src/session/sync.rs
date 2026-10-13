use dioxus::prelude::*;

pub(super) struct CookieLock {
    #[cfg(target_arch = "wasm32")]
    eval: document::Eval,
}
impl Drop for CookieLock {
    fn drop(&mut self) {
        #[cfg(target_arch = "wasm32")]
        let _ = self.eval.send(());
    }
}

// Native rendering never performs browser cookie coordination.
#[cfg_attr(not(target_arch = "wasm32"), allow(clippy::unused_async))]
pub(super) async fn lock() -> ServerFnResult<CookieLock> {
    #[cfg(target_arch = "wasm32")]
    {
        let mut eval = document::eval(
            r"
            if (!navigator.locks) { dioxus.send('unsupported'); return; }
            const controller=new AbortController();
            const waiting=setTimeout(()=>controller.abort(),20000);
            try {
                await navigator.locks.request('backoffice.cookie', {signal:controller.signal}, async()=>{
                    clearTimeout(waiting);dioxus.send('locked');
                    await Promise.race([dioxus.recv(),new Promise(resolve=>setTimeout(resolve,45000))]);
                });
            } catch (_) { dioxus.send('failed'); }
        ",
        );
        let guard = CookieLock { eval };
        let result = eval
            .recv::<String>()
            .await
            .map_err(|_| ServerFnError::new("could not coordinate authentication"))?;
        if result != "locked" {
            return Err(ServerFnError::new(
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
