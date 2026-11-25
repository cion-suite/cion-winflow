# Устройство приложения

Серверная версия: React SPA в `src/` и сервер `server/` на axum. В разработке — два процесса (Vite и axum), в проде — один origin. Целевая архитектура и фазы — [planned-now.md](planned-now.md).

## Слои фронтенда

Код `src/` разложен по слоям. Импорты между файлами `.ts` и `.tsx` проверяет `pnpm lint`. Имена файлов линтер не проверяет, это договорённость.

| Слой | Что лежит | Сейчас в приложении |
|---|---|---|
| `app/` | запуск: `index.tsx`, провайдеры, маршрутизатор, глобальные стили | `AppProvider.tsx`, `AppRouter.tsx`, `styles/index.css` |
| `shell/` | оболочка окна: то, что не принадлежит ни одному экрану | `layout/`, `navbar/` (шапка, язык, тема), `sidebar/` |
| `pages/<имя>/` | экраны, по папке на экран | `catalog/`, `title/`, `library/`, `reader/`, `watch/`, `changelog/`, `settings/`, `login/`, `register/` |
| `api/` | HTTP-транспорт и хуки областей | `client.ts`, `auth.tsx`, `catalog.ts`, `library.ts`, `errors.ts`, `index.ts` (области собираются здесь) |
| `ui/` | `shadcn/` и мелкие общие компоненты | `shadcn/`, `PageHeader.tsx`, `PlaceholderPage.tsx`, `AuthShell.tsx`, `TitleForm.tsx`, `CoverImage.tsx`, `Pagination.tsx`, `Select.tsx`, `Textarea.tsx` |
| `lib/` | утилиты, хуки и типы без UI | `utils.ts`, `settings.ts`, `routes.ts`, `catalog.ts`, `apiErrors.ts`, `i18n/` |

Файл `.ts` или `.tsx` вне этих папок — ошибка линтера (`boundaries/no-unknown-files`).

## Кто кого импортирует

| Слой | Может импортировать |
|---|---|
| `app` | `shell`, `pages`, `api`, `ui`, `lib` |
| `shell` | `api`, `ui`, `lib` |
| `pages` | `api`, `ui`, `lib` |
| `api` | `lib` |
| `ui` | `lib` |
| `lib` | ничего из этих слоёв |

Внутри своей папки импортировать можно всё. Снаружи `api` доступен только через `@/api` и `@/api/client`.

## Куда положить код

- Нужен одному экрану — в папку этого экрана.
- Компонент нужен нескольким экранам — в `ui/`, данные он получает пропсами.
- Хук или утилита нужны нескольким экранам — в `lib/`.
- Логика приложения — в серверный сервис или адаптер источника.

## Экраны и меню

Экран — `pages/<имя>/index.tsx`, рядом лежат его компоненты. Файл называется `index`, а компонент — по экрану (`export function CatalogPage`), импорт выглядит так: `@/pages/catalog`. Импортировать экран может только `app`, экраны друг друга не импортируют.

Пункты меню и их порядок — единая таблица `shell/sidebar/sections.ts`. Она же задаёт `SectionPath`, а роутер (`app/AppRouter.tsx`) требует экран для каждого пути кроме корневого: забытый маршрут не скомпилируется. Корневой маршрут — `index` (`/`, каталог), поэтому в таблице у него путь `/`.

| Группа | Заголовок | Экраны |
|---|---|---|
| `overview` | Обзор | Каталог (`/`), Библиотека (`/library`) |
| `footer` | без заголовка | Что нового, Настройки |

`SidebarGroupLabel` сам скрывает заголовок, когда сайдбар свёрнут до иконок. Свернуть сайдбар можно кнопкой в шапке или `Ctrl+B`; состояние сохраняется в настройках.

Параметрические маршруты (`/title/:id`, `/reader/:titleId/:chapterKey`, `/watch/:titleId/:chapterKey`) не входят в `SectionPath` и `sectionForPath`: они перечислены отдельным списком `PARAM_ROUTES` в `AppRouter`. Меню остаётся статическим, активный пункт ищется по префиксу пути, а заголовок в шапке — через `pageTitleKey`. Ссылки на эти экраны собирай через `src/lib/routes.ts`: `chapterKey` содержит `:` и `.`, поэтому кодируется `encodeURIComponent`.

## Авторизация

Сервер выдаёт сессию в HttpOnly-cookie `sid`. `AuthProvider` (`api/auth.tsx`) один раз запрашивает `GET /api/me`:

- `null` — гость: 401 превращается в `null`, это не ошибка;
- `user` — вошедший пользователь.

`AppRouter` делит маршруты на публичные (`/login`, `/register`, вне `Layout`) и защищённые: `<RequireAuth>` показывает загрузку или ошибку, а гостя уводит на `/login` с `state.from`. После входа экран возвращает на `from`.

`useLoginMutation` и `useRegisterMutation` кладут ответ в кэш `["me"]`; `useLogoutMutation` очищает весь кэш React Query, чтобы данные прошлого пользователя не остались на общем браузере. 401 от любого запроса тоже очищает кэш — `RequireAuth` уводит на вход. 401 не ретраится (`AppProvider`).

Снаружи `src/api` импортируется только через `@/api` (хуки, провайдер) и `@/api/client` (`request`, `ApiError`) — это проверяет `eslint-plugin-boundaries`. Ошибки с известным `code` переводятся в i18n через `lib/apiErrors.ts` и хук `useErrorText` (`api/errors.ts`); незнакомый код показывается текстом сервера.

## Каталог и библиотека

- **Каталог** (`pages/catalog/`) — `GET /api/catalog?q=&kind=&page=&page_size=`, ответ `{ items, total, page, page_size }`. `q` ищется по колонке `titles.name_lower` (Unicode-lowercase копия названия: SQLite `lower()` понимает только ASCII), `%` и `_` экранируются; `kind` — `manga|anime|book`. Администратор видит кнопку «Добавить тайтл» с формой `POST /api/titles`.
- **Тайтл** (`pages/title/`) — `GET /api/titles/{id}` отдаёт `{ title, library }`, где `library` — запись текущего пользователя или `null`. Администратор может править тайтл (`PUT /api/titles/{id}`, полная замена полей) и загружать обложку. Сменить `kind` при привязанных `source_entries` нельзя — 409 `kind_locked`. Главы появятся в фазе источников.
- **Библиотека** (`pages/library/`) — `GET /api/library?page=&page_size=` возвращает `{ items, total, page, page_size }` с краткой карточкой тайтла; `PUT /api/library/{title_id}` (`{ status, score }`) вставляет или обновляет запись, `DELETE` убирает. Статусы: `planned|in_progress|completed|on_hold|dropped`, оценка — 1..10 или `null`. Библиотека у каждого пользователя своя.
- **Обложки.** `POST /api/titles/{id}/cover` (multipart, поле `file`, до 5 МиБ, тело до 6 МиБ) принимает JPEG/PNG/WebP/GIF, декодирует с лимитами в `spawn_blocking`, ресайзит в `full` (1200×1800) и `thumb` (320×480) и сохраняет WebP в `DATA_DIR/covers/{id}/`. `GET /api/titles/{id}/cover?size=thumb|full` отдаёт файл с `ETag` и `Cache-Control: private, max-age=86400`; UI добавляет `v={updated_at}` как cache-buster. Оригинал не сохраняется и не отдаётся.
- **Типы и статусы** общие для формы и API — строковые константы в `lib/catalog.ts`. Форма тайтла (`ui/TitleForm.tsx`) не знает об API: получает значения и колбэк.

## Настройки

`lib/settings.ts` хранит пары ключ-значение в `localStorage` под ключом `cion-reader:settings`. Ключи: `locale` и `sidebarOpen`. Серверные настройки появятся вместе с профилем пользователя. Битый JSON логируется, а не молча заменяется значениями по умолчанию.

Порядок запуска в `app/index.tsx` важен: `initSettings()` читает сохранённые значения, `initI18n()` берёт из них язык, и только потом приложение рендерится — иначе интерфейс мигал бы языком по умолчанию. Компоненты читают настройку через `useSetting(key)` и пишут через `settings.set(key, value)`.

## Язык интерфейса

Переводы — i18next и react-i18next. Все строки под пространством имён `common` в `lib/i18n/locales/en.ts` и `ru.ts`; состав языков — `resources.ts` (`SUPPORTED_LOCALES`, `DEFAULT_LOCALE`). В компонентах `useT()` возвращает функцию перевода, `useLocale()` — текущий язык.

Выбранный язык лежит в настройке `locale`; `initI18n()` подписан на её изменение и вызывает `i18n.changeLanguage`. Переключатель — `shell/navbar/LocaleMenu.tsx`. При первом запуске берётся язык системы, если он есть в `SUPPORTED_LOCALES`.

## Тема

Тема — это класс `dark` на `<html>`. `public/theme.js` ставит его до первой отрисовки: берёт выбор из `localStorage` (ключ `theme`), а если выбора нет, то тему системы. Файл внешний, а не inline-скрипт, чтобы CSP не требовал `unsafe-inline` или хеша. `shell/navbar/ThemeToggle.tsx` переключает класс и сохраняет выбор.

## Страница «Что нового»

`pages/changelog/` импортирует `docs/versions/release.md` как текст (`?raw`) и разбирает разделы `### Версия vX.Y.Z` с буллетами (`release-notes.ts`). Отдельного запроса к бэкенду нет: страница всегда соответствует файлу на момент сборки.

## Связь с бэкендом

```
pages/<экран>/index.tsx → api/<область>.ts → api/client.ts → /api/...
```

| Файл | За что отвечает |
|---|---|
| `api/client.ts` | `request<T>(path, init)`: путь относительно префикса API; `apiUrl(path)` для `<img src>` и ссылок; `jsonMutation(body, method)` для JSON-мутаций; `ApiError` с `status` и `code` |
| `api/auth.tsx` | `AuthProvider`, `useAuth`, хуки входа/регистрации/выхода |
| `api/catalog.ts` | Каталог, тайтл, создание/правка, загрузка обложки, `titleCoverUrl` |
| `api/library.ts` | Библиотека пользователя: список, upsert, удаление |
| `api/errors.ts` | `useErrorText`: `ApiError.code` -> ключ i18n (`lib/apiErrors.ts`) |
| `api/<область>.ts` | Хуки области: `useQuery` для чтения, `useMutation` для изменений |
| `api/index.ts` | Собирает области в объект `api` и реэкспортирует публичные хуки и типы |

Правила:

- **`fetch` только в `client.ts`.** Ссылки, которые браузер запрашивает сам (картинки, файлы), собираются через `apiUrl`.
- **Ошибки.** Сервер отвечает `{ code, message }` с корректным HTTP-статусом. `request` превращает его в `ApiError`; сетевой сбой — `ApiError` со `status: 0` и `code: "network_error"`. 401 не повторяется: `AuthProvider` чистит кэш, `RequireAuth` уводит на `/login`.
- **Пустой ответ.** `204` и пустое тело дают `undefined`.
- **Ключ запроса** — `[имя области или путь, ...аргументы]`. Перечитывание данных после изменения настраивается в хуке через `onSuccess` и `invalidateQueries`, а страница об этом не знает.
- **Типы** сверяются с Rust вручную: `request<T>` ничего не проверяет во время работы.
- **`QueryClient`** в `AppProvider.tsx` создаётся с умолчаниями TanStack Query — это HTTP-клиент: повторы и обновление при фокусе окна.

Автотестов фронтенда нет, поэтому после изменения `client.ts` проверь вручную в консоли браузера при запущенных `pnpm dev` и `pnpm dev:server`:

1. `await import("/src/api/client.ts").then((m) => m.request("/health"))` — объект со `status: "ok"`.
2. `await import("/src/api/client.ts").then((m) => m.request("/missing"))` — `ApiError` со `status: 404`, `code: "not_found"` и сообщением из JSON.
3. Остановить сервер и повторить тот же запрос — `ApiError` со `status: 0`, `code: "network_error"`.
4. `request("/me")` без cookie — `ApiError` со `status: 401`, без автоматического повтора.
5. Ответ `204` или пустое тело — `undefined`.

## Сервер

```
server/
  migrations/
    0001_auth.sql     users, invites, sessions
    0002_catalog.sql  sources, titles, source_entries, chapters, aliases,
                      pages, chapter_texts, library, progress, chapter_reads
  src/
    main.rs          конфиг, БД, DataDir, bootstrap админа, tracing, запуск
    app.rs           Router: health, auth, catalog, titles, library, rate limit, слои
    auth/            handlers, service, password, session, middleware, limiter
    catalog/         service (каталог, тайтл, валидация), handlers, cover (загрузка/выдача)
    library/         service (библиотека пользователя), handlers
    config.rs        env и допустимые origin
    db.rs            пул SQLite (WAL), миграции при старте
    state.rs         AppState: конфиг, пул, семафоры Argon2 и картинок, лимитер входа
    error.rs         AppError -> { code, message } (+ 404-вариант)
    extract.rs       ApiJson/ApiQuery/ApiPath: ошибки извлечения тоже { code, message }
    pagination.rs    page/page_size: значения по умолчанию и потолок
    static_files.rs  ServeDir, SPA fallback, заголовки кэша
  tests/http.rs      health, API 401/404, SPA fallback, /assets
  tests/auth.rs      вход, инвайты, сессии, rate limit
  tests/catalog.rs   тайтлы, поиск, пагинация, обложки, права admin
  tests/library.rs   библиотека, оценки, изоляция пользователей
```

- `GET /api/health` отдаёт `{ "status": "ok", "version": "..." }` без сессии и проверяет БД (`SELECT 1`); ошибка БД — 500.
- Публичные пути перечислены явно: `/api/health` и `/api/auth/*`. Остальной `/api` требует cookie `sid`: без сессии — JSON 401, неизвестный путь с сессией — JSON 404.
- Auth: `POST /api/auth/register` (по инвайту, сразу создаёт сессию), `POST /api/auth/login`, `POST /api/auth/logout`, `GET /api/me`; админские `GET/POST /api/admin/invites`.
- Каталог: `GET /api/catalog` (поиск по `name_lower`, фильтр `kind`, пагинация), `GET /api/titles/{id}`, админские `POST /api/titles`, `PUT /api/titles/{id}`, `POST /api/titles/{id}/cover`, `GET /api/titles/{id}/cover?size=thumb|full`.
- Библиотека: `GET /api/library?page=&page_size=` (`{ items, total, page, page_size }`), `PUT /api/library/{title_id}` (`{ status, score }`), `DELETE /api/library/{title_id}` — всегда для текущего пользователя.
- Пагинация каталога и библиотеки — общая `pagination::resolve`: `page >= 1`, `page_size` по умолчанию 50, потолок 200; `page=0` — 400 `validation`.
- Обложки: multipart-поле `file` до 5 МиБ, только JPEG/PNG/WebP/GIF, декодирование с `image::Limits` в `spawn_blocking` с семафором; результат — два WebP-файла в `DATA_DIR/covers/{id}/`, запись атомарная (tmp + rename). Оригинал не отдаётся; ответ с `ETag`, `nosniff`, `Cache-Control: private`.
- Пароли — Argon2id в `spawn_blocking` с семафором. Токен сессии — 32 случайных байта, в БД только SHA-256; idle 30 дней, абсолютный срок 90. Cookie `sid`: `HttpOnly`, `SameSite=Lax`, `Secure` по `COOKIE_SECURE`, `Max-Age` 30 дней.
- Мутации (`POST`/`PUT`/`PATCH`/`DELETE`) требуют разрешённый `Origin`; `Sec-Fetch-Site: cross-site` отклоняется. В dev разрешены localhost/127.0.0.1 на `:1420` и `:3000`; в release без `PUBLIC_ORIGIN`/`ALLOWED_ORIGINS` мутации запрещены.
- Rate limit: IP — `tower_governor` (login — burst 10, пополнение раз в 2 с; register — burst 5, раз в 10 с), username — счётчик в памяти (10 попыток за 15 минут). `X-Forwarded-For` читается только от адресов из `TRUSTED_PROXIES`.
- Первый администратор — `ADMIN_USERNAME`/`ADMIN_PASSWORD` при пустой таблице `users`; повторно не создаётся.
- БД: `DATABASE_URL` (по умолчанию `sqlite://data/cion.db`), WAL, `foreign_keys=ON`, `busy_timeout=5s`, `create_if_missing`; миграции — `sqlx::migrate!` при старте. Время — unix seconds.
- `index.html` получают только навигационные запросы: `Sec-Fetch-Mode: navigate` или `Accept: text/html`. Путь вида `/reader/1/c12.5` не считается файлом по расширению и работает.
- Отсутствующий `/assets/*` — 404 даже при навигации: ошибка деплоя не маскируется.
- `dist/assets/*` отдаются с `public, max-age=31536000, immutable`, остальное — `no-cache`.
- Конфиг из env: `BIND_ADDR` (по умолчанию `127.0.0.1:3000`), `STATIC_DIR` (`dist`), `DATABASE_URL`, `DATA_DIR` (`data`), `REGISTRATION_MODE` (`invite|closed`), `PUBLIC_ORIGIN`, `ALLOWED_ORIGINS`, `COOKIE_SECURE`, `TRUSTED_PROXIES`, `ADMIN_USERNAME`/`ADMIN_PASSWORD`, `RUST_LOG`. В dev `.env` читает `dotenvy`, пример — `.env.example`.
- Статика читается с диска при каждом запросе: обновление `dist/` не требует перезапуска сервера. `rust-embed` не используем.

## Разработка

```powershell
pnpm dev:server   # axum, 127.0.0.1:3000
pnpm dev          # Vite, 1420, прокси /api -> http://127.0.0.1:3000
```

CORS не нужен: в dev работает прокси, в проде origin один. Правка `.ts`/`.tsx`/CSS пересобирает только Vite; правка Rust требует перезапуска процесса `dev:server` (или `cargo watch`).

Проверка прод-раздачи: `pnpm build`, затем `pnpm dev:server` и http://127.0.0.1:3000. После пересборки UI сервер перезапускать не нужно — `dist/` читается с диска.
