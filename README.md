# cion-reader

Приватный self-hosted агрегатор манги, аниме и книг: один поиск по источникам, одна библиотека и один прогресс для себя и небольшого круга читателей.

**Текущее состояние.** Работают сервер `server/` на Rust (axum), SQLite с миграциями, вход по инвайту и сессии в HttpOnly-cookie. Корневой экран — библиотека без сайдбара: вкладки статусов, избранное, до трёх своих закладок, сортировка и поиск по своей библиотеке в шапке. Тайтл добавляется лениво: поиск по всем источникам и автоматический импорт при первом добавлении. Страница тайтла делится на вкладки «Главы» и «Главная», недоступные главы скрыты, прогресс и отметки прочитанного видны. Администратор создаёт тайтлы вручную в `/admin`. Два источника — Remanga (JSON API) и RuMix (HTML): импорт глав, чтение манги через серверный прокси картинок с дисковым кэшем и сохранение прогресса. Второй источник связан с тем же тайтлом; работают приоритеты по `(kind, lang)`, выбор preferred source, fallback по главам, ручные chapter_aliases и админ-экран. Следующая фаза — видео. План — [docs/planned-now.md](docs/planned-now.md), причины решений — [docs/decisions/](docs/decisions/).

## Что будет в приложении

- вход по инвайту и профили пользователей;
- поиск по источникам и библиотека: манга, аниме и книги в одном месте;
- несколько источников у одного тайтла и ручное связывание;
- ридер манги и книг, плеер видео;
- синхронизация прогресса между устройствами. *(Есть: два источника, приоритеты и fallback, ридер манги и прогресс.)*

## Стек

- Фронтенд: React 19, TypeScript 6, Vite 8, shadcn/ui (Base UI), Tailwind CSS 4, TanStack Query 5, React Router 8, i18next.
- Бэкенд: Rust 2024, axum 0.8, tokio, tower-http; sqlx + SQLite, image, reqwest, scraper.

## Быстрый старт

Нужны Rust, Node.js и pnpm.

```powershell
pnpm install
pnpm dev:server   # терминал 1: axum на 127.0.0.1:3000
pnpm dev          # терминал 2: Vite на 1420
```

Откройте http://localhost:1420 — Vite проксирует API на сервер. Правка UI не пересобирает Rust.

Первый администратор создаётся один раз на пустой БД:

```powershell
$env:ADMIN_USERNAME='admin'; $env:ADMIN_PASSWORD='change-me-please'; pnpm dev:server
```

После создания войдите на http://localhost:1420/login и уберите `ADMIN_*` из окружения. Инвайты для остальных выдаёт `POST /api/admin/invites` с сессией администратора. SQLite лежит в `data/cion.db` и создаётся автоматически.

Проверка прод-раздачи локально:

```powershell
pnpm build
pnpm dev:server
```

Откройте http://127.0.0.1:3000 — сервер отдаст SPA и `/api/health`. Конфиг сервера: `BIND_ADDR`, `STATIC_DIR`, `DATABASE_URL`, `DATA_DIR`, `REGISTRATION_MODE`, `PUBLIC_ORIGIN`/`ALLOWED_ORIGINS`, `COOKIE_SECURE`, `TRUSTED_PROXIES`, `ADMIN_USERNAME`/`ADMIN_PASSWORD`, `RUST_LOG` (пример в `.env.example`).

## Проверки

```powershell
pnpm lint
pnpm build
pnpm test        # Rust-тесты
pnpm check:rust  # fmt --check + clippy
```

## Разработка

Рабочие команды: `/planner`, `/isplan`, `/isgood`, `/sync-docs`, `/ship`, `/notes`. Правила ревью — `.claude/rules/`, механика — `.claude/scripts/`.

## Документация

- [docs/architecture.md](docs/architecture.md) — устройство фронтенда и сервера;
- [docs/planned-now.md](docs/planned-now.md) — переезд на сервер и фазы;
- [docs/decisions/](docs/decisions/) — принятые решения.
