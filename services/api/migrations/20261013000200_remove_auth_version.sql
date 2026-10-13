-- JWTs intentionally remain valid until expiry. Preserve the applied migration history.
ALTER TABLE admin DROP COLUMN auth_version;
ALTER TABLE refresh_tokens DROP COLUMN auth_version;
