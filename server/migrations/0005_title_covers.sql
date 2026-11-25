-- Обложки тайтла по размерам (low/mid/high). Одна строка на размер:
-- low показывается в поиске и диалоге добавления, mid — в библиотеке,
-- high — на странице тайтла. Файлы лежат в DATA_DIR/covers/{id}/.

CREATE TABLE title_covers (
    title_id   INTEGER NOT NULL REFERENCES titles (id) ON DELETE CASCADE,
    size       TEXT    NOT NULL CHECK (size IN ('low', 'mid', 'high')),
    path       TEXT    NOT NULL,
    width      INTEGER,
    height     INTEGER,
    source_id  INTEGER REFERENCES sources (id) ON DELETE SET NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (title_id, size)
);

CREATE INDEX title_covers_path ON title_covers (path);

-- Старая схема хранила один каталог на тайтл с файлами thumb.webp/full.webp.
-- thumb был размером библиотеки, поэтому он же становится low.
INSERT INTO title_covers (title_id, size, path, updated_at)
SELECT id, 'low', cover_path || '/thumb.webp', updated_at
FROM titles WHERE cover_path IS NOT NULL;

INSERT INTO title_covers (title_id, size, path, updated_at)
SELECT id, 'mid', cover_path || '/thumb.webp', updated_at
FROM titles WHERE cover_path IS NOT NULL;

INSERT INTO title_covers (title_id, size, path, updated_at)
SELECT id, 'high', cover_path || '/full.webp', updated_at
FROM titles WHERE cover_path IS NOT NULL;

ALTER TABLE titles DROP COLUMN cover_path;
