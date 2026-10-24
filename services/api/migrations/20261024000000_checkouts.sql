CREATE TABLE checkouts (
    id UUID PRIMARY KEY,
    guest_secret_hash BYTEA NOT NULL CHECK (octet_length(guest_secret_hash) = 32),
    owner_expires_at TIMESTAMPTZ NOT NULL,
    request_fingerprint BYTEA NOT NULL CHECK (octet_length(request_fingerprint) = 32),
    convention_id UUID NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('stripe', 'paypal')),
    provider_account_id TEXT NOT NULL,
    provider_environment TEXT NOT NULL,
    evidence_hash BYTEA NOT NULL CHECK (octet_length(evidence_hash) = 32),
    required_tag_hashes BYTEA[] NOT NULL,
    success_url TEXT,
    cancel_url TEXT,
    gateway_request_fingerprint BYTEA CHECK (octet_length(gateway_request_fingerprint) = 32),
    payment_state TEXT NOT NULL DEFAULT 'not_started'
        CHECK (payment_state IN ('not_started', 'creating', 'requires_action', 'processing', 'unpaid', 'paid', 'uncertain')),
    draft_state TEXT NOT NULL DEFAULT 'not_started'
        CHECK (draft_state IN ('not_started', 'creating', 'reserved', 'uncertain')),
    completion_state TEXT NOT NULL DEFAULT 'not_started'
        CHECK (completion_state IN ('not_started', 'completing', 'completed', 'uncertain')),
    draft_id TEXT UNIQUE,
    provider_resource_id TEXT,
    order_id TEXT UNIQUE,
    reservation_expires_at TIMESTAMPTZ,
    claim_token UUID,
    claim_expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK ((claim_token IS NULL) = (claim_expires_at IS NULL)),
    CHECK (completion_state <> 'completed' OR (payment_state = 'paid' AND order_id IS NOT NULL)),
    UNIQUE (provider, provider_resource_id)
);
