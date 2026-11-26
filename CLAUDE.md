# cion-reader

Приватный self-hosted агрегатор манги, аниме и книг: каталог, библиотека и прогресс в одном месте для себя и небольшого круга читателей. React SPA + сервер на Rust (axum).

У проекта две цели:

1. **Продукт** — cion-reader для закрытого круга: авторизация, несколько источников, ридер и плеер.
2. **Обучение** — владелец проекта изучает Rust на практике.

Целевая платформа — веб-сервер (Linux); локальная разработка — Windows.

## Текущее состояние

Фазы 0–5 плана выполнены: `server/` на axum отдаёт API и статику `dist/`, работает SQLite (sqlx, миграции при старте), вход по инвайту, сессии в HttpOnly-cookie и защищённые экраны. Корневой экран — библиотека без сайдбара: вкладки статусов, избранное, до трёх своих закладок, сортировка, поиск по своей библиотеке в шапке. Тайтл добавляется лениво: диалог ищет по всем источникам, `POST /api/discover/add` импортирует тайтл (если его нет на сервере) и сразу кладёт в библиотеку. Страница тайтла делится на вкладки «Главы» (первая) и «Главная»; недоступные главы скрыты, прогресс и прочитанное видны. Администратор создаёт тайтлы вручную в `/admin`. Подключены два источника: Remanga v2 (JSON API) и RuMix (HTML): админские поиск и импорт, привязка к существующему тайтлу, приоритеты по `(kind, lang)`, preferred source, fallback по главам, ручные `chapter_aliases`, выдача контента, прокси картинок с дисковым кэшем и `ETag`, ридер с окном страниц и сохранением прогресса. Следующая фаза — видео.

- Целевая архитектура, фазы и критерии готовности — [docs/planned-now.md](docs/planned-now.md).
- Причины выбора и альтернативы — [docs/decisions/](docs/decisions/) (0001 архитектура, 0004 Remanga, 0005 RuMix).

## Режим обучения

- Когда в коде появляется новая для проекта концепция Rust (владение, заимствование, времена жизни, `Result`/`?`, трейты, `async`), коротко объясняй её на примере этого кода.
- Ошибки компилятора разбирай по сути: что он проверяет и почему код не прошёл. Не обходи их через `clone()`, `unwrap()` или `unsafe` без объяснения.
- На вопрос «как сделать» — объясни и покажи подход. На просьбу «сделай» — сделай и кратко поясни неочевидные решения.
- Предпочитай простой идиоматичный код продвинутым приёмам, если они не нужны для задачи.

## Стек

- Rust edition 2024, axum 0.8 (multipart), tokio, tower-http; tracing, thiserror, image.
- Фронтенд: React 19, TypeScript 6, Vite 8
- UI: shadcn/ui (база Base UI, стиль `base-nova`), Tailwind CSS 4
- Данные и навигация: TanStack Query 5, React Router 8 (`BrowserRouter`)
- Язык интерфейса: i18next + react-i18next (en, ru)
- Настройки: `localStorage` (серверные настройки появятся с профилем)
- Проверки: ESLint 10 (`eslint-plugin-boundaries` держит границы слоёв)
- Пакетный менеджер: pnpm

Дальше по плану: видео (фаза 6) — `content_kind = 'video'`, `embed_url`, плеер `watch` и CSP `frame-src`.

## Структура

```
src/
  app/                     запуск: index.tsx (initSettings -> initI18n -> render), AppProvider (BrowserRouter, QueryClient, AuthProvider), AppRouter, RequireAuth, styles/index.css
  shell/                   navbar/ (шапка, поиск по библиотеке, пользователь, язык, тема), layout/, nav.ts (ссылки шапки и меню)
  pages/                   экраны: library (карточки, вкладки, добавление), title (вкладки, главы), admin, reader, watch, changelog, settings, login, register
  api/                     client.ts (request/apiUrl/ApiError/jsonMutation/requestKeepalive), auth.tsx, catalog.ts, chapters.ts, library.ts, discover.ts, progress.ts, admin.ts, errors.ts (useErrorText), index.ts (области API)
  ui/                      shadcn/ и общие компоненты: PageHeader.tsx, PlaceholderPage.tsx, AuthShell.tsx, TitleForm.tsx, CoverImage.tsx, Pagination.tsx, Textarea.tsx
  lib/                     utils.ts (cn), settings.ts, routes.ts, catalog.ts (типы и статусы), apiErrors.ts, i18n/
public/theme.js            тема до первой отрисовки; внешний файл для CSP
server/
  Cargo.toml
  migrations/              0001_auth.sql: users, invites, sessions; 0002_catalog.sql: каталог, источники, главы, библиотека, прогресс; 0003_source_entry_per_source.sql: один entry на (title_id, source_id); 0004_library_tabs.sql: not_interested, favorite, свои закладки; 0005_title_covers.sql: обложки по размерам low/mid/high
  src/
    main.rs                конфиг, БД, DataDir, bootstrap админа, tracing, запуск
    app.rs                 Router, rate limit, слои Origin/session
    auth/                  handlers, service, password, session, middleware, limiter
    catalog/               service (каталог, тайтл), handlers, cover; selection (приоритеты,
                           fallback), chapters (группировка), content (выдача главы),
                           links (attach/detach, primary, chapter_aliases)
    discover/              поиск по всем источникам и ленивый импорт для библиотеки
    images/                cache (диск, single-flight), handlers (прокси страниц и обложек)
    library/               service (библиотека, избранное, свои закладки), handlers
    progress.rs            прогресс по каноническому chapter_key + chapter_reads
    source_admin/          service (источники, конфиг, поиск, импорт, probe, refresh),
                           priorities (kind/lang), handlers
    sources/               dto, descriptor, config, error, http (SSRF, circuit breaker,
                           POST-форма), keys (общий chapter_key), registry, contract,
                           remanga/ (JSON API), rumix/ (HTML), fixture.rs
    config.rs              BIND_ADDR, STATIC_DIR, DATABASE_URL, DATA_DIR, auth env
    db.rs                  пул SQLite (WAL), миграции при старте
    state.rs               AppState: конфиг, пул, реестр источников, HTTP-клиенты, кэш
    error.rs               AppError -> формат ошибок API (400/403/404/409/422/429/502/503)
    extract.rs             ApiJson/ApiQuery/ApiPath: ошибки разбора тоже { code, message }
    pagination.rs          общие правила page/page_size для списков
    static_files.rs        ServeDir + SPA fallback + заголовки кэша
  tests/http.rs            health, API 401/404, SPA fallback
  tests/auth.rs            вход, инвайты, сессии, rate limit
  tests/catalog.rs         тайтлы, поиск, пагинация, обложки, права admin
  tests/library.rs         библиотека, оценки, изоляция пользователей
  tests/library_tabs.rs    фильтры, счётчики, избранное, свои закладки
  tests/discover.rs        поиск по источникам, ленивый импорт, отметка прочитанного
  tests/sources.rs         источники, импорт, контент, прогресс, кэш картинок, fallback
  tests/multi_source.rs    два источника, приоритеты, preferred, alias, группы, attach/detach
  tests/live_rumix.rs      live-проверка RuMix (ignored)
dist/                      билд фронтенда (в git не коммитится)
```

## Команды

Запускать из корня репозитория. Cargo-команды идут через `.claude/scripts/run-cargo.mjs`: он сам подставляет `--manifest-path server/Cargo.toml` и на Windows окружение Visual Studio.

```powershell
pnpm install                                 # зависимости фронтенда
pnpm dev                                     # Vite :1420 с прокси /api -> :3000
pnpm dev:server                              # axum на 127.0.0.1:3000
pnpm build                                   # проверка типов (tsc) и сборка фронтенда
pnpm lint                                    # ESLint, в том числе границы слоёв
pnpm exec shadcn add <компонент>             # добавить компонент shadcn

pnpm test                                    # cargo test (server)
pnpm check:rust                              # fmt --check + clippy -D warnings
```

Перед завершением задачи с Rust-кодом прогоняй `test`, `fmt --check` и `clippy`, с фронтендом — `pnpm lint` и `pnpm build`.

## Команды разработки

- `/planner <задача>` — фазовый план с quality gates и DRY-контрактом.
- `/isplan [файл]` — adversarial review плана (по умолчанию `docs/planned-now.md`).
- `/isgood [diff]` — adversarial review диффа и fix-loop.
- `/sync-docs` — сверка ссылок в документации с репозиторием.
- `/ship` — pre-flight перед merge: typecheck, lint, Rust-тесты, fmt/clippy, скан, циклы, build.
- `/notes` — changelog и commit-сообщение.

Правила для ревью — `.claude/rules/`, механика — `.claude/scripts/`, профили — `.claude/agents/`. Гейт после каждой фазы: `typecheck -> lint -> test -> check:rust -> /isgood -> /notes`.

## Фронтенд

Раскладка и правила импортов — в `docs/architecture.md`. Кто кого может импортировать — таблица там же, это проверяет `pnpm lint`.

- Все экраны лежат в `src/pages/<имя>/`. Экран импортируется только из роутера `src/app/AppRouter.tsx`.
- `fetch` есть только в `src/api/client.ts`; там же `request<T>`, `apiUrl` и `ApiError`. `src/api` импортируют `pages`, `shell` и `app`, но не `ui`.
- Путь для `request` задаётся относительно префикса API: `request("/health")` -> `/api/health`. Ошибки сервера — `{ code, message }`; сетевой сбой — `ApiError` со `status: 0`.
- Снаружи `src/api` импортируется только через `@/api` (хуки, `AuthProvider`) и `@/api/client` (`request`, `ApiError`) — это проверяет `eslint-plugin-boundaries`.
- Авторизация: `useAuth()` даёт `user`, `isLoading`, `error`; защищённые экраны обёрнуты в `<RequireAuth>` в `AppRouter`, гость уходит на `/login`. 401 от запросов чистит кэш React Query и возвращает на вход. `/login` и `/register` живут вне `Layout`.
- Параметрические маршруты: `/title/:id` — в `Layout`; `/reader/:titleId/:chapterKey` и `/watch/:titleId/:chapterKey` — вне `Layout`, во весь экран, но под `RequireAuth`. Корневой `/` — библиотека, `/library` редиректит на неё. Ссылки собирай через `src/lib/routes.ts`, чтобы `chapterKey` кодировался.
- Библиотека — единственный основной экран: вкладки (статусы, избранное, свои закладки), сортировка, карточки с прогрессом `read_count/chapter_count`. Список — `useLibraryQuery` (`q`, `status`, `favorite`, `tabId`, `sort`); счётчики вкладок приходят в `counts`, свои закладки — из `useLibraryTabsQuery`. Своих закладок не больше трёх; название уникально у пользователя.
- Добавление тайтла — диалог на библиотеке: `useDiscoverSearchQuery` ищет по всем включённым источникам, `useAddDiscoveredTitleMutation` импортирует тайтл при необходимости и добавляет его в библиотеку, после успеха открывается `/title/:id`. Обложки выдачи — `sourceCoverUrl`, через серверный прокси, а не напрямую с источника.
- Поиск в шапке (`LibrarySearch`) — `useLibraryQuery` с `q` и малым `page_size`; ищет только по своей библиотеке.
- Читалка ходит только в canonical `/content` (никогда в `/api/chapters/{id}/content`), картинки берёт через `pageImageUrl` (`/api/pages/{id}/image`), прогресс — через `useProgressQuery`/`useSaveProgressMutation`; flush при закрытии вкладки — `requestKeepalive` (PUT, не `sendBeacon`). Смена главы пересоздаёт состояние через `key`. Прогресс восстанавливай по каноническому `content.chapter_key`, а не по ключу из URL.
- Страница тайтла: первая вкладка — «Главы», вторая — «Главная». В списке скрывай главы, у которых нет рабочего варианта (`locked`/`missing`), сортируй по тому/номеру, фильтруй по названию; отметка прочитанного — по `read_keys`. Кнопка «Читать» продолжает с прогресса, иначе открывает первую доступную главу. «Любимое» — `favorite` в библиотеке, независимый от статуса; оценка пользователя — `score`, рейтинг источника — `title.rating`.
- Админка: `/admin` — приоритеты `(kind, lang)`, состояние источников, `base_url`, enable/disable, probe, импорт из конкретного источника и ручное создание тайтла (форма уводит на созданную карточку). Пункт меню скрыт для не-админа, но сервер всё равно требует роль. На странице тайтла `AdminSourcePanel` делает поиск/привязку/отвязку источника, primary и refresh, `AliasesPanel` — alias-ключи. Обычному пользователю эти панели не монтируются, иначе админские хуки дадут 403.
- `GET /api/titles/{id}` отдаёт `aliases`, `read_keys`, `published_at` и `source_enabled` у вариантов, `title.cover_sizes` и `title.manual_cover_sizes`, `library.{status,score,favorite,tab_ids}`; у варианта главы есть собственный `chapter_key` (может отличаться от canonical). Выбранный вариант — первый доступный, а не свежий locked preferred. После attach/detach/primary/alias/refresh инвалидируй `title`, `library`, `library-tabs`, `chapter-content` и `progress` тайтла.
- Обложки: `titleCoverUrl(title, size)`, где `low` — выдача поиска и диалог добавления (у источников это `cover_ref` из выдачи, через `sourceCoverUrl`), `mid` — карточки библиотеки, `high` — страница тайтла. Админский `CoverEditorDialog` на странице тайтла загружает и удаляет каждый размер отдельно; занятые слоты — `title.cover_sizes`, отсутствующий размер сервер отдаёт из ближайшего. Кнопка «Получить из источника» (`useFetchSourceCoversMutation`) заново скачивает обложки привязанных источников; если есть ручные размеры (`title.manual_cover_sizes`), сначала спрашивает, заменять ли их.
- Компоненты shadcn не ставятся как библиотека: их код копируется в `src/ui/shadcn/` и принадлежит проекту. Перед написанием своего компонента проверь, нет ли готового в shadcn.
- Цвета бери из токенов темы (`bg-primary`, `text-muted-foreground`), а не задавай вручную: так работает смена темы.
- Алиас `@/` задан в трёх местах: `paths` в `tsconfig.json`, `resolve.alias` в `vite.config.ts` и `aliases` в `components.json`. Меняй их вместе, иначе `shadcn add` без ошибки создаст файлы не в той папке.
- `tsconfig.json` один, `baseUrl` не нужен: в TypeScript 6 он устарел. Документация shadcn для Vite описывает другую схему (`tsconfig.app.json`, `baseUrl`), её не применяй.
- Вызов бэкенда может завершиться ошибкой, всегда показывай её. Транспорт отдаёт `ApiError`, текст берётся из `error.message`; для известных кодов используй `useErrorText` (`api/errors.ts`), он переводит их через `lib/apiErrors.ts`.
- Библиотека и закладки ходят через `api.library`, поиск/добавление — через `api.discover`, тайтл — через `api.catalog`; строковые статусы и типы (`manga|anime|book`, статусы тайтла и библиотеки) — константы в `lib/catalog.ts`, не дублируй их литералами.
- Строки интерфейса не пиши в компонентах: добавляй ключи в `lib/i18n/locales/en.ts` и `ru.ts` и бери их через `useT()`. Настройки читай через `useSetting()`, пиши через `settings.set()`; прямой `localStorage` — только в `lib/settings.ts`, `shell/navbar/ThemeToggle.tsx` и `public/theme.js`.
- Сайдбара нет. Шапка — логотип со ссылкой на `/`, поиск и меню пользователя; его пункты задаёт таблица `shell/nav.ts` (`MENU_NAV`). Статические экраны и параметрические маршруты (`/title/:id`, `/reader/:titleId/:chapterKey`, `/watch/:titleId/:chapterKey`) перечислены списками `PAGES`/`PARAM_ROUTES` в `AppRouter`.
- `Button` из Base UI без `type` рендерится как `type="button"` и форму не отправляет: кнопке отправки нужен явный `type="submit"`.
- У `Button` вместо `asChild` проп `render`. Если он рендерит не `<button>`, добавь `nativeButton={false}`. Ссылку так не делай: она получит `role="button"`. Стилизуй её напрямую: `<a className={buttonVariants()} href="...">`.

## Сервер

- HTTP-обработчики тонкие, логика — в сервисах и репозиториях. Ошибки API — в формате `{ code, message }` (`server/src/error.rs`, `AppError` -> `IntoResponse`). Для JSON/Query/Path бери `ApiJson`/`ApiQuery`/`ApiPath` из `extract.rs`: стандартные экстракторы axum отвечают plain text.
- Неизвестный `/api/*` без сессии — JSON 401, с сессией — JSON 404; отсутствующий `/assets/*` — 404. `index.html` получают только навигационные запросы (`Sec-Fetch-Mode: navigate` или `Accept: text/html`). `GET /api/titles/{id}` и прочие пути с точками в идентификаторах работают.
- Публичные API-пути перечислены явно в `auth/middleware.rs` (`/api/health`, `/api/auth/*`); остальное под `/api` требует сессию. Мутации проверяют `Origin` и `Sec-Fetch-Site`; браузерная cookie `sid` — `HttpOnly`, `SameSite=Lax`, `Secure` по `COOKIE_SECURE`.
- Пароли — Argon2id в `spawn_blocking` с семафором; в БД только хеш. Сессия — 32 случайных байта, в БД SHA-256; idle 30 дней, абсолютный срок 90. Вход ограничен по IP (`tower_governor`) и по username (счётчик в памяти).
- Каталог: `GET /api/catalog` (поиск по `titles.name_lower` — Unicode-копия названия, `%`/`_` экранируются; фильтр `kind`, пагинация) остаётся серверным API, но своего экрана в UI больше нет. `GET /api/titles/{id}` -> `{ title, library, chapters, sources, aliases, read_keys }`; `POST /api/titles`, `PUT /api/titles/{id}`, `POST /api/titles/{id}/cover` — admin. Ручное создание тайтла живёт в `/admin`.
- Библиотека: `GET /api/library?q=&status=&favorite=&tab_id=&sort=&page=&page_size=` (`{ items, total, page, page_size, counts }`), `PUT/DELETE /api/library/{title_id}` для текущего пользователя. Поля записи: `favorite`, `read_count`, `chapter_count`, `tab_ids`. Статусы — `planned|in_progress|completed|on_hold|dropped|not_interested`; `favorite` независим от статуса. `sort` — `updated|added|title|score`. Пагинация общая — `pagination::resolve`.
- Свои закладки: `GET/POST /api/library/tabs`, `PUT/DELETE /api/library/tabs/{id}`, `PUT /api/library/{title_id}/tabs` (`{ tab_ids }` заменяет состав). Не больше трёх у пользователя, имя уникально; закладка ссылается только на тайтлы из библиотеки (составной FK).
- Поиск по источникам для пользователя: `GET /api/discover/search?q=&kind=` (включённые источники с `Search` и `Chapters`, ошибки в `errors`) и `POST /api/discover/add` (`{ code, external_id, kind }`): импортирует тайтл тем же кодом, что админский импорт, если `source_entry` ещё нет, и добавляет запись в библиотеку со статусом `planned`; у уже импортированного тайтла без обложек добирает их из источника. Роль admin не нужна; `add` под отдельным rate limit. Обложка выдачи `GET /api/sources/{code}/cover?ref=` доступна любому пользователю с сессией.
- Обложки: `title_covers` хранит до трёх размеров — `low` (поиск и диалог добавления), `mid` (карточки библиотеки), `high` (страница тайтла). `POST/DELETE /api/titles/{id}/cover?size=low|mid|high`: multipart `file` до 5 МиБ, JPEG/PNG/WebP/GIF, `image::Limits` в `spawn_blocking` с семафором, WebP в `DATA_DIR/covers/{id}/{size}.webp`, атомарная запись tmp + rename. `GET` отдаёт `ETag`, `Cache-Control: private` и при отсутствии размера — ближайший сохранённый (сначала более крупный, затем мелкий); UI добавляет `v={updated_at}`. При импорте и привязке источника обложки адаптера (`SourceCovers` low/mid/high) скачиваются автоматически, но только в пустые размеры — ручную не перетирают; сбой загрузки не отменяет импорт. Явная admin-операция `POST /api/titles/{id}/cover/from-source` (`{ replace_manual? }`) перебирает привязанные источники в порядке preferred → primary → привязка и обновляет source-размеры, а ручные (`source_id IS NULL`, включая строки 0005) — только с `replace_manual=true` (запись без файла на диске лечится всегда); ответ `{ sources, imported, skipped_manual, errors }`, ошибки одного источника не отменяют остальные. `GET /api/titles/{id}` отдаёт `title.cover_sizes` и `title.manual_cover_sizes`.
- Источники: дескриптор — данные (`sources/descriptor.rs`), dispatch — `enum SourceKind`, реестр собирается из таблицы `sources` в `AppState.sources` (`Arc<RwLock<SourceRegistry>>`) и пересобирается после `PUT`; guard не держится через `.await`. Адаптеры не знают о БД. Битый конфиг отключает только свой источник и персистится в `sources.config_error`; preferred его пропускает. Remanga — `sources/remanga/` (JSON API, `external_id` тайтла — `dir`, главы — числовой `id`, `branches`). RuMix — `sources/rumix/` (HTML, `external_id` — slug, главы — `data-id`, поиск POST-формой, `base_url` — origin сайта, UA обязан начинаться с `Mozilla/5.0`, картинки подписаны, 18+ через `?mtr=true`). `chapter_key` строит общий `sources/keys.rs`. Парсеры — чистые функции с фикстурами и `contract_suite`.
- Приоритеты: `source_priorities(kind, lang, source_id, rank)`, ранг 0-based, `lang='*'` — запасной **только если для языка тайтла нет ни одной строки** (наборы не смешиваются). `GET/PUT /api/admin/source-priorities` заменяет список целиком и пересчитывает preferred у всех тайтлов типа. `recompute_preferred` учитывает effective enabled (`enabled`, `base_url IS NOT NULL`, `config_error IS NULL`), затем `chapter_count`, rank, `is_primary`, `id`; пересчёт также после `save_config`, `update_title(lang)`, detach и attach (заполняет `titles.lang`, если NULL).
- Alias и fallback: `candidates()` включает главы с canonical-ключом **и** alias-варианты, поэтому fallback видит связанную главу другого источника. Цепочки alias запрещены; при отвязке удаляются alias, чей alias-ключ исчез (canonical-only alias сохраняется как стабильный ключ прогресса). `GET /api/titles/{id}` отдаёт `aliases` и raw `chapter_key` варианта; для отображения выбирается первый доступный вариант рабочего источника. `is_locked` синхронизируется с availability при mark/успешном refresh страниц.
- Инвариант источников: один `source_entry` на `(title_id, source_id)` (уникальный индекс 0003); повторная привязка другого `external_id` того же источника к тому же тайтлу — 409 `source_already_attached`.
- Админские связи: `POST/DELETE /api/titles/{id}/sources`, `PUT /api/titles/{id}/primary-source` (`{ source_entry_id }`), `GET/POST/DELETE /api/titles/{id}/chapter-aliases`, `POST /api/admin/refresh/{title_id}` (per-entry статус, сеть до транзакции; заодно добирает отсутствующие обложки источника — так их получают тайтлы, импортированные до автоскачивания). `PUT /api/sources/{code}` без `base_url` сохраняет прежний origin.
- Импорт: `POST /api/sources/{code}/import` (`{ external_id, kind, title_id? }`) заново получает метаданные, главы пишет upsert в транзакции; `chapter_key` фиксируется при первом импорте, расхождение — в `source_entries.last_error`. После импорта считаются статистики и `titles.preferred_source_id`.
- Контент главы: `GET /api/titles/{title_id}/chapters/{chapter_key}/content` выбирает вариант и делает fallback (`source_locked` 403, `source_not_found` 404, временные — 502/503); известные страницы отдаются из БД без сети. `GET /api/chapters/{id}/content` — прямой вариант. Прогресс: `GET/PUT /api/progress/{title_id}`, alias → canonical, поле позиции проверяется по `content_kind`; `PUT` также пишет `chapter_reads` по каноническому ключу (`read_count` и отметки глав).
- `SourceError::Redirect` (3xx без следования): RuMix различает платную главу (`/internal/purchase/...` → `Locked`) и запланированную (`NotFound`); остальные 3xx — 502. Общий `get_text` редиректы не выполняет.
- Картинки: `GET /api/pages/{id}/image` и `GET /api/sources/{code}/cover?ref=` (вторая — любому пользователю с сессией для диалога добавления). Кэш `DATA_DIR/cache/images/<aa>/<sha256>` по ключу `(source_entry_id, external chapter id, idx)`, атомарная запись, single-flight, магические байты (JPEG/PNG/WebP/GIF), `ETag`, `nosniff`, `private`. При `401/403/410` до двух перезапросов списка страниц, затем глава помечается `locked`.
- SSRF: общий DNS-резолвер отбрасывает private/loopback/link-local/CGNAT/ULA/IPv4-mapped, редиректы клиентов выключены, allowlist хостов проверяется до запроса. URL источника — только из реестра; `external_id` валидируется. `SSRF_ALLOW_LOOPBACK` включается только интеграционными тестами с локальным сервером.
- Конфиг — из env: `BIND_ADDR` (по умолчанию `127.0.0.1:3000`), `STATIC_DIR` (`dist`), `DATABASE_URL` (`sqlite://data/cion.db`), `DATA_DIR` (`data`), `REGISTRATION_MODE` (`invite|closed`), `PUBLIC_ORIGIN`/`ALLOWED_ORIGINS`, `COOKIE_SECURE`, `TRUSTED_PROXIES`, `ADMIN_USERNAME`/`ADMIN_PASSWORD`, `RUST_LOG`; в dev `.env` читает `dotenvy`, пример — `.env.example`.
- Миграции — `server/migrations/`, `sqlx::migrate!` при старте. Схема: `users`, `invites`, `sessions`; каталог и пользовательские данные — `sources`, `titles`, `title_covers`, `source_entries`, `chapters`, `chapter_aliases`, `source_priorities`, `pages`, `chapter_texts`, `library`, `library_tabs`, `library_tab_items`, `progress`, `chapter_reads`; время — unix seconds (INTEGER). `library` пересобирается в 0004 ради нового CHECK (`not_interested`) и `favorite`.
- Статику отдаёт `ServeDir` с диска; `rust-embed` не используем, иначе правка UI требует пересборки Rust.
- Тесты API — `tower::ServiceExt::oneshot` по `Router` (`server/tests/`); временные каталоги, SQLite и `DATA_DIR` — в тестах, не в проде.
- В production-коде нет `unwrap` / `expect` / `panic!`; причина `unsafe` — в WHY-комментарии.

## Подводные камни

- Сайдбара и экрана каталога больше нет: `/` — библиотека, `/library` редиректит на `/`. Серверный `GET /api/catalog` остался, но UI его не использует.
- Добавление тайтла идёт через `/api/discover/*`: поиск по всем включённым источникам, импорт — при первом добавлении. Импорт полного тайтла — серия запросов и может занять секунды; `add` ограничен по IP.
- «Любимое» — флаг `favorite`, а не статус: он не мешает «Читаю»/«Прочитано». «Не интересно» — полноценный статус. Своих закладок не больше трёх, имя уникально; в закладку можно положить только тайтл из библиотеки.
- Настройки браузера теперь хранят только `locale`; старый `sidebarOpen` игнорируется.
- Для dev нужны оба процесса: без `pnpm dev:server` прокси API отвечает ошибкой.
- Первый администратор создаётся только на пустой таблице `users` из `ADMIN_USERNAME`/`ADMIN_PASSWORD`. После bootstrap переменные убери: пароль остаётся в env и больше не нужен.
- `DATABASE_URL` по умолчанию `sqlite://data/cion.db`, `DATA_DIR` — `data`; каталоги создаются сами и в git не попадают. Миграции применяются при каждом старте; `data/covers/` хранит обложки.
- В dev разрешены Origin `http://localhost:1420`/`127.0.0.1:1420` и те же на `:3000`. Release-сборка без `PUBLIC_ORIGIN`/`ALLOWED_ORIGINS` отклоняет все мутации (403) — это fail closed, а не забытая настройка. В проде также `COOKIE_SECURE=true`.
- Rate limit входа хранится в памяти процесса и сбрасывается при перезапуске; для одного self-host инстанса этого достаточно.
- Сервер раздаёт статику из `dist/`: чтобы проверить SPA на `:3000`, сначала выполни `pnpm build`. После пересборки UI сервер перезапускать не нужно.
- `typescript-eslint` требует TypeScript ниже 6.1: не обновляй `typescript` до 7, пока этот диапазон не расширят.
- `Cargo.lock` (в `server/`) и `pnpm-lock.yaml` коммитятся. `node_modules`, `dist`, `server/target`, `data/`, `.env*` (кроме `.env.example`) игнорируются.
- CSP пока не задаётся: это фаза 8 (заголовки сервера или Caddy). Инлайновый скрипт темы уже вынесен в `public/theme.js`, чтобы позже не требовать `'unsafe-inline'`; в dev строгий CSP отключён ради Vite/React Refresh.
- Ключ темы `theme` читают и `public/theme.js`, и `ThemeToggle.tsx`; меняй их вместе.
- Источник нужно завести и включить через `PUT /api/sources/{code}` (`base_url`, `enabled`) до поиска и импорта. У Remanga `base_url` — API-хост `https://api.remanga.org`, а не сайт; `dir` тайтла содержит `<...>` и обслуживается только закодированным в пути. У RuMix `base_url` — сайт `https://rumix.me` (не `http://` и не `www`); без UA с префиксом `Mozilla/5.0` сайт отвечает 402, поэтому в конфиге есть `user_agent`. Импорт полного тайтла — это серия запросов, он может занять секунды.
- RuMix не помечает платные главы в списке: `content()` получает 302 на `/internal/purchase/` и возвращает `source_locked`, после чего fallback идёт в другой источник. На импорте такие главы остаются `unknown` (ленивое определение). Картинки на `*.rumix.me` подписаны и протухают — прокси перезапрашивает `content()` при 401/403/410.
- `source_priorities` заменяется целиком: `PUT` с `source_codes` в нужном порядке. Ранг в API 0-based. После сохранения preferred пересчитывается, но `is_primary` не меняется. Alias — один hop: canonical не может сам быть alias, alias не может быть canonical другой связи; иначе 409 `alias_chain`.
- `page` в прогрессе — 0-based индекс `pages.idx`, не человеческий номер страницы. `PUT /api/progress/{title_id}` до первого открытия главы с `content_kind = NULL` вернёт 409 `content_kind_unknown`.
- Тестовый адаптер `fixture` компилируется только в `debug_assertions`; в release его нет. `SSRF_ALLOW_LOOPBACK=true` существует только для тестов с локальным HTTP-сервером.

## Происхождение

Проект начинался как десктопный шаблон `cion-winflow` на Tauri. Теперь это продукт `cion-reader`: фронтенд переехал в веб, Tauri-слой удалён. История осталась в git.

## Документация для ИИ

- Этот файл — точка входа: только то, что нужно почти в каждой задаче. Держи его коротким.
- Подробности выноси в `docs/` и ставь сюда ссылку. Папку и файлы создавай, когда появляется содержимое:
  - `docs/architecture.md` — устройство фронтенда и сервера;
  - `docs/planned-now.md` — целевая архитектура и порядок переезда;
  - `docs/decisions/NNNN-название.md` — принятые решения и их причины;
  - `docs/learning/` — конспекты по Rust;
  - `.claude/rules/` — правила ревью и навигации; `.claude/commands/` — рабочие команды.
- Лог версий для пользователей (`docs/versions/release.md`) пишет только команда `/notes`, руками его не правь. Порядок релиза — в `.claude/commands/notes.md`.
- Если изменение делает документацию неверной, обновляй её в той же задаче.
- Записывай только проверенные факты и не описывай то, чего ещё нет в коде.
