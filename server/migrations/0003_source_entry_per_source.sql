-- Один source_entry на пару (title_id, source_id).
-- titles.preferred_source_id хранит source_id, поэтому две записи одного
-- источника у одного тайтла делают выбор неоднозначным.

CREATE UNIQUE INDEX source_entries_title_source ON source_entries (title_id, source_id);
