let paymentWindow;
let handedOff = false;
try {
    paymentWindow = window.open('about:blank', '_blank');
    if (!paymentWindow) {
        dioxus.send({ok: false, error: 'Allow pop-ups for this shop, then try opening your payment again.'});
    } else {
        paymentWindow.opener = null;
        dioxus.send({ok: true, value: null});
        const command = await dioxus.recv();
        if (command.action === 'open') {
            if (paymentWindow.closed) throw new Error('Payment window closed');
            if (localStorage.getItem(`enzumeii.checkout.ignored.v1.${command.checkout_id}`) === 'true') throw new Error('Checkout was cancelled');
            paymentWindow.location.replace(command.url);
            handedOff = true;
            try { paymentWindow.focus(); } catch (_) {}
            dioxus.send({ok: true, value: null});
        } else {
            paymentWindow.close();
        }
    }
} catch (_) {
    if (paymentWindow && !handedOff) {
        try { paymentWindow.close(); } catch (_) {}
    }
    dioxus.send({ok: false, error: 'The payment window could not be opened or was closed. Resume this payment to open it again.'});
}
