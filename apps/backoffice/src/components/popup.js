const [id, initiallyOpen, dismissOnBackdrop] = await dioxus.recv();
const dialog = document.getElementById(id);
if (!dialog) return;

let open = initiallyOpen;
let returnFocus;
let pressedOutside = false;
const outside = event => {
    const bounds = dialog.getBoundingClientRect();
    return event.target === dialog && (event.clientX < bounds.left || event.clientX > bounds.right || event.clientY < bounds.top || event.clientY > bounds.bottom);
};
const onPointerDown = event => { pressedOutside = outside(event); };
const onClick = event => {
    if (dismissOnBackdrop && pressedOutside && outside(event)) {
        dialog.dispatchEvent(new Event('cancel', { cancelable: true }));
    }
    pressedOutside = false;
};
dialog.addEventListener('pointerdown', onPointerDown);
dialog.addEventListener('click', onClick);
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
    dialog.removeEventListener('pointerdown', onPointerDown);
    dialog.removeEventListener('click', onClick);
    if (dialog.open) dialog.close();
    if (!dialog.isConnected && returnFocus?.isConnected) returnFocus.focus();
}
