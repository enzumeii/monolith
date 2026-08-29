INSERT INTO admin (username, password_hash, role)
VALUES (
    'wichayutdew',
    '$argon2id$v=19$m=65536,p=4,t=3$8Et4eGAj4Jq54qQ5J1Qu2Q$0s03nBALKQvgtZGtud+h+pDQrB7pqfgRt4jagBxrsu4',
		'SUPER_ADMIN'
)
ON CONFLICT (username) DO NOTHING;
