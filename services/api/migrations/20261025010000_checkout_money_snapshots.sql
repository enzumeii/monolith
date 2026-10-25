ALTER TABLE checkouts
    ADD COLUMN payment_amount_minor BIGINT,
    ADD COLUMN payment_currency TEXT,
    ADD COLUMN shopify_amount_minor BIGINT,
    ADD COLUMN shopify_currency TEXT,
    DROP CONSTRAINT checkouts_correlation_version_check,
    ADD CONSTRAINT checkouts_correlation_version_check
        CHECK (correlation_version IN (1, 2, 3)),
    ADD CONSTRAINT checkouts_payment_snapshot_check CHECK (
        (payment_amount_minor IS NULL AND payment_currency IS NULL)
        OR (payment_amount_minor IS NOT NULL AND payment_amount_minor >= 0
            AND payment_currency IS NOT NULL AND payment_currency IN (
                'AUD', 'BHD', 'BRL', 'CAD', 'CHF', 'CNY', 'CZK', 'DKK',
                'EUR', 'GBP', 'HKD', 'HUF', 'ILS', 'IQD', 'JOD', 'JPY',
                'KWD', 'LYD', 'MXN', 'MYR', 'NOK', 'NZD', 'OMR', 'PHP',
                'PLN', 'SEK', 'SGD', 'THB', 'TND', 'TWD', 'USD'
            ))
    ),
    ADD CONSTRAINT checkouts_shopify_snapshot_check CHECK (
        (shopify_amount_minor IS NULL AND shopify_currency IS NULL)
        OR (shopify_amount_minor IS NOT NULL AND shopify_amount_minor >= 0
            AND shopify_currency IS NOT NULL AND shopify_currency IN (
                'AUD', 'BHD', 'BRL', 'CAD', 'CHF', 'CNY', 'CZK', 'DKK',
                'EUR', 'GBP', 'HKD', 'HUF', 'ILS', 'IQD', 'JOD', 'JPY',
                'KWD', 'LYD', 'MXN', 'MYR', 'NOK', 'NZD', 'OMR', 'PHP',
                'PLN', 'SEK', 'SGD', 'THB', 'TND', 'TWD', 'USD'
            ))
    ),
    ADD CONSTRAINT checkouts_versioned_snapshot_check CHECK (
        (correlation_version IN (1, 2)
            AND payment_amount_minor IS NULL AND payment_currency IS NULL
            AND shopify_amount_minor IS NULL AND shopify_currency IS NULL)
        OR (correlation_version = 3 AND payment_amount_minor IS NOT NULL
            AND payment_currency IS NOT NULL
            AND ((provider_resource_id IS NULL AND gateway_request_fingerprint IS NULL)
                OR shopify_amount_minor IS NOT NULL))
    );
