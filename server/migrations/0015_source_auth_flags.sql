-- Учётка источника нужна, чтобы получить главу/контент.
-- Флаги глобальные: это свойство главы на источнике, а не конкретного
-- пользователя. Персональная учётка лишь решает, можно ли её прочитать.
ALTER TABLE source_entries ADD COLUMN auth_required INTEGER NOT NULL DEFAULT 0;
ALTER TABLE chapters ADD COLUMN auth_required INTEGER NOT NULL DEFAULT 0;
