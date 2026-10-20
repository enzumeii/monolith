CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE cron_configurations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL CHECK (char_length(btrim(name)) BETWEEN 1 AND 120),
    report_type TEXT NOT NULL CHECK (report_type IN ('TRANSACTION', 'INVENTORY')),
    cron_expression TEXT NOT NULL,
    receiver_email TEXT NOT NULL CHECK (char_length(receiver_email) BETWEEN 1 AND 254),
    period_mode TEXT NULL CHECK (period_mode IN ('PREVIOUS_MONTH', 'FIRST_HALF_MONTH')),
    inventory_id TEXT NULL,
    status TEXT NOT NULL DEFAULT 'DISABLED' CHECK (status IN ('ENABLED', 'DISABLED')),
    next_run_at TIMESTAMP WITHOUT TIME ZONE NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'),
    updated_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'),
    CHECK (
        (report_type = 'TRANSACTION' AND period_mode IS NOT NULL AND inventory_id IS NULL)
        OR (report_type = 'INVENTORY' AND period_mode IS NULL AND inventory_id ~ '^[1-9][0-9]*$')
    ),
    CHECK (
        (status = 'ENABLED' AND next_run_at IS NOT NULL)
        OR (status = 'DISABLED' AND next_run_at IS NULL)
    )
);

CREATE TABLE report_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    cron_configuration_id UUID NULL REFERENCES cron_configurations(id) ON DELETE SET NULL,
    configuration_id_snapshot UUID NOT NULL,
    cron_name TEXT NOT NULL,
    cron_expression TEXT NOT NULL,
    report_type TEXT NOT NULL CHECK (report_type IN ('TRANSACTION', 'INVENTORY')),
    receiver_email TEXT NOT NULL,
    period_mode TEXT NULL CHECK (period_mode IN ('PREVIOUS_MONTH', 'FIRST_HALF_MONTH')),
    inventory_id TEXT NULL,
    scheduled_for TIMESTAMP WITHOUT TIME ZONE NOT NULL,
    period_start TIMESTAMP WITHOUT TIME ZONE NULL,
    period_end_exclusive TIMESTAMP WITHOUT TIME ZONE NULL,
    generation_status TEXT NOT NULL CHECK (generation_status IN ('QUEUED', 'RUNNING', 'SUCCEEDED', 'FAILED')),
    email_status TEXT NOT NULL CHECK (email_status IN ('NOT_READY', 'PENDING', 'SENDING', 'SENT', 'FAILED', 'UNKNOWN')),
    report_schema_version INTEGER NOT NULL DEFAULT 1 CHECK (report_schema_version > 0),
    report_data JSONB NULL,
    row_count BIGINT NULL CHECK (row_count >= 0),
    started_at TIMESTAMP WITHOUT TIME ZONE NULL,
    generated_at TIMESTAMP WITHOUT TIME ZONE NULL,
    email_first_attempt_at TIMESTAMP WITHOUT TIME ZONE NULL,
    sent_at TIMESTAMP WITHOUT TIME ZONE NULL,
    generation_error TEXT NULL,
    email_error TEXT NULL,
    email_message_id TEXT NULL,
    generation_attempts INTEGER NOT NULL DEFAULT 0 CHECK (generation_attempts >= 0),
    email_attempts INTEGER NOT NULL DEFAULT 0 CHECK (email_attempts >= 0),
    lease_token UUID NULL,
    lease_until TIMESTAMP WITHOUT TIME ZONE NULL,
    next_attempt_at TIMESTAMP WITHOUT TIME ZONE NULL,
    created_at TIMESTAMP WITHOUT TIME ZONE NOT NULL DEFAULT (CURRENT_TIMESTAMP AT TIME ZONE 'UTC'),
    CHECK (
        (report_type = 'TRANSACTION' AND period_mode IS NOT NULL AND inventory_id IS NULL
            AND period_start IS NOT NULL AND period_end_exclusive IS NOT NULL
            AND period_start < period_end_exclusive)
        OR (report_type = 'INVENTORY' AND period_mode IS NULL
            AND inventory_id ~ '^[1-9][0-9]*$'
            AND period_start IS NULL AND period_end_exclusive IS NULL)
    ),
    CHECK (
        generation_status <> 'SUCCEEDED'
        OR (report_data IS NOT NULL AND row_count IS NOT NULL
            AND started_at IS NOT NULL AND generated_at IS NOT NULL)
    ),
    CHECK (generation_status <> 'RUNNING' OR (lease_token IS NOT NULL AND lease_until IS NOT NULL)),
    CHECK (email_status NOT IN ('PENDING', 'SENDING', 'SENT', 'FAILED', 'UNKNOWN')
        OR generation_status = 'SUCCEEDED'),
    UNIQUE (configuration_id_snapshot, scheduled_for)
);

CREATE INDEX cron_configurations_enabled_due_idx
    ON cron_configurations (next_run_at, id) WHERE status = 'ENABLED';
CREATE INDEX report_results_history_idx
    ON report_results (scheduled_for DESC, id DESC);
CREATE INDEX report_results_configuration_history_idx
    ON report_results (configuration_id_snapshot, scheduled_for DESC, id DESC);
CREATE INDEX report_results_type_history_idx
    ON report_results (report_type, scheduled_for DESC, id DESC);
CREATE INDEX report_results_worker_idx
    ON report_results (generation_status, next_attempt_at, lease_until)
    WHERE generation_status IN ('QUEUED', 'RUNNING');
CREATE INDEX report_results_email_worker_idx
    ON report_results (email_status, next_attempt_at, lease_until)
    WHERE email_status IN ('PENDING', 'SENDING');
