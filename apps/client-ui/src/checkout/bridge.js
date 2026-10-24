const input = __INPUT__;
const recordKey = id => `enzumeii.checkout.v1.${id}`;
const activeKey = convention => `enzumeii.checkout.active.v1.${convention}`;
const payloadKey = id => `enzumeii.checkout.payload.v1.${id}`;
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
try {
    if (input.action === 'read') {
        const id = input.id || localStorage.getItem(activeKey(input.convention_id));
        const attempt = read(id);
        if (attempt && attempt.slug !== input.slug) throw new Error('Recovery belongs to another convention');
        dioxus.send({ok: true, value: attempt});
    } else {
        if (!navigator.locks || !crypto.randomUUID || !crypto.getRandomValues) throw new Error('Secure browser ownership is unavailable');
        const lockName = `enzumeii.checkout.${input.convention_id || input.id}`;
        await navigator.locks.request(lockName, async () => {
            let attempt = read(input.id || localStorage.getItem(activeKey(input.convention_id)));
            let payload = null;
            let fresh = false;
            if (attempt && attempt.slug !== input.slug) throw new Error('Recovery belongs to another convention');
            if (input.action === 'start' && (!attempt || ['unpaid', 'completed', 'rejected'].includes(attempt.state))) {
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
            if (command.status) attempt.state = command.status;
            persist(attempt);
            let cleared = false;
            if (attempt.state === 'completed') {
                sessionStorage.removeItem(payloadKey(attempt.operation_id));
                if (localStorage.getItem(activeKey(attempt.snapshot.convention_id)) === attempt.operation_id) localStorage.removeItem(activeKey(attempt.snapshot.convention_id));
                const key = `enzumeii.cart.v1.${attempt.snapshot.convention_id}`;
                const raw = localStorage.getItem(key);
                if (raw && JSON.stringify(JSON.parse(raw)) === JSON.stringify(attempt.snapshot)) {
                    localStorage.setItem(key, JSON.stringify({...attempt.snapshot, checkout_id: crypto.randomUUID(), items: []}));
                    cleared = true;
                }
            } else if (['unpaid', 'rejected', 'expired'].includes(attempt.state)) {
                sessionStorage.removeItem(payloadKey(attempt.operation_id));
            }
            dioxus.send({ok: true, value: cleared});
        });
    }
} catch (_) {
    fail('Unable to safely recover this payment in this browser. Keep any payment receipt and contact the shop before paying again. Secure browser storage and payment ownership must be available.');
}
