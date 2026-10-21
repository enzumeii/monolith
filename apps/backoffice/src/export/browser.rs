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

pub(crate) fn encoded_bytes(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
#[cfg(test)]
#[path = "../tests/export/browser.rs"]
mod tests;
