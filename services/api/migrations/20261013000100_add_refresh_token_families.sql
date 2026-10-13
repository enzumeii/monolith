ALTER TABLE refresh_tokens ADD COLUMN family_id UUID NOT NULL DEFAULT gen_random_uuid();
CREATE INDEX refresh_tokens_family_idx ON refresh_tokens(admin_id, family_id);
