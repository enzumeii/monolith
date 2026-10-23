use base64::Engine;
use dioxus::prelude::*;

pub(crate) async fn download(
    bytes: Vec<u8>,
    filename: &str,
    mime: &str,
    id: &str,
) -> Result<(), String> {
    let mut download = document::eval(
        r"
        const [encoded, filename, mime, id] = await dioxus.recv();
        if (!document.getElementById(id)?.isConnected) { dioxus.send(false); return; }
        const url = URL.createObjectURL(new Blob([Uint8Array.from(atob(encoded), character => character.charCodeAt(0))], {type: mime}));
        const link = document.createElement('a');
        link.href = url; link.download = filename;
        document.body.appendChild(link); link.click(); link.remove();
        setTimeout(() => URL.revokeObjectURL(url), 1000);
        dioxus.send(true);
    ",
    );
    download
        .send((encoded_bytes(&bytes), filename, mime, id))
        .map_err(|_| "Unable to save report".to_string())?;
    download
        .recv::<bool>()
        .await
        .map_err(|_| "Unable to save report".to_string())?;
    Ok(())
}

pub(crate) async fn download_url(url: &str) -> Result<(), String> {
    let mut navigation = document::eval(
        r"
        const value = await dioxus.recv();
        const url = new URL(value);
        if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) {
            dioxus.send(false); return;
        }
        window.location.assign(url.href);
        dioxus.send(true);
        ",
    );
    navigation
        .send(url)
        .map_err(|_| "Unable to start download".to_string())?;
    match navigation.recv::<bool>().await {
        Ok(true) => Ok(()),
        _ => Err("Unable to start download".into()),
    }
}

pub(crate) fn encoded_bytes(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
#[cfg(test)]
#[path = "../tests/export/browser.rs"]
mod tests;
