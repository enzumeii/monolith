CREATE TABLE conventions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    logo_url TEXT,
    country_code TEXT NOT NULL,
    event_starts_on DATE NOT NULL,
    event_ends_on DATE NOT NULL,
    description_markdown TEXT NOT NULL,
    preorder_enabled BOOLEAN NOT NULL DEFAULT false,
    currency_code TEXT NOT NULL CHECK (currency_code ~ '^[A-Z]{3}$'),
    payment_methods TEXT[] NOT NULL CHECK (cardinality(payment_methods) > 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (event_starts_on <= event_ends_on)
);

CREATE TABLE convention_variants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    convention_id UUID NOT NULL REFERENCES conventions(id) ON DELETE CASCADE,
    shopify_product_id TEXT NOT NULL,
    shopify_variant_id TEXT NOT NULL,
    final_price NUMERIC NOT NULL CHECK (final_price > 0 AND final_price < 'Infinity'::numeric),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    product_title TEXT,
    variant_title TEXT,
    sku TEXT,
    product_image_url TEXT,
    product_image_alt_text TEXT,
    variant_image_url TEXT,
    variant_image_alt_text TEXT,
    UNIQUE (convention_id, shopify_variant_id)
);

CREATE FUNCTION preserve_convention_identity() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF NEW.slug IS DISTINCT FROM OLD.slug OR NEW.currency_code IS DISTINCT FROM OLD.currency_code THEN
        RAISE EXCEPTION 'Convention slug and currency are immutable' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER convention_identity
    BEFORE UPDATE ON conventions
    FOR EACH ROW
    EXECUTE FUNCTION preserve_convention_identity();
