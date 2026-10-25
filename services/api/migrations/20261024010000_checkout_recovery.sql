ALTER TABLE checkouts
    ADD COLUMN next_recovery_at TIMESTAMPTZ DEFAULT (CURRENT_TIMESTAMP + INTERVAL '60 seconds'),
    ADD COLUMN recovery_failures INTEGER NOT NULL DEFAULT 0 CHECK (recovery_failures >= 0),
    ADD COLUMN recovery_code TEXT,
    ADD COLUMN recovery_manual BOOLEAN NOT NULL DEFAULT FALSE;

UPDATE checkouts
SET next_recovery_at = CASE
    WHEN completion_state <> 'completed' AND payment_state <> 'unpaid'
        THEN created_at + INTERVAL '60 seconds'
    ELSE NULL
END;

CREATE INDEX checkouts_recovery_due
ON checkouts (next_recovery_at, id)
WHERE next_recovery_at IS NOT NULL
  AND NOT recovery_manual
  AND completion_state <> 'completed'
  AND payment_state <> 'unpaid';

ALTER TABLE checkouts
    ADD COLUMN completion_version SMALLINT NOT NULL DEFAULT 1,
    ADD COLUMN convention_slug TEXT,
    ADD COLUMN payment_recording_state TEXT NOT NULL DEFAULT 'not_started',
    ADD CONSTRAINT checkouts_completion_version_check CHECK (completion_version IN (1, 2)),
    ADD CONSTRAINT checkouts_payment_recording_state_check
        CHECK (payment_recording_state IN ('not_started', 'recording', 'uncertain', 'recorded')),
    ADD CONSTRAINT checkouts_v2_slug_check
        CHECK (completion_version = 1 OR convention_slug IS NOT NULL);

ALTER TABLE checkouts
    ADD COLUMN correlation_version SMALLINT NOT NULL DEFAULT 1,
    ADD CONSTRAINT checkouts_correlation_version_check CHECK (correlation_version IN (1, 2)),
    ADD CONSTRAINT checkouts_metadata_slug_check
        CHECK (correlation_version = 1 OR convention_slug IS NOT NULL);
