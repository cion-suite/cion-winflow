-- Своя закладка становится ещё одним статусом: у тайтла ровно один
-- статус — встроенный или свой. `status` остаётся встроенным значением
-- (оно нужно как fallback при удалении закладки), `custom_tab_id` его
-- перекрывает.
--
-- Мультивыбор из library_tab_items убирается: при миграции у тайтла
-- остаётся первая по порядку вкладок.

ALTER TABLE library ADD COLUMN custom_tab_id INTEGER
    REFERENCES library_tabs (id) ON DELETE SET NULL;

UPDATE library
SET custom_tab_id = (
    SELECT i.tab_id
    FROM library_tab_items i
    JOIN library_tabs t ON t.id = i.tab_id
    WHERE i.user_id = library.user_id AND i.title_id = library.title_id
    ORDER BY t.position, t.id
    LIMIT 1
);

DROP TABLE library_tab_items;

CREATE INDEX library_custom_tab ON library (user_id, custom_tab_id);
