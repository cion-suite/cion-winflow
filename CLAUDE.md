# cion-reader

Приватный self-hosted агрегатор манги, аниме и книг: каталог, библиотека и прогресс в одном месте для себя и небольшого круга читателей. React SPA + сервер на Rust (axum).

У проекта две цели:

1. **Продукт** — cion-reader для закрытого круга: авторизация, несколько источников, ридер и плеер.
2. **Обучение** — владелец проекта изучает Rust на практике.

Целевая платформа — веб-сервер (Linux); локальная разработка — Windows.

## Текущее состояние

Фазы 0–2 плана выполнены: `server/` на axum отдаёт API и статику `dist/`, работает SQLite (sqlx, миграции при старте), вход по инвайту, сессии в HttpOnly-cookie и защищённые экраны. Фронтенд ходит в API через `fetch`, навигация — чистые URL. Каталог, источники и ридер — следующие фазы.

- Целевая архитектура, фазы и критерии готовности — [docs/planned-now.md](docs/planned-now.md).
- Причины выбора и альтернативы — [docs/decisions/0001-server-architecture.md](docs/decisions/0001-server-architecture.md).

## Режим обучения

- Когда в коде появляется новая для проекта концепция Rust (владение, заимствование, времена жизни, `Result`/`?`, трейты, `async`), коротко объясняй её на примере этого кода.
- Ошибки компилятора разбирай по сути: что он проверяет и почему код не прошёл. Не обходи их через `clone()`, `unwrap()` или `unsafe` без объяснения.
- На вопрос «как сделать» — объясни и покажи подход. На просьбу «сделай» — сделай и кратко поясни неочевидные решения.
- Предпочитай простой идиоматичный код продвинутым приёмам, если они не нужны для задачи.

## Стек

- Rust edition 2024, axum 0.8, tokio, tower-http; tracing, thiserror.
- Фронтенд: React 19, TypeScript 6, Vite 8
- UI: shadcn/ui (база Base UI, стиль `base-nova`), Tailwind CSS 4
- Данные и навигация: TanStack Query 5, React Router 8 (`BrowserRouter`)
- Язык интерфейса: i18next + react-i18next (en, ru)
- Настройки: `localStorage` (серверные настройки появятся с профилем)
- Проверки: ESLint 10 (`eslint-plugin-boundaries` держит границы слоёв)
- Пакетный менеджер: pnpm

Дальше по плану: sqlx + SQLite, reqwest, scraper, argon2.

## Структура

```
src/
  app/                     запуск: index.tsx (initSettings -> initI18n -> render), AppProvider (BrowserRouter, QueryClient, AuthProvider), AppRouter, RequireAuth, styles/index.css
  shell/                   navbar/ (шапка, пользователь, язык, тема), sidebar/ (меню), layout/
  pages/                   экраны: home, title, reader, watch, changelog, settings, login, register
  api/                     client.ts (request/apiUrl/ApiError), auth.tsx (хуки, AuthProvider), index.ts (области API)
  ui/                      shadcn/ и общие компоненты: PageHeader.tsx, PlaceholderPage.tsx, AuthShell.tsx
  lib/                     utils.ts (cn), settings.ts, routes.ts, authErrors.ts, i18n/
public/theme.js            тема до первой отрисовки; внешний файл для CSP
server/
  Cargo.toml
  migrations/              0001_auth.sql: users, invites, sessions
  src/
    main.rs                конфиг, БД, bootstrap админа, tracing, запуск
    app.rs                 Router, rate limit, слои Origin/session
    auth/                  handlers, service, password, session, middleware, limiter
    config.rs              BIND_ADDR, STATIC_DIR, DATABASE_URL, auth env
    db.rs                  пул SQLite (WAL), миграции при старте
    state.rs               AppState: конфиг, пул, семафор Argon2, лимитер входа
    error.rs               AppError -> формат ошибок API
    static_files.rs        ServeDir + SPA fallback + заголовки кэша
  tests/http.rs            health, API 401/404, SPA fallback
  tests/auth.rs            вход, инвайты, сессии, rate limit
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
- Параметрические маршруты (`/title/:id`, `/reader/:titleId/:chapterKey`, `/watch/:titleId/:chapterKey`) живут в `AppRouter` отдельным списком и не входят в `SectionPath`; ссылки собирай через `src/lib/routes.ts`, чтобы `chapterKey` кодировался.
- Компоненты shadcn не ставятся как библиотека: их код копируется в `src/ui/shadcn/` и принадлежит проекту. Перед написанием своего компонента проверь, нет ли готового в shadcn.
- Цвета бери из токенов темы (`bg-primary`, `text-muted-foreground`), а не задавай вручную: так работает смена темы.
- Алиас `@/` задан в трёх местах: `paths` в `tsconfig.json`, `resolve.alias` в `vite.config.ts` и `aliases` в `components.json`. Меняй их вместе, иначе `shadcn add` без ошибки создаст файлы не в той папке.
- `tsconfig.json` один, `baseUrl` не нужен: в TypeScript 6 он устарел. Документация shadcn для Vite описывает другую схему (`tsconfig.app.json`, `baseUrl`), её не применяй.
- Вызов бэкенда может завершиться ошибкой, всегда показывай её. Транспорт отдаёт `ApiError`, текст берётся из `error.message`.
- Строки интерфейса не пиши в компонентах: добавляй ключи в `lib/i18n/locales/en.ts` и `ru.ts` и бери их через `useT()`. Настройки читай через `useSetting()`, пиши через `settings.set()`; прямой `localStorage` — только в `lib/settings.ts`, `shell/navbar/ThemeToggle.tsx` и `public/theme.js`.
- Пункты меню и статические маршруты задаются таблицей `shell/sidebar/sections.ts`; параметрические экраны (`/title/:id`, `/reader/:titleId/:chapterKey`, `/watch/:titleId/:chapterKey`) живут отдельным списком `PARAM_ROUTES` в `AppRouter`.
- `Button` из Base UI без `type` рендерится как `type="button"` и форму не отправляет: кнопке отправки нужен явный `type="submit"`.
- У `Button` вместо `asChild` проп `render`. Если он рендерит не `<button>`, добавь `nativeButton={false}`. Ссылку так не делай: она получит `role="button"`. Стилизуй её напрямую: `<a className={buttonVariants()} href="...">`.

## Сервер

- HTTP-обработчики тонкие, логика — в сервисах и репозиториях. Ошибки API — в формате `{ code, message }` (`server/src/error.rs`, `AppError` -> `IntoResponse`).
- Неизвестный `/api/*` без сессии — JSON 401, с сессией — JSON 404; отсутствующий `/assets/*` — 404. `index.html` получают только навигационные запросы (`Sec-Fetch-Mode: navigate` или `Accept: text/html`). `GET /api/titles/{id}` и прочие пути с точками в идентификаторах работают.
- Публичные API-пути перечислены явно в `auth/middleware.rs` (`/api/health`, `/api/auth/*`); остальное под `/api` требует сессию. Мутации проверяют `Origin` и `Sec-Fetch-Site`; браузерная cookie `sid` — `HttpOnly`, `SameSite=Lax`, `Secure` по `COOKIE_SECURE`.
- Пароли — Argon2id в `spawn_blocking` с семафором; в БД только хеш. Сессия — 32 случайных байта, в БД SHA-256; idle 30 дней, абсолютный срок 90. Вход ограничен по IP (`tower_governor`) и по username (счётчик в памяти).
- Конфиг — из env: `BIND_ADDR` (по умолчанию `127.0.0.1:3000`), `STATIC_DIR` (`dist`), `DATABASE_URL` (`sqlite://data/cion.db`), `REGISTRATION_MODE` (`invite|closed`), `PUBLIC_ORIGIN`/`ALLOWED_ORIGINS`, `COOKIE_SECURE`, `TRUSTED_PROXIES`, `ADMIN_USERNAME`/`ADMIN_PASSWORD`, `RUST_LOG`; в dev `.env` читает `dotenvy`, пример — `.env.example`.
- Миграции — `server/migrations/`, `sqlx::migrate!` при старте. Схема auth: `users`, `invites`, `sessions`; время — unix seconds (INTEGER).
- Статику отдаёт `ServeDir` с диска; `rust-embed` не используем, иначе правка UI требует пересборки Rust.
- Тесты API — `tower::ServiceExt::oneshot` по `Router` (`server/tests/http.rs`); временные каталоги и SQLite — в тестах, не в проде.
- В production-коде нет `unwrap` / `expect` / `panic!`; причина `unsafe` — в WHY-комментарии.

## Подводные камни

- Для dev нужны оба процесса: без `pnpm dev:server` прокси API отвечает ошибкой.
- Первый администратор создаётся только на пустой таблице `users` из `ADMIN_USERNAME`/`ADMIN_PASSWORD`. После bootstrap переменные убери: пароль остаётся в env и больше не нужен.
- `DATABASE_URL` по умолчанию `sqlite://data/cion.db`; каталог `data/` создаётся сам и в git не попадает. Миграции применяются при каждом старте.
- В dev разрешены Origin `http://localhost:1420`/`127.0.0.1:1420` и те же на `:3000`. Release-сборка без `PUBLIC_ORIGIN`/`ALLOWED_ORIGINS` отклоняет все мутации (403) — это fail closed, а не забытая настройка. В проде также `COOKIE_SECURE=true`.
- Rate limit входа хранится в памяти процесса и сбрасывается при перезапуске; для одного self-host инстанса этого достаточно.
- Сервер раздаёт статику из `dist/`: чтобы проверить SPA на `:3000`, сначала выполни `pnpm build`. После пересборки UI сервер перезапускать не нужно.
- `typescript-eslint` требует TypeScript ниже 6.1: не обновляй `typescript` до 7, пока этот диапазон не расширят.
- `Cargo.lock` (в `server/`) и `pnpm-lock.yaml` коммитятся. `node_modules`, `dist`, `server/target`, `data/`, `.env*` (кроме `.env.example`) игнорируются.
- CSP пока не задаётся: это фаза 8 (заголовки сервера или Caddy). Инлайновый скрипт темы уже вынесен в `public/theme.js`, чтобы позже не требовать `'unsafe-inline'`; в dev строгий CSP отключён ради Vite/React Refresh.
- Ключ темы `theme` читают и `public/theme.js`, и `ThemeToggle.tsx`; меняй их вместе.

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
