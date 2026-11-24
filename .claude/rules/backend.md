---
paths:
  - "server/**"
  - "src-tauri/src/**"
---

# Бэкенд

Целевое устройство - `docs/planned-now.md`; пока серверного слоя нет, правила действуют для `src-tauri/src/`.

- **Слои.** HTTP-обработчик тонкий: разбор запроса, вызов сервиса, ответ. Логика - в сервисах и репозиториях; адаптеры источников не знают о БД.
- **Ошибки.** `Result<T, AppError>`, `AppError: IntoResponse`, `thiserror`. В production-коде нет `unwrap` / `expect` / `panic!`; причина `unsafe` - в WHY-комментарии.
- **БД.** sqlx + SQLite: WAL, `foreign_keys=ON`, `busy_timeout`, `create_if_missing(true)`. Миграции - `server/migrations/`, запуск при старте. Импорт - upsert по уникальным ключам, иначе refresh создаёт дубли.
- **Auth.** Пароли - Argon2id; сессии - opaque-токен, в БД SHA-256, cookie `HttpOnly`/`SameSite=Lax`/`Secure`; инвайт захватывается атомарно; мутации проверяют `Origin`; login/register под rate limit.
- **Источники.** URL не приходит от клиента: только `source_id` + `external_id`. Свой `Referer`/`User-Agent`, таймауты, отдельные лимиты на метаданные и картинки, ошибка одного источника не ломает выдачу.
- **Картинки.** Прокси с allowlist хостов сайта и CDN, проверка IP после DNS, только http/https, растровые форматы, `nosniff`; кэш пишется атомарно (temp + rename), эвикшен не трогает незаписанные файлы.
- **Секреты.** Только env/`.env` (в git не попадает). В логах нет паролей, токенов и чужих URL с параметрами.
- **Тесты.** Парсеры - чистые функции на фикстурах; API - через `tower::ServiceExt::oneshot` на временной SQLite; auth-тесты: без cookie -> 401, чужой Origin -> 403.
