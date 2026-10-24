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
