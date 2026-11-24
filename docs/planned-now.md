# План перевода cion-reader на сервер

- **Статус:** принят, выполняется
- **Дата:** 2026-11-24
- **Решение:** [0001-server-architecture.md](decisions/0001-server-architecture.md)
- **Исходное состояние:** десктопное приложение на Tauri 2

## 1. Цель и границы

cion-reader — закрытый self-hosted агрегатор: манга, аниме и книги из нескольких источников в одном каталоге, с общей библиотекой и синхронизацией прогресса для себя и небольшого круга читателей.

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
| `src/api/*` | `client.ts` переписывается на `fetch`; `index.ts` остаётся; хуков пока нет — появятся вместе с эндпоинтами |
| `src-tauri/` | удаляется целиком: Tauri, плагины, capabilities, `build.rs`, иконки, `tauri.conf.json` |
| `server/` | новый крейт: axum + tokio + sqlx |
| `eslint.config.js` | убрать `src-tauri` из `globalIgnores` и запрет `@tauri-apps/*` |
| `package.json` | убрать `tauri` и `tauri:dev`, `@tauri-apps/*`; добавить `dev:server` |
| `src/lib/settings.ts` | остаётся только ветка `localStorage` |
| `src/shell/navbar/ThemeToggle.tsx` | убрать импорты `@tauri-apps/*` и `syncWindowTheme`, оставить класс темы |
| `vite.config.ts` | заменить блок `server` целиком: proxy `/api`, убрать `watch.ignored`, `clearScreen` и Tauri-комментарии |
| `.gitignore` | `server/target`, `data/`, `.env*` (кроме примера); `server/Cargo.lock` коммитится |
| `.claude/commands/notes.md`, `.claude/scripts/notes.mjs` | версия в `server/Cargo.toml`, области для `server/` вместо `src-tauri/` |
| `docs/` | `architecture.md` обновляется, план и решение добавлены |

Временное сосуществование: пока `src-tauri/` не удалён, `server/` — отдельный крейт, workspace не нужен. После удаления Tauri `server/` остаётся единственным Rust-крейтом.

## 3. Целевая структура

```
cion-reader/
  src/                         фронтенд (слои как сейчас)
    app/                       запуск, провайдеры, роутер
    shell/                     navbar, sidebar, layout
    pages/                     catalog, title, reader, watch, library, profile, settings, login
    api/                       client.ts (fetch), auth.ts, catalog.ts, library.ts, progress.ts
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
      library/                 библиотека и прогресс
      sources/                 реестр, адаптеры, нормализованные DTO
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
| `sqlx` 0.8 (`sqlite`, `runtime-tokio`, `migrate`, `time` или `chrono`) | БД и миграции |
| `thiserror` | `AppError` |
| `tracing`, `tracing-subscriber` | структурные логи |
| `reqwest` 0.12, `default-features = false`, features `rustls-tls`, `http2`, `charset` | исходящие запросы к источникам без OpenSSL |
| `scraper` | CSS-селекторы для HTML-источников |
| `argon2` | хеши паролей |
| `rand` | токены сессий и инвайтов |
| `sha2`, `hex` | хеши токенов и ключи кэша |
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

sources(id, code UNIQUE, name, base_url, adapter, enabled, config_json, created_at)
titles(id, kind, name, alt_names_json, description, cover_path, created_at, updated_at)
source_entries(id, title_id, source_id, external_id, external_url, raw_json,
               is_primary, last_synced_at)
chapters(id, source_entry_id, external_id, number, volume, name,
         external_url, embed_url, published_at)
pages(id, chapter_id, idx, image_ref, image_url, width, height)

library(user_id, title_id, status, score, added_at, updated_at,
        PRIMARY KEY(user_id, title_id))
progress(user_id, title_id, chapter_number, page, seconds, updated_at,
         PRIMARY KEY(user_id, title_id))
chapter_reads(user_id, title_id, chapter_number, read_at,
              PRIMARY KEY(user_id, title_id, chapter_number))  -- опционально

jobs(id, kind, payload_json, status, attempts, run_after, last_error, created_at)
```

Ключевые решения:

- **Канонический прогресс.** `progress` хранится по `title_id` + номеру главы, а не по `chapter_id` конкретного источника: при переключении источника та же глава имеет другой `chapter_id`, и прогресс терялся бы. `PUT /api/progress/{titleId}` принимает `chapterId`, но обработчик проверяет, что глава принадлежит тайтлу (`chapters → source_entries.title_id`), и сохраняет её `number`. Для видео `number` — номер эпизода, `seconds` — позиция.
- **Дедупликация.** `chapters.external_id` и `UNIQUE(source_entry_id, external_id)`; `pages` — `UNIQUE(chapter_id, idx)`. Импорт — `INSERT ... ON CONFLICT ... DO UPDATE`, иначе refresh создаст дубли.
- **Ссылки на страницы.** `pages.image_ref` — стабильная ссылка источника (путь или id), `image_url` — последний разрешённый URL. Подписанные CDN-URL живут недолго: при `401/403` адаптер перезапрашивает список страниц, а кэш байтов ключуется по `(source_id, external chapter id, idx)`, не по подписанному URL.
- **Конфиг источника.** `sources.config_json`: `image_hosts`, `player_hosts`, заголовки/куки. `image_hosts` нужен потому, что картинки часто лежат на отдельном CDN, а не на `base_url`.
- **Сессии.** `last_seen_at` обновлять не чаще раза в 5–15 минут (условный `UPDATE ... WHERE last_seen_at < now - 300`), иначе каждый запрос пишет в единственную SQLite-базу.

Индексы и ограничения: `UNIQUE(source_id, external_id)`, `UNIQUE(source_entry_id, external_id)`, `UNIQUE(chapter_id, idx)`, частичный `UNIQUE(title_id) WHERE is_primary = 1`, `pages(chapter_id)`, `source_entries(title_id)`, `chapters(source_entry_id, volume, number)`, `progress(user_id, updated_at)`, `jobs(status, run_after)`.

Поиск: на первом этапе — `LIKE` по `lower(name)` с экранированием `%` и `_`. FTS5 подключать позже, когда `alt_names` будут храниться склеенным текстом (JSON-массив FTS5 индексирует плохо), а пользовательский ввод будет экранироваться от синтаксиса `MATCH`.

## 6. API

Базовый путь — `/api`, JSON, ошибки `{ "code": "...", "message": "..." }` с корректным HTTP-статусом. Версия axum — 0.8: параметры пути записываются в фигурных скобках (`/api/titles/{id}`), двоеточие в 0.8 паникует при сборке роутера. Авторизация — HttpOnly-cookie `sid`. Версионирование (`/api/v1`) не нужно, пока клиент и сервер деплоятся вместе.

Доступ: всё под `/api`, кроме `/api/health` и `/api/auth/*`, требует валидную сессию. Middleware сессии вешается на весь API-роутер, а публичные маршруты перечислены явно — не «всё, что не мутация». Тест: запрос без cookie → 401.

Пагинация: `page >= 1` (иначе 400), `page_size` по умолчанию 50 с жёстким потолком; ответ — `{ items, total, page, page_size }`.

| Метод и путь | Что делает |
|---|---|
| `GET /api/health` | проверка сервера и БД |
| `POST /api/auth/register` | регистрация по инвайту |
| `POST /api/auth/login` | вход, выставляет cookie |
| `POST /api/auth/logout` | удаляет сессию |
| `GET /api/me` | текущий пользователь |
| `GET /api/catalog?q=&kind=&page=&page_size=` | каталог и поиск |
| `GET /api/titles/{id}` | тайтл, источники, главы |
| `GET /api/chapters/{id}/pages` | список страниц с внутренними URL |
| `GET /api/chapters/{id}/watch` | данные плеера и `embed_url` |
| `GET /api/pages/{id}/image` | прокси страницы из источника |
| `GET /api/titles/{id}/cover` | обложка (локальная или прокси) |
| `GET /api/library` | библиотека пользователя |
| `PUT /api/library/{titleId}` | статус и оценка |
| `DELETE /api/library/{titleId}` | убрать из библиотеки |
| `GET /api/progress/{titleId}` | последняя позиция |
| `PUT /api/progress/{titleId}` | сохранить главу, страницу или секунды |
| `POST /api/me/avatar` | загрузка аватарки (multipart, лимит тела) |
| `GET /api/me/avatar`, `GET /api/users/{id}/avatar` | аватары |
| `GET /api/sources` | реестр источников (admin) |
| `POST /api/sources/{code}/search` | поиск по источнику (admin) |
| `POST /api/sources/{code}/import` | импорт: `{ externalId, titleId? }`; без `titleId` создаёт тайтл |
| `GET/POST /api/admin/invites` | список и создание инвайтов (admin) |
| `POST /api/titles`, `PUT /api/titles/{id}` | создание и правка тайтла (admin) |
| `POST /api/titles/{id}/cover` | загрузка обложки (admin) |
| `POST /api/titles/{id}/sources` | привязать источник к тайтлу (admin) |
| `DELETE /api/titles/{id}/sources/{entryId}` | отвязать источник (admin) |
| `PUT /api/titles/{id}/primary-source` | выбрать основной источник (admin) |
| `POST /api/admin/refresh/{titleId}` | обновить главы (admin) |

Поток импорта: поиск по источнику → пользователь выбирает результат → `POST /api/sources/{code}/import` создаёт канонический тайтл и `source_entry` (или привязывает к существующему `titleId`) → адаптер импортирует главы и страницы.

Правила: мутирующие запросы требуют сессии и проверки `Origin`; админские — роли; список страниц отдаёт внутренние URL, а не ссылки на чужой сайт; для загрузки аватарки повесить `DefaultBodyLimit::max(6 * 1024 * 1024)` (дефолтные 2 МиБ не пропустят 5-мегабайтный файл) и отдельно проверить размер в обработчике. Изображения клиент получает только через сервер; видео — через встроенный плеер, браузер обращается к домену плеера напрямую (см. §9).

## 7. Источники

Реестр — таблица `sources` плюс скомпилированные адаптеры. Новый источник = новый модуль + запись в реестре, без изменения остального кода.

```rust
struct SourceChapter { external_id: String, number: f32, name: Option<String> }
struct SourcePage { index: u32, image_ref: String }

enum SourceKind { Remanga(Remanga), MangaLib(MangaLib) }

impl SourceKind {
    async fn search(&self, query: &str) -> Result<Vec<SourceTitle>> {
        match self { /* ... */ }
    }
}
```

Трейт `Source` не нужен: `async fn` в трейтах не dyn-совместим и по умолчанию даёт non-`Send` future, а набор источников фиксирован. Enum dispatch проще и работает в axum/tokio. Общий `reqwest::Client`, семафоры и circuit breaker живут в `AppState`.

Политика обращений:

- свой `User-Agent` и `Referer` у каждого источника, таймаут ~10 с;
- лимиты раздельные: 1–2 параллельных запроса на метаданные (поиск, тайтл, главы) и отдельный пул 8–16 на скачивание страниц; учитывать `429` и `Retry-After`; prefetch ограничивать;
- 2 ретрая с backoff на сетевые ошибки, circuit breaker после серии неудач;
- ошибка одного источника не ломает выдачу: поиск возвращает частичный результат и статус по каждому источнику;
- URL источника никогда не приходит от клиента: только `source_id` из реестра и `external_id` из БД;
- парсинг отделять от загрузки: чистые `parse_search(&str)`, `parse_chapter(&str)` тестируются через `#[cfg(test)]` и `include_str!` на фикстурах, без сети;
- в `sources.config_json` хранить разрешённые хосты изображений и CDN, домены плееров, заголовки/куки;
- Cloudflare и JS-челленджи заголовками не обходятся: источник либо имеет API/статический HTML, либо не подходит. Проверяйте кандидата до написания адаптера.

## 8. Изображения и файлы

Клиент не ходит на чужие сайты за изображениями. Страницы и обложки идут через сервер:

- `GET /api/pages/{id}/image` берёт разрешённую ссылку из БД, скачивает с нужным `Referer` и стримит байты;
- кэш на диске: `data/cache/images/<aa>/<sha256>.<ext>`, где `aa` — первые два символа хеша; ключ не зависит от подписанного URL;
- запись атомарная: временный файл в том же каталоге + `rename`; одновременные запросы одного ключа дедуплицируются (single-flight), недоскачанный файл читателям не отдаётся;
- рядом с файлом хранится проверенный `Content-Type`; эвикшен по `mtime` не трогает файлы, которые ещё пишутся;
- TTL и лимит размера (например, 5 ГБ), эвикшен — фоновая задача;
- обложки при импорте ресайзятся в 2 размера WebP (`image`), аватарки — в WebP; для URL с хешем содержимого допустим `immutable`, для остальных — `ETag` и `304`;
- ссылки, отдаваемые по сессии, кэшируются как `private, max-age=86400`; `public, immutable` — только для действительно публичных ассетов;
- SSRF-защита: разрешённые хосты брать из `sources.config_json` (сайт и CDN), только `http/https` и порты 80/443; отключить автоматические редиректы (`reqwest::redirect::Policy::none()`) или проверять каждый хоп; резолвить имя, проверять **все** полученные IP и соединяться с проверенным (кастомный `reqwest::dns::Resolve` или подключение по IP); отклонять private/loopback/link-local, CGNAT `100.64.0.0/10`, `169.254.169.254`, ULA `fc00::/7` и IPv4-mapped IPv6; лимит тела ~15 МБ;
- проверять магические байты и разрешать только `image/jpeg`, `image/png`, `image/webp`, `image/gif` (AVIF — если включён в `image`); SVG и HTML отклонять, иначе чужой файл исполнится в вашем origin; отдавать с `X-Content-Type-Options: nosniff` и `Content-Disposition: inline`;
- загрузки: лимит 5 МБ, `DefaultBodyLimit` на маршруте, декодирование через `image` с `Limits` (`max_alloc`, `max_image_width/height`, лимит площади пикселей), чтобы сжатая «бомба» не съела память; перекодирование, удаление EXIF, первый кадр для анимаций или отказ; оригинал не отдаётся;
- локальные обложки тайтлов — тот же путь загрузки или импорт по разрешённому URL.

## 9. Видео

1. **Первый вариант — встраивание.** В `chapters.embed_url` лежит ссылка, клиент рендерит `<iframe>` с `referrerpolicy="no-referrer"`, `allowfullscreen` и минимальным набором разрешений: `sandbox="allow-scripts allow-same-origin allow-presentation allow-popups allow-forms"`; голый `sandbox` заблокирует плеер. Домены плееров хранить в конфиге источника и генерировать `frame-src`. Учтите: браузер не даёт подставить свой `Referer`, поэтому часть источников встраивание запретит — проверяйте до интеграции.
2. **HLS-прокси — только если источники не дают встраивать.** `hls.js` на клиенте, маршруты `/api/titles/{id}/video/playlist` и `/segment` с непрозрачными токенами вместо сырых URL. Парсить m3u8 и переписывать все URI: варианты master-плейлиста, `EXT-X-KEY`, `EXT-X-MAP`, абсолютные и относительные ссылки; поддерживать `Range`, отдельный allowlist медиа-хостов и лимит кэша сегментов. Для `hls.js` в CSP нужны `worker-src 'self' blob:` и `media-src 'self' blob:`.
3. Полное скачивание и перекодирование видео не делаем.

Прогресс видео хранится в `progress.seconds` и восстанавливается при открытии.

## 10. Авторизация и безопасность

- регистрация только по инвайту; `REGISTRATION_MODE=invite|closed`: в `closed` `POST /api/auth/register` возвращает 403 независимо от инвайта;
- первый администратор создаётся подкомандой CLI или одноразовыми `ADMIN_USERNAME`/`ADMIN_PASSWORD` из env при пустой таблице `users` — иначе создать инвайт некому;
- пароли — Argon2id, минимум 10 символов, одинаковое сообщение об ошибке входа;
- сессии — opaque-токен: 32 случайных байта, в БД только SHA-256, cookie `sid` с `HttpOnly`, `SameSite=Lax`, `Secure` в проде; idle-срок 30 дней, абсолютный — 90; ротация при входе; logout удаляет запись; `last_seen_at` обновляется с троттлингом;
- инвайт захватывается атомарно в транзакции: `UPDATE invites SET used_by = ? WHERE id = ? AND used_by IS NULL AND (expires_at IS NULL OR expires_at > ?)`, проверка `rows_affected() == 1`, затем вставка пользователя;
- CSRF: same-origin + `SameSite=Lax` + проверка `Origin` у всех POST/PUT/DELETE. Допустимые origin — `PUBLIC_ORIGIN`/`ALLOWED_ORIGINS`: `http://localhost:1420` в dev, один https-домен в проде. Отсутствующий или несовпавший `Origin` отклоняется; дополнительно проверяется `Sec-Fetch-Site`. `COOKIE_SECURE` включается в проде, `X-Forwarded-Proto` доверять только от Caddy;
- rate limit на login и регистрацию по паре `(IP, username)`, обязателен в фазе 2. За Caddy axum видит `127.0.0.1`: IP брать из `X-Forwarded-For` только если peer — доверенный прокси, иначе из `ConnectInfo`;
- админ-роль для источников, импорта и ручного связывания;
- CSP задаётся заголовками: в Caddy для HTML (вариант 2) или в axum (вариант 1), но не в обоих. Инлайновый скрипт темы вынести в `public/theme.js` и подключить блокирующим `<script src>`. Директивы: `default-src 'self'`, `script-src 'self'`, `style-src 'self' 'unsafe-inline'` (Base UI ставит inline-стили), `img-src 'self' data: blob:`, `connect-src 'self'`, `font-src 'self'`, `frame-src` домены плееров, `media-src 'self' blob:`, `worker-src 'self' blob:` (HLS), `frame-ancestors 'none'`, `object-src 'none'`, `base-uri 'self'`, `form-action 'self'`. В dev строгий CSP отключается: Vite/React Refresh используют инлайновые скрипты;
- секреты — только env, `.env` в dev через `dotenvy`, в git не попадает;
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

`dev:server` — это `cargo run --manifest-path server/Cargo.toml`; без `cargo watch` правка Rust подхватится только после перезапуска процесса.

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
cargo build --release --manifest-path server/Cargo.toml   # локальная проверка; релиз — в CI
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
  @html path / /index.html
  header @html Cache-Control "no-cache"
  header /assets/* Cache-Control "public, max-age=31536000, immutable"

  handle /api/* { reverse_proxy 127.0.0.1:3000 }
  handle /assets/* { file_server }
  handle { try_files {path} /index.html; file_server }
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

### Фаза 1 — каркас сервера и транспорт

- создать `server/` (axum 0.8), `GET /api/health`, конфиг из env, `tracing`;
- отдача статики из `dist/` с SPA fallback только для навигации;
- заменить блок `server` в `vite.config.ts` (proxy `/api`, убрать `watch.ignored`, `clearScreen`, Tauri-комментарии);
- переписать `src/api/client.ts` на `fetch`;
- убрать Tauri-ветку из `settings.ts` и `ThemeToggle.tsx` (импорты, `syncWindowTheme`, `useEffect`);
- `HashRouter` → `BrowserRouter`, `QueryClient` — под HTTP;
- вынести инлайновый скрипт темы из `index.html` в `public/theme.js` (CSP);
- удалить `src-tauri/`, `tauri*` скрипты, `@tauri-apps/*`, Tauri-правила из ESLint;
- обновить `.gitignore` (`server/target`, `data/`, `.env*`), `README`, `CLAUDE.md`;
- решить судьбу шаблонных экранов (`home`, `network`, `apps`, `utils`, `system`, `changelog`) и синхронно обновить `sections.ts`, `AppRouter`, i18n, mock-данные;
- добавить `dev:server` в `package.json`;
- обновить `.claude/commands/notes.md` и `.claude/scripts/notes.mjs` под `server/`; новый `server/Cargo.lock` коммитится.

Критерий: приложение открывается в браузере на `:1420` через Vite, `/api/health` отвечает через прокси; после `pnpm build` сервер отдаёт SPA и API на `:3000`; в `src/` нет `@tauri-apps`; навигация использует чистые URL.

### Фаза 2 — БД, авторизация, инвайты

- sqlx + SQLite, миграции, `SqliteConnectOptions` (WAL, `foreign_keys`, `busy_timeout`, `create_if_missing`);
- таблицы `users`, `invites`, `sessions`;
- регистрация по инвайту с атомарным захватом, вход, выход, `GET /api/me`, middleware сессии;
- bootstrap первого администратора (CLI или env при пустой БД);
- `REGISTRATION_MODE=invite|closed`, rate limit login/register;
- экраны `login`/`register`, защищённые маршруты, `AuthProvider`;
- тесты: без cookie → 401, чужой `Origin` → 403, лимит входа, инвайт нельзя использовать дважды.

Критерий: регистрация по одноразовому инвайту, вход, сохранение сессии после перезагрузки, выход; в режиме `closed` регистрация возвращает 403; пароли — Argon2; cookie с нужными флагами.

### Фаза 3 — каталог и библиотека без источников

- таблицы `titles`, `source_entries`, `chapters`, `library`, `progress`;
- админские эндпоинты создания/правки тайтла и загрузки обложки;
- библиотека: добавить, статус, оценка, удалить;
- каталог, поиск, экраны `catalog`, `title`, `library`;
- заменить/удалить шаблонные экраны и пункты меню.

Критерий: тайтл создаётся вручную, добавляется в библиотеку, поиск находит его по названию; статус и оценка сохраняются; в меню нет демо-разделов.

### Фаза 4 — первый источник, ридер, прокси картинок

- реестр источников и первый адаптер (API или HTML); парсеры — чистые функции с фикстурами;
- поиск, импорт тайтла, импорт глав и страниц с upsert;
- `GET /api/pages/{id}/image` с дисковым кэшем, single-flight и SSRF-защитой;
- экран `reader`: виртуализация страниц, prefetch следующих, клавиатура, сохранение прогресса с дебаунсом;
- обработка подписанных CDN-URL: при `401/403` перезапросить страницы главы.

Критерий: реальный тайтл импортируется, глава читается через проксированные картинки, прогресс сохраняется и восстанавливается; падение источника показывается ошибкой, а не белым экраном; повторное открытие главы не скачивает картинки заново.

### Фаза 5 — второй источник и дедупликация

- второй адаптер;
- `source_entries`: связывание с каноническим тайтлом, `is_primary` (частичный уникальный индекс);
- админ-экран связывания, переключение источника у тайтла;
- объединённый список глав; прогресс не зависит от источника.

Критерий: один тайтл из двух источников даёт одну запись в каталоге и одну в библиотеке; источник переключается; прогресс сохраняется при переключении; дублей в библиотеке нет.

### Фаза 6 — видео

- `chapters.embed_url`, экран `watch`, `GET /api/chapters/{id}/watch`;
- iframe с `sandbox`, `referrerpolicy`, `allowfullscreen`; домены плееров в конфиге источника;
- прогресс в секундах, продолжение просмотра;
- CSP `frame-src` только для разрешённых доменов.

Критерий: видео играет во встроенном плеере, позиция восстанавливается, CSP не блокирует разрешённые домены и блокирует остальные. HLS-прокси — отдельная задача, только при реальной необходимости.

### Фаза 7 — фоновые задачи и полировка

- `jobs` + воркер: обновление глав, prefetch, эвикшен кэша, чистка сессий;
- уведомление о новых главах в интерфейсе;
- настройки ридера (режим, ширина, направление), PWA;
- локализация новых экранов (en/ru), мобильная вёрстка;
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
| Трафик VPS упирается в лимит | проксировать страницы по требованию, ограничить prefetch, HLS-кэш с лимитом, мониторить egress |
| Кэш картинок съедает диск | лимит, эвикшен, single-flight, мониторинг размера `data/cache` |
| SSRF через прокси изображений | allowlist хостов и CDN, проверка IP после DNS, запрет редиректов, лимит тела, отклонение private/CGNAT/metadata |
| SQLite и конкурентные записи | WAL, `busy_timeout`, троттлинг `last_seen_at`, один инстанс; при росте — Postgres |
| CSP ломает встраиваемые плееры | whitelist `frame-src`, проверка на каждом новом источнике |
| Подписанные URL протухают | хранить стабильную ссылку/`external_id`, разрешать URL при запросе, перезапрашивать страницы при 401/403 |
| Потеря данных | ежедневный бэкап БД и `data/`, проверка восстановления, бэкап перед миграциями |
| Расползание объёма | вертикальные фазы, одна фича за раз, без микросервисов и очередей |

## 15. Definition of done

- в репозитории нет Tauri и `src-tauri/`;
- `server/` собирается в release и отдаёт SPA + API;
- работает вход по инвайту и закрытая регистрация;
- есть bootstrap администратора;
- все `/api`, кроме health и auth, требуют сессию; тест «без cookie → 401» и проверка `Origin` зелёные;
- один источник пройден от поиска до чтения главы;
- второй источник связывается с тем же тайтлом, прогресс переживает переключение;
- ридер и плеер сохраняют прогресс;
- тесты адаптеров, авторизации и библиотеки; `fmt` и `clippy` чистые;
- деплой воспроизводим: CI под Linux, systemd, Caddy, атомарная выкладка, бэкапы;
- `CLAUDE.md` и `docs/architecture.md` описывают серверную реальность, а не прошлое.
