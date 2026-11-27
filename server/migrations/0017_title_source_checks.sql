-- Per-user кэш поиска тайтла на источниках, которых ещё нет в привязке.
-- Открытие страницы тайтла не должно повторять один и тот же поиск на каждом
-- рендере: негативный результат живёт сутки, auth/error — час.
CREATE TABLE title_source_checks (
    title_id INTEGER NOT NULL REFERENCES titles(id) ON DELETE CASCADE,
    source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    checked_at INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('found', 'not_found', 'auth_required', 'error')),
    error TEXT,
    PRIMARY KEY (title_id, source_id, user_id)
);

-- После сохранения/удаления учётки строки пользователя для источника
-- удаляются: поиск с новым аккаунтом должен повториться.
CREATE INDEX idx_title_source_checks_user_source
    ON title_source_checks (user_id, source_id);
