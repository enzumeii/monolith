ALTER TABLE checkouts
    ADD COLUMN completion_version SMALLINT NOT NULL DEFAULT 1,
    ADD COLUMN convention_slug TEXT,
    ADD COLUMN payment_recording_state TEXT NOT NULL DEFAULT 'not_started',
    ADD CONSTRAINT checkouts_completion_version_check CHECK (completion_version IN (1, 2)),
    ADD CONSTRAINT checkouts_payment_recording_state_check
        CHECK (payment_recording_state IN ('not_started', 'recording', 'uncertain', 'recorded')),
    ADD CONSTRAINT checkouts_v2_slug_check
        CHECK (completion_version = 1 OR convention_slug IS NOT NULL);
