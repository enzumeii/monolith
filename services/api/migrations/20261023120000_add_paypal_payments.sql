CREATE TABLE paypal_payments (
    id UUID PRIMARY KEY,
    idempotency_key UUID NOT NULL UNIQUE,
    capability_digest BYTEA NOT NULL CHECK (octet_length(capability_digest) = 32),
    reference TEXT NOT NULL CHECK (octet_length(reference) BETWEEN 1 AND 127),
    amount TEXT NOT NULL CHECK (amount ~ '^(0|[1-9][0-9]*)([.][0-9]{2})?$' AND amount::numeric > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    minor_units BIGINT NOT NULL CHECK (minor_units > 0),
    merchant_id TEXT NOT NULL CHECK (merchant_id <> ''),
    environment TEXT NOT NULL CHECK (environment <> ''),
    api_origin TEXT NOT NULL CHECK (api_origin <> ''),
    paypal_order_id TEXT,
    approval_url TEXT,
    status TEXT NOT NULL DEFAULT 'CREATING' CHECK (status IN ('CREATING','AWAITING_APPROVAL','PENDING','COMPLETED','DENIED','EXCEPTION')),
    create_request_id UUID NOT NULL UNIQUE,
    capture_request_id UUID NOT NULL UNIQUE,
    operation TEXT CHECK (operation IN ('CREATE','CAPTURE','RECONCILE')),
    lease_token UUID,
    lease_until TIMESTAMPTZ,
    create_started_at TIMESTAMPTZ,
    capture_started_at TIMESTAMPTZ,
    verified_capture_id TEXT,
    verified_at TIMESTAMPTZ,
    exception TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    UNIQUE (merchant_id, environment, paypal_order_id),
    UNIQUE (id, merchant_id, environment),
    CHECK ((operation IS NULL AND lease_token IS NULL AND lease_until IS NULL)
        OR (operation IS NOT NULL AND lease_token IS NOT NULL AND lease_until IS NOT NULL)),
    CHECK ((status = 'COMPLETED') = (verified_at IS NOT NULL AND verified_capture_id IS NOT NULL))
);
CREATE INDEX paypal_payments_successful_reference ON paypal_payments (merchant_id, environment, reference) WHERE verified_at IS NOT NULL;

CREATE TABLE paypal_payment_captures (
    capture_id TEXT NOT NULL,
    payment_id UUID NOT NULL,
    merchant_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    paypal_order_id TEXT NOT NULL,
    amount TEXT NOT NULL CHECK (amount ~ '^(0|[1-9][0-9]*)([.][0-9]{2})?$' AND amount::numeric > 0),
    currency TEXT NOT NULL CHECK (currency ~ '^[A-Z]{3}$'),
    minor_units BIGINT NOT NULL CHECK (minor_units > 0),
    provider_status TEXT NOT NULL CHECK (provider_status = 'COMPLETED'),
    verified_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (merchant_id, environment, capture_id),
    UNIQUE (payment_id, capture_id),
    FOREIGN KEY (payment_id, merchant_id, environment) REFERENCES paypal_payments (id, merchant_id, environment)
);
ALTER TABLE paypal_payments ADD CONSTRAINT paypal_verified_capture_fk
    FOREIGN KEY (id, verified_capture_id) REFERENCES paypal_payment_captures (payment_id, capture_id);

CREATE TABLE paypal_payment_events (
    merchant_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    event_id TEXT NOT NULL CHECK (event_id <> ''),
    payload_digest BYTEA NOT NULL CHECK (octet_length(payload_digest) = 32),
    event_type TEXT NOT NULL,
    payment_id UUID REFERENCES paypal_payments (id),
    paypal_order_id TEXT,
    capture_id TEXT,
    state TEXT NOT NULL DEFAULT 'PENDING' CHECK (state IN ('PENDING','PROCESSED','IGNORED')),
    verified_at TIMESTAMPTZ NOT NULL,
    processed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY (merchant_id, environment, event_id),
    CHECK ((state = 'PENDING') = (processed_at IS NULL))
);
