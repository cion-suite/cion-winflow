-- «Любимое» становится обычным статусом библиотеки, а не отдельным флагом.
-- SQLite не умеет менять CHECK у существующей таблицы, поэтому library
-- пересобирается. library_tab_items ссылается на library составным FK:
-- при DROP TABLE родителя сработал бы каскад, поэтому состав закладок
-- сохраняется в промежуточной таблице и возвращается после пересборки.

CREATE TABLE library_tab_items_backup AS
SELECT user_id, tab_id, title_id, added_at FROM library_tab_items;

DROP TABLE library_tab_items;

CREATE TABLE library_new (
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title_id   INTEGER NOT NULL REFERENCES titles (id) ON DELETE CASCADE,
    status     TEXT    NOT NULL CHECK (status IN ('favorite', 'planned', 'in_progress', 'completed', 'on_hold', 'dropped', 'not_interested')),
    score      INTEGER CHECK (score IS NULL OR (score BETWEEN 1 AND 10)),
    added_at   INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, title_id)
);

-- Избранное важнее прежнего статуса: иначе выставленный флаг потерялся бы
-- без следа. Остальные записи сохраняют свой статус.
INSERT INTO library_new (user_id, title_id, status, score, added_at, updated_at)
SELECT user_id, title_id,
       CASE WHEN favorite = 1 THEN 'favorite' ELSE status END,
       score, added_at, updated_at
FROM library;

DROP TABLE library;
ALTER TABLE library_new RENAME TO library;
CREATE INDEX library_updated_at ON library (user_id, updated_at);

CREATE TABLE library_tab_items (
    user_id  INTEGER NOT NULL,
    tab_id   INTEGER NOT NULL REFERENCES library_tabs (id) ON DELETE CASCADE,
    title_id INTEGER NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (tab_id, title_id),
    FOREIGN KEY (user_id, title_id) REFERENCES library (user_id, title_id) ON DELETE CASCADE
);

CREATE INDEX library_tab_items_user_title ON library_tab_items (user_id, title_id);

INSERT INTO library_tab_items (user_id, tab_id, title_id, added_at)
SELECT b.user_id, b.tab_id, b.title_id, b.added_at
FROM library_tab_items_backup b
JOIN library l ON l.user_id = b.user_id AND l.title_id = b.title_id;

DROP TABLE library_tab_items_backup;
