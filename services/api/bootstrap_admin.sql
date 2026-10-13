\set ON_ERROR_STOP on
BEGIN;
CREATE TEMP TABLE bootstrap_input ON COMMIT DROP AS
SELECT :'username'::text AS username, :'password_hash'::text AS password_hash, :'role'::text AS role;

DO $$
DECLARE
    account bootstrap_input%ROWTYPE;
    phc text[];
    salt bytea;
    digest bytea;
BEGIN
    SELECT * INTO STRICT account FROM bootstrap_input;
    IF char_length(account.username) NOT BETWEEN 1 AND 20
       OR btrim(account.username, E' \t\n\r\f' || U&'\000B\0085\00A0\1680\2000\2001\2002\2003\2004\2005\2006\2007\2008\2009\200A\2028\2029\202F\205F\3000') <> account.username THEN
        RAISE EXCEPTION 'Username must contain 1-20 characters without surrounding whitespace';
    END IF;
    IF account.role NOT IN ('ADMIN', 'SUPER_ADMIN') THEN
        RAISE EXCEPTION 'Invalid admin role';
    END IF;
    phc := regexp_match(account.password_hash, '^\$argon2(id|i|d)\$v=(16|19)\$m=([0-9]+),t=([0-9]+),p=([0-9]+)\$([A-Za-z0-9+/]+)\$([A-Za-z0-9+/]+)$');
    IF char_length(account.password_hash) > 255 OR phc IS NULL
       OR phc[3]::numeric NOT BETWEEN 8 AND 4294967295
       OR phc[4]::numeric NOT BETWEEN 1 AND 4294967295
       OR phc[5]::numeric NOT BETWEEN 1 AND 16777215
       OR phc[3]::numeric < 8 * phc[5]::numeric THEN
        RAISE EXCEPTION 'A valid Argon2 PHC password hash is required';
    END IF;
    BEGIN
        salt := decode(phc[6] || repeat('=', (4 - length(phc[6]) % 4) % 4), 'base64');
        digest := decode(phc[7] || repeat('=', (4 - length(phc[7]) % 4) % 4), 'base64');
    EXCEPTION WHEN OTHERS THEN
        RAISE EXCEPTION 'Invalid Argon2 PHC encoding';
    END;
    IF octet_length(salt) < 8 OR length(phc[6]) > 64 OR octet_length(digest) NOT BETWEEN 10 AND 64
       OR rtrim(replace(encode(salt, 'base64'), E'\n', ''), '=') <> phc[6]
       OR rtrim(replace(encode(digest, 'base64'), E'\n', ''), '=') <> phc[7] THEN
        RAISE EXCEPTION 'Invalid Argon2 PHC salt or digest';
    END IF;
END $$;

INSERT INTO admin (username, password_hash, role)
SELECT username, password_hash, role::ADMIN_ROLE FROM bootstrap_input
RETURNING id, username, role;
COMMIT;
