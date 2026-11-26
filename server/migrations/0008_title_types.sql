-- Тип произведения принадлежит тайтлу, а не источнику.
--
-- Источник — провайдер контента: он отдаёт метаданные и главы, но не
-- определяет тип. Тип берётся из метаданных источника (у Remanga — поле
-- `type`) или выбирается вручную. Список повторяет типы Remanga.
--
-- SQLite не умеет менять CHECK на месте, поэтому titles, source_entries и
-- source_priorities пересобираются. Дочерние таблицы ссылаются на них с
-- ON DELETE CASCADE: удаление родителя при включённых внешних ключах
-- удалило бы данные. SQLx для SQLite оборачивает миграцию в транзакцию,
-- а PRAGMA foreign_keys внутри транзакции — no-op. Поэтому `db.rs`
-- выключает внешние ключи на соединении мигратора до запуска и включает
-- после. Миграцию не запускай вручную без этого шага.
CREATE TABLE titles_new (
    id                          INTEGER PRIMARY KEY,
    kind                        TEXT    NOT NULL CHECK (kind IN (
        'manga', 'manhwa', 'manhua', 'western_comic',
        'ru_comic', 'indonesian_comic', 'other', 'original'
    )),
    name                        TEXT    NOT NULL,
    name_lower                  TEXT    NOT NULL,
    alt_names_json              TEXT    NOT NULL DEFAULT '[]',
    authors_json                TEXT    NOT NULL DEFAULT '[]',
    genres_json                 TEXT    NOT NULL DEFAULT '[]',
    description                 TEXT,
    status                      TEXT    CHECK (status IS NULL OR status IN ('ongoing', 'completed', 'hiatus', 'cancelled')),
    year                        INTEGER CHECK (year IS NULL OR (year BETWEEN 1800 AND 2200)),
    lang                        TEXT,
    rating                      REAL    CHECK (rating IS NULL OR (rating >= 0.0 AND rating <= 10.0)),
    origin_source_id            INTEGER REFERENCES sources (id) ON DELETE SET NULL,
    preferred_source_id         INTEGER REFERENCES sources (id) ON DELETE SET NULL,
    preferred_source_updated_at INTEGER,
    created_at                  INTEGER NOT NULL,
    updated_at                  INTEGER NOT NULL
);

-- Старые anime/book не имеют прямого аналога в типах Remanga: данные
-- сохраняются как «Другое», чтобы не потерять библиотеку и прогресс.
INSERT INTO titles_new (
    id, kind, name, name_lower, alt_names_json, authors_json, genres_json,
    description, status, year, lang, rating, origin_source_id,
    preferred_source_id, preferred_source_updated_at, created_at, updated_at
)
SELECT
    id,
    CASE kind
        WHEN 'manga' THEN 'manga'
        WHEN 'manhwa' THEN 'manhwa'
        WHEN 'manhua' THEN 'manhua'
        WHEN 'western_comic' THEN 'western_comic'
        WHEN 'ru_comic' THEN 'ru_comic'
        WHEN 'indonesian_comic' THEN 'indonesian_comic'
        WHEN 'original' THEN 'original'
        ELSE 'other'
    END,
    name, name_lower, alt_names_json, authors_json, genres_json,
    description, status, year, lang, rating, origin_source_id,
    preferred_source_id, preferred_source_updated_at, created_at, updated_at
FROM titles;

-- Старый kind знал только manga/anime/book. Уже импортированные из Remanga
-- тайтлы хранят исходный type в raw_json записи источника: возвращаем
-- фактический тип, иначе все манхвы/маньхуа остались бы «Мангой».
UPDATE titles_new
SET kind = COALESCE((
    SELECT CASE json_extract(e.raw_json, '$.type.name')
        WHEN 'Манга' THEN 'manga'
        WHEN 'Манхва' THEN 'manhwa'
        WHEN 'Маньхуа' THEN 'manhua'
        WHEN 'Западный комикс' THEN 'western_comic'
        WHEN 'Рукомикс' THEN 'ru_comic'
        WHEN 'Индонезийский комикс' THEN 'indonesian_comic'
        WHEN 'Другое' THEN 'other'
        WHEN 'Оригиналы' THEN 'original'
    END
    FROM source_entries e
    WHERE e.title_id = titles_new.id AND json_valid(e.raw_json)
    ORDER BY (e.is_primary = 1) DESC, e.id
    LIMIT 1
), kind);

DROP TABLE titles;
ALTER TABLE titles_new RENAME TO titles;
CREATE INDEX titles_name_lower ON titles (name_lower);
CREATE INDEX titles_kind ON titles (kind);

-- Один source_entry на (source_id, external_id): kind больше не участвует.
CREATE TABLE source_entries_new (
    id               INTEGER PRIMARY KEY,
    title_id         INTEGER NOT NULL REFERENCES titles (id) ON DELETE CASCADE,
    source_id        INTEGER NOT NULL REFERENCES sources (id),
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

INSERT INTO source_entries_new (
    id, title_id, source_id, external_id, external_url, raw_json, is_primary,
    adapter_version, chapter_count, available_count, locked_count,
    missing_count, stats_updated_at, last_synced_at, last_error
)
SELECT
    id, title_id, source_id, external_id, external_url, raw_json, is_primary,
    adapter_version, chapter_count, available_count, locked_count,
    missing_count, stats_updated_at, last_synced_at, last_error
FROM source_entries;

DROP TABLE source_entries;
ALTER TABLE source_entries_new RENAME TO source_entries;
CREATE UNIQUE INDEX source_entries_external ON source_entries (source_id, external_id);
CREATE UNIQUE INDEX source_entries_primary ON source_entries (title_id) WHERE is_primary = 1;
CREATE INDEX source_entries_title ON source_entries (title_id);
CREATE UNIQUE INDEX source_entries_title_source ON source_entries (title_id, source_id);

-- Приоритеты задаются на язык: источник больше не привязан к типу.
-- Строки разных прежних kind для одного языка сливаются, rank
-- перенумеровывается без дыр.
CREATE TABLE source_priorities_new (
    id         INTEGER PRIMARY KEY,
    lang       TEXT    NOT NULL,
    source_id  INTEGER NOT NULL REFERENCES sources (id) ON DELETE CASCADE,
    rank       INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

INSERT INTO source_priorities_new (lang, source_id, rank, created_at)
SELECT
    lang,
    source_id,
    ROW_NUMBER() OVER (PARTITION BY lang ORDER BY MIN(rank), source_id) - 1,
    MIN(created_at)
FROM source_priorities
GROUP BY lang, source_id;

DROP TABLE source_priorities;
ALTER TABLE source_priorities_new RENAME TO source_priorities;
CREATE UNIQUE INDEX source_priorities_source ON source_priorities (lang, source_id);
CREATE UNIQUE INDEX source_priorities_rank ON source_priorities (lang, rank);
