CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE cron_configurations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    report_type TEXT NOT NULL,
    cron_expression TEXT NOT NULL,
    receiver_email TEXT NOT NULL,
    parameter TEXT NOT NULL,
    enabled BOOLEAN NOT NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')
);

CREATE TABLE cron_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cron_configuration_id UUID NOT NULL REFERENCES cron_configurations(id) ON DELETE CASCADE,
    scheduled_for TIMESTAMP WITHOUT TIME ZONE NOT NULL,
    status TEXT NOT NULL DEFAULT 'PENDING',
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT NULL,
    published_at TIMESTAMP WITHOUT TIME ZONE NULL,
    lease_token UUID NULL,
    lease_until TIMESTAMP WITHOUT TIME ZONE NULL,
    execution_checkpoint JSONB NULL,
    UNIQUE (cron_configuration_id, scheduled_for)
);

CREATE TABLE report_results (
    id UUID PRIMARY KEY,
    report_type TEXT NOT NULL,
    attempted_at TIMESTAMP WITHOUT TIME ZONE NOT NULL,
    generated_at TIMESTAMP WITHOUT TIME ZONE NULL,
    status TEXT NOT NULL,
    error TEXT NULL,
    parameters TEXT NOT NULL
);

CREATE TABLE report_result_files (
    report_result_id UUID NOT NULL REFERENCES report_results(id) ON DELETE CASCADE,
    filename TEXT NOT NULL CHECK (btrim(filename) <> ''),
    position INTEGER NOT NULL CHECK (position >= 0),
    content_type TEXT NOT NULL CHECK (btrim(content_type) <> ''),
    bucket TEXT NOT NULL CHECK (btrim(bucket) <> ''),
    storage_key TEXT NOT NULL CHECK (btrim(storage_key) <> ''),
    size_bytes BIGINT NOT NULL CHECK (size_bytes >= 0),
    sha256 TEXT NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
    PRIMARY KEY (report_result_id, filename),
    UNIQUE (report_result_id, position),
    UNIQUE (bucket, storage_key)
);

CREATE INDEX cron_configurations_enabled_idx ON cron_configurations (id) WHERE enabled;
CREATE INDEX cron_jobs_worker_idx ON cron_jobs (status, lease_until, scheduled_for);
CREATE INDEX report_audit_history_idx ON report_results ((COALESCE(generated_at, attempted_at)) DESC, id DESC);
CREATE INDEX report_audit_type_history_idx ON report_results (report_type, (COALESCE(generated_at, attempted_at)) DESC, id DESC);
