-- Своя закладка может происходить из внешнего статуса источника
-- (импорт закладок Remanga). `source_ref` — стабильный идентификатор
-- внешнего статуса; имя закладки пользователь может изменить, поэтому
-- повторный импорт ищет закладку по нему, а не по названию.
ALTER TABLE library_tabs ADD COLUMN source_ref TEXT;

CREATE UNIQUE INDEX library_tabs_user_source_ref
    ON library_tabs (user_id, source_ref)
    WHERE source_ref IS NOT NULL;
