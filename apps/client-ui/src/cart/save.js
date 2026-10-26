const key = __STORAGE_KEY__;
const convention = __CONVENTION_ID__;
const value = __CART_JSON__;
async function save() {
    dioxus.send({raw: localStorage.getItem(key)});
    if (await dioxus.recv()) {
        localStorage.setItem(key, value);
        if (localStorage.getItem(key) !== value) throw new Error('Cart storage unavailable');
    }
    dioxus.send(true);
}
try {
    if (navigator.locks) {
        await navigator.locks.request(`enzumeii.cart.v1.${convention}`, save);
    } else {
        await save();
    }
} catch (_) {
    dioxus.send(false);
}
