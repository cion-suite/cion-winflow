# cion-reader

Приватный self-hosted агрегатор манги, аниме и книг: один каталог, одна библиотека и один прогресс для себя и небольшого круга читателей.

**Текущее состояние.** Фронтенд переехал в веб (React SPA), в репозитории появился сервер `server/` на Rust (axum): `/api/health`, конфиг из env и раздача `dist/` с SPA-fallback. Авторизация, БД, каталог, источники и ридер — следующие фазы. План — [docs/planned-now.md](docs/planned-now.md), причины решения — [docs/decisions/0001-server-architecture.md](docs/decisions/0001-server-architecture.md).

## Что будет в приложении

- вход по инвайту и профили пользователей;
- каталог и библиотека: манга, аниме и книги в одном месте;
- несколько источников у одного тайтла и ручное связывание;
- ридер манги и книг, плеер видео;
- синхронизация прогресса между устройствами.

## Стек

- Фронтенд: React 19, TypeScript 6, Vite 8, shadcn/ui (Base UI), Tailwind CSS 4, TanStack Query 5, React Router 8, i18next.
- Бэкенд: Rust 2024, axum 0.8, tokio, tower-http; sqlx + SQLite, reqwest, scraper — по фазам плана.

## Быстрый старт

Нужны Rust, Node.js и pnpm.

```powershell
pnpm install
pnpm dev:server   # терминал 1: axum на 127.0.0.1:3000
pnpm dev          # терминал 2: Vite на 1420
```

Откройте http://localhost:1420 — Vite проксирует API на сервер. Правка UI не пересобирает Rust.

Проверка прод-раздачи локально:

```powershell
pnpm build
pnpm dev:server
```

Откройте http://127.0.0.1:3000 — сервер отдаст SPA и `/api/health`. Конфиг сервера — `BIND_ADDR`, `STATIC_DIR`, `RUST_LOG` (пример в `.env.example`).

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
