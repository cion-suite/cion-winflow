# План перевода cion-reader на сервер

- **Статус:** принят; фазы 0–5 и 5.5 (библиотека без каталога) выполнены, дальше фаза 6
- **Дата:** 2026-11-24 (фаза 5 завершена 2026-11-25, фаза 5.5 — 2026-11-25)
- **Решение:** [0001-server-architecture.md](decisions/0001-server-architecture.md)
- **Дополнение:** [0002-source-abstraction.md](decisions/0002-source-abstraction.md) — гетерогенные источники
- **Дополнение:** [0003-source-selection.md](decisions/0003-source-selection.md) — выбор источника и fallback по главам
- **Дополнение:** [0004-first-source-remanga.md](decisions/0004-first-source-remanga.md) — первый источник
- **Дополнение:** [0005-second-source-rumix.md](decisions/0005-second-source-rumix.md) — второй источник
- **Дополнение:** [0006-library-first.md](decisions/0006-library-first.md) — библиотека вместо каталога, ленивый импорт
- **Дополнение:** [0011-auto-link-sources.md](decisions/0011-auto-link-sources.md) — автопривязка источников при добавлении
- **Исходное состояние:** десктопное приложение на Tauri 2
- **Актуализация 2026-11-26:** тип произведения принадлежит тайтлу (8 типов Remanga), источники тип-агностичны: `source_entries` и `source_priorities` больше не хранят `kind`, отдельного админского импорта нет. Добавлен Mangamir; приоритет источника стал главным критерием выбора (не тай-брейкером), метаданные и обложки сливаются из всех привязанных источников, а страница тайтла запускает фоновую синхронизацию. Выдача поиска дедуплицируется по приоритету источника, а добавление тайтла автоматически привязывает его копии с других включённых источников того же языка (сначала по основному названию, затем по альтернативным). В исторических разделах 7.x ниже ещё встречаются старые `kind`-детали и правило «полнота важнее приоритета»; актуальные контракты — CLAUDE.md и architecture.md.

## 1. Цель и границы

cion-reader — закрытый self-hosted агрегатор тайтлов (манга, манхва, маньхуа, западные и русские комиксы и т. п.) из нескольких источников, с единым поиском, общей библиотекой и синхронизацией прогресса для себя и небольшого круга читателей. Аниме и книги — связанный контент будущих фаз, а не тип тайтла.

Границы проекта:

- публичной регистрации нет, только инвайты;
- контент не хостится: метаданные лежат в БД, изображения кэшируются, видео встраивается;
- SSR и SEO не нужны: приложение работает за логином;
- целевой вариант — один VPS и один процесс приложения;
- мобильные приложения не планируются, но интерфейс должен работать на телефоне.

## 2. Что сохраняется и что меняется

| Часть | Судьба |
|---|---|
| `src/` — React, слои, shadcn, i18n | сохраняется; меняется транспорт и добавляются экраны |
| `src/app/AppProvider.tsx` | `HashRouter` → `BrowserRouter`; `QueryClient` перевести с локального режима на HTTP |
| `src/api/*` | `client.ts` на `fetch`; `index.ts` собирает области; хуки `catalog`, `library`, `discover`, `chapters`, `progress`, `admin` |
| `src-tauri/` | удаляется целиком: Tauri, плагины, capabilities, `build.rs`, иконки, `tauri.conf.json` |
| `server/` | новый крейт: axum + tokio + sqlx |
| `eslint.config.js` | убрать `src-tauri` из `globalIgnores` и запрет `@tauri-apps/*` |
| `package.json` | убрать `tauri` и `tauri:dev`, `@tauri-apps/*`; добавить `dev:server` |
| `src/lib/settings.ts` | остаётся только ветка `localStorage` |
| `src/shell/navbar/ThemeToggle.tsx` | убрать импорты `@tauri-apps/*` и `syncWindowTheme`, оставить класс темы |
| `vite.config.ts` | заменить блок `server` целиком: proxy `/api`, убрать `watch.ignored`, `clearScreen` и Tauri-комментарии |
| `.gitignore` | `server/target`, `data/`, `.env*` (кроме примера); `server/Cargo.lock` коммитится |
| `.claude/commands/notes.md`, `.claude/scripts/notes.mjs` | источник версии остаётся `package.json` (`notes.mjs` читает его), `server/Cargo.toml` bump-ается тем же значением; области для `server/` вместо `src-tauri/` |
| `docs/` | `architecture.md` обновляется, план и решение добавлены |

Временное сосуществование: пока `src-tauri/` не удалён, `server/` — отдельный крейт, workspace не нужен. После удаления Tauri `server/` остаётся единственным Rust-крейтом.

## 3. Целевая структура

```
cion-reader/
  src/                         фронтенд (слои как сейчас)
    app/                       запуск, провайдеры, роутер
    shell/                     navbar, layout, nav (ссылки шапки и меню)
    pages/                     library, title, reader, watch, admin, profile, settings, login
    api/                       client.ts (fetch), auth.ts, catalog.ts, library.ts, discover.ts, progress.ts, admin.ts
    ui/                        shadcn и общие компоненты
    lib/                       utils, settings, i18n
  public/theme.js              тема до первой отрисовки, внешний файл для CSP
  server/
    Cargo.toml
    migrations/                SQL-файлы sqlx
    src/
      main.rs                  конфиг, пул, роутер, запуск
      app.rs                   Router, слои middleware
      config.rs                переменные окружения
      error.rs                 AppError + IntoResponse
      state.rs                 AppState: пул, конфиг, клиенты источников, кэши
      auth/                    инвайты, вход, сессии, middleware
      db/                      пул, миграции, репозитории
      catalog/                 тайтлы, источники тайтла, главы, поиск
        selection.rs           preferred source, fallback по главам, доступность
        links.rs               attach/detach источника, primary, chapter_aliases
      library/                 библиотека и прогресс
      sources/                 контракт источников
        mod.rs                 SourceKind (enum dispatch), реестр, AdapterCtx
        descriptor.rs          SourceDescriptor, Capability, ConfigField
        dto.rs                 TitleKind, ContentKind, SourceTitle, SourceChapter, SourceContent, SourceText, SourceSearchItem
        error.rs               SourceError -> AppError
        http.rs                общий клиент, лимиты, ретраи, circuit breaker, POST-форма
        keys.rs                общий chapter_key и нормализация номера
        contract.rs            #[cfg(test)] общий набор проверок адаптера
        <code>/                mod.rs, api.rs, parse.rs, config.rs, fixtures/
                               remanga/ — JSON API; rumix/ — HTML
      images/                  прокси страниц, кэш, обложки, аватарки
      jobs/                    фоновые задачи
      static_files.rs          ServeDir + SPA fallback + заголовки кэша
  dist/                        билд фронтенда (в git не коммитится)
  docs/
```

Правило: HTTP-обработчики тонкие, логика — в сервисах и репозиториях. Адаптеры источников не знают о БД, БД не знает об HTTP. Это позволяет тестировать парсеры на фикстурах, а логику — на временной SQLite.

## 4. Крейты

| Крейт | Назначение |
|---|---|
| `axum` 0.8 (`multipart`) | HTTP-роутер и обработчики; multipart для загрузки аватарок |
| `tokio` | асинхронный runtime |
| `tower`, `tower-http` | middleware: trace, compression, timeout, `ServeDir`; CORS не нужен |
| `serde`, `serde_json` | DTO и JSON |
| `serde_ignored` | предупреждения о неизвестных ключах `config_json` вместо падения |
| `ammonia` | санитайзинг текста книг (фаза 7, раньше не нужен) |
| `sqlx` 0.8 (`sqlite`, `runtime-tokio`, `migrate`, `time` или `chrono`) | БД и миграции |
| `thiserror` | `AppError` |
| `tracing`, `tracing-subscriber` | структурные логи |
| `reqwest` 0.12, `default-features = false`, features `rustls-tls`, `http2`, `charset` | исходящие запросы к источникам без OpenSSL |
| `scraper` | CSS-селекторы для HTML-источников |
| `argon2` | хеши паролей |
| `rand` | токены сессий и инвайтов |
| `sha2`, `hex` | хеши токенов, ключи кэша и глав без номера |
| `image` | проверка, ресайз и перекодирование загруженных картинок |
| `time` или `chrono` | время (выбрать одно и не смешивать) |
| `tower-governor` | rate limit login/register (обязателен в фазе 2) |
| `axum-extra` (`cookie`) | cookie (если не писать разбор вручную) |
| `dotenvy` | `.env` в dev |

У `image` включите нужные форматы (JPEG, PNG, WebP, GIF; AVIF — по необходимости), у `sqlx` — feature `time` или `chrono` под выбранный тип времени. У `axum` нет CORS: в dev работает прокси, в проде origin один.

## 5. Модель данных

Миграции — в `server/migrations/`, запуск при старте через `sqlx::migrate!().run(&pool)`. Пул — 4–8 соединений, настраивается через `SqliteConnectOptions` + `connect_with`: `journal_mode=WAL`, `synchronous=NORMAL`, `busy_timeout=5000`, `foreign_keys=ON`, `create_if_missing(true)` (sqlx по умолчанию не создаёт БД). Внутренние ключи — `INTEGER PRIMARY KEY`, внешние идентификаторы источников — `TEXT`. Время — `INTEGER` (unix seconds), чтобы не зависеть от формата строк.

Запросы: начните с runtime-варианта (`sqlx::query(...).bind(...)`), он не требует БД на этапе компиляции. Макросы `query!` дают проверку типов, но требуют `DATABASE_URL` при сборке или `SQLX_OFFLINE` + `cargo sqlx prepare` в CI; переходите на них, когда схема устоится.

```
users(id, username UNIQUE, password_hash, role, created_at, disabled_at)
invites(id, code_hash UNIQUE, created_by, used_by, expires_at, created_at)
sessions(id, token_hash UNIQUE, user_id, created_at, expires_at, last_seen_at)

sources(id, code UNIQUE, name, base_url, enabled, config_json,
        descriptor_version, config_error, last_checked_at, last_error, created_at)
titles(id, kind, name, name_lower, alt_names_json, authors_json, genres_json, description,
       status, year, lang, rating, origin_source_id, preferred_source_id,
       preferred_source_updated_at, created_at, updated_at)
       -- kind: manga|manhwa|manhua|western_comic|ru_comic|indonesian_comic|other|original
title_covers(title_id, size, path, width, height, source_id, updated_at,
             PRIMARY KEY(title_id, size))
source_entries(id, title_id, source_id, external_id, external_url, raw_json,
               is_primary, adapter_version, chapter_count, available_count,
               locked_count, missing_count, stats_updated_at, last_synced_at, last_error,
               UNIQUE(source_id, external_id), UNIQUE(title_id, source_id))
chapters(id, source_entry_id, external_id, chapter_key, number_text, number_milli,
         ordinal_site, volume, chapter_kind, content_kind NULL, name, group_name, language,
         is_locked, availability, availability_checked_at, page_count,
         external_url, embed_url, published_at, last_seen_at, removed_at, raw_json)
chapter_aliases(id, title_id, canonical_key, alias_key, created_at,
                UNIQUE(title_id, alias_key))
source_priorities(id, lang, source_id, rank, created_at,
                  UNIQUE(lang, source_id), UNIQUE(lang, rank))
pages(id, chapter_id, idx, image_ref, image_url, width, height)
chapter_texts(chapter_id PK, format, body, byte_len, fetched_at, expires_at)

library(user_id, title_id, status, score, custom_tab_id, added_at, updated_at,
        PRIMARY KEY(user_id, title_id),
        CHECK(status IN ('in_progress','planned','favorite','completed','dropped','on_hold','not_interested')),
        FOREIGN KEY(custom_tab_id) REFERENCES library_tabs(id) ON DELETE SET NULL)
library_tabs(id, user_id, name, position, created_at,
             UNIQUE(user_id, name))
progress(user_id, title_id, chapter_key, page, seconds, scroll, updated_at,
         PRIMARY KEY(user_id, title_id))
chapter_reads(user_id, title_id, chapter_key, read_at,
              PRIMARY KEY(user_id, title_id, chapter_key))

jobs(id, kind, payload_json, status, attempts, run_after, last_error, created_at)
```

Ключевые решения:

- **Канонический прогресс.** `progress` и `chapter_reads` живут в SQLite, а не в localStorage, и хранятся по `title_id` + каноническому `chapter_key`, а не по `chapter_id` конкретного источника: при переключении источника та же глава имеет другой `chapter_id`, и прогресс терялся бы. `PUT /api/progress/{title_id}` принимает `chapter_key`, проверяет, что ключ принадлежит тайтлу (глава или alias), и в той же транзакции берёт `content_kind` выбранного варианта. Для изображений позиция — `page`, для видео — `seconds`, для текста — `scroll` (0..1); несовпадение поля с `content_kind` — 400, а если `content_kind` ещё NULL — 409 `content_kind_unknown` (клиент сначала открывает главу через `/content`). Alias резолвится в canonical тем же правилом. Автоматически ключи не склеиваются: если у источников нумерация разошлась, администратор связывает их через `POST /api/titles/{title_id}/chapter-aliases`; до связывания у каждого ключа свой прогресс.
- **Прочитанное.** `PUT /api/progress/{title_id}` в той же транзакции пишет `chapter_reads` по каноническому ключу; карточка библиотеки показывает `read_count` и `chapter_count` (максимум по `source_entries`), страница тайтла — отметку у главы. Журнал не чистится при удалении из библиотеки: возврат тайтла сохраняет историю.
- **Библиотека и свои закладки.** Статусы — `in_progress|planned|favorite|completed|dropped|on_hold|not_interested`; «Любимое» — такой же статус, отдельного флага нет (миграция 0006). Своя закладка — ещё один статус, один на тайтл: `library.custom_tab_id` перекрывает встроенный `status`, а удаление закладки возвращает сохранённый встроенный (миграция 0007, `library_tab_items` удалена). Своих закладок не больше трёх (`library_tabs`), имя уникально у пользователя.
- **Приоритеты источников.** `source_priorities(lang, source_id, rank)`: меньший `rank` — выше приоритет, `lang = '*'` — запасной набор. Источник не привязан к типу тайтла, поэтому набор один на язык: `ru` = Remanga → RuMix → Mangamir, `en` — свой. **Приоритет — главный критерий**: главы, которые есть на более приоритетном источнике, берутся с него, а менее приоритетный заполняет только пропуски. Mangamir подключён; MangaLib остаётся кандидатом на будущее.
- **Полнота источника и preferred source.** После импорта и refresh `source_entries` хранит `chapter_count` (уникальные `chapter_key` без `removed_at`, не варианты групп), `available_count`, `locked_count`, `missing_count`, `stats_updated_at`. `titles.preferred_source_id` — включённый источник с наименьшим `rank`; при равенстве — `is_primary`, затем больший `chapter_count`, затем `id`; источник с `available_count = 0` может быть preferred ради метаданных, его главы доберут другие источники; источник без `base_url` или с `config_error` не выбирается. `titles.origin_source_id` — откуда тайтл импортирован (это разные поля). Пересчёт — при импорте, refresh, включении/выключении источника, смене приоритетов, отвязке и синхронизации, не на каждый запрос.
- **Fallback по главе.** Для канонического ключа варианты сортируются по известной недоступности (`locked`/`missing` — в конец; `unknown` ещё не повод отдавать главу другому источнику), затем по приоритету источника, preferred, `is_primary`, `ordinal_site`, `id`; если запрос к выбранному варианту не удался, пробуется следующий. Успех помечает главу `available` и заполняет `content_kind`/`page_count`; `Locked`/`NotFound` — `locked`/`missing`; временные ошибки не меняют доступность, но позволяют попробовать следующий источник.
- **Метаданные доступности.** `chapters.availability` (`unknown|available|locked|missing`), `availability_checked_at`, `page_count`; `locked` доверяем ~6 ч, `missing` ~24 ч, после чего состояние снова `unknown`. `GET /api/titles/{id}` читает только БД; `POST /api/titles/{id}/sync` (TTL 10 минут) в фоне обновляет главы, обложки и общие метаданные при открытии страницы, запрос к источнику — также при открытии главы.
- **`is_primary` — тай-брейкер.** Флаг не является жёстким override: после `rank` он решает ничью по полноте и используется для отображения. Если предпочтительный источник ошибочен, администратор меняет приоритеты или выключает источник.
- **Тип произведения — свойство тайтла.** `titles.kind` — канонический тип из списка Remanga (`manga`, `manhwa`, `manhua`, `western_comic`, `ru_comic`, `indonesian_comic`, `other`, `original`). Источник не объявляет типы: он провайдер контента, а тип берётся из метаданных источника при импорте (Remanga `type.name`, RuMix заголовок, Mangamir `type[0]`) или выбирается вручную. `source_entries` и `source_priorities` поле `kind` не хранят. Аниме и книги — не типы тайтла, а связанный контент будущих фаз (`content_kind = video` и т. п.).
- **Канонические метаданные общие, но ручные правки не теряются.** `titles` заполняются из источника при создании. При синхронизации метаданные сливаются из всех привязанных источников в порядке приоритета: первый непустой `name`/`description`/`status`/`year`/`rating`/`lang`, первый конкретный `kind`, списки объединяются. `titles.source_metadata_json` хранит прошлый снимок: поле, отличающееся от него, считается ручной правкой и не перезаписывается. При привязке второго источника поля не перетираются молча.
- **Один тайтл — один язык в первых фазах.** `titles.lang` задаёт набор приоритетов; иноязычные версии — отдельные тайтлы. Смешивать RU/EN в одной записи будем только при явной необходимости, иначе выбор источника и название становятся неоднозначными.
- **Глава — единица контента с типом.** `chapters.content_kind` может быть NULL до первого открытия: не все источники знают формат из списка глав. Когда тип известен — `images` | `video` | `text` (последнее зарезервировано до книги-источника); при первом успешном `content()` фактический вариант записывается в БД. `pages` заполняется только для `images`, `embed_url` — только для `video`. Основной путь — `GET /api/titles/{title_id}/chapters/{chapter_key}/content` (сам выбирает источник и делает fallback); `/api/chapters/{id}/content` — прямой вариант для админки и отладки; `/text` для не-текстовой главы — 409.
- **Ключ главы.** Номер нельзя хранить как `f32`: у источников разная база (0/1), тома, дробные номера, диапазоны «1-2», спецвыпуски без номера и несколько групп перевода. В БД: `number_text` — нормализованная строка как у источника, `number_milli` — только для сортировки (NULL, если число не выделяется, например у спецвыпуска без номера; у диапазона берётся начало), `ordinal_site` — порядок в выдаче источника. `chapter_key` строит адаптер, и он NOT NULL: с томом — `v{vol}:c{number_text}`, без тома — `c{number_text}`; для любого `chapter_kind != normal` — `{vol_seg}{kind}:c{number_text}`; без номера — `{vol_seg}{kind}:x:h{sha256(external_id)[..16]}`, где `{vol_seg}` — `v{vol}:` или пустая строка. Хеш стабилен между переимпортами и не зависит от отображаемого имени; безномерные главы не склеиваются между источниками и между группами с разными `external_id` — это осознанное ограничение, для остальных случаев есть alias. `chapter_key` фиксируется при первом импорте: обычный refresh обновляет всё, кроме ключа, даже если источник поправил номер или том; расхождение пишется в `last_error` и решается миграцией, а не молчаливой потерей прогресса. `progress` пишется только по каноническому ключу; `chapter_id` в API прогресса не участвует и остаётся деталью ответа `/content`.
- **Несколько групп перевода.** Одна логическая глава может прийти несколькими строками с одинаковым `chapter_key` и разными `group_name`/`language`. Уникальность — `(source_entry_id, external_id)`, не по ключу. API группирует варианты по ключу; прогресс общий, а при смене группы позиция страницы сбрасывается, если выходит за границы новой группы. Вариант для открытия выбирает селектор из 7.10 (preferred source → доступность → приоритет → `is_primary` → `ordinal_site`); клиент может переключить вариант вручную.
- **Исчезнувшие главы не удаляются сразу.** После полного успешного списка глав отсутствующие помечаются `removed_at`, API их скрывает, прогресс сохраняется; при частичном ответе источника sweep не запускается. Физическая чистка — фоном.
- **`external_id` — непрозрачная строка.** UTF-8, непустая, ≤ 256 символов, без control-символов, `/`, `://` и `..`. Адаптер сам URL-кодирует её при сборке запроса; ограничивать ASCII нельзя — у источников бывают кириллические и percent-encoded id.
- **Дедупликация.** `UNIQUE(source_id, external_id)` и `UNIQUE(source_entry_id, external_id)`; `pages` — `UNIQUE(chapter_id, idx)`. Импорт — `INSERT ... ON CONFLICT ... DO UPDATE`, иначе refresh создаст дубли. **Один `source_entry` на пару `(title_id, source_id)`** (миграция 0003): `preferred_source_id` хранит source_id, поэтому две записи одного источника у одного тайтла неоднозначны; attach второго external_id того же источника к тому же тайтлу — 409 `source_already_attached`.
- **Импорт не атомарен между сетью и БД.** `source_entry` создаётся или привязывается в транзакции, главы пишутся пачками upsert; сбой на любом шаге оставляет `source_entries.last_error` и не удаляет уже импортированное. Повторный импорт идемпотентен по `(source_entry_id, external_id)`; администратор видит ошибку синхронизации и повторяет её, а не получает полупустой тайтл без объяснения.
- **Ссылки на страницы.** `pages.image_ref` — стабильная ссылка источника (путь или id), `image_url` — последний разрешённый URL. Подписанные CDN-URL живут недолго: при `401/403/410` прокси не больше двух раз перезапрашивает список страниц через адаптер, обновляет `image_url` и отдаёт байты; если не помогло — глава помечается `locked`, статистика обновляется, клиент повторяет canonical `/content` и получает другой источник. Кэш байтов ключуется по `(source_id, external chapter id, idx)`, не по подписанному URL.
- **Обложки из поиска.** В выдаче — `cover_ref` (стабильная ссылка), не URL. `GET /api/sources/{code}/cover?ref=` резолвит её через адаптер и тот же SSRF-allowlist, что страницы; иначе поиск становится SSRF-пробой. В выдаче берётся `low`-вариант, чтобы диалог добавления не тянул тяжёлый файл.
- **Обложки по размерам.** `title_covers` хранит до трёх вариантов: `low` — выдача поиска и диалог добавления, `mid` — карточки библиотеки, `high` — страница тайтла. При импорте, привязке источника и ручном refresh сервер сам скачивает размеры, которые отдал адаптер (`SourceCovers`), и заполняет только отсутствующие: ручную обложку автоимпорт не перетирает, а сбой загрузки не отменяет импорт глав. Поэтому `POST /api/admin/refresh/{title_id}` можно использовать, чтобы добрать обложки у тайтлов, импортированных раньше. `GET /api/titles/{id}/cover` при отсутствии запрошенного размера отдаёт ближайший сохранённый (сначала более крупный, затем мелкий), поэтому клиент всегда просит семантический размер. Администратор загружает и удаляет каждый размер отдельно; редактор — диалог с тремя слотами. Явная операция «получить из источника» (`POST /api/titles/{id}/cover/from-source`) перебирает привязанные источники в порядке preferred → primary → привязка и обновляет source-размеры даже при наличии файла (обложка могла смениться), а ручные (`source_id IS NULL`) заменяет только с `replace_manual=true` (запись без файла на диске считается пустой и лечится); диалог спрашивает об этом, если у тайтла есть `manual_cover_sizes`. Ошибки источников возвращаются в `errors` и не отменяют остальные.
- **Конфиг источника.** `sources.config_json` — типизированный конфиг адаптера: `image_hosts`, `player_hosts`, заголовки/куки, лимиты, язык/перевод, особенности нумерации. `base_url` живёт только в колонке `sources.base_url`, а не в конфиге: иначе у одного хоста два источника истины. `image_hosts` нужен потому, что картинки часто лежат на отдельном CDN, а не на `base_url`. Секреты маскируются в API и не попадают в логи; для self-host допустимо хранить их в SQLite. `descriptor_version` фиксирует версию адаптера, под которую валидировался конфиг, `config_error` — причину отключения источника.
- **Версия адаптера.** `descriptor.version` пишется в `sources.descriptor_version` при валидации конфига и в `source_entries.adapter_version` при импорте. Рост версии перевалидирует конфиг (если схема не менялась — это no-op) и служит поводом переимпортировать главы; `chapter_key` при этом обязан остаться прежним, иначе прогресс потеряется, — менять семантику ключа можно только с миграцией.
- **Отключённый источник.** Записи `source_entries` и главы остаются в БД, тайтл и библиотека читаются. Чтение контента из отключённого источника — `source_disabled` (503), а не 500. Селектор пропускает выключенные источники и пересчитывает `preferred_source`; `is_primary` автоматически не переписывается.
- **Сессии.** `last_seen_at` обновлять не чаще раза в 5–15 минут (условный `UPDATE ... WHERE last_seen_at < now - 300`), иначе каждый запрос пишет в единственную SQLite-базу.

Индексы и ограничения: `UNIQUE(source_id, external_id)`, `UNIQUE(source_entry_id, external_id)`, `UNIQUE(title_id, source_id)` (миграция 0003), `UNIQUE(chapter_id, idx)`, частичный `UNIQUE(title_id) WHERE is_primary = 1`, `UNIQUE(title_id, alias_key)`, `UNIQUE(lang, source_id)`, `UNIQUE(lang, rank)`, `pages(chapter_id)`, `source_entries(title_id)`, `chapters(source_entry_id, chapter_key)`, `chapters(source_entry_id, volume, number_milli, ordinal_site)`, `chapters(source_entry_id, availability)`, `chapter_aliases(title_id, canonical_key)`, `chapter_texts` по PK, `source_priorities(lang, rank)`, `progress(user_id, updated_at)`, `jobs(status, run_after)`, `titles(name_lower)`, `titles(kind)`.

Поиск: на первом этапе — `LIKE` по `name_lower` с экранированием `%` и `_`. `name_lower` — Unicode-lowercase копия `name`, которую заполняет приложение: SQLite `lower()` понимает только ASCII, и поиск по кириллице иначе не работал бы. FTS5 подключать позже, когда `alt_names` будут храниться склеенным текстом (JSON-массив FTS5 индексирует плохо), а пользовательский ввод будет экранироваться от синтаксиса `MATCH`.

## 6. API

Базовый путь — `/api`, JSON, ошибки `{ "code": "...", "message": "..." }` с корректным HTTP-статусом. Версия axum — 0.8: параметры пути записываются в фигурных скобках (`/api/titles/{id}`), двоеточие в 0.8 паникует при сборке роутера. Авторизация — HttpOnly-cookie `sid`. Версионирование (`/api/v1`) не нужно, пока клиент и сервер деплоятся вместе.

Доступ: всё под `/api`, кроме `/api/health` и `/api/auth/*`, требует валидную сессию. Middleware сессии вешается на весь API-роутер, а публичные маршруты перечислены явно — не «всё, что не мутация». Тест: запрос без cookie → 401.

Пагинация списков (каталог, библиотека): `page >= 1` (иначе 400), `page_size` по умолчанию 50 с жёстким потолком; ответ — `{ items, total, page, page_size }`. Поиск источников пагинируется курсором (`cursor`/`next_cursor`), правило `page` к нему не применяется.

| Метод и путь | Что делает |
|---|---|
| `GET /api/health` | проверка сервера и БД |
| `POST /api/auth/register` | регистрация по инвайту |
| `POST /api/auth/login` | вход, выставляет cookie |
| `POST /api/auth/logout` | удаляет сессию |
| `GET /api/me` | текущий пользователь |
| `GET /api/catalog?q=&kind=&page=&page_size=` | каталог и поиск |
| `GET /api/titles/{id}` | тайтл, `origin_source`, `preferred_source`, статистика источников, главы с выбранным вариантом и вариантами групп; только БД, без запросов к источникам |
| `GET /api/titles/{title_id}/chapters/{chapter_key}/content` | лучший доступный вариант главы: `{ chapter_id, chapter_key, source, content, fallback? }`; сервер сам делает fallback |
| `GET /api/titles/{title_id}/chapters/{chapter_key}/text` | текст лучшего доступного варианта; только для текстовой главы, иначе 409 |
| `GET /api/chapters/{id}/content` | конкретный вариант без fallback (админ/отладка) |
| `GET /api/chapters/{id}/text` | текст конкретного варианта (админ/отладка) |
| `GET /api/pages/{id}/image` | прокси страницы из источника |
| `GET /api/titles/{id}/cover?size=low\|mid\|high` | обложка нужного размера; отсутствующий размер отдаётся из ближайшего |
| `GET /api/library?q=&status=&tab_id=&sort=&page=&page_size=` | библиотека пользователя со счётчиками вкладок |
| `PUT /api/library/{title_id}` | статус, оценка и своя закладка (`custom_tab_id`) |
| `DELETE /api/library/{title_id}` | убрать из библиотеки (закладки чистятся FK) |
| `GET/POST /api/library/tabs` | свои закладки пользователя; не больше трёх |
| `PUT/DELETE /api/library/tabs/{id}` | переименовать или удалить закладку |
| `GET /api/discover/search?q=&kind=` | поиск по всем включённым источникам (любая сессия) |
| `POST /api/discover/add` | ленивый импорт найденного тайтла и добавление в библиотеку |
| `GET /api/progress/{title_id}` | последняя позиция: `chapter_key` и `page`/`seconds`/`scroll` |
| `PUT /api/progress/{title_id}` | сохранить `{ chapter_key, page? }` / `{ chapter_key, seconds? }` / `{ chapter_key, scroll? }` по типу контента |
| `PUT /api/titles/{title_id}/chapters/read` | вручную отметить `{ chapter_keys, read }` пачкой; возвращает полный `read_keys` |
| `POST /api/me/avatar` | загрузка аватарки (multipart, лимит тела) |
| `GET /api/me/avatar`, `GET /api/users/{id}/avatar` | аватары |
| `GET /api/sources` | скомпилированные дескрипторы + состояние: `capabilities`, схемы config и фильтров, `enabled`, `config_error`, `last_checked_at`, `last_error` (admin) |
| `GET /api/sources/{code}/cover?ref=` | обложка из выдачи поиска через allowlist источника (любая сессия) |
| `PUT /api/sources/{code}` | создать или обновить запись источника: `base_url`, `enabled`; `code` обязан быть среди скомпилированных дескрипторов (admin) |
| `POST /api/sources/{code}/search` | поиск: `{ query, cursor?, filters? }`; ответ `{ items, next_cursor, total }`, у каждого элемента необязательный `kind` и `external_id` (admin) |
| `PUT /api/sources/{code}/config` | сохранить и провалидировать `config_json` существующего источника; ответ `{ warnings, config_error? }`, ошибка валидации — 422 (admin) |
| `POST /api/sources/{code}/probe` | dry-run проверки источника: `ok` / `auth` / `cloudflare` / `parse` (admin) |
| `GET/POST /api/admin/invites` | список и создание инвайтов (admin) |
| `POST /api/admin/users/{id}/disable` | отключить пользователя (admin) |
| `POST /api/titles`, `PUT /api/titles/{id}` | создание и правка тайтла (admin) |
| `POST /api/titles/{id}/cover?size=low\|mid\|high` | загрузка обложки в конкретный размер (admin) |
| `DELETE /api/titles/{id}/cover?size=low\|mid\|high` | удалить размер обложки (admin) |
| `POST /api/titles/{id}/cover/from-source` | обновить обложки из привязанных источников; `{ replace_manual? }` разрешает заменить ручные (admin) |
| `POST /api/titles/{id}/sources` | привязать источник к тайтлу: `{ code, external_id }` (admin) |
| `DELETE /api/titles/{id}/sources/{entry_id}` | отвязать источник; висячие alias чистятся (admin) |
| `PUT /api/titles/{id}/primary-source` | задать `is_primary` (`{ source_entry_id }`) — тай-брейкер и признак отображения, не жёсткий override (admin) |
| `GET /api/titles/{title_id}/chapter-aliases` | список alias → canonical (admin) |
| `POST /api/titles/{title_id}/chapter-aliases` | связать `alias_key` с `canonical_key`; цепочки и несуществующие ключи — 4xx (admin) |
| `POST /api/titles/{title_id}/chapter-aliases/bulk` | пачка связей до 500 одной транзакцией; переносит прогресс и отметки на canonical; перепривязка готового alias — 409 (admin) |
| `DELETE /api/titles/{title_id}/chapter-aliases/{alias_key}` | убрать связь ключей (admin) |
| `GET /api/admin/source-priorities?lang=` | список приоритетов источников для языка (admin) |
| `PUT /api/admin/source-priorities` | заменить список `{ lang, source_codes: [...] }`; меньший индекс — выше приоритет (admin) |
| `POST /api/admin/refresh/{title_id}` | обновить главы и статистику источников, пересчитать `preferred_source`, добрать отсутствующие обложки (admin) |

Поток импорта: поиск по источнику → пользователь выбирает результат → `POST /api/discover/add` (`{ code, external_id }`) или привязка `POST /api/titles/{id}/sources` создаёт канонический тайтл и `source_entry` → адаптер заново получает метаданные по `external_id` (ответ поиска не считается доверенным), определяет тип тайтла и импортирует главы; страницы — лениво при открытии главы. `external_id` приходит из ответа поиска (в БД его ещё нет); адаптер валидирует его и URL-кодирует.

Ошибки источников типизированы и не превращаются в 500: `source_bad_request`/`source_unsupported_kind`/`source_invalid_external_id` — 400, `source_not_found` — 404, `source_locked` — 403, `source_auth_required`/`source_upstream_failed`/`source_parse_failed` — 502, `source_rate_limited` — 503 с `Retry-After`, `source_disabled` — 503. Текст ошибки не содержит чужих URL и тела ответа.

Правила: мутирующие запросы требуют сессии и проверки `Origin`; админские — роли; контент главы отдаёт внутренние URL, а не ссылки на чужой сайт; для загрузки аватарки повесить `DefaultBodyLimit::max(6 * 1024 * 1024)` (дефолтные 2 МиБ не пропустят 5-мегабайтный файл) и отдельно проверить размер в обработчике. Изображения клиент получает только через сервер; видео — через встроенный плеер, браузер обращается к домену плеера напрямую (см. §9). Аватарки опциональны: не входят в критерии готовности и могут быть отложены.

## 7. Источники

Источники неоднородны: различаются типы произведений, модели глав, формат контента, пагинация и фильтры, метаданные, авторизация и лимиты. Поэтому общим является только узкий контракт, а всё остальное живёт внутри адаптера; каталог, БД и API не знают особенностей конкретного сайта.

Решение и альтернативы — [0002-source-abstraction.md](decisions/0002-source-abstraction.md).

### 7.1. Дескриптор источника

Каждый адаптер объявляет статические данные, а не заставляет UI угадывать поведение:

```rust
pub struct SourceDescriptor {
    pub code: &'static str,              // "remanga"; он же ключ в sources.code
    pub name: &'static str,
    pub version: u32,                    // растёт при смене семантики парсинга или контракта
    pub capabilities: &'static [Capability],
    pub config: &'static [ConfigField],  // схема config_json для админки
    pub filters: &'static [ConfigField], // схема filters в поиске
}

pub struct ConfigField {
    pub key: &'static str,
    pub kind: FieldType,
    pub required: bool,
    pub secret: bool,
}

pub enum FieldType {
    Text { max_len: usize },
    Url,
    UrlList,
    Integer { min: i64, max: i64 },
    Bool,
    Select(&'static [&'static str]),
    MultiSelect(&'static [&'static str]),
    Range { min: i64, max: i64, step: i64 },
    TriState,
}

pub enum Capability {
    Search, Chapters, Images, VideoEmbed, Text,
    Volumes, AltNames, SearchFilters, AuthCookie, SignedUrls,
}
```

`Capability` — данные, а не набор трейтов. Обработчик проверяет флаг и возвращает 400 `source_bad_request` до вызова адаптера. Источник не объявляет типы тайтлов: он провайдер контента, а тип приходит из метаданных источника при импорте или выбирается вручную. Дескрипторы компилируются в бинарник; `GET /api/sources` отдаёт их админке.

### 7.2. Нормализованные DTO

```rust
pub enum TitleKind {
    Manga, Manhwa, Manhua, WesternComic, RuComic, IndonesianComic, Other, Original,
}
pub enum ContentKind { Images, Video, Text }

pub struct SourceCovers {                // все поля — стабильные ссылки, не URL для клиента
    pub low: Option<String>,             // поиск и диалог добавления
    pub mid: Option<String>,             // карточки библиотеки
    pub high: Option<String>,            // страница тайтла
}

pub struct SourceTitle {
    pub external_id: ExternalId,
    pub kind: TitleKind,                  // тип из метаданных источника
    pub name: String,
    pub alt_names: Vec<String>,
    pub description: Option<String>,
    pub covers: SourceCovers,            // источник может отдать 1..3 размера
    pub authors: Vec<String>,
    pub genres: Vec<String>,
    pub status: Option<TitleStatus>,
    pub year: Option<i16>,
    pub rating: Option<f32>,             // адаптер нормализует в 0..=10
    pub external_url: Option<String>,    // «открыть на источнике»
}

pub struct SourceChapter {
    pub external_id: ExternalId,
    pub chapter_key: ChapterKey,
    pub number_text: Option<String>,     // "0", "12.5", "1-2"; у спецвыпуска без номера — None
    pub number_milli: Option<i64>,       // только для сортировки
    pub volume: Option<u16>,
    pub chapter_kind: ChapterKind,       // normal | special | extra | oneshot | preview
    pub content_kind: Option<ContentKind>, // None — формат станет известен при content()
    pub name: Option<String>,
    pub group_name: Option<String>,      // команда перевода
    pub language: Option<String>,
    pub is_locked: bool,
    pub external_url: Option<String>,    // «открыть на источнике»
    pub published_at: Option<i64>,
}

pub struct SourcePage {
    pub index: u32,
    pub image_ref: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

pub enum SourceContent {
    Images { pages: Vec<SourcePage> },
    Video { embed_url: String },         // хост проверен по player_hosts
    Text { format: TextFormat },         // тело не инлайнится, см. 7.4
}

pub struct SourceText { pub format: TextFormat, pub body: String } // ≤ 2 МиБ, проверяет сервер
pub enum TextFormat { Html, Markdown, Plain }

pub struct SourceSearchItem {
    pub external_id: ExternalId,
    pub kind: Option<TitleKind>,          // None — источник не отдаёт тип в выдаче
    pub name: String,
    pub alt_names: Vec<String>,
    pub cover_ref: Option<String>,       // low-вариант: диалог добавления не тянет тяжёлый файл
    pub year: Option<i16>,
    pub status: Option<TitleStatus>,
    pub external_url: Option<String>,
}

pub struct SearchPage {
    pub items: Vec<SourceSearchItem>,
    pub next_cursor: Option<String>,
    pub total: Option<u64>,
}
```

Правила:

- `external_id` — непрозрачная строка; никаких предположений о формате. Валидация: непустая, ≤ 256, без control-символов, `/`, `://` и `..`; URL-кодирование — в адаптере.
- неизвестное поле — `None`, а не пустая строка или ноль; `raw_json` записи хранится для отладки и будущих полей;
- `description` из источника нормализуется в plain text (HTML-теги удаляются), а не рендерится как HTML: иначе чужой HTML попадёт в UI;
- `chapter_key` считает адаптер (7.3), канонический слой его только хранит и фиксирует при первом импорте;
- `content_kind` может быть `None`; если он известен и не совпадает с вариантом `content()` — 502 `source_parse_failed`; если `None`, фактический вариант записывается в БД;
- `ordinal_site` в DTO нет: сервер выставляет его по индексу элемента в возвращённом `Vec<SourceChapter>`, а `external_url` адаптер строит сам.

### 7.3. Главы, нумерация и ключи

- `number_text` хранит номер как у источника после лёгкой нормализации: запятая → точка, обрезка пробелов, в строке остаются только цифры, точка и дефис (это же делает `chapter_key` безопасным для URL и БД); диапазоны «1-2» не разворачиваются и не теряются.
- `number_milli` заполняется, когда из номера можно выделить число: «12.5» → 12500, у диапазона «1-2» берётся начало (1000, только для сортировки), у спецвыпуска без номера — NULL.
- `ordinal_site` — порядок в выдаче источника; сервер выставляет его по индексу элемента в `Vec<SourceChapter>`. Это последний ключ сортировки и запасной вариант, но не часть `chapter_key`: он меняется при перестановке глав.
- `chapter_key`: с томом — `v{vol}:c{number_text}`, без тома — `c{number_text}`; для любого `chapter_kind != normal` — `{vol_seg}{kind}:c{number_text}`; без номера — `{vol_seg}{kind}:x:h{sha256(external_id)[..16]}`, где `{vol_seg}` — `v{vol}:` или пустая строка. Хеш стабилен между переимпортами и не зависит от отображаемого имени; безномерные главы не склеиваются между источниками и между группами с разными `external_id` — это осознанное ограничение.
- `chapter_key` фиксируется при первом импорте: refresh обновляет всё, кроме ключа, даже если источник поправил номер или том. Расхождение пишется в `last_error`; пересчёт ключа — только миграцией, иначе прогресс теряется молча.
- База 0/1, тома и дробные номера — особенность источника. Адаптер нормализует базу только если знает её из конфига; иначе сохраняет как есть. Автоматически склеивать «0» и «1» не пытаемся: ошибка хуже ручного связывания через `chapter_aliases` (alias → canonical для прогресса и группировки). `canonical_key` должен указывать на существующую главу тайтла; цепочки alias и `alias_key = canonical_key` отклоняются — иначе резолв зациклится.
- Несколько групп перевода — отдельные строки с одинаковым `chapter_key`; уникальность по `external_id`. API отдаёт варианты, прогресс общий. Вариант для открытия выбирает селектор из 7.10: preferred source → доступность → приоритет → `is_primary` → `ordinal_site`; клиент может переключить вручную, если ему нужен другой вариант.
- Залоченные главы импортируются с `is_locked = 1`; `content()` для них возвращает `source_locked` (403), страницы не запрашиваются.

### 7.4. Контент по типам произведения

| `titles.kind` | `content_kind` | Что отдаёт `content()` | Где хранится | Фаза |
|---|---|---|---|---|
| `manga` | `images` | страницы с `image_ref` | `pages`, кэш картинок | 4 |
| `anime` | `video` | `embed_url` (позже HLS) | `chapters.embed_url` | 6 |
| `book` | `text` | формат текста | `chapter_texts` как кэш с TTL и лимитом | 7 |

`content_kind` может быть NULL, пока источник не отдал его в списке глав; тогда фактический вариант определяется при первом `content()` и записывается в БД. Для видео хранится только стабильный `embed_url`; подписанные и сессионные плееры на первом этапе отклоняются на `probe`, потому что сервер не увидит истечение ссылки внутри iframe. Если такие источники понадобятся, добавим `embed_ref` и разрешение URL при каждом запросе.

Один сайт может отдавать несколько типов: `kinds` в дескрипторе, `kind` в элементе поиска, `titles.kind` при импорте. Манга и аниме одного произведения — два тайтла; связывать их через `title_relations` будем позже, если понадобится.

`SourceContent::Text` не возвращает тело: `content()` отдаёт только формат, тело забирает `text()` с лимитом 2 МиБ на главу, и оно кладётся в `chapter_texts` с TTL. `GET /api/chapters/{id}/text` отдаёт его из кэша с `ETag`. `TextFormat` — `Html | Markdown | Plain`. HTML санитайзится `ammonia` с явным allowlist: запрещены `script`, `style`, `iframe`, `object`, `srcset`; `<img>` либо вырезается, либо переписывается на внутренний прокси с тем же SSRF-allowlist; внешние ссылки получают `rel="noreferrer noopener"`. Markdown рендерится и санитайзится так же, Plain экранируется. Иначе «тонкое зеркало» из 0001 превращается в хранение книг и дыру XSS.

### 7.5. Конфиг источника

`sources.config_json` — типизированный конфиг адаптера: общий `CommonConfig` (`image_hosts`, `player_hosts`, headers, cookie, лимиты) плюс поля конкретного источника. `base_url` хранится в колонке `sources.base_url`, а не в конфиге: иначе у одного хоста два источника истины. `#[serde(default)]` для новых полей; неизвестные ключи — предупреждения, а не падение. Валидация при старте и при `PUT .../config`: обязательные поля, хосты — точный домен или `*.example.com` (без голого `*`, userinfo и портов кроме 80/443), IDNA-нормализация, проверка каждого редиректа; лимиты в разумных границах.

- битый конфиг отключает только свой источник: `config_error` пишется в БД, сервер стартует, остальные источники работают;
- секреты (cookie/token) маскируются в `GET /api/sources` и никогда не логируются; для self-host допустимо хранить их в SQLite. В `PUT .../config` отсутствующее поле означает «не менять», `null` — очистить, поэтому маскированный ответ можно безопасно отправлять обратно;
- `descriptor_version` в `sources` — версия адаптера, под которую валидировался конфиг; при несовпадении конфиг перевалидируется, при ошибке источник отключается;
- `adapter_version` в `source_entries` — версия парсера, которым импортированы данные; рост версии — повод переимпортировать главы;
- `probe` и refresh пишут `sources.last_checked_at`/`last_error`, импорт — `source_entries.last_synced_at`/`last_error`; `last_error` короткий, без чужих URL и секретов. Эти поля отдаются в `GET /api/sources` и `GET /api/titles/{id}`, чтобы деградация была видна, а не только в логах.

### 7.6. Реестр и dispatch

`enum SourceKind` и один `impl` с исчерпывающим `match` — единственная точка dispatch. Трейт `Source` не нужен: `async fn` в трейте не dyn-совместим, а набор источников фиксирован. Компилятор требует arm для каждого варианта, поэтому новый источник нельзя забыть в методе.

```rust
pub enum SourceKind {
    Remanga(Remanga),
    Rumix(Rumix),
    // MangaLib(MangaLib) — кандидат на будущее
}

impl SourceKind {
    pub fn descriptor(&self) -> &'static SourceDescriptor;
    pub async fn search(&self, req: SearchRequest) -> Result<SearchPage, SourceError>;
    pub async fn title(&self, id: &ExternalId) -> Result<SourceTitle, SourceError>;
    pub async fn chapters(&self, id: &ExternalId) -> Result<Vec<SourceChapter>, SourceError>;
    pub async fn content(&self, chapter: &SourceChapter) -> Result<SourceContent, SourceError>;
    pub async fn text(&self, chapter: &SourceChapter) -> Result<SourceText, SourceError>;
    pub fn resolve_image(&self, image_ref: &str) -> Result<reqwest::Url, SourceError>;
}
```

- Реестр `code -> SourceKind` собирается при старте из таблицы `sources`; `code` — ключ скомпилированного адаптера, отдельной колонки `adapter` нет.
- `AdapterCtx` хранит общий `reqwest::Client`, `base_url`, заголовки, семафоры и таймаут; адаптеры не знают о БД.
- `resolve_image` — чистое преобразование стабильной ссылки в URL с проверкой allowlist; нужно и страницам, и обложкам из поиска.
- `AppState.sources` — `RwLock<Arc<SourceRegistry>>`; guard отпускается до сетевого `.await`, иначе запросы сериализуются. `PUT /api/sources/{code}/config` пересобирает реестр под write-lock, чтобы новые заголовки/куки применились без перезапуска.
- Ошибки — `SourceError` (`thiserror`) с вариантами `UnsupportedKind`, `UnsupportedCapability`, `BadFilter`, `InvalidExternalId`, `NotFound`, `AuthRequired`, `RateLimited`, `Http`, `Parse { stage, detail }`, `Locked`, `Disabled`, `Network`, `Config`; `From<SourceError> for AppError` задаёт HTTP-код и `code` (7.9).

### 7.7. Поиск, фильтры и пагинация

- поиск — возможность, а не обязанность: `Capability::Search`. Источник без поиска нельзя использовать для импорта, пока не появятся popular/latest или ручной ввод `external_id`; такие адаптеры в первых фазах не подключаем.
- запрос: `{ query, cursor?, filters? }`. Ответ: `{ items, next_cursor, total }`; тип элемента — `Option<TitleKind>`, если источник его отдаёт.
- пагинация — непрозрачный `cursor` (номер страницы строкой — допустимая реализация); адаптер сам решает, страница это или offset.
- фильтры — generic-схема в дескрипторе (`Select`, `MultiSelect`, `Range`, `Text`, `TriState`) и JSON-значения в запросе; UI рендерит форму по схеме, адаптер валидирует значения. Если фильтров нет — `Capability::SearchFilters` не выставлен.
- в текущем per-source поиске ошибка возвращается типизированно; при агрегированном поиске (появится позже) на каждый источник будет свой статус, и одна упавшая площадка не сломает выдачу.

### 7.8. HTTP-политика

- свой `User-Agent` и `Referer` у каждого источника, таймаут ~10 с;
- лимиты per-source: 1–2 параллельных запроса на метаданные (поиск, тайтл, главы) и отдельный пул 8–16 на скачивание страниц; учитывать `429` и `Retry-After`; prefetch ограничивать;
- 2 ретрая с backoff на сетевые ошибки, circuit breaker на источник; состояние — в `AppState` по `code`, сбрасывается успешным `probe`;
- текущий per-source поиск возвращает типизированную ошибку; при агрегированном поиске (позже) ошибка одного источника не ломает выдачу — возвращаются частичный результат и статус по каждому;
- URL источника никогда не приходит от клиента: только `source_id`/`code` из реестра и `external_id` из БД; исключение — импорт: `external_id` берётся из ответа поиска, адаптер валидирует и URL-кодирует его; после импорта ссылки только из БД;
- домен API — `sources.base_url`, `image_hosts` и `player_hosts` — из `config_json`; SSRF-правила из §8 применяются к страницам и обложкам одинаково;
- Cloudflare и JS-челленджи заголовками не обходятся: источник либо имеет API/статический HTML, либо не подходит. Проверяйте кандидата через `probe` до написания адаптера.

### 7.9. Ошибки

| `SourceError` | HTTP | `code` |
|---|---|---|
| `UnsupportedKind` | 400 | `source_unsupported_kind` |
| `UnsupportedCapability`, `BadFilter` | 400 | `source_bad_request` |
| `InvalidExternalId` | 400 | `source_invalid_external_id` |
| `NotFound` | 404 | `source_not_found` |
| `Locked` | 403 | `source_locked` |
| `AuthRequired` | 502 | `source_auth_required` |
| `RateLimited` | 503 + `Retry-After` | `source_rate_limited` |
| `Http`, `Parse`, `Network` | 502 | `source_upstream_failed` / `source_parse_failed` |
| `Disabled`, `Config` | 503 | `source_disabled` (детали — в `config_error`) |

`Parse` хранит только `stage` и короткий `detail` (≤ 200 символов), без тела ответа, URL с параметрами и заголовков. Ошибка валидации `PUT .../config` — это не `SourceError`, а 422 с `{ errors: [{ path, message }] }`.

Импорт с `kind`, который источник не поддерживает, — 400 `source_unsupported_kind`; 409 остаётся только для несовпадения `kind` с уже существующим тайтлом. `409 content_kind_unknown` при записи прогресса — это состояние БД, а не ошибка источника: сначала нужно открыть главу через `/content`.

### 7.10. Выбор источника и fallback

Цель — не «основной источник навсегда», а автоматический лучший доступный вариант: самый полный источник для тайтла и fallback на уровне конкретной главы. Полное решение и примеры — [0003-source-selection.md](decisions/0003-source-selection.md).

**Приоритеты.** `source_priorities(lang, source_id, rank)`: меньший `rank` выше; `lang` — ISO 639-1 (`ru`, `en`), `lang = '*'` — запасной набор. Источник не привязан к типу тайтла, поэтому набор один на язык. Набор языка тайтла **полностью заменяет** `*`; `*` применяется, только если для языка нет ни одной строки, иначе rank'и разных наборов смешивались бы. Приоритет решает только ничью по полноте, а не назначает источник жёстко; Mangamir/MangaLib можно добавить позже, когда их API/HTML станут доступны без JS-челленджа.

**Статистика источника.** После импорта и каждого refresh для `source_entry` считаются `chapter_count` — число **уникальных `chapter_key`** без `removed_at` (не строк-вариантов, иначе группы перевода раздувают полноту), `available_count` (у ключа есть хотя бы один не locked/missing вариант; `unknown` считается доступным оптимистично), `locked_count`, `missing_count`, `stats_updated_at`. Это метаданные, по которым тайтл и глава выбираются без сетевых запросов.

**Preferred source тайтла.** Среди включённых `source_entries` побеждает максимальный `chapter_count`; при равенстве — меньший `rank` из `source_priorities` для `titles.lang`; затем `is_primary`; затем `id`. Источник с `available_count = 0` не может быть preferred, пока есть источник с доступными главами; источник без `base_url` или с `config_error` не выбирается; если доступных нет нигде, preferred всё равно выбирается по полноте, а открытие вернёт `source_locked`. Результат — `titles.preferred_source_id`, `preferred_source_updated_at`; `titles.origin_source_id` хранит источник импорта отдельно. Пересчёт — при импорте, refresh, включении/выключении источника, смене приоритетов, отвязке и когда `available_count` падает до нуля.

**Fallback по главе.** Кандидаты — главы с каноническим ключом и главы, чьи ключи связаны с ним через `chapter_aliases`. Для канонического ключа:
1. Вариант из `preferred_source`, если он есть и не помечен свежим `locked`/`missing`.
2. Иначе — остальные варианты по порядку: доступность (`available` → `unknown` → `locked`/`missing`), приоритет источника, `is_primary`, `ordinal_site`, `id`.
3. `Locked`/`NotFound` помечают `availability` и `checked_at`, после чего пробуется следующий вариант; временные `RateLimited`/`Upstream` не меняют доступность, но тоже позволяют попробовать следующий.
4. Успех помечает главу `available`, заполняет `content_kind` и `page_count`; в ответе `fallback` показывает, с какого источника пришлось откатиться и почему.

**Почему preferred по `chapter_count`, а не `available_count`.** Пример пользователя: при равном количестве глав и нескольких locked на Remanga остальные главы должны остаться на Remanga, а locked — уйти на другой источник. Если выбирать по `available_count`, две locked-главы переключили бы весь тайтл. Поэтому полнота — по общему числу глав, доступность — на уровне главы.

**Метаданные вместо постоянных запросов.** `chapters.availability` (`unknown|available|locked|missing`), `availability_checked_at`, `page_count`; `locked` доверяем ~6 ч, `missing` ~24 ч, после чего состояние снова `unknown` и проверяется при открытии или фоновым refresh. `GET /api/titles/{id}` читает только БД; запрос к источнику делается при открытии главы или фоновом обновлении.

**Если глава «сломалась» на страницах.** Бывает, что `content()` вернул список страниц, а картинки отдают `401/403/410` (глава стала платной, подпись протухла). Прокси после перезапроса списка помечает главу `locked`, обновляет статистику и отдаёт `source_locked`; клиент повторяет canonical `/content`, и селектор выбирает следующий источник. Это единственный случай, когда fallback запускается ошибкой страницы, а не ответом `content()`.

**Прогресс и ссылки.** Читалка ходит по `title_id` + `chapter_key`; прогресс и `chapter_reads` — в SQLite по тому же ключу. Смена preferred source и fallback не меняют прогресс. При расхождении нумерации ключи связываются вручную через `chapter_aliases`.

**Ограничение.** Автоматически склеиваются только одинаковые `chapter_key`; безномерные главы и разная база 0/1 требуют alias. Это осознанно: эвристика ошибётся незаметно, а alias виден администратору.

### 7.11. Тесты и приёмка источника

- парсеры — чистые `pub(crate) fn parse_search(&str, ...)`, `parse_chapters`, `parse_content`, `parse_text`; сеть — тонкая обёртка, которая не тестируется юнитами;
- фикстуры в `sources/<code>/fixtures/` через `include_str!`; golden JSON для ключевых DTO;
- обязательный `contract_suite` для каждого адаптера: `descriptor_is_sane` (code, уникальные ключи config/filters, capability ↔ chapters), непустые обязательные поля поисковой выдачи, уникальность `external_id`, допустимость NULL `content_kind` и совпадение известного типа, уникальность `chapter_key` с учётом групп, стабильность ключа при повторном импорте, формат и лимит текста, сборка `config.example.json`;
- live-тесты помечены `#[ignore]` и запускаются вручную; отдельный тест: `chapter_key` не меняется, если источник поправил номер/том; после `chapter_aliases` прогресс переходит на канонический ключ;
- селектор: равное число глав → приоритетный источник; больше глав → более полный; locked-глава → fallback и пометка `availability`; все варианты locked → `source_locked`, без бесконечного цикла; повторный запрос к свежему `locked` не ходит в сеть;
- `GET /api/titles/{id}` не делает сетевых запросов: тест с источником-заглушкой, который считает вызовы;
- перед написанием адаптера — `POST /api/sources/{code}/probe` на реальном сайте: ok/auth/cloudflare/parse.

### 7.12. Чего не делаем сейчас

- динамические расширения (WASM/APK/плагины): компилируемые адаптеры, добавление источника = модуль + пересборка; для закрытого проекта этого достаточно;
- универсальный scraping-DSL: декларативные селекторы можно ввести, когда появится третий простой HTML-источник с той же формой, а не до него;
- обход DMR/descrambling картинок, JS-челленджей и авторизации с капчей; такие источники отклоняются на `probe`;
- архивы (CBZ/PDF) как формат главы: `content_kind` их не поддерживает, такие источники в первых фазах не подключаются;
- ручной ввод `external_id` и источники без поиска: для импорта нужен поиск, иначе адаптер не подключается;
- автоматическое склеивание глав и тайтлов по эвристикам; несопоставленные ключи глав связываются вручную через `chapter_aliases`, тайтлы — через админский API;
- книги и текст — только после появления реального источника, в фазе 7.

## 8. Изображения и файлы

Клиент не ходит на чужие сайты за изображениями. Страницы и обложки идут через сервер:

- `GET /api/pages/{id}/image` берёт разрешённую ссылку из БД, скачивает с нужным `Referer` и стримит байты;
- кэш на диске: `data/cache/images/<aa>/<sha256>.<ext>`, где `aa` — первые два символа хеша; ключ не зависит от подписанного URL;
- запись атомарная: временный файл в том же каталоге + `rename`; одновременные запросы одного ключа дедуплицируются (single-flight), недоскачанный файл читателям не отдаётся;
- рядом с файлом хранится проверенный `Content-Type`; эвикшен по `mtime` не трогает файлы, которые ещё пишутся;
- TTL и лимит размера (например, 5 ГБ), эвикшен — фоновая задача;
- обложки при импорте ресайзятся в до трёх размеров WebP (`image`), аватарки — в WebP; для URL с хешем содержимого допустим `immutable`, для остальных — `ETag` и `304`;
- ссылки, отдаваемые по сессии, кэшируются как `private, max-age=86400`; `public, immutable` — только для действительно публичных ассетов;
- SSRF-защита: сайт — `sources.base_url`, CDN и плееры — из `sources.config_json`, только `http/https` и порты 80/443; отключить автоматические редиректы (`reqwest::redirect::Policy::none()`) или проверять каждый хоп; резолвить имя, проверять **все** полученные IP и соединяться с проверенным (кастомный `reqwest::dns::Resolve` или подключение по IP); отклонять private/loopback/link-local, CGNAT `100.64.0.0/10`, `169.254.169.254`, ULA `fc00::/7` и IPv4-mapped IPv6; лимит тела ~15 МБ;
- проверять магические байты и разрешать только `image/jpeg`, `image/png`, `image/webp`, `image/gif` (AVIF — если включён в `image`); SVG и HTML отклонять, иначе чужой файл исполнится в вашем origin; отдавать с `X-Content-Type-Options: nosniff` и `Content-Disposition: inline`;
- загрузки: лимит 5 МБ, `DefaultBodyLimit` на маршруте, декодирование через `image` с `Limits` (`max_alloc`, `max_image_width/height`, лимит площади пикселей), чтобы сжатая «бомба» не съела память; перекодирование, удаление EXIF, первый кадр для анимаций или отказ; оригинал не отдаётся; декодирование и перекодирование - в `tokio::task::spawn_blocking` с ограничением параллелизма;
- локальные обложки тайтлов — тот же путь загрузки или импорт по разрешённому URL.

## 9. Видео

1. **Первый вариант — встраивание.** В `chapters.embed_url` лежит ссылка, клиент рендерит `<iframe title="..."` с `referrerpolicy="no-referrer"`, `allowfullscreen` и минимальным набором разрешений: `sandbox="allow-scripts allow-same-origin allow-presentation allow-popups allow-forms"`; голый `sandbox` заблокирует плеер. `embed_url` проверяется на импорте и при отдаче: схема только `https`, хост из `player_hosts` конфига источника; `javascript:`/`data:` и чужие хосты отклоняются, CSP `frame-src` — вторая линия, не замена проверке. Выбор источника для видео идёт тем же селектором: preferred source тайтла, затем fallback по главе; `content_kind = 'video'` записывается после первого успешного `content()`. Домены плееров хранить в конфиге источника и генерировать `frame-src`. Учтите: браузер не даёт подставить свой `Referer`, поэтому часть источников встраивание запретит — проверяйте до интеграции.
2. **HLS-прокси — только если источники не дают встраивать.** `hls.js` на клиенте, маршруты `/api/titles/{id}/video/playlist` и `/segment` с непрозрачными токенами вместо сырых URL. Парсить m3u8 и переписывать все URI: варианты master-плейлиста, `EXT-X-KEY`, `EXT-X-MAP`, абсолютные и относительные ссылки; поддерживать `Range`, отдельный allowlist медиа-хостов и лимит кэша сегментов. Для `hls.js` в CSP нужны `worker-src 'self' blob:` и `media-src 'self' blob:`.
3. Полное скачивание и перекодирование видео не делаем.

Прогресс видео хранится в `progress.seconds` и восстанавливается при открытии.

## 10. Авторизация и безопасность

- регистрация только по инвайту; `REGISTRATION_MODE=invite|closed`: в `closed` `POST /api/auth/register` возвращает 403 независимо от инвайта;
- первый администратор создаётся подкомандой CLI или одноразовыми `ADMIN_USERNAME`/`ADMIN_PASSWORD` из env при пустой таблице `users` — иначе создать инвайт некому;
- пароли — Argon2id, минимум 10 символов, одинаковое сообщение об ошибке входа; хеш и проверка - в `tokio::task::spawn_blocking` с семафором, иначе CPU-работа блокирует tokio-воркеры;
- сессии — opaque-токен: 32 случайных байта, в БД только SHA-256, cookie `sid` с `HttpOnly`, `SameSite=Lax`, `Secure` в проде, `Path=/`; `Max-Age`/`Expires` выставляются при входе (иначе cookie сессионная и пропадает при закрытии браузера); idle-срок 30 дней, абсолютный — 90; ротация при входе; logout удаляет запись и выставляет `Set-Cookie` с `Max-Age=0`; `last_seen_at` обновляется с троттлингом;
- инвайт захватывается атомарно в транзакции: `UPDATE invites SET used_by = ? WHERE id = ? AND used_by IS NULL AND (expires_at IS NULL OR expires_at > ?)`, проверка `rows_affected() == 1`, затем вставка пользователя;
- CSRF: same-origin + `SameSite=Lax` + проверка `Origin` у всех POST/PUT/DELETE. Допустимые origin — `PUBLIC_ORIGIN`/`ALLOWED_ORIGINS`: `http://localhost:1420` и `http://127.0.0.1:1420` в dev, один https-домен в проде. Отсутствующий или несовпавший `Origin` отклоняется; дополнительно проверяется `Sec-Fetch-Site`. `COOKIE_SECURE` включается в проде, `X-Forwarded-Proto` доверять только от Caddy;
- отключённый пользователь (`disabled_at`) не проходит middleware сессии, даже если cookie ещё валидна;
- rate limit на login и регистрацию по паре `(IP, username)`, обязателен в фазе 2. За Caddy axum видит `127.0.0.1`: IP брать из `X-Forwarded-For` только если peer — доверенный прокси, иначе из `ConnectInfo`. `tower-governor` видит только запрос без тела: IP-лимит — middleware, лимит по `username` — счётчик в обработчике после разбора JSON;
- админ-роль для источников, импорта и ручного связывания;
- CSP задаётся заголовками: в Caddy для HTML (вариант 2) или в axum (вариант 1), но не в обоих. Инлайновый скрипт темы вынести в `public/theme.js` и подключить блокирующим `<script src>`. Директивы: `default-src 'self'`, `script-src 'self'`, `style-src 'self' 'unsafe-inline'` (Base UI ставит inline-стили), `img-src 'self' data: blob:`, `connect-src 'self'`, `font-src 'self'`, `frame-src` домены плееров, `media-src 'self' blob:`, `worker-src 'self' blob:` (HLS), `frame-ancestors 'none'`, `object-src 'none'`, `base-uri 'self'`, `form-action 'self'`. `frame-src` собирается из `player_hosts` включённых источников только в варианте 1 (axum); в варианте 2 CSP статичен, и новый домен плеера добавляется в Caddyfile. В dev строгий CSP отключается: Vite/React Refresh используют инлайновые скрипты;
- секреты приложения (пароли БД, ключи) — только env, `.env` в dev через `dotenvy`, в git не попадает; учётные данные источников (cookie/token) — в `sources.config_json`, маскируются в API и не логируются;
- логи без паролей, токенов и чужих URL с параметрами; фоновая задача чистит истёкшие сессии.

## 11. Dev: два процесса, пересборки нет

Фронтенд и бэкенд в разработке — независимые процессы. Правка UI не пересобирает Rust.

```powershell
# терминал 1 — сервер (порт 3000)
pnpm dev:server
# или с перезапуском при изменении Rust
cargo watch -x "run --manifest-path server/Cargo.toml"

# терминал 2 — фронтенд с HMR (порт 1420)
pnpm dev
```

`dev:server` — это `node .claude/scripts/run-cargo.mjs run`: скрипт сам подставляет `--manifest-path` и на Windows окружение Visual Studio, прямой `cargo` в обычном shell падает на линковке. Без `cargo watch` правка Rust подхватится только после перезапуска процесса; `cargo watch` на Windows запускай из Developer PowerShell или через vcvars.

В `vite.config.ts`:

```ts
server: {
  port: 1420,
  strictPort: true,
  proxy: {
    "/api": "http://127.0.0.1:3000",
  },
},
```

Что происходит при правках:

| Правка | Что пересобирается | Время |
|---|---|---|
| `.ts`/`.tsx`/CSS | только Vite, HMR в браузере | доли секунды |
| Rust-файл | только крейт `server`; без `cargo watch` процесс надо перезапустить | первые секунды, дальше быстро |
| Релиз | всё сразу | редко, в CI или перед деплоем |

Ключевое ограничение: **не использовать `rust-embed`** для `dist/`. Он вшивает фронтенд в бинарник, и тогда любая правка UI требует пересборки сервера. Вместо него — `ServeDir` с папки `dist/` во время работы. Проверка прод-сборки локально: `pnpm build`, запустить сервер, обновить страницу — Rust не пересобирается. `pnpm build` (с `tsc`) в обычном dev-цикле не нужен, только `pnpm dev`.

CORS не нужен: в dev работает прокси, в проде origin один. Если когда-нибудь понадобится прямой запрос к axum без прокси, CORS включается отдельным env-флагом, а не по `cfg!(debug_assertions)`.

Физическое разделение не обязательно. Один репозиторий и один origin — основной вариант: фронтенд и бэкенд расходятся на два процесса только в разработке. В проде обновление фронтенда — это замена `dist/` (или синхронизация статики в Caddy), без пересборки и, как правило, без перезапуска сервера. Отдельные репозитории, CDN и поддомен для API — опция на будущее, а не требование.

## 12. Production

Целевая платформа — Linux; локальная сборка на Windows даёт `.exe`, поэтому релиз собирается в CI на `ubuntu-22.04` (или под `x86_64-unknown-linux-musl`, если glibc на VPS старый) и публикуется артефактом.

```powershell
pnpm build
node .claude/scripts/run-cargo.mjs build --release   # локальная проверка; релиз — в CI (там обычный cargo)
```

Два варианта отдачи статики:

1. **Всё через axum** — один процесс: `/api` + `ServeDir` из `dist/` с fallback на `index.html` только для навигационных путей. Убедиться, что неизвестный `/api/...` и отсутствующий `/assets/...` возвращают 404, а не `index.html`.
2. **Caddy отдаёт статику, axum — только API.** Быстрее и позволяет обновлять фронтенд без перезапуска сервера. Для одного сервера это удобнее.

Заголовки кэша:

- `dist/assets/*` (хешированные) — `public, max-age=31536000, immutable`;
- `index.html` и навигационные ответы — `no-cache`;
- картинки, отдаваемые по сессии, — `private, max-age=86400` + `ETag`; `public` — только для действительно публичных файлов;
- ответы API с приватными данными — `private, no-store` или короткий `max-age`.

Сжатие — `tower-http` `CompressionLayer` или `encode zstd gzip` в Caddy. HTTP/2/3 и TLS — Caddy автоматически.

Пример Caddyfile:

```
reader.example.com {
  encode zstd gzip
  root * /srv/cion-reader/current/dist

  header {
    X-Content-Type-Options nosniff
    Referrer-Policy strict-origin-when-cross-origin
    X-Frame-Options DENY
    Content-Security-Policy "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; font-src 'self'; frame-src https://player.example; media-src 'self' blob:; worker-src 'self' blob:; frame-ancestors 'none'; object-src 'none'; base-uri 'self'; form-action 'self'"
    -Server
  }
  header /assets/* Cache-Control "public, max-age=31536000, immutable"

  handle /api/* { reverse_proxy 127.0.0.1:3000 }
  handle /assets/* { file_server }
  handle {
    header Cache-Control "no-cache"
    try_files {path} /index.html
    file_server
  }
}
```

SPA-fallback применяется только к навигационным путям: отсутствующий `/assets/*.js` должен давать 404, иначе ошибка деплоя маскируется.

`frame-src` в примере — заглушка: домены плееров берутся из конфига источников. При варианте 2 (Caddy отдаёт HTML) CSP статичен, поэтому новый домен плеера добавляется в Caddyfile; при варианте 1 CSP собирается в axum из конфига.

Фронтенд выкладывается атомарно: `releases/<timestamp>/dist`, сначала загружаются `assets/`, затем `index.html`, последним переключается симлинк `current`. Предыдущий релиз оставлять на диске: открытая вкладка может запросить старый хешированный чанк. `rsync --delete` в единственный каталог не использовать. Изменения API и миграции всё равно требуют перезапуска сервера и бэкапа БД.

systemd: отдельный пользователь и группа, `EnvironmentFile=/etc/cion-reader.env` (там `BIND_ADDR=127.0.0.1:3000`, `DATABASE_URL=sqlite:///srv/cion-reader/data/cion.db`, `DATA_DIR=/srv/cion-reader/data`, `STATIC_DIR=/srv/cion-reader/current/dist`, `PUBLIC_ORIGIN=https://reader.example.com`, `COOKIE_SECURE=true`, `REGISTRATION_MODE=invite`, `RUST_LOG=info`), `Restart=on-failure`. Каталог `data/` создаётся при установке и принадлежит служебному пользователю.

Бэкапы: ежедневно `sqlite3 data/cion.db ".backup ..."`; отдельно копировать только `data/covers` и `data/avatars`, исключая `data/cache` и `cion.db*` (иначе снимок БД будет несогласованным, а кэш раздует бэкап). При восстановлении проверять `PRAGMA integrity_check`. Перед миграциями на деплое делать обязательный бэкап: миграции sqlx по умолчанию необратимы.

CI (GitHub Actions): `pnpm install --frozen-lockfile`, `pnpm lint`, `pnpm build`; `cargo fmt --manifest-path server/Cargo.toml --check`, `cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings`, `cargo test --manifest-path server/Cargo.toml`, `cargo build --release --manifest-path server/Cargo.toml`. В корне нет `Cargo.toml`, поэтому `--manifest-path` обязателен. Если используются макросы `sqlx::query!`, добавить `cargo sqlx prepare` и `SQLX_OFFLINE=true`; с runtime-запросами это не нужно. Docker не обязателен.

## 13. Фазы

Каждая фаза заканчивается работающим состоянием и проверками. Не переходить к следующей, пока не выполнены критерии.

### Фаза 0 — переименование и документы (выполнено)

- [x] `package.json`, `src-tauri/Cargo.toml`, `main.rs`, `tauri.conf.json`, `index.html` → `cion-reader`;
- [x] ключ браузерных настроек → `cion-reader:settings`;
- [x] README, `CLAUDE.md`, `docs/architecture.md` обновлены;
- [x] этот план и решение 0001 добавлены;
- [x] команды `/planner`, `/isplan`, `/isgood`, `/sync-docs`, `/ship` перенесены из `cion-warden` вместе со скриптами и агентами, адаптированы под cion-reader.

Критерий: старых имён нет в конфигах, коде и документации (кроме намеренного упоминания в разделе «Происхождение»); `pnpm lint` и `pnpm build` проходят.

### Фаза 1 — каркас сервера и транспорт (выполнено)

- [x] создать `server/` (axum 0.8), `GET /api/health`, конфиг из env, `tracing`;
- [x] отдача статики из `dist/` с SPA fallback только для навигации;
- [x] заменить блок `server` в `vite.config.ts` (proxy `/api`, убрать `watch.ignored`, `clearScreen`, Tauri-комментарии);
- [x] переписать `src/api/client.ts` на `fetch`; ручной чек-лист (`toError` на строке и объекте, 401, сетевой сбой) записан в `docs/architecture.md`, автотестов фронтенда пока нет;
- [x] убрать Tauri-ветку из `settings.ts` и `ThemeToggle.tsx` (импорты, `syncWindowTheme`, `useEffect`); битый JSON в `readBrowserSettings` логируется, а не молча возвращает defaults;
- [x] `HashRouter` → `BrowserRouter`, `QueryClient` — под HTTP;
- [x] вынести инлайновый скрипт темы из `index.html` в `public/theme.js` (CSP);
- [x] удалить `src-tauri/`, `tauri*` скрипты, `@tauri-apps/*`, Tauri-правила из ESLint; из `.claude/rules/build.md` убрать `pnpm tauri`, `run-vs.mjs` и `--manifest-path` на `src-tauri`, удалить `.claude/scripts/run-vs.mjs` (`vs.mjs` остаётся — `run-cargo.mjs` берёт из него `findVcvars`/`quote`);
- [x] обновить `.gitignore` (`server/target`, `data/`, `.env*`), `README`, `CLAUDE.md`;
- [x] решить судьбу шаблонных экранов: демо-разделы `network`, `apps`, `utils`, `system` и их mock-данные удалены, остались `home`, `changelog`, `settings` (в фазе 3 `home` заменён каталогом); `sections.ts`, `AppRouter`, i18n обновлены синхронно;
- [x] параметрические маршруты (`/title/:id`, `/reader/:titleId/:chapterKey`, `/watch/:titleId/:chapterKey`) держать вне `SectionPath`/`sectionForPath`: отдельный список `PARAM_ROUTES` в `AppRouter`, активный пункт меню — по префиксу пути, меню остаётся статическим; `chapterKey` кодируется через `encodeURIComponent` в `src/lib/routes.ts`;
- [x] добавить `dev:server` в `package.json`;
- [x] обновить `.claude/commands/notes.md` и `.claude/scripts/notes.mjs` под `server/`; новый `server/Cargo.lock` коммитится.

Критерий: приложение открывается в браузере на `:1420` через Vite, `/api/health` отвечает через прокси; после `pnpm build` сервер отдаёт SPA и API на `:3000`; в `src/` нет `@tauri-apps`; навигация использует чистые URL.

### Фаза 2 — БД, авторизация, инвайты (выполнено)

- [x] sqlx + SQLite, миграции, `SqliteConnectOptions` (WAL, `foreign_keys`, `busy_timeout`, `create_if_missing`);
- [x] таблицы `users`, `invites`, `sessions`;
- [x] регистрация по инвайту с атомарным захватом, вход, выход, `GET /api/me`, middleware сессии;
- [x] bootstrap первого администратора (env `ADMIN_USERNAME`/`ADMIN_PASSWORD` при пустой БД);
- [x] `REGISTRATION_MODE=invite|closed`, rate limit login/register (IP + username);
- [x] экраны `login`/`register`, защищённые маршруты, `AuthProvider`; logout и 401 очищают кэш React Query (`queryClient.clear()`); 401 не ретраится и уводит на login;
- [x] тесты: без cookie → 401, чужой `Origin` → 403, лимит входа, инвайт нельзя использовать дважды.

Критерий: регистрация по одноразовому инвайту, вход, сохранение сессии после перезагрузки, выход; в режиме `closed` регистрация возвращает 403; пароли — Argon2; cookie с нужными флагами.

### Фаза 3 — каталог и библиотека без источников (выполнено)

- [x] таблицы `sources`, `source_priorities`, `titles`, `source_entries`, `chapters`, `chapter_aliases`, `pages`, `chapter_texts`, `library`, `progress`, `chapter_reads` — сразу в итоговой форме из §5, чтобы фазы 4–6 не делали `ALTER`; `jobs` добавляется в фазе 7;
- [x] `titles.origin_source_id`/`preferred_source_id`, статистика `source_entries`, `chapters.availability`/`page_count` создаются сразу — фаза 5 только заполняет их;
- [x] `titles.kind` с CHECK; создание и правка тайтла принимают `kind`, смена `kind` при наличии привязанных источников — 409;
- [x] админские эндпоинты создания/правки тайтла и загрузки обложки;
- [x] библиотека: добавить, статус, оценка, удалить;
- [x] каталог, поиск, экраны `catalog`, `title`, `library`;
- [x] заменить/удалить шаблонные экраны и пункты меню.

Дополнительно: `titles.name_lower` (Unicode-поиск), обложки — до трёх WebP-размеров (`low`/`mid`/`high`) в `DATA_DIR/covers/<id>/` с ETag, общая пагинация каталога и библиотеки, тесты `tests/catalog.rs` и `tests/library.rs`.

Критерий: тайтл создаётся вручную, добавляется в библиотеку, поиск находит его по названию; статус и оценка сохраняются; в меню нет демо-разделов.

### Фаза 4 — первый источник, ридер, прокси картинок (выполнено)

- [x] `SourceDescriptor`, `Capability`, нормализованные DTO и `SourceError` из §7; реестр `code -> SourceKind` собирается при старте, битый конфиг отключает только свой источник; `AppState.sources` пересобирается после `PUT` источника или конфига без удержания guard через `.await`;
- [x] источник заводится через `PUT /api/sources/{code}` (base_url, enabled) и `PUT .../config` (merge, 422 со списком полей, предупреждения о неизвестных ключах); `GET /api/sources` показывает все скомпилированные дескрипторы, включая ещё не заведённые, и маскирует секреты;
- [x] первый адаптер — Remanga v2 (`https://api.remanga.org`): `search`, `titles/{dir}`, `titles/chapters/?branch_id=`, `titles/chapters/{id}`; парсеры — чистые функции с фикстурами; `contract_suite` проверяет дескриптор, уникальность ключей и стабильность `chapter_key`;
- [x] `POST /api/sources/{code}/search` — поиск с курсором и фильтрами по схеме дескриптора (у Remanga фильтров нет, `SearchFilters` не выставлен); импорта с отдельным `kind` больше нет: `discover/add` и привязка сами получают метаданные, ответ поиска не считается доверенным;
- [x] поиск, импорт тайтла, импорт глав и страниц с upsert; `chapter_key` при конфликте не перезаписывается, при расхождении пишется `source_entries.last_error`; повторный импорт идемпотентен;
- [x] главы импортируются сразу, страницы — лениво при первом `/content`; полный импорт Naruto (703 главы) укладывается в пагинацию API;
- [x] после импорта/refresh считаются `chapter_count`/`available_count`/`locked_count`/`missing_count` по уникальным ключам; `titles.preferred_source_id` и `origin_source_id` заполняются, функция пересчёта общая с fallback;
- [x] `GET /api/titles/{title_id}/chapters/{chapter_key}/content` для `content_kind = 'images'` (селектор и fallback готовы к фазе 5); `GET /api/chapters/{id}/content` — прямой отладочный вариант; `GET /api/pages/{id}/image` с дисковым кэшем, single-flight и SSRF-защитой; `GET /api/sources/{code}/cover` — тот же allowlist;
- [x] экран `reader` ходит по `/reader/:titleId/:chapterKey` и живёт вне `Layout`: окно страниц по накопленным высотам, prefetch следующих, клавиатура (стрелки, PageUp/PageDown, Home/End; поля ввода и меню не перехватываются), дебаунс прогресса и keepalive-flush на `visibilitychange`/`pagehide` и при размонтировании;
- [x] обработка подписанных CDN-URL: при `401/403/410` не больше двух раз перезапросить страницы главы и обновить `pages.image_url`; если не помогло — глава помечается `locked`, возвращается `source_locked`, клиент повторяет canonical `/content`;
- [x] `POST /api/sources/{code}/probe` — dry-run до и после написания адаптера: ok/auth/cloudflare/parse/error;
- [x] прогресс: `GET/PUT /api/progress/{title_id}` по каноническому `chapter_key`, alias → canonical, `content_kind` берётся у выбранного варианта, несовпадающее поле — 400, неизвестный тип — 409;
- [x] источник-заглушка `fixture` под `cfg(debug_assertions)` для интеграционных тестов: страница тайтла не делает сетевых вызовов, кэш картинок и ETag проверяются на локальном HTTP-сервере.

Дополнительно: `SSRF_ALLOW_LOOPBACK` (по умолчанию `false`) разрешает loopback только тестовому серверу картинок; в проде DNS-резолвер отбрасывает private/loopback/CGNAT/ULA/IPv4-mapped, редиректы у клиентов выключены, allowlist хостов проверяется до запроса. Ключ кэша страницы — `(source_entry_id, external chapter id, idx)`, а не подписанный URL. `SourceTitle.lang` добавлен как аддитивное поле: `titles.lang` нужен для приоритетов источника.

Критерий: реальный тайтл импортируется, глава читается через проксированные картинки, прогресс сохраняется и восстанавливается; падение источника показывается типизированной ошибкой, а не белым экраном; повторное открытие главы не скачивает картинки заново; `contract_suite` и golden JSON зелёные; неподдержанный тип (например, ранобэ RuMix) — 400 `source_unsupported_kind`; неизвестный источник — 404, ошибка парсинга — 502 `source_parse_failed`; обложка из поиска с чужим хостом отклоняется.

### Фаза 5 — второй источник и дедупликация (выполнено)

- [x] второй адаптер намеренно другой формы: **RuMix** — HTML-источник на движке Grouple, другая нумерация (тома, 0-based, «Экстра», без номера), подписанные картинки; контракт, БД и API не менялись — только модуль и дескриптор (решение [0005-second-source-rumix.md](decisions/0005-second-source-rumix.md));
- [x] на нём проверяются `number_text`/`number_milli`/`ordinal_site`, ключи глав без номера, несколько групп перевода и переимпорт без смены `chapter_key`; общий `sources/keys.rs` вынесен из Remanga;
- [x] `source_entries`: связывание с каноническим тайтлом через `POST /api/titles/{id}/sources`, отвязка, `is_primary` (частичный уникальный индекс) как тай-брейкер; статистика полноты и доступности;
- [x] `source_priorities` и админ-экран приоритетов по языку: для русского набора Remanga → RuMix, для другого языка — свой список;
- [x] `titles.preferred_source_id`: максимальный `chapter_count`, ничья — приоритет, затем `is_primary`, затем `id`; источник с `available_count = 0` не выбирается, пока есть доступные; пересчёт при импорте, refresh, включении/выключении, смене приоритетов и отвязке; источники без `base_url`/с `config_error` не выбираются;
- [x] `chapter_aliases`: если ключи одной главы у источников не совпали, админ связывает их; прогресс, группировка **и fallback** резолвят alias → canonical; цепочки запрещены, висячие alias чистятся при отвязке; автоматических эвристик нет;
- [x] fallback по главе: preferred → доступные варианты по приоритету; `Locked`/`NotFound` помечают `availability` и `checked_at`, временные ошибки не меняют доступность; TTL `locked`/`missing`; для отображения выбирается доступный вариант, а не свежий locked preferred;
- [x] объединённый список глав: группировка по каноническому ключу, выбранный вариант и варианты групп; `GET /api/titles/{title_id}/chapters/{chapter_key}/content` делает выбор и fallback; страница тайтла читает только БД;
- [x] админ-экран `/admin` (источники, приоритеты) и панели на странице тайтла: поиск и привязка источника, отвязка, primary, refresh, добавление/удаление aliases, варианты глав.

Дополнительно: `GET /api/titles/{id}` отдаёт `aliases` и `raw chapter_key` варианта; `POST /api/admin/refresh/{title_id}` обновляет главы всех источников с per-entry статусом; `SourceError::Redirect` различает платную главу RuMix и запланированную; `PUT /api/sources/{code}` без `base_url` сохраняет прежний origin; `config_error` персистится при сборке реестра; один `source_entry` на пару `(title_id, source_id)` (миграция 0003); effective enabled учитывает `base_url`/`config_error` в статистике и выбранном варианте; `is_locked` синхронизируется при mark/refresh. Тесты: `tests/multi_source.rs` (приоритеты и `*`, preferred, fallback, alias, группы, attach/detach/primary/refresh, duplicate, выключение) и `tests/live_rumix.rs` (`#[ignore]`, в том числе полный путь через роутер).

Критерий: один тайтл из двух источников даёт одну запись в каталоге и одну в библиотеке; при равном числе глав выбирается приоритетный источник, при большем числе — более полный; locked-глава preferred-источника открывается из другого и помечается в метаданных; повторное открытие не ходит в сеть для проверки свежего `locked`; прогресс сохраняется при переключении и fallback, если ключи совпали или связаны alias; дублей в библиотеке нет; второй адаптер не потребовал правок контракта; главы без номера не потеряны; выключение источника даёт `source_disabled` или fallback, а не 500.

### Фаза 5.5 — библиотека без каталога и ленивый импорт (выполнено)

- [x] сайдбар удалён; корневой экран `/` — библиотека, `/library` редиректит; шапка: бренд, ссылки, поиск по библиотеке по центру, меню пользователя; настройка `sidebarOpen` удалена;
- [x] экран каталога удалён, `GET /api/catalog` остаётся серверным API без UI; ручное создание тайтла перенесено в `/admin` (форма уводит на созданную карточку);
- [x] вкладки библиотеки: статусы `in_progress|planned|favorite|completed|dropped|on_hold|not_interested` («Любимое» — статус, миграция 0006), до трёх своих закладок-статусов, сортировка `updated|added|title|score` и счётчики;
- [x] `library_tabs` (миграция 0004) и `custom_tab_id` в `library` (миграция 0007); CRUD закладок, одна своя закладка на тайтл;
- [x] `GET /api/discover/search` по всем включённым источникам с per-source `errors` и `POST /api/discover/add` (любая сессия, отдельный rate limit): импорт выполняется один раз, затем добавляется запись библиотеки; обложки выдачи доступны с сессией;
- [x] страница тайтла: вкладки «Главы» (первая), «О тайтле» и «Настройки» (только админ); недоступные главы скрыты; поиск, сортировка по тому/номеру, дата и источник; глаз — кнопка отметки прочитанного, с выбором «только эта» или «эта и все предыдущие»; кнопка «Читать»/«Продолжить»/«Прочитано» зависит от прогресса; статус (включая «Любимое» и свои закладки) и оценка; в «Настройках» — порядок источников, alias по каноническим главам, auto-match по номеру и ручная связка; том/номер группы берутся у canonical-главы;
- [x] `PUT /api/progress` пишет `chapter_reads`; `GET /api/titles/{id}` отдаёт `read_keys`, `published_at`, `source_enabled`; карточка библиотеки показывает `read_count/chapter_count`;
- [x] тесты `library_tabs.rs` и `discover.rs`; `lint`, `build`, `test`, `fmt`, `clippy` зелёные.

Критерий: пользователь без роли администратора находит тайтл в источниках и добавляет его в библиотеку; импорт происходит один раз; вкладки и статусы (включая свои закладки) изолированы по пользователю; недоступные главы не показываются; сохранение прогресса отмечает главу прочитанной.

### Фаза 6 — видео

- первый video-адаптер: `content_kind = 'video'`, `SourceContent::Video { embed_url }`; если тип был NULL, после первого `content()` записывается `video`; `/api/chapters/{id}/content` уже отдаёт нужный вариант, отдельного эндпоинта не добавляем;
- экран `watch` ходит по `/watch/:titleId/:chapterKey` и получает лучший вариант через тот же селектор;
- iframe с `sandbox`, `referrerpolicy="no-referrer"`, `allowfullscreen`; домены плееров в конфиге источника, `embed_url` проверяется по `player_hosts` на импорте и при отдаче; подписанные и сессионные embed-ссылки отклоняются на `probe`;
- прогресс в секундах, продолжение просмотра;
- CSP `frame-src` только для разрешённых доменов.

Критерий: видео играет во встроенном плеере, позиция восстанавливается, CSP не блокирует разрешённые домены и блокирует остальные; `content_kind = 'video'` не требует миграции схемы. HLS-прокси — отдельная задача, только при реальной необходимости; отдельную capability для него добавим вместе с ним.

### Фаза 7 — фоновые задачи и полировка

- `jobs` (миграция фазы 7) + воркер: обновление глав, статистики источников, `preferred_source`, prefetch, эвикшен кэша, чистка сессий; в `payload_json` — `kind` и `content_kind`;
- обновление списка глав с mark-and-sweep: полный успешный ответ помечает исчезнувшие главы `removed_at`, частичный — не помечает;
- фоновая проверка доступности по TTL: `locked` ~6 ч, `missing` ~24 ч; после истечения `availability` снова `unknown`, проверка — при открытии или в фоне, без запросов на каждый показ тайтла;
- книги: первый `content_kind = 'text'` адаптер, `SourceContent::Text`, `text()`, `SourceText`, `TextFormat`, `GET /api/chapters/{id}/text`, санитайзинг `ammonia` с allowlist, `chapter_texts` как кэш с TTL и лимитом (≤ 2 МиБ на главу), читалка;
- уведомление о новых главах в интерфейсе;
- настройки ридера (режим, ширина, направление), PWA;
- проверка локализации новых экранов: пропущенные ключи (строки добавляются в `lib/i18n/locales/{en,ru}.ts` в той же фазе, где экран), мобильная вёрстка;
- мониторинг egress и лимиты prefetch/HLS.

Критерий: обновление в фоне добавляет главы; задачи повторяются после ошибки; кэш не растёт бесконечно; ридер и плеер удобны на телефоне.

### Фаза 8 — деплой

- release-сборка в CI под Linux, Caddy, systemd, отдельный пользователь;
- HTTPS, заголовки безопасности, CSP;
- атомарная выкладка `releases/<ts>` + `current`;
- бэкапы и проверенное восстановление;
- CI и порядок выкладки.

Критерий: сайт открывается по HTTPS с телефона и компьютера, переживает перезапуск сервера, восстановление из бэкапа проверено, CI зелёный.

## 14. Риски

| Риск | Что делать |
|---|---|
| Источники блокируют VPS или Cloudflare | адаптеры с нужными заголовками, кэш, фоновые задачи, выбор локации сервера; не обещать обход JS-челленджей, выбирать источники с API/статическим HTML |
| Парсеры ломаются после редизайна | тесты на фикстурах, изоляция адаптеров, быстрый фикс одного модуля |
| Абстракция подстроена под первый источник | дескриптор и capability с первого дня, второй адаптер в фазе 5 намеренно другой формы, `contract_suite` на каждом адаптере |
| Нумерация глав не сравнима между источниками | `number_text` сохраняется как есть, `number_milli` только для сортировки, ключ без номера — стабильный хеш; несопоставленные ключи связываются вручную через `chapter_aliases`, иначе прогресс у каждого свой |
| Источник требует DMR/descrambling или JS-челлендж | `probe` до написания адаптера; такие источники отклоняются, а не обходятся |
| Источник выключен или удалён, а записи в библиотеке остались | `source_disabled` (503) вместо 500; селектор пропускает выключенные и делает fallback, `preferred_source` пересчитывается |
| Preferred source выбран по числу глав, но часть из них locked | fallback по каждой главе; `available_count`/`locked_count` видны в метаданных; при необходимости администратор меняет приоритеты или выключает источник |
| Метаданные доступности устарели | TTL `locked`/`missing`, проверка при открытии и фоновый refresh; ошибка открытия обновляет состояние |
| Fallback-шторм при падении источника | один проход по кандидатам на запрос, отрицательный кэш/backoff, временные ошибки не помечают главу недоступной |
| Смена preferred source ломает ссылки или прогресс | читалка и прогресс по каноническому `chapter_key`; `chapter_id` — только деталь ответа, не идентификатор ссылки |
| Адаптер вернул контент не того типа, что в дескрипторе | runtime-проверка `content_kind` и варианта `SourceContent`; расхождение известного типа — 502 `source_parse_failed`, NULL заполняется фактическим |
| Глава исчезла у источника после DMCA/перезалива | mark-and-sweep только по полному успешному списку: `removed_at`, скрытие в API, прогресс сохраняется |
| Подписанный embed-URL протух внутри iframe | на первом этапе такие источники отклоняются на `probe`; иначе — `embed_ref` и разрешение URL при каждом запросе |
| Трафик VPS упирается в лимит | проксировать страницы по требованию, ограничить prefetch, HLS-кэш с лимитом, мониторить egress |
| Кэш картинок съедает диск | лимит, эвикшен, single-flight, мониторинг размера `data/cache` |
| SSRF через прокси изображений | allowlist хостов и CDN, проверка IP после DNS, запрет редиректов, лимит тела, отклонение private/CGNAT/metadata |
| SQLite и конкурентные записи | WAL, `busy_timeout`, троттлинг `last_seen_at`, один инстанс; при росте — Postgres |
| CSP ломает встраиваемые плееры | whitelist `frame-src`, проверка на каждом новом источнике |
| Подписанные URL протухают | хранить стабильную ссылку/`external_id`, разрешать URL при запросе, перезапрашивать страницы при 401/403/410 (не больше двух раз) |
| Потеря данных | ежедневный бэкап БД и `data/`, проверка восстановления, бэкап перед миграциями |
| Расползание объёма | вертикальные фазы, одна фича за раз, без микросервисов и очередей |

## 15. Definition of done

- в репозитории нет Tauri и `src-tauri/`;
- `server/` собирается в release и отдаёт SPA + API;
- работает вход по инвайту и закрытая регистрация;
- есть bootstrap администратора;
- все `/api`, кроме health и auth, требуют сессию; тест «без cookie → 401» и проверка `Origin` зелёные;
- один источник пройден от поиска до чтения главы;
- библиотека — корневой экран без сайдбара и каталога; тайтл добавляется ленивым импортом из поиска по источникам; в библиотеке работают статусы (включая «Любимое» и свои закладки) и отметка прочитанного;
- второй источник другой формы (иная нумерация или API вместо HTML) связывается с тем же тайтлом без правок контракта, БД и API; прогресс переживает переключение при совпадении ключей или после ручного `chapter_aliases`;
- селектор источников проверен: равное число глав → приоритетный источник, больше глав → более полный, locked-глава → fallback с пометкой доступности; страница тайтла не делает запросов к источникам;
- ридер и плеер сохраняют прогресс;
- тесты адаптеров, авторизации и библиотеки; `fmt` и `clippy` чистые;
- деплой воспроизводим: CI под Linux, systemd, Caddy, атомарная выкладка, бэкапы;
- `CLAUDE.md` и `docs/architecture.md` описывают серверную реальность, а не прошлое.
