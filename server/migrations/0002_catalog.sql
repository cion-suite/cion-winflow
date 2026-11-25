-- Каталог, источники, главы и пользовательские данные (фаза 3).
-- Таблицы создаются сразу в итоговой форме из docs/planned-now.md §5,
-- чтобы фазы 4-6 заполняли их, не делая ALTER. `jobs` появится в фазе 7.

CREATE TABLE sources (
    id                 INTEGER PRIMARY KEY,
    code               TEXT    NOT NULL UNIQUE,
    name               TEXT    NOT NULL,
    base_url           TEXT,
    enabled            INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    config_json        TEXT    NOT NULL DEFAULT '{}',
    descriptor_version INTEGER,
    config_error       TEXT,
    last_checked_at    INTEGER,
    last_error         TEXT,
    created_at         INTEGER NOT NULL
);

CREATE TABLE titles (
    id                          INTEGER PRIMARY KEY,
    kind                        TEXT    NOT NULL CHECK (kind IN ('manga', 'anime', 'book')),
    name                        TEXT    NOT NULL,
    -- Unicode-lowercase копия name: SQLite lower() понимает только ASCII,
    -- а поиск должен работать для кириллицы.
    name_lower                  TEXT    NOT NULL,
    alt_names_json              TEXT    NOT NULL DEFAULT '[]',
    authors_json                TEXT    NOT NULL DEFAULT '[]',
    genres_json                 TEXT    NOT NULL DEFAULT '[]',
    description                 TEXT,
    status                      TEXT    CHECK (status IS NULL OR status IN ('ongoing', 'completed', 'hiatus', 'cancelled')),
    year                        INTEGER CHECK (year IS NULL OR (year BETWEEN 1800 AND 2200)),
    lang                        TEXT,
    rating                      REAL    CHECK (rating IS NULL OR (rating >= 0.0 AND rating <= 10.0)),
    cover_path                  TEXT,
    origin_source_id            INTEGER REFERENCES sources (id) ON DELETE SET NULL,
    preferred_source_id         INTEGER REFERENCES sources (id) ON DELETE SET NULL,
    preferred_source_updated_at INTEGER,
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
);

CREATE INDEX titles_name_lower ON titles (name_lower);
CREATE INDEX titles_kind ON titles (kind);

CREATE TABLE source_entries (
    id               INTEGER PRIMARY KEY,
    title_id         INTEGER NOT NULL REFERENCES titles (id) ON DELETE CASCADE,
    source_id        INTEGER NOT NULL REFERENCES sources (id),
    kind             TEXT    NOT NULL CHECK (kind IN ('manga', 'anime', 'book')),
    external_id      TEXT    NOT NULL,
    external_url     TEXT,
    raw_json         TEXT,
    is_primary       INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    adapter_version  INTEGER NOT NULL DEFAULT 0,
    chapter_count    INTEGER NOT NULL DEFAULT 0,
    available_count  INTEGER NOT NULL DEFAULT 0,
    locked_count     INTEGER NOT NULL DEFAULT 0,
    missing_count    INTEGER NOT NULL DEFAULT 0,
    stats_updated_at INTEGER,
    last_synced_at   INTEGER,
    last_error       TEXT
);

CREATE UNIQUE INDEX source_entries_external ON source_entries (source_id, kind, external_id);
CREATE UNIQUE INDEX source_entries_primary ON source_entries (title_id) WHERE is_primary = 1;
CREATE INDEX source_entries_title ON source_entries (title_id);

CREATE TABLE chapters (
    id                      INTEGER PRIMARY KEY,
    source_entry_id         INTEGER NOT NULL REFERENCES source_entries (id) ON DELETE CASCADE,
    external_id             TEXT    NOT NULL,
    chapter_key             TEXT    NOT NULL,
    number_text             TEXT,
    number_milli            INTEGER,
    ordinal_site            INTEGER NOT NULL DEFAULT 0,
    volume                  INTEGER,
    chapter_kind            TEXT    NOT NULL DEFAULT 'normal' CHECK (chapter_kind IN ('normal', 'special', 'extra', 'oneshot', 'preview')),
    content_kind            TEXT    CHECK (content_kind IS NULL OR content_kind IN ('images', 'video', 'text')),
    name                    TEXT,
    group_name              TEXT,
    language                TEXT,
    is_locked               INTEGER NOT NULL DEFAULT 0 CHECK (is_locked IN (0, 1)),
    availability            TEXT    NOT NULL DEFAULT 'unknown' CHECK (availability IN ('unknown', 'available', 'locked', 'missing')),
    availability_checked_at INTEGER,
    page_count              INTEGER,
    external_url            TEXT,
    embed_url               TEXT,
    published_at            INTEGER,
    last_seen_at            INTEGER NOT NULL DEFAULT 0,
    removed_at              INTEGER,
    raw_json                TEXT
);

CREATE UNIQUE INDEX chapters_external ON chapters (source_entry_id, external_id);
CREATE INDEX chapters_key ON chapters (source_entry_id, chapter_key);
CREATE INDEX chapters_order ON chapters (source_entry_id, volume, number_milli, ordinal_site);
CREATE INDEX chapters_availability ON chapters (source_entry_id, availability);

CREATE TABLE chapter_aliases (
    id            INTEGER PRIMARY KEY,
    title_id      INTEGER NOT NULL REFERENCES titles (id) ON DELETE CASCADE,
    canonical_key TEXT    NOT NULL,
    alias_key     TEXT    NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE UNIQUE INDEX chapter_aliases_alias ON chapter_aliases (title_id, alias_key);
CREATE INDEX chapter_aliases_canonical ON chapter_aliases (title_id, canonical_key);

CREATE TABLE source_priorities (
    id         INTEGER PRIMARY KEY,
    kind       TEXT    NOT NULL CHECK (kind IN ('manga', 'anime', 'book')),
    lang       TEXT    NOT NULL,
    source_id  INTEGER NOT NULL REFERENCES sources (id) ON DELETE CASCADE,
    rank       INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX source_priorities_source ON source_priorities (kind, lang, source_id);
CREATE UNIQUE INDEX source_priorities_rank ON source_priorities (kind, lang, rank);

CREATE TABLE pages (
    id         INTEGER PRIMARY KEY,
    chapter_id INTEGER NOT NULL REFERENCES chapters (id) ON DELETE CASCADE,
    idx        INTEGER NOT NULL,
    image_ref  TEXT    NOT NULL,
    image_url  TEXT,
    width      INTEGER,
    height     INTEGER
);

CREATE UNIQUE INDEX pages_chapter_idx ON pages (chapter_id, idx);

CREATE TABLE chapter_texts (
    chapter_id INTEGER PRIMARY KEY REFERENCES chapters (id) ON DELETE CASCADE,
    format     TEXT    NOT NULL CHECK (format IN ('html', 'markdown', 'plain')),
    body       TEXT    NOT NULL,
    byte_len   INTEGER NOT NULL,
    fetched_at INTEGER NOT NULL,
    expires_at INTEGER
);

CREATE TABLE library (
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title_id   INTEGER NOT NULL REFERENCES titles (id) ON DELETE CASCADE,
    status     TEXT    NOT NULL CHECK (status IN ('planned', 'in_progress', 'completed', 'on_hold', 'dropped')),
    score      INTEGER CHECK (score IS NULL OR (score BETWEEN 1 AND 10)),
    added_at   INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, title_id)
);

CREATE INDEX library_updated_at ON library (user_id, updated_at);

CREATE TABLE progress (
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title_id    INTEGER NOT NULL REFERENCES titles (id) ON DELETE CASCADE,
    chapter_key TEXT    NOT NULL,
    page        INTEGER,
    seconds     INTEGER,
    scroll      REAL,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (user_id, title_id)
);

CREATE INDEX progress_updated_at ON progress (user_id, updated_at);

CREATE TABLE chapter_reads (
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title_id    INTEGER NOT NULL REFERENCES titles (id) ON DELETE CASCADE,
    chapter_key TEXT    NOT NULL,
    read_at     INTEGER NOT NULL,
    PRIMARY KEY (user_id, title_id, chapter_key)
);
