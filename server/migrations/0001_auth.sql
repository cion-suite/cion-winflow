-- Пользователи, инвайты и сессии. Время хранится как unix seconds (INTEGER),
-- чтобы не зависеть от строкового формата даты в SQLite.

CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    username      TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT    NOT NULL,
    role          TEXT    NOT NULL DEFAULT 'user' CHECK (role IN ('admin', 'user')),
    created_at    INTEGER NOT NULL,
    disabled_at   INTEGER
);

CREATE TABLE invites (
    id         INTEGER PRIMARY KEY,
    code_hash  TEXT    NOT NULL UNIQUE,
    created_by INTEGER REFERENCES users (id),
    used_by    INTEGER REFERENCES users (id),
    expires_at INTEGER,
    created_at INTEGER NOT NULL
);

CREATE TABLE sessions (
    id           INTEGER PRIMARY KEY,
    token_hash   TEXT    NOT NULL UNIQUE,
    user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at   INTEGER NOT NULL,
    expires_at   INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL
);

CREATE INDEX sessions_user_id ON sessions (user_id);
