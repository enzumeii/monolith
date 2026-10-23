ALTER TABLE convention_variants
    ADD COLUMN product_title TEXT,
    ADD COLUMN variant_title TEXT,
    ADD COLUMN sku TEXT,
    ADD COLUMN product_image_url TEXT,
    ADD COLUMN product_image_alt_text TEXT,
    ADD COLUMN variant_image_url TEXT,
    ADD COLUMN variant_image_alt_text TEXT;
