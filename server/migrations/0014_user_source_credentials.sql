-- Учётки источников на пользователя: каждый читатель подключает свой
-- аккаунт Remanga. Плейнтекст в БД не пишется: ciphertext + nonce
-- зашифрованы AEAD-ключом из env (SOURCE_CREDENTIALS_KEY), aad привязан
-- к user_id, source_id и key_version. key_version зарезервирован под
-- будущую ротацию ключа; сейчас всегда 1.
CREATE TABLE user_source_credentials (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    ciphertext BLOB NOT NULL,
    nonce BLOB NOT NULL,
    key_version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    last_verified_at INTEGER,
    last_error TEXT,
    PRIMARY KEY (user_id, source_id)
);
