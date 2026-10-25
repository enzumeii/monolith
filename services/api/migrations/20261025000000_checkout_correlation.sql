ALTER TABLE checkouts
    ADD COLUMN correlation_version SMALLINT NOT NULL DEFAULT 1,
    ADD CONSTRAINT checkouts_correlation_version_check CHECK (correlation_version IN (1, 2)),
    ADD CONSTRAINT checkouts_metadata_slug_check
        CHECK (correlation_version = 1 OR convention_slug IS NOT NULL);
