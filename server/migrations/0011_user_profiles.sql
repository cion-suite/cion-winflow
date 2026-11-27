-- Профиль пользователя: публичный uid, аватар и статистика чтения.
-- Файл аватара лежит по детерминированному пути DATA_DIR/avatars/{user_id}.webp,
-- поэтому в БД достаточно времени обновления: оно же признак наличия аватара
-- и cache-buster для ETag. NULL — аватара нет.

ALTER TABLE users ADD COLUMN avatar_updated_at INTEGER;
