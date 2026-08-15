INSERT INTO admin (id, username, password_hash)
VALUES (
    'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11',
    'wichayutdew',
    '$argon2id$v=19$m=65536,p=4,t=3$8Et4eGAj4Jq54qQ5J1Qu2Q$0s03nBALKQvgtZGtud+h+pDQrB7pqfgRt4jagBxrsu4'
)
ON CONFLICT (username) DO NOTHING;
