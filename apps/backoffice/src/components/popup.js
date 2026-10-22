const [id, initiallyOpen] = await dioxus.recv();
const dialog = document.getElementById(id);
if (!dialog) return;

let open = initiallyOpen;
let returnFocus;
try {
    while (open !== null) {
        if (open && !dialog.open && dialog.isConnected) {
            returnFocus = document.activeElement;
            dialog.showModal();
        } else if (!open && dialog.open) {
            dialog.close();
        }
        open = await dioxus.recv();
    }
} finally {
    if (dialog.open) dialog.close();
    if (!dialog.isConnected && returnFocus?.isConnected) returnFocus.focus();
}
