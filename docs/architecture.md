# Устройство приложения

Серверная версия: React SPA в `src/` и сервер `server/` на axum. В разработке — два процесса (Vite и axum), в проде — один origin. Целевая архитектура и фазы — [planned-now.md](planned-now.md).

## Слои фронтенда

Код `src/` разложен по слоям. Импорты между файлами `.ts` и `.tsx` проверяет `pnpm lint`. Имена файлов линтер не проверяет, это договорённость.

| Слой | Что лежит | Сейчас в приложении |
|---|---|---|
| `app/` | запуск: `index.tsx`, провайдеры, маршрутизатор, глобальные стили | `AppProvider.tsx`, `AppRouter.tsx`, `styles/index.css` |
| `shell/` | оболочка окна: то, что не принадлежит ни одному экрану | `layout/`, `navbar/` (шапка, язык, тема), `sidebar/` |
| `pages/<имя>/` | экраны, по папке на экран | `home/`, `title/`, `reader/`, `watch/`, `changelog/`, `settings/` |
| `api/` | HTTP-транспорт и хуки областей | `client.ts`, `index.ts` (областей пока нет) |
| `ui/` | `shadcn/` и мелкие общие компоненты | `shadcn/`, `PageHeader.tsx`, `PlaceholderPage.tsx` |
| `lib/` | утилиты, хуки и типы без UI | `utils.ts`, `settings.ts`, `routes.ts`, `i18n/` |

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

Экран — `pages/<имя>/index.tsx`, рядом лежат его компоненты. Файл называется `index`, а компонент — по экрану (`export function HomePage`), импорт выглядит так: `@/pages/home`. Импортировать экран может только `app`, экраны друг друга не импортируют.

Пункты меню и их порядок — единая таблица `shell/sidebar/sections.ts`. Она же задаёт `SectionPath`, а роутер (`app/AppRouter.tsx`) требует экран для каждого пути кроме главной: забытый маршрут не скомпилируется. Главная — `index`-маршрут `/`, поэтому в таблице у неё путь `/`.

| Группа | Заголовок | Экраны |
|---|---|---|
| `overview` | Обзор | Главная |
| `footer` | без заголовка | Что нового, Настройки |

`SidebarGroupLabel` сам скрывает заголовок, когда сайдбар свёрнут до иконок. Свернуть сайдбар можно кнопкой в шапке или `Ctrl+B`; состояние сохраняется в настройках.

Параметрические маршруты (`/title/:id`, `/reader/:titleId/:chapterKey`, `/watch/:titleId/:chapterKey`) не входят в `SectionPath` и `sectionForPath`: они перечислены отдельным списком `PARAM_ROUTES` в `AppRouter`. Меню остаётся статическим, активный пункт ищется по префиксу пути, а заголовок в шапке — через `pageTitleKey`. Ссылки на эти экраны собирай через `src/lib/routes.ts`: `chapterKey` содержит `:` и `.`, поэтому кодируется `encodeURIComponent`.

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
| `api/client.ts` | `request<T>(path, init)`: путь относительно префикса API; `apiUrl(path)` для `<img src>` и ссылок; `ApiError` с `status` и `code` |
| `api/<область>.ts` | Хуки области: `useQuery` для чтения, `useMutation` для изменений |
| `api/index.ts` | Собирает области в объект `api`; сейчас пуст |

Правила:

- **`fetch` только в `client.ts`.** Ссылки, которые браузер запрашивает сам (картинки, файлы), собираются через `apiUrl`.
- **Ошибки.** Сервер отвечает `{ code, message }` с корректным HTTP-статусом. `request` превращает его в `ApiError`; сетевой сбой — `ApiError` со `status: 0` и `code: "network_error"`. 401 не повторяется на уровне клиента: решение о редиректе примет `AuthProvider` в фазе авторизации.
- **Пустой ответ.** `204` и пустое тело дают `undefined`.
- **Ключ запроса** — `[имя области или путь, ...аргументы]`. Перечитывание данных после изменения настраивается в хуке через `onSuccess` и `invalidateQueries`, а страница об этом не знает.
- **Типы** сверяются с Rust вручную: `request<T>` ничего не проверяет во время работы.
- **`QueryClient`** в `AppProvider.tsx` создаётся с умолчаниями TanStack Query — это HTTP-клиент: повторы и обновление при фокусе окна.

Автотестов фронтенда нет, поэтому после изменения `client.ts` проверь вручную в консоли браузера при запущенных `pnpm dev` и `pnpm dev:server`:

1. `await import("/src/api/client.ts").then((m) => m.request("/health"))` — объект со `status: "ok"`.
2. `await import("/src/api/client.ts").then((m) => m.request("/missing"))` — `ApiError` со `status: 404`, `code: "not_found"` и сообщением из JSON.
3. Остановить сервер и повторить тот же запрос — `ApiError` со `status: 0`, `code: "network_error"`.
4. 401 (появится в фазе 2) — `ApiError.status === 401`, без автоматического повтора.
5. Ответ `204` или пустое тело — `undefined`.

## Сервер

```
server/
  src/
    main.rs          конфиг, tracing, запуск
    app.rs           Router: /api/health, JSON-404 для /api, слои trace/compression
    config.rs        BIND_ADDR, STATIC_DIR
    error.rs         ErrorBody { code, message }
    static_files.rs  ServeDir, SPA fallback, заголовки кэша
  tests/http.rs      health, API 404, SPA fallback, /assets
```

- `GET /api/health` отдаёт `{ "status": "ok", "version": "..." }`.
- Неизвестный `/api/*` — JSON 404, а не `index.html`.
- `index.html` получают только навигационные запросы: `Sec-Fetch-Mode: navigate` или `Accept: text/html`. Путь вида `/reader/1/c12.5` не считается файлом по расширению и работает.
- Отсутствующий `/assets/*` — 404 даже при навигации: ошибка деплоя не маскируется.
- `dist/assets/*` отдаются с `public, max-age=31536000, immutable`, остальное — `no-cache`.
- Конфиг из env: `BIND_ADDR` (по умолчанию `127.0.0.1:3000`), `STATIC_DIR` (по умолчанию `dist`), `RUST_LOG`. В dev `.env` читает `dotenvy`.
- Статика читается с диска при каждом запросе: обновление `dist/` не требует перезапуска сервера. `rust-embed` не используем.

## Разработка

```powershell
pnpm dev:server   # axum, 127.0.0.1:3000
pnpm dev          # Vite, 1420, прокси /api -> http://127.0.0.1:3000
```

CORS не нужен: в dev работает прокси, в проде origin один. Правка `.ts`/`.tsx`/CSS пересобирает только Vite; правка Rust требует перезапуска процесса `dev:server` (или `cargo watch`).

Проверка прод-раздачи: `pnpm build`, затем `pnpm dev:server` и http://127.0.0.1:3000. После пересборки UI сервер перезапускать не нужно — `dist/` читается с диска.
