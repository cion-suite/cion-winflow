-- Метаданные тайтла общие для всех источников. Храним последний снимок
-- источника, чтобы ручные правки полей не затирались автосинхронизацией:
-- если текущее значение отличается от снимка, считается, что его правил
-- пользователь, и обновление оставляет его как есть.
ALTER TABLE titles ADD COLUMN source_metadata_json TEXT;
ALTER TABLE titles ADD COLUMN metadata_source_id INTEGER REFERENCES sources (id) ON DELETE SET NULL;
ALTER TABLE titles ADD COLUMN metadata_synced_at INTEGER;
