CREATE TABLE checkout_completions (
    id UUID PRIMARY KEY,
    draft_id TEXT NOT NULL UNIQUE,
    snapshot JSONB NOT NULL,
    attempt JSONB NOT NULL,
    guest_token_hash BYTEA NOT NULL CHECK (octet_length(guest_token_hash)=32),
    guest_token_expires_at TIMESTAMPTZ NOT NULL,
    guest_token_revoked_at TIMESTAMPTZ,
    next_verify_at TIMESTAMPTZ,
    payment_state TEXT NOT NULL DEFAULT 'PENDING' CHECK (payment_state IN ('PENDING','UNPAID','EXPIRED','PAID')),
    completion_state TEXT NOT NULL DEFAULT 'READY' CHECK (completion_state IN ('READY','PROCESSING','UNCERTAIN','ORDER_SAVED','COMPLETE','OPERATOR')),
    trusted_payment JSONB,
    last_event_id TEXT,
    operator_payment JSONB,
    order_id TEXT UNIQUE,
    completion_intent BOOLEAN NOT NULL DEFAULT FALSE,
    lease_token UUID,
    lease_until TIMESTAMPTZ,
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((lease_token IS NULL) = (lease_until IS NULL)),
    CHECK (payment_state <> 'PAID' OR trusted_payment IS NOT NULL),
    CHECK (completion_state <> 'COMPLETE' OR (order_id IS NOT NULL AND payment_state='PAID'))
);
CREATE TABLE checkout_payment_aliases (
    checkout_id UUID NOT NULL REFERENCES checkout_completions(id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (provider IN ('STRIPE','PAYPAL')),
    account TEXT NOT NULL,
    environment TEXT NOT NULL CHECK (environment IN ('TEST','LIVE')),
    kind TEXT NOT NULL CHECK (kind IN ('STRIPE_SESSION','STRIPE_PAYMENT_INTENT','STRIPE_CHARGE','PAYPAL_ORDER','PAYPAL_CAPTURE')),
    value TEXT NOT NULL CHECK (length(value)>0),
    PRIMARY KEY (provider,account,environment,kind,value),
    UNIQUE(checkout_id,kind)
);
CREATE INDEX checkout_payment_alias_reference ON checkout_payment_aliases(provider,value);
CREATE FUNCTION protect_checkout_completion_identity() RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.id IS DISTINCT FROM OLD.id OR NEW.draft_id IS DISTINCT FROM OLD.draft_id OR NEW.snapshot IS DISTINCT FROM OLD.snapshot OR NEW.attempt IS DISTINCT FROM OLD.attempt OR NEW.guest_token_hash IS DISTINCT FROM OLD.guest_token_hash THEN
        RAISE EXCEPTION 'checkout identity is immutable';
    END IF;
    IF OLD.order_id IS NOT NULL AND NEW.order_id IS DISTINCT FROM OLD.order_id THEN
        RAISE EXCEPTION 'checkout order is immutable';
    END IF;
    IF OLD.payment_state='PAID' AND NEW.payment_state<>'PAID' THEN
        RAISE EXCEPTION 'paid checkout cannot be downgraded';
    END IF;
    RETURN NEW;
END; $$;
CREATE TRIGGER protect_checkout_completion_identity BEFORE UPDATE ON checkout_completions FOR EACH ROW EXECUTE FUNCTION protect_checkout_completion_identity();
