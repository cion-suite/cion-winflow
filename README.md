# cion-reader

Приватный self-hosted агрегатор манги, аниме и книг: один каталог, одна библиотека и один прогресс для себя и небольшого круга читателей.

**Текущее состояние.** Работают сервер `server/` на Rust (axum), SQLite с миграциями, вход по инвайту и сессии в HttpOnly-cookie. Каталог с поиском и пагинацией, карточка тайтла, библиотека со статусом и оценкой, ручное создание тайтлов и загрузка обложек для администратора. Источники и ридер — следующие фазы. План — [docs/planned-now.md](docs/planned-now.md), причины решения — [docs/decisions/0001-server-architecture.md](docs/decisions/0001-server-architecture.md).

## Что будет в приложении

- вход по инвайту и профили пользователей;
- каталог и библиотека: манга, аниме и книги в одном месте;
- несколько источников у одного тайтла и ручное связывание;
- ридер манги и книг, плеер видео;
- синхронизация прогресса между устройствами.

## Стек

- Фронтенд: React 19, TypeScript 6, Vite 8, shadcn/ui (Base UI), Tailwind CSS 4, TanStack Query 5, React Router 8, i18next.
- Бэкенд: Rust 2024, axum 0.8, tokio, tower-http; sqlx + SQLite, image, reqwest, scraper — по фазам плана.

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
