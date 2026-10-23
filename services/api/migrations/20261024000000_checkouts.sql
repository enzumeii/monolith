CREATE TABLE checkouts (
    id UUID PRIMARY KEY,
    operation_id UUID NOT NULL UNIQUE,
    guest_secret_hash BYTEA NOT NULL CHECK (octet_length(guest_secret_hash) = 32),
    owner_expires_at TIMESTAMPTZ NOT NULL,
    request_fingerprint BYTEA NOT NULL CHECK (octet_length(request_fingerprint) = 32),
    provider TEXT NOT NULL CHECK (provider IN ('stripe', 'paypal')),
    snapshot JSONB NOT NULL CHECK (jsonb_typeof(snapshot) = 'object'),
    provider_context JSONB NOT NULL CHECK (jsonb_typeof(provider_context) = 'object'),
    payment_state TEXT NOT NULL DEFAULT 'not_started'
        CHECK (payment_state IN ('not_started', 'creating', 'requires_action', 'processing', 'unpaid', 'paid', 'uncertain')),
    draft_state TEXT NOT NULL DEFAULT 'not_started'
        CHECK (draft_state IN ('not_started', 'creating', 'reserved', 'uncertain')),
    completion_state TEXT NOT NULL DEFAULT 'not_started'
        CHECK (completion_state IN ('not_started', 'completing', 'completed', 'uncertain')),
    draft_id TEXT UNIQUE,
    provider_resource_id TEXT,
    payment_reference TEXT,
    order_id TEXT UNIQUE,
    order_name TEXT,
    redirect_url TEXT,
    reservation_expires_at TIMESTAMPTZ,
    create_request_id TEXT NOT NULL,
    capture_request_id TEXT NOT NULL,
    claim_token UUID,
    claim_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (id = operation_id),
    CHECK ((claim_token IS NULL) = (claim_expires_at IS NULL)),
    CHECK (completion_state <> 'completed' OR (payment_state = 'paid' AND order_id IS NOT NULL AND order_name IS NOT NULL)),
    UNIQUE (provider, provider_resource_id),
    UNIQUE (provider, payment_reference)
);

CREATE TABLE checkout_items (
    checkout_id UUID NOT NULL REFERENCES checkouts(id),
    variant_id UUID NOT NULL,
    shopify_product_id TEXT NOT NULL,
    shopify_variant_id TEXT NOT NULL,
    product_title TEXT,
    variant_title TEXT,
    sku TEXT,
    quantity INTEGER NOT NULL CHECK (quantity BETWEEN 1 AND 999),
    unit_price_minor BIGINT NOT NULL CHECK (unit_price_minor > 0),
    total_minor BIGINT NOT NULL CHECK (total_minor = quantity::bigint * unit_price_minor),
    PRIMARY KEY (checkout_id, variant_id),
    UNIQUE (checkout_id, shopify_variant_id)
);

CREATE TABLE checkout_events (
    provider TEXT NOT NULL CHECK (provider IN ('stripe', 'paypal')),
    account_id TEXT NOT NULL,
    environment TEXT NOT NULL,
    event_id TEXT NOT NULL,
    checkout_id UUID NOT NULL REFERENCES checkouts(id),
    processed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (provider, account_id, environment, event_id)
);

CREATE INDEX checkout_events_pending ON checkout_events (checkout_id)
    WHERE processed_at IS NULL;

CREATE TABLE checkout_rate_limits (
    source_hash BYTEA PRIMARY KEY CHECK (octet_length(source_hash) = 32),
    window_started_at TIMESTAMPTZ NOT NULL,
    attempts INTEGER NOT NULL CHECK (attempts > 0)
);
