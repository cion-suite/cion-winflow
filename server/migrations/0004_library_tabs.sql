-- Библиотека: статус «не интересно», избранное и пользовательские закладки.
-- SQLite не умеет менять CHECK у существующей таблицы, поэтому library
-- пересобирается: новые строки получают favorite = 0.

CREATE TABLE library_new (
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title_id   INTEGER NOT NULL REFERENCES titles (id) ON DELETE CASCADE,
    status     TEXT    NOT NULL CHECK (status IN ('planned', 'in_progress', 'completed', 'on_hold', 'dropped', 'not_interested')),
    score      INTEGER CHECK (score IS NULL OR (score BETWEEN 1 AND 10)),
    favorite   INTEGER NOT NULL DEFAULT 0 CHECK (favorite IN (0, 1)),
    added_at   INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, title_id)
);

INSERT INTO library_new (user_id, title_id, status, score, favorite, added_at, updated_at)
SELECT user_id, title_id, status, score, 0, added_at, updated_at FROM library;

DROP TABLE library;
ALTER TABLE library_new RENAME TO library;
CREATE INDEX library_updated_at ON library (user_id, updated_at);

-- Пользовательские закладки: не больше трёх на пользователя, лимит
-- проверяет сервис. Позиция задаёт порядок вкладок в интерфейсе.
CREATE TABLE library_tabs (
    id         INTEGER PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       TEXT    NOT NULL,
    position   INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX library_tabs_user_name ON library_tabs (user_id, name);

-- Закладка содержит только тайтлы из библиотеки пользователя: составной FK
-- на library удаляет записи закладок вместе с записью библиотеки.
CREATE TABLE library_tab_items (
    user_id  INTEGER NOT NULL,
    tab_id   INTEGER NOT NULL REFERENCES library_tabs (id) ON DELETE CASCADE,
    title_id INTEGER NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (tab_id, title_id),
    FOREIGN KEY (user_id, title_id) REFERENCES library (user_id, title_id) ON DELETE CASCADE
);

CREATE INDEX library_tab_items_user_title ON library_tab_items (user_id, title_id);
