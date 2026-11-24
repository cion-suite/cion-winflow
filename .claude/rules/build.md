---
paths:
  - "package.json"
  - ".claude/scripts/**"
  - "vite.config.ts"
---

# Сборка и проверки

- **Гейт фазы:** `pnpm typecheck` -> `pnpm lint` -> `pnpm test` -> `pnpm check:rust` -> `/isgood` -> `/notes`.
- `pnpm typecheck` - `tsc --noEmit`; `pnpm build` - `tsc && vite build`.
- `pnpm test` - Rust-тесты; `pnpm check:rust` - `cargo fmt --check` + `clippy -D warnings`. Оба идут через `.claude/scripts/run-cargo.mjs` с `--manifest-path server/Cargo.toml`.
- **Windows и cargo.** Обычный shell не знает `INCLUDE`/`LIB`; `run-cargo.mjs` находит установку Visual Studio с настоящими x64-библиотеками и вызывает `cargo` после `vcvars64.bat`. Если не нашёл - запускай из Developer PowerShell.
- **Dev.** `pnpm dev:server` - axum на `127.0.0.1:3000`; `pnpm dev` - Vite на `1420` с прокси API. Правка UI не пересобирает Rust.
- **Lock-файлы.** `pnpm-lock.yaml` и `server/Cargo.lock` коммитятся. `node_modules`, `dist`, `server/target`, `data/`, `.env*` (кроме `.env.example`) - нет.
- **Статика.** `dist/` отдаётся с диска (`ServeDir`/Caddy); `rust-embed` не используем, иначе правка UI требует пересборки Rust.
- **Перед merge** - `/ship`: typecheck, lint, Rust-тесты, fmt/clippy, механический скан, циклы импортов, build.
