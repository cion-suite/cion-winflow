---
paths:
  - "package.json"
  - ".claude/scripts/**"
  - "vite.config.ts"
---

# Сборка и проверки

- **Гейт фазы:** `pnpm typecheck` -> `pnpm lint` -> `pnpm test` -> `pnpm check:rust` -> `/isgood` -> `/notes`.
- `pnpm typecheck` - `tsc --noEmit`; `pnpm build` - `tsc && vite build`.
- `pnpm test` - Rust-тесты; `pnpm check:rust` - `cargo fmt --check` + `clippy -D warnings`. Оба идут через `.claude/scripts/run-cargo.mjs`.
- **Windows и cargo.** Обычный shell не знает `INCLUDE`/`LIB`; `run-cargo.mjs` находит установку Visual Studio с настоящими x64-библиотеками и вызывает `cargo` после `vcvars64.bat`. Если не нашёл - запускай из Developer PowerShell.
- `--manifest-path` подставляется сам: `server/Cargo.toml`, пока его нет - `src-tauri/Cargo.toml`.
- **Lock-файлы.** `pnpm-lock.yaml`, `src-tauri/Cargo.lock` (позже `server/Cargo.lock`) коммитятся. `node_modules`, `dist`, `src-tauri/target`, `src-tauri/gen/schemas`, `server/target`, `data/`, `.env*` - нет.
- **Статика.** `dist/` отдаётся с диска (`ServeDir`/Caddy); `rust-embed` не используем, иначе правка UI требует пересборки Rust.
- **Перед merge** - `/ship`: typecheck, lint, Rust-тесты, fmt/clippy, механический скан, циклы импортов, build.
