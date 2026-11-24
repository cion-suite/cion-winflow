# cion-reader

Приватный self-hosted агрегатор манги, аниме и книг: один каталог, одна библиотека и один прогресс для себя и небольшого круга читателей.

**Текущее состояние.** В репозитории пока десктопная версия на Tauri, оставшаяся от шаблона. Фронтенд (React, Vite, shadcn, TanStack Query) переходит в веб без переписывания; слой Tauri заменяется сервером на Rust (axum). План — [docs/planned-now.md](docs/planned-now.md), причины решения — [docs/decisions/0001-server-architecture.md](docs/decisions/0001-server-architecture.md).

## Что будет в приложении

- вход по инвайту и профили пользователей;
- каталог и библиотека: манга, аниме и книги в одном месте;
- несколько источников у одного тайтла и ручное связывание;
- ридер манги и книг, плеер видео;
- синхронизация прогресса между устройствами.

## Стек

- Фронтенд: React 19, TypeScript 6, Vite 8, shadcn/ui (Base UI), Tailwind CSS 4, TanStack Query 5, React Router 8, i18next.
- Бэкенд: Rust 2024, axum, tokio, sqlx + SQLite, reqwest, scraper.

## Быстрый старт (текущая десктопная версия)

```powershell
pnpm install
pnpm tauri dev
```

Нужны Rust, Node.js и pnpm. Проверки: `pnpm lint`, `pnpm build`, а для Rust — `cargo test`, `cargo fmt --check`, `cargo clippy` (команды — в [CLAUDE.md](CLAUDE.md)).

## Разработка

Рабочие команды: `/planner`, `/isplan`, `/isgood`, `/sync-docs`, `/ship`, `/notes`. Правила ревью — `.claude/rules/`, механика — `.claude/scripts/`.

## Документация

- [docs/architecture.md](docs/architecture.md) — устройство фронтенда;
- [docs/planned-now.md](docs/planned-now.md) — переезд на сервер;
- [docs/decisions/](docs/decisions/) — принятые решения.
