-- Версия аватара отдельно от времени загрузки: время обнуляется при удалении,
-- поэтому требует монотонного счётчика для ETag и cache-buster. 0 — аватара
-- нет; каждое успешное обновление файла увеличивает версию и она не сбрасывается.

ALTER TABLE users ADD COLUMN avatar_version INTEGER NOT NULL DEFAULT 0;

-- Уже загруженные аватары получают ненулевую версию
UPDATE users SET avatar_version = avatar_updated_at WHERE avatar_updated_at IS NOT NULL;
