Completion steps (после каждой фазы)

1. pnpm typecheck
2. pnpm lint
3. pnpm test (Rust-тесты)
4. pnpm check:rust (fmt + clippy)
5. /isgood - adversarial review + fix-loop.
6. /notes - лог версии и commit-сообщение (uncommitted mode).
7. Отметить фазу [done] в docs/planned-now.md.
8. STOP & WAIT - подтверждение перед следующей фазой.

---

# Импорт из Remanga: авторизация, прогресс и источники

- **Статус:** план на согласование
- **Дата:** 2026-11-27
- **Заменяет:** прежний `docs/planned-now.md` (877 строк, коммит `6c6400c`); старый план остаётся в git
- **Решения:** 0017 — per-user credentials источников; 0018 — импортированный прогресс; 0019 — поиск источников при открытии (создаются в соответствующих фазах)
- **Причина:** три класса проблем после первого импорта закладок: Remanga требует вход для 18+ тайтлов, импорт теряет прогресс больше доступных глав, источники привязываются только при добавлении через поиск.

## Согласованные решения

1. **Учётки источников — per-user.** Каждый пользователь подключает свой аккаунт Remanga. Токены/куки хранятся в БД **зашифрованными**; мастер-ключ — только в env. Плейнтекст не пишется в БД и не логируется.
2. **Тип credential — Bearer и Cookie.** Оба поля необязательны; отправляется то, что заполнено. Сервер различает «учётка не подключена» и «учётка отклонена».
3. **Вход по кнопке — через расширение-компаньон.** Приложение не может прочитать HttpOnly-cookie чужого origin; расширение после ручного входа на remanga.org забирает cookie и `Authorization` и передаёт их по одноразовому коду. До появления расширения (фаза 5) в настройках остаётся ручная вставка как fallback.
4. **Область действия — текущий пользователь.** Поиск, импорт, sync, главы и картинки идут с учёткой того, кто выполняет запрос. Картинки, полученные с пользовательской учёткой, кэшируются в отдельном пространстве пользователя.
5. **Прогресс — high-water + автопометка.** `library.imported_read_count` хранит импортированную отметку; эффективный `read_count = max(фактические chapter_reads, imported_read_count)`. Карточка показывает `930/17`, где 17 — доступные главы. Новые главы автоматически отмечаются до high-water, но вручную снятые старые отметки не восстанавливаются.
6. **Источники — при открытии тайтла.** Сервер ищет тайтл на включённых источниках того же языка, которых ещё нет в привязке, с per-user TTL и блокировкой на тайтл. Если все источники привязаны — только обновление глав.

## Что уже есть (карта переиспользования)

- Ошибки API и экстракторы: `server/src/error.rs`, `server/src/extract.rs` (`ApiJson`, `ApiQuery`, `ApiPath`).
- Сессия и `CurrentUser`: `server/src/auth/middleware.rs`; публичные пути перечислены там же.
- Источники: `SourceRuntime` и адаптеры — `server/src/sources/mod.rs`; HTTP-клиент и разбор статусов — `server/src/sources/http.rs`; Remanga — `server/src/sources/remanga/api.rs`; реестр — `server/src/sources/registry.rs`.
- Импорт тайтла: `source_admin::import` — `server/src/source_admin/service.rs:322-512`; единственный писатель `source_entries`.
- Автопривязка: `auto_link_sources` — `server/src/discover/service.rs:281-480`; вызывается только из `discover::add`.
- Синхронизация страницы тайтла: `sync_title` — `server/src/source_admin/service.rs:915-1002`; TTL 10 минут в `titles.metadata_synced_at`.
- Библиотека и прогресс: SQL `read_count`/`chapter_count` — `server/src/library/service.rs:246-267`; `mark_progress` — `server/src/bookmarks/service.rs:661-755`; канонические ключи — `server/src/progress.rs`.
- Карточка библиотеки: `src/ui/LibraryCard.tsx`; используется библиотекой и профилем.
- Импорт/экспорт: `server/src/bookmarks/`; DTO ошибок — `ImportErrorDto`; UI — `src/pages/settings/BookmarksImportDialog.tsx`, `src/api/profile.ts`.
- Настройки: вкладки — `src/pages/settings/index.tsx`; API-области — `src/api/index.ts`; пример области — `src/api/admin.ts`.
- Изображения: `server/src/images/`; кэш и single-flight — `server/src/images/cache.rs`.
- i18n: `src/lib/i18n/locales/en.ts`, `ru.ts`; ошибки — `src/lib/apiErrors.ts`.

---

## Phase 1: Зашифрованные учётные данные источников на пользователя [done]

- **What:** Появляется таблица per-user учёток источников, шифрование AEAD с ключом из env, API `GET/PUT/DELETE /api/me/source-credentials` и вкладка «Источники» в `/settings`. На этом этапе учётка вводится вручную (Bearer/Cookie) как временный fallback; адаптеры её ещё не используют.
- **Where:**
  - `server/migrations/0014_user_source_credentials.sql` (create)
  - `server/src/source_auth/mod.rs` (create)
  - `server/src/source_auth/crypto.rs` (create) — AEAD, nonce, AAD, zeroize
  - `server/src/source_auth/service.rs` (create) — upsert/list/delete, расшифровка для будущих фаз
  - `server/src/source_auth/handlers.rs` (create)
  - `server/src/sources/descriptor.rs` (modify) — `Capability::Auth`
  - `server/src/config.rs` (modify) — `SOURCE_CREDENTIALS_KEY`
  - `server/Cargo.toml` (modify) — `chacha20poly1305`, `zeroize`, `base64`, `secrecy`
  - `.env.example` (modify) — `SOURCE_CREDENTIALS_KEY`
  - `server/src/state.rs` (modify) — `CredentialCipher`
  - `server/src/app.rs` (modify) — маршруты `/api/me/source-credentials*`
  - `server/src/error.rs` (modify) — `credentials_unavailable`, `source_credentials_invalid`
  - `src/api/sourceCredentials.ts` (create), `src/api/index.ts` (modify)
  - `src/pages/settings/SourceCredentialsSettings.tsx` (create), `src/pages/settings/index.tsx` (modify)
  - `src/lib/i18n/locales/en.ts`, `src/lib/i18n/locales/ru.ts` (modify)
  - `server/tests/source_credentials.rs` (create)
  - `docs/decisions/0017-per-user-source-credentials.md` (create)
  - `docs/architecture.md`, `CLAUDE.md` (modify)
- **How:**
  - Миграция:
    ```sql
    CREATE TABLE user_source_credentials (
      user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
      ciphertext BLOB NOT NULL,
      nonce BLOB NOT NULL,
      key_version INTEGER NOT NULL DEFAULT 1,
      updated_at INTEGER NOT NULL,
      last_verified_at INTEGER,
      last_error TEXT,
      PRIMARY KEY (user_id, source_id)
    );
    ```
  - Шифрование: JSON `{"bearer":"...","cookie":"..."}` → ChaCha20-Poly1305; случайный 12-байтный nonce; AAD `user_id:source_id:key_version`; plaintext в `Zeroizing`/`secrecy`, не логируется. Ключ `SOURCE_CREDENTIALS_KEY` — 32 байта base64 (`openssl rand -base64 32`), пример — в `.env.example`. Если ключ не задан, приложение работает без per-user учёток, а маршруты отвечают 503 `credentials_unavailable`; если в БД уже есть учётки, старт с неверным ключом логирует ошибку и не расшифровывает их.
  - API: `GET /api/me/source-credentials` → только `configured`, `updated_at`, `last_verified_at`, `last_error`, `code`, `name`; секретов нет. `PUT /api/me/source-credentials/{code}` `{ bearer?, cookie? }` — заменяет payload; `null`/пустая строка очищает поле; `DELETE` удаляет. Валидация: источник существует, разрешён для per-user auth; размер полей ограничен; маршруты под rate limit и обычной проверкой Origin.
  - Сохранение/удаление учётки удаляет `title_source_checks` этого пользователя для источника и сбрасывает `titles.metadata_synced_at` у тайтлов, где источник привязан: иначе sync вернёт `refreshed=false` до 10 минут, и подключивший аккаунт пользователь не увидит изменений. Фронтенд после мутации инвалидирует учётки, `title`, `library` и `chapter-content`.
  - UI: третья вкладка «Источники»; карточка Remanga со статусом, полями Bearer/Cookie (`type="password"`), «Сохранить», «Очистить», «Проверить» (проверка появится в фазе 2). После сохранения инвалидировать `me`-учётки; секрет после сохранения не показывать.
  - Добавить `Capability::Auth` (serde `auth`) и объявить его у Remanga; неиспользуемый `AuthCookie` не трогать. UI показывает только источники с этим признаком.
  - Документировать, что ручная вставка — временная: целевой вход по кнопке — фаза 5.
- **Reuse:** `AppError`/`IntoResponse` — `server/src/error.rs`; `ApiJson` — `server/src/extract.rs`; `CurrentUser` — `server/src/auth/middleware.rs`; миграционный стиль `ALTER ADD`/новых таблиц — `0007`/`0013`; паттерн маскирования секретов — `sources/config.rs:296-311`; вкладки настроек — `src/pages/settings/index.tsx`; область API — `src/api/admin.ts`; i18n и `useT`.
- **Depends on:** —
- **Готово, когда:** секрет пишется только шифротекстом; API никогда не возвращает plaintext; `cargo test` покрывает round-trip, неверный ключ, чужого пользователя и очистку; `pnpm lint`/`typecheck` зелёные.

## Phase 2: Remanga — авторизация в запросах и честные ошибки [done]

- **What:** Все запросы, которые могут требовать вход, получают учётку действующего пользователя: Remanga отправляет `Authorization: Bearer` и/или `Cookie` в `search`, `title`, `chapters`, `content`. Заголовки уходят только на хосты самого источника, но не на сторонние CDN картинок. Импорт и выдача различают «учётка не подключена» и «учётка отклонена»; ошибка авторизации одного пользователя не портит глобальную доступность главы.
- **Where:**
  - `server/migrations/0015_source_auth_flags.sql` (create) — `source_entries.auth_required`, `chapters.auth_required`
  - `server/src/sources/mod.rs` (modify) — параметр `auth: Option<&SourceAuth>` у `search/title/chapters/content/text`
  - `server/src/sources/http.rs` (modify) — сборка заголовков, `get_text`/`fetch_bytes` с auth
  - `server/src/sources/remanga/api.rs` (modify) — все четыре запроса; auth только для хостов источника
  - `server/src/sources/remanga/config.rs` (modify) — источник auth-хостов (`base_url`, `site_url`), не image/CDN
  - `server/src/sources/rumix/api.rs`, `server/src/sources/mangamir/api.rs`, `server/src/sources/fixture.rs` (modify) — сигнатуры, auth игнорируется
  - `server/src/source_auth/service.rs` (modify) — `for_user(user_id, source_id)`
  - `server/src/source_admin/service.rs` (modify) — import/refresh/sync/probe принимают auth; per-user 401 не пишется в `source_entries.last_error`
  - `server/src/discover/service.rs` (modify) — поиск, add и auto-link используют auth зрителя
  - `server/src/catalog/content.rs`, `server/src/catalog/cover.rs`, `server/src/catalog/selection.rs`, `server/src/catalog/chapters.rs` (modify) — контент/обложки с auth; `auth_required` в `ChapterCandidate` и `TitleSourceDto`; `user_has_credentials` в `TitleSourceDto`
  - `src/api/catalog.ts` (modify) — `TitleSource.auth_required`/`user_has_credentials`, `TitleSyncEntry.code`
  - `server/src/images/handlers.rs`, `server/src/images/cache.rs` (modify) — auth-scope кэша и запрет глобального `locked` из-за отсутствия пользовательской учётки
  - `server/src/bookmarks/service.rs`, `server/src/bookmarks/handlers.rs` (modify) — учётка инициатора на всё задание; `code` в `ImportErrorDto`
  - `server/src/error.rs` (modify) — коды `source_auth_not_configured`, `source_auth_invalid`, `source_auth_maybe_required`, `source_blocked`
  - `src/api/profile.ts`, `src/api/discover.ts` (modify) — `code` в ошибках импорта и поиска
  - `src/lib/apiErrors.ts`, `src/pages/settings/BookmarksImportDialog.tsx`, `src/pages/library/AddTitleDialog.tsx`, `src/pages/title/index.tsx` (modify) — подсказка и ссылка на настройки
  - `src/lib/i18n/locales/en.ts`, `ru.ts` (modify)
  - `server/tests/source_auth.rs`, `server/tests/content_auth.rs` (create), `server/tests/bookmarks.rs`, `server/tests/sources.rs`, `server/tests/discover.rs` (modify)
  - `docs/decisions/0017-per-user-source-credentials.md` (modify)
  - `docs/architecture.md`, `CLAUDE.md` (modify)
- **How:**
  - `SourceAuth { bearer: Option<SecretString>, cookie: Option<SecretString> }`; заголовки добавляются в каждом запросе; `Debug` не печатает значения, ошибки не содержат URL и секретов.
  - В сервисах учётка резолвится по `(user_id, source_id)`; для фонового импорта — один раз на задание (или на элемент), дальше передаётся по ссылке. Успешный авторизованный запрос обновляет `last_verified_at`; 401 — `last_error='auth_invalid'` (без секретов), чтобы настройки показали «переподключить». Адаптеры по-прежнему не знают БД.
  - Маппинг: 401 без учётки → `source_auth_not_configured`; 401 с учёткой → `source_auth_invalid`; 403 — `source_blocked` (Cloudflare/регион), не auth. `apiErrors.ts` и локали переводят все три auth-кода и `source_blocked` в человеческий текст, а не показывают сырой `message` сервера. Для Remanga 404 без учётки неоднозначен (18+ тайтл скрыт или отсутствует) → `source_auth_maybe_required` с подсказкой «если тайтл 18+, подключите Remanga»; с учёткой 404 остаётся `source_not_found`. Fallback по имени не глотает 401/403 и не превращает такой 404 в окончательный `not_found`. `ImportErrorDto` и `DiscoverSourceError` получают `code`, `SyncEntryDto` тоже; UI группирует ошибки и показывает ссылку на `/settings`. Если учётка сохранена, но не расшифровывается (нет/неверный ключ), `for_user` возвращает `credentials_unavailable` 503, а не «учётки нет».
  - HTTP-коды: `source_auth_not_configured`/`source_auth_maybe_required` → 403, `source_auth_invalid` → 502; **никогда 401**, иначе фронтенд примет это за истёкшую сессию и разлогинит. `source_auth_required` остаётся внутренним кодом адаптера/probe.
  - Хосты: `Authorization`/`Cookie` добавляются только к origin `base_url` и `site_url` источника. На CDN картинок (`*.reimg.org`, `*.reimg2.org` и т. п.) сессионные заголовки не уходят; если CDN требует auth, используются подписанные URL, а не cookie.
  - Контент: добавить `chapters.auth_required` и `source_entries.auth_required` (0015). При первом открытии главы сначала пробуем без auth; на 401/404 (403 — blocked, 410 — refresh) и наличии учётки повторяем с ней. Успех без auth → `auth_required=0`; успех с учёткой → `chapters.auth_required=1` и обновление `last_verified_at`; повтор с учёткой снова 401 → `source_auth_invalid`; повтор с учёткой 404 → `missing`/`source_not_found`. Без учётки → `source_auth_not_configured`/`source_auth_maybe_required`, глобально главу не помечаем. Если глава помечена, `canonical`/`direct` не отдают `cached_content` пользователю без собственной **проверенной** учётки (наличие строки + `last_verified_at` не старше 24 ч; иначе сначала авторизованный запрос) — возвращают `source_auth_not_configured`; при auth-ошибке работает fallback на другой вариант/источник. Картинки: для помеченной главы без учётки — ошибка; с учёткой — загрузка и кэш `user:{id}`; публичные — общий кэш. `pages`/`chapters` остаются глобальными, но выдача страниц гейтится флагом.
  - Availability: 401/403/410 при картинках **не** ставят глобальный `locked`; 403 также не ставит `auth_required`; `locked` — только явный `SourceError::Locked` (платная глава) или подтверждённая недоступность без auth. Cloudflare/регион 403 не скрывает главу у всех.
  - Sync: per-user auth-ошибки возвращаются в ответе, но не перезаписывают глобальный `source_entries.last_error`; 401 без учётки ставит `source_entries.auth_required=1`, успешный запрос без auth сбрасывает его в 0; `titles.metadata_synced_at` не сдвигается, если были auth-ошибки, чтобы пользователь с учёткой повторил sync сразу. Сохранение/удаление учётки сбрасывает TTL (Phase 1).
  - Карточка «Источники»: `TitleSourceDto` получает `auth_required` и `user_has_credentials` (для текущего пользователя), чтобы UI показал «подключить аккаунт» без сетевого запроса.
  - Проверка учётки: `POST /api/me/source-credentials/{code}/test` делает лёгкий авторизованный запрос; если стабильного endpoint нет, проверка выполняется первым реальным обращением, а в UI показывается «сохранено, не проверено».
- **Reuse:** `SourceError`/`get_text` — `sources/http.rs:289-368`; точки запросов Remanga — `sources/remanga/api.rs:76,114,142,197`; `source_admin::import`; `ImportErrorDto`; `useErrorText`/`apiErrors.ts`; кэш и single-flight — `images/cache.rs`; локальный HTTP-сервер в тестах — `server/tests/mangamir_livewire.rs`.
- **Depends on:** Phase 1
- **Готово, когда:** тест с локальным сервером подтверждает заголовки на всех четырёх запросах и их отсутствие на CDN-хосте; 401 без учётки и с учёткой дают разные коды; 403 остаётся `source_blocked` и не ставит `auth_required`; 404 без учётки даёт подсказку `source_auth_maybe_required`, с учёткой — `source_not_found`; импорт 18+ тайтла с учёткой проходит, без учётки — понятная ошибка; кэшированные страницы и картинка, полученные с учёткой A, не отдаются B без его учётки; 403 картинки не ставит глобальный `locked`; ни один auth-ответ источника не приходит как HTTP 401; sync B не пишет глобальный `last_error` и не сдвигает TTL.

## Phase 3: Импортированный прогресс — 930/17 и автопометка

- **What:** Импорт сохраняет high-water прогресс; библиотека показывает `930/17`, где числитель — эффективный прочитанный объём, знаменатель — доступные главы; новые главы автоматически отмечаются до high-water; экспорт отдаёт прогресс в пределах `chapter_count`.
- **Where:**
  - `server/migrations/0016_library_imported_progress.sql` (create)
  - `server/src/bookmarks/service.rs` (modify) — запись high-water, порог статуса
  - `server/src/library/service.rs` (modify) — эффективный `read_count`, `available_count`, DTO
  - `server/src/progress.rs` или новый `server/src/library/progress.rs` (modify/create) — общий helper отметки доступных глав
  - `server/src/source_admin/service.rs` (modify) — автопометка только новых глав после upsert
  - `server/src/bookmarks/export.rs` (modify)
  - `src/api/library.ts`, `src/api/profile.ts`, `src/ui/LibraryCard.tsx` (modify)
  - `src/pages/settings/ImportExportSettings.tsx` (modify) — подсказка о повторном импорте для старых тайтлов
  - `src/lib/i18n/locales/en.ts`, `ru.ts` (modify)
  - `server/tests/imported_progress.rs` (create), `server/tests/bookmarks.rs`, `server/tests/library.rs`, `server/tests/profile.rs` (modify)
  - `docs/decisions/0018-imported-read-progress.md` (create)
  - `docs/architecture.md`, `CLAUDE.md` (modify)
- **How:**
  - `ALTER TABLE library ADD COLUMN imported_read_count INTEGER;` — монотонная отметка, не уменьшается при ручном снятии.
  - Бэкфилл невозможен: числа 930 в БД нет. После миграции high-water появится только после повторного импорта закладок; в настройках импорта показать подсказку «переимпортируйте закладки, чтобы восстановить прогресс у старых тайтлов». Это документированное ограничение, не баг.
  - Импорт: в `INSERT/UPDATE library` писать `MAX(COALESCE(imported_read_count,0), item.progress)`. Перезапись статуса сравнивать с эффективным числом `max(COUNT(chapter_reads), imported_read_count)`, а не только с `chapter_reads`.
  - SQL библиотеки: `read_count = MAX((SELECT COUNT(*) FROM chapter_reads ...), COALESCE(l.imported_read_count,0))`; `available_count` — `available_count` записи `titles.preferred_source_id` (источник, из которого реально читается; для `930/17` это Remanga), fallback на `MAX(e.available_count)`, если preferred нет или у него 0. Не MAX по всем источникам: после Phase 4 Mangamir может дать другой знаменатель. Не считаем union и alias коррелированным подзапросом на каждой карточке: это горячий путь библиотеки и профиля. Авторитетный объединённый список остаётся на странице тайтла; при появлении разноязычных источников знаменатель уточняется отдельной задачей. Тесты: Remanga preferred 930 total / 17 available + Mangamir 100 available → `930/17`; preferred без глав → `MAX`.
  - `LibraryItemDto` получает `imported_read_count` и `available_count`; `read_count` — эффективный. Карточка: `hasProgress = available_count > 0 || read_count > 0`; текст `read_count/available_count` без `Math.min`; при `available_count = 0` показывать только числитель.
  - Автопометка: после upsert сравнить набор доступных канонических ключей до/после (новая строка или `locked`/`unknown` → `available`); отмечать только появившиеся ключи до `min(imported_read_count, available_count)`. Вручную снятые неизменившиеся главы не восстанавливать. Указатель `progress` не двигать.
  - Экспорт: `progress = min(effective_read_count, chapter_count)`, `progress_total = chapter_count` — не отдавать `930/17` в Remanga-формат; high-water остаётся локальным. Тест фиксирует clamp.
  - Публичный профиль получает те же поля; `stats.read_chapters` остаётся фактическим числом `chapter_reads` (не high-water).
- **Reuse:** `mark_progress` — `bookmarks/service.rs:661-755`; SQL и DTO — `library/service.rs:78-91,246-267`; `LibraryCard`; `bookmarks/export.rs:56-92`; канонические ключи — `progress.rs`; `recompute_stats` — `catalog/selection.rs:234-277`; миграционный стиль `ALTER ADD` — `0007:9`.
- **Depends on:** —
- **Готово, когда:** импорт 930 при 17 доступных даёт `read_count=930`, `available_count=17`, экспорт клампится к `progress=17`; второй источник с большим `available_count` не меняет знаменатель, пока preferred — Remanga; новая 18-я глава или `locked`→`available` после sync становится прочитанной; вручную снятая неизменившаяся старая глава не восстанавливается; второй пользователь не затронут; в настройках есть подсказка о повторном импорте для старых тайтлов; тесты библиотеки/профиля/импорта зелёные.

## Phase 4: Автопривязка недостающих источников при открытии

- **What:** При открытии страницы тайтла сервер от имени текущего пользователя ищет тайтл на включённых источниках того же языка, которых ещё нет в привязке, и привязывает точные совпадения по основному/альтернативным названиям. Негативные результаты кэшируются per-user; при всех привязанных источниках выполняется только обновление глав.
- **Where:**
  - `server/migrations/0017_title_source_checks.sql` (create)
  - `server/src/source_discovery/mod.rs`, `server/src/source_discovery/service.rs` (create)
  - `server/src/discover/service.rs` (modify) — вынести переиспользуемую логику, сохранить поведение `add`
  - `server/src/source_admin/service.rs` (modify) — `sync_title` запускает discovery до TTL-раннего выхода; новые поля ответа; не писать per-user auth-ошибки в глобальный `last_error`
  - `server/src/state.rs` (modify) — блокировка на тайтл
  - `src/api/catalog.ts` (modify) — `sources_linked`, `discovery`, инвалидация в `useSyncTitleMutation.onSuccess`
  - `src/pages/title/index.tsx` (modify) — только подсказки и карточка источников; query keys страница не знает
  - `src/lib/apiErrors.ts`, `src/lib/i18n/locales/en.ts`, `ru.ts` (modify)
  - `server/tests/discover_on_open.rs` (create), `server/tests/discover.rs`, `server/tests/multi_source.rs` (modify)
  - `docs/decisions/0019-source-discovery-on-open.md` (create); `docs/decisions/0011-auto-link-sources.md` (modify — отметить поправку)
  - `docs/architecture.md`, `CLAUDE.md` (modify)
- **How:**
  - Таблица:
    ```sql
    CREATE TABLE title_source_checks (
      title_id INTEGER NOT NULL REFERENCES titles(id) ON DELETE CASCADE,
      source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
      user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      checked_at INTEGER NOT NULL,
      status TEXT NOT NULL CHECK (status IN ('found','not_found','auth_required','error')),
      error TEXT,
      PRIMARY KEY (title_id, source_id, user_id)
    );
    ```
  - Вынести из `auto_link_sources` выбор кандидатов (enabled, `Search`+`Chapters`, не привязан, тот же `titles.lang`, порядок по `source_priorities`) и сопоставление (точное нормализованное основное название, затем до 5 альтернативных, только первая страница поиска). `discover::add` продолжает вызывать ту же логику.
  - `discover_for_title(title_id, user_id)`: общий per-title lock с `auto_link_sources` (иначе `discover::add` и открытие тайтла гоняются за один `(title, source)`); для каждого кандидата проверить `title_source_checks` (TTL: `not_found` 24 ч, `auth_required`/`error` 1 ч); поиск с auth пользователя; 404 без учётки Remanga записывать как `auth_required` (1 ч), не как `not_found`; при совпадении — `source_admin::import` с явным `title_id`; ошибки изолированы; бюджет — не более 3 источников и 6 поисковых запросов на источник плюс общий таймаут.
  - `discover::add` вызывает ту же логику под тем же lock; если lock занят, auto-link пропускается (title-open discovery догонит позже), а не создаёт второй импорт.
  - «Фоном» означает: страница уже отрисована, `sync` вызывается после монтирования; запрос может занять секунды, поэтому discovery ограничен бюджетом и таймаутом. Отдельный polling-статус не вводится.
  - `sync_title`: discovery проверяется независимо от 10-минутного TTL глав; если найден и привязан источник — не делать ранний выход и вернуть `refreshed=true`; если все источники привязаны — discovery пропускается, работает обычное обновление глав. Ответ: `{ refreshed, entries, metadata_updated, sources_linked, discovery: { checked, skipped, errors } }`.
  - При сохранении/удалении учётки пользователя для источника удалять его строки `title_source_checks`, чтобы после подключения аккаунта поиск повторился.
  - Фронтенд: инвалидация `title`/`library`/`chapter-content`/`progress` — в `useSyncTitleMutation.onSuccess` при `refreshed` **или** непустом `sources_linked`; страница только показывает подсказки. Auth-ошибки discovery — подсказкой со ссылкой на настройки; карточка источников показывает «подключить аккаунт», если `auth_required` и у пользователя нет учётки.
  - Не делать массовый обход всех импортированных тайтлов после импорта: это тысячи поисков. Discovery ленивый, при открытии; пакетный/admin-вариант — отдельная будущая задача.
- **Reuse:** `auto_link_sources` — `discover/service.rs:281-480`; `source_admin::import`; `source_priorities`; `sync_title` и его TTL — `source_admin/service.rs:915-1002`; `useSyncTitleMutation` — `src/api/catalog.ts:240-254`; `find_title_by_names` — `source_admin/service.rs:571-607`; таблица checks — новое (в `source_entries` нельзя: строка означает привязку).
- **Depends on:** Phase 2 (учётка пользователя для поиска); Phase 1.
- **Готово, когда:** тайтл с одной Remanga при открытии находит Mangamir и привязывает его; повторное открытие в пределах TTL не делает поиск; при всех привязанных источниках поиска нет; auth-ошибка пользователя A не блокирует поиск для B; 404 без учётки не кэшируется как `not_found` на 24 ч; параллельные `discover::add` и открытие не создают дубль; `refreshed=false` с новым `sources_linked` всё равно инвалидирует UI через хук; тесты multi-source зелёные.

## Phase 5: Расширение-компаньон — вход по кнопке и автоматический handoff

- **What:** Появляется MV3-расширение: пользователь нажимает «Подключить Remanga», открывается remanga.org, он логинится сам (капча — на стороне сайта), расширение забирает Cookie и `Authorization` и передаёт их в cion-reader по одноразовому коду. Ручная вставка остаётся fallback.
- **Where:**
  - `extension/manifest.json`, `extension/src/background.ts`, `extension/src/content.ts`, `extension/src/options.ts` (create)
  - `extension/package.json`, `extension/tsconfig.json`, `extension/README.md`, `extension/icons/*` (create)
  - `package.json` (modify) — скрипты `ext:build`, `ext:dev`; сборка через esbuild/Vite без влияния на `pnpm build`
  - `server/src/source_auth/pairing.rs`, `handlers.rs`, `app.rs`, `auth/middleware.rs` (modify/create) — pairing и handoff
  - `src/api/sourceCredentials.ts`, `src/pages/settings/SourceCredentialsSettings.tsx` (modify) — кнопка, диалог, polling
  - `src/lib/i18n/locales/en.ts`, `ru.ts` (modify)
  - `server/tests/source_credentials.rs` (modify) — pairing
  - `docs/decisions/0017-per-user-source-credentials.md` (modify)
  - `docs/architecture.md`, `CLAUDE.md`, `extension/README.md` (modify/create)
- **How:**
  - Первый запуск: пользователь открывает options расширения (правая кнопка по иконке → «Параметры»), вводит URL инстанса и жмёт «Разрешить доступ». `chrome.permissions.request` вызывается из этого жеста; без разрешения content script на странице приложения не внедряется, поэтому приложение не может само передать origin. Диалог «Подключить Remanga» показывает инструкцию и кнопку «Проверить снова»; после разрешения и перезагрузки страницы поток работает автоматически.
  - Manifest MV3: permissions `cookies`, `storage`, `webRequest`; host_permissions `*://*.remanga.org/*`, `*://api.remanga.org/*`; `optional_host_permissions` для origin инстанса. `chrome.webRequest` используется только для наблюдения за `Authorization` на `api.remanga.org`, без модификации запросов.
  - Поток: приложение создаёт pairing-код (`POST /api/me/source-credentials/{code}/pairing`, 32 байта, TTL 5 минут, single-use, привязка к user+source) и открывает `https://remanga.org` в новой вкладке. Приложение отправляет код из диалога через `window.postMessage`; content script расширения на странице приложения получает его, кладёт в `chrome.storage.session` и ждёт вкладку remanga.org. Options page расширения просит optional host permission на origin инстанса через `chrome.permissions.request({ origins: [instance] })` по явному клику пользователя: браузер не даёт запросить произвольный self-host origin без жеста; при смене URL — повторный запрос.
  - Background: `chrome.cookies.getAll({ domain: "remanga.org" })` и `{ domain: "api.remanga.org" }` (включая HttpOnly) + `chrome.webRequest.onBeforeSendHeaders` для `api.remanga.org` перехватывает `Authorization`, чтобы получить Bearer. Плюс чтение localStorage/sessionStorage по ключам `token|jwt|auth` как резерв. `webRequest` нужен только для наблюдения; если браузер его не даёт, остаются cookie + localStorage, а при неудаче — ручная вставка. После появления сессии отправляет `POST {instance}/api/source-credentials/handoff` с `{ code, bearer?, cookie? }`.
  - Handoff: публичный маршрут (без сессии), защищён одноразовым кодом; rate limit; `Cache-Control: no-store`; ограничение размера; Origin-check для этого маршрута допускает `chrome-extension://` или отсутствие Origin, остальные мутации остаются строгими. Код гасится в той же транзакции, что и запись шифротекста.
  - Настройки: кнопка «Подключить Remanga» → диалог со статусами «открыли Remanga», «войдите в аккаунт», «получили доступ», «переподключить»; polling `GET /api/me/source-credentials` каждые 2 с до 5 минут. Ошибки расширения показываются текстом, секреты — никогда.
  - Безопасность: HTTPS в проде (http разрешён только localhost); расширение не хранит токены, только код на время сессии; host permissions ограничены `remanga.org`, `api.remanga.org` и настроенным origin инстанса; сервер отправляет auth только на хосты источника (Phase 2), расширение — только cookie этих доменов; в логах только факт успеха/ошибки.
  - Обратная совместимость: если расширение не установлено, диалог предлагает ручную вставку из фазы 1. После установки расширения страницу настроек нужно перезагрузить, чтобы content script появился; это описано в README.
- **Reuse:** pairing/крипто из Phase 1; `CurrentUser` и публичные пути — `auth/middleware.rs`; rate limit — `app.rs`; `useErrorText`; диалоги/поля shadcn; ручная вставка из Phase 1. Расширение — новое: в репозитории нет браузерного расширения, а веб-приложение не может прочитать cross-origin HttpOnly-cookie.
- **Depends on:** Phase 1; Phase 2 для проверки учётки на реальных запросах.
- **Готово, когда:** в локальном браузере вход на remanga.org через кнопку приводит к появлению зашифрованной учётки без ручной вставки; код одноразовый и истекает; повторный вход перезаписывает учётку; удаление в настройках очищает её; ручной fallback продолжает работать; тесты pairing зелёные; README расширения описывает установку и permissions.

---

## Риски и ограничения

- **Cross-origin.** Автоматический захват возможен только расширением; до Phase 5 — ручная вставка. Это зафиксировано в решении 0017.
- **Per-user vs глобальные данные.** `source_entries`, главы, обложки и метаданные остаются глобальными; страницы контента и картинки гейтятся per-user через `auth_required`. Обложки и названия глав могут быть видны без входа — осознанный компромисс. Auth-ошибка одного пользователя не должна менять глобальную доступность главы; это отдельный тест Phase 2.
- **Кэш картинок.** Публичные картинки — общий кэш; полученные с учёткой — `user:{id}`. Сессионные заголовки не уходят на сторонние CDN. Это осознанная плата диском за отсутствие обхода доступа.
- **Стоимость discovery.** Поиск — до 6 запросов на источник; поэтому TTL, per-title lock, бюджет и отказ от массового обхода после импорта. При необходимости пакетный обход — отдельная задача.
- **Знаменатель прогресса.** Для `930/17` используется `available_count` предпочтительного источника (`titles.preferred_source_id`), fallback на `MAX(source_entries.available_count)`, а не `chapter_count` (тот включает locked/missing). Это не объединение источников и не alias-группировка; все текущие источники `ru`. При разноязычных или расходящихся источниках знаменатель уточняется отдельной задачей. Сортировка и `stats.read_chapters` не меняются.
- **Бэкфилл прогресса.** High-water уже прочитанных 930 глав из текущей БД восстановить нельзя; после миграции нужно повторно импортировать закладки. В настройках есть подсказка.
- **404 без входа.** Remanga может отдавать 404 вместо 401 для скрытого 18+ тайтла. Такой ответ помечается `source_auth_maybe_required` и кэшируется в discovery на 1 час, а не как окончательный `not_found` на 24 часа.
- **Экспорт.** High-water не отдаётся в Remanga-формат больше `chapter_count`: `progress` клампится, иначе обратный импорт получит прогресс больше тотала. 930 остаётся локальным отображением.
- **Ключ шифрования.** Ротация не входит в scope; `key_version` зарезервирован. Потеря ключа делает сохранённые учётки нечитаемыми — их нужно переподключить.
- **Решение 0011.** Оно запрещало auto-link при синхронизации; Phase 4 добавляет отдельный per-user discovery с TTL, поэтому 0011 помечается поправкой, а 0019 фиксирует новое поведение.

## Before merge

`/ship` - typecheck + lint + Rust-тесты + fmt/clippy + скан ветки + циклы + build. Ревью в нём нет - это `/isgood` каждой фазы.
