---
description: Pre-flight перед merge - typecheck, lint, Rust-тесты, fmt/clippy, скан ветки, циклы, build
argument-hint: "[range] [--no-build]"
allowed-tools: Bash, Read
---

# /ship - pre-flight перед merge

```bash
node .claude/scripts/ship.mjs $ARGUMENTS
```

Один вызов Bash с `timeout: 600000`. Аргументы: `[range] [--no-build]`; range по умолчанию `main...HEAD`, на `main` - `origin/main..HEAD`.

Скрипт параллельно гонит `typecheck`, `lint`, `test` (Rust), `check:rust` (fmt + clippy), `diff-scan --totals`, `graph cycles`; всё чисто - `build` (`pnpm build`). Полный вывод - `.claude/cache/ship/*.log`.

Rust-команды идут через `run-cargo.mjs`: на Windows он сам подхватывает окружение Visual Studio с настоящими x64-библиотеками. Если ни одна установка не подошла, будет обычный `cargo` и ошибка линковки - запусти из Developer PowerShell.

## Итог

Таблица скрипта и есть отчёт - отдай её пользователю как есть. Проверки руками не повторяй.

`-> ISSUES FOUND` (exit 1) при: ошибке typecheck / lint / тестов / fmt / clippy, провале build или самих скриптов скана / циклов, любом `CONFIRMED`, `NEW > 0` (цикл импортов внесли ветка или рабочая копия).
Не блокируют: `CHECK` - вход для `/isgood`; warnings lint - baseline; `TOUCHED` при `NEW=0` - старый цикл, который ветка лишь задела: это бэклог, не работа merge; `CYCLES` - все циклы репозитория.

Под каждым провалом - до 20 значимых строк и путь к логу. Лог открывай, только если их не хватило: `grep` / `Read` с `offset`, не целиком.

Уборка: READY - кеш удалил скрипт; ISSUES - после разбора `rm -rf .claude/cache/ship`.

Границы слоёв проверяет `lint` (`boundaries/dependencies`); Rust - clippy через `check:rust`.

Не делает: семантическое ревью - `/isgood`, commit-сообщение - `/notes`, релизную сборку сервера (`cargo build --release`).
