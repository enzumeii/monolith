const input = __INPUT__;
const recordKey = id => `enzumeii.checkout.v1.${id}`;
const activeKey = convention => `enzumeii.checkout.active.v1.${convention}`;
const payloadKey = id => `enzumeii.checkout.payload.v1.${id}`;
const ignoredKey = id => `enzumeii.checkout.ignored.v1.${id}`;
const ignored = id => localStorage.getItem(ignoredKey(id)) === 'true';
const fail = message => dioxus.send({ok: false, error: message});
function read(id) {
    if (!id) return null;
    const raw = localStorage.getItem(recordKey(id));
    if (!raw) return null;
    const attempt = JSON.parse(raw);
    if (attempt.version !== 1 || attempt.operation_id !== id || !/^[a-f0-9]{64}$/.test(attempt.secret) || !attempt.snapshot || typeof attempt.slug !== 'string') throw new Error('Invalid recovery record');
    return attempt;
}
function persist(attempt) {
    const raw = JSON.stringify(attempt);
    localStorage.setItem(recordKey(attempt.operation_id), raw);
    if (localStorage.getItem(recordKey(attempt.operation_id)) !== raw) throw new Error('Recovery storage unavailable');
}
function sameCart(current, snapshot) {
    const sameShape = (left, right, fields) => [left, right].every(value =>
        value && typeof value === 'object' && !Array.isArray(value)
        && Object.keys(value).length === fields.length
        && fields.every(field => Object.hasOwn(value, field)));
    if (!sameShape(current, snapshot, ['version', 'convention_id', 'checkout_id', 'items'])
        || !Array.isArray(current.items) || !Array.isArray(snapshot.items)
        || current.items.length !== snapshot.items.length) return false;
    return current.version === snapshot.version
        && current.convention_id === snapshot.convention_id
        && current.checkout_id === snapshot.checkout_id
        && current.items.every((item, index) => {
            const saved = snapshot.items[index];
            return sameShape(item, saved, ['variant_id', 'quantity', 'accepted_unit_price'])
                && item.variant_id === saved.variant_id
                && item.quantity === saved.quantity
                && item.accepted_unit_price === saved.accepted_unit_price;
        });
}
function checkedWrite(key, raw) {
    localStorage.setItem(key, raw);
    if (localStorage.getItem(key) !== raw) throw new Error('Recovery storage unavailable');
}
function currentCart(attempt) {
    const raw = localStorage.getItem(`enzumeii.cart.v1.${attempt.snapshot.convention_id}`);
    const cart = raw ? JSON.parse(raw) : {...attempt.snapshot, checkout_id: crypto.randomUUID(), items: []};
    if (cart.version !== 1 || cart.convention_id !== attempt.snapshot.convention_id
        || typeof cart.checkout_id !== 'string' || !Array.isArray(cart.items)) throw new Error('Invalid saved cart');
    return cart;
}
try {
    if (['abandon', 'ignored'].includes(input.action)) {
        const attempt = read(input.id);
        if (!attempt || attempt.slug !== input.slug) throw new Error('Recovery belongs to another convention');
        if (input.action === 'ignored') {
            dioxus.send({ok: true, value: ignored(attempt.operation_id)});
        } else {
            if (!navigator.locks || !crypto.randomUUID) throw new Error('Secure browser ownership is unavailable');
            // Publish the fence before waiting for an in-flight request to release ownership.
            checkedWrite(ignoredKey(attempt.operation_id), 'true');
            window.dispatchEvent(new Event('enzumeii-checkout-change'));
            await navigator.locks.request(`enzumeii.checkout.${attempt.snapshot.convention_id}`, async () => {
                const cart = await navigator.locks.request(`enzumeii.cart.v1.${attempt.snapshot.convention_id}`, () => {
                    const current = currentCart(attempt);
                    if (current.checkout_id === attempt.snapshot.checkout_id) current.checkout_id = crypto.randomUUID();
                    checkedWrite(`enzumeii.cart.v1.${attempt.snapshot.convention_id}`, JSON.stringify(current));
                    return current;
                });
                const key = activeKey(attempt.snapshot.convention_id);
                if (localStorage.getItem(key) === attempt.operation_id) {
                    localStorage.removeItem(key);
                    if (localStorage.getItem(key) !== null) throw new Error('Recovery storage unavailable');
                }
                sessionStorage.removeItem(payloadKey(attempt.operation_id));
                if (sessionStorage.getItem(payloadKey(attempt.operation_id)) !== null) throw new Error('Session storage unavailable');
                dioxus.send({ok: true, value: cart});
            });
        }
    } else if (input.action === 'read') {
        const id = input.id || localStorage.getItem(activeKey(input.convention_id));
        const attempt = read(id);
        if (attempt && attempt.slug !== input.slug) throw new Error('Recovery belongs to another convention');
        dioxus.send({ok: true, value: !input.id && attempt && ignored(attempt.operation_id) ? null : attempt});
    } else {
        if (!navigator.locks || !crypto.randomUUID || !crypto.getRandomValues) throw new Error('Secure browser ownership is unavailable');
        const lockName = `enzumeii.checkout.${input.convention_id || input.id}`;
        await navigator.locks.request(lockName, async () => {
            let attempt = read(input.id || localStorage.getItem(activeKey(input.convention_id)));
            if (attempt && ignored(attempt.operation_id)) throw new Error('Checkout was abandoned');
            let payload = null;
            let fresh = false;
            if (attempt && attempt.slug !== input.slug) throw new Error('Recovery belongs to another convention');
            if (input.action === 'start' && (!attempt || ['unpaid', 'completed', 'rejected'].includes(attempt.state))) {
                const current = localStorage.getItem(`enzumeii.cart.v1.${input.convention_id}`);
                if (current && JSON.parse(current).checkout_id !== input.snapshot.checkout_id) throw new Error('Cart identity changed');
                const id = crypto.randomUUID();
                const secret = Array.from(crypto.getRandomValues(new Uint8Array(32)), b => b.toString(16).padStart(2, '0')).join('');
                payload = input.payload.replace('"operation_id":""', `"operation_id":"${id}"`);
                if (payload === input.payload) throw new Error('Invalid checkout payload');
                attempt = {version: 1, operation_id: id, secret, slug: input.slug, provider: input.provider, snapshot: input.snapshot, state: 'prepared', currency: input.currency, total: input.total};
                sessionStorage.setItem(payloadKey(id), payload);
                if (sessionStorage.getItem(payloadKey(id)) !== payload) throw new Error('Session storage unavailable');
                persist(attempt);
                localStorage.setItem(activeKey(input.convention_id), id);
                if (localStorage.getItem(activeKey(input.convention_id)) !== id) throw new Error('Recovery storage unavailable');
                fresh = true;
            }
            if (!attempt) throw new Error('No checkout recovery information exists in this browser');
            if (!payload) payload = sessionStorage.getItem(payloadKey(attempt.operation_id));
            // Persist uncertainty before control can transmit an external request.
            attempt.state = ['completed', 'unpaid', 'rejected'].includes(attempt.state) ? attempt.state : 'submitted';
            persist(attempt);
            dioxus.send({ok: true, value: {attempt, payload, fresh}});
            const command = await dioxus.recv();
            if (ignored(attempt.operation_id)) {
                sessionStorage.removeItem(payloadKey(attempt.operation_id));
                dioxus.send({ok: true, value: false});
                return;
            }
            if (command.status) attempt.state = command.status;
            persist(attempt);
            let cleared = false;
            if (attempt.state === 'completed') {
                cleared = await navigator.locks.request(`enzumeii.cart.v1.${attempt.snapshot.convention_id}`, () => {
                    sessionStorage.removeItem(payloadKey(attempt.operation_id));
                    if (ignored(attempt.operation_id)) return false;
                    if (localStorage.getItem(activeKey(attempt.snapshot.convention_id)) === attempt.operation_id) localStorage.removeItem(activeKey(attempt.snapshot.convention_id));
                    const key = `enzumeii.cart.v1.${attempt.snapshot.convention_id}`;
                    const raw = localStorage.getItem(key);
                    if (!raw || !sameCart(JSON.parse(raw), attempt.snapshot)) return false;
                    checkedWrite(key, JSON.stringify({...attempt.snapshot, checkout_id: crypto.randomUUID(), items: []}));
                    return true;
                });
            } else if (['unpaid', 'rejected', 'expired'].includes(attempt.state)) {
                sessionStorage.removeItem(payloadKey(attempt.operation_id));
            }
            dioxus.send({ok: true, value: cleared});
        });
    }
} catch (_) {
    fail('Unable to safely recover this payment in this browser. Keep any payment receipt and contact the shop before paying again. Secure browser storage and payment ownership must be available.');
}
