# cion-reader

Приватный self-hosted агрегатор манги, аниме и книг: каталог, библиотека и прогресс в одном месте для себя и небольшого круга читателей. Проект вырос из десктопного шаблона на Tauri и переезжает на веб: React SPA + сервер на Rust.

У проекта две цели:

1. **Продукт** — cion-reader для закрытого круга: авторизация, несколько источников, ридер и плеер.
2. **Обучение** — владелец проекта изучает Rust на практике.

Целевая платформа — веб-сервер (Linux). Пока в репозитории десктопная версия (Windows); Android/iOS не настроены.

## Текущее состояние и переход

Сейчас код — десктопное приложение на Tauri 2. Фронтенд (`src/`) перейдёт в веб без переписывания; слой `src-tauri/` будет заменён сервером `server/` на axum.

- Целевая архитектура, фазы и критерии готовности — [docs/planned-now.md](docs/planned-now.md).
- Причины выбора и альтернативы — [docs/decisions/0001-server-architecture.md](docs/decisions/0001-server-architecture.md).

Пока серверного слоя нет в коде, всё ниже про Tauri и `src-tauri/` актуально. После переезда этот файл обновляется вместе с кодом.

## Режим обучения

- Когда в коде появляется новая для проекта концепция Rust (владение, заимствование, времена жизни, `Result`/`?`, трейты, `async`), коротко объясняй её на примере этого кода.
- Ошибки компилятора разбирай по сути: что он проверяет и почему код не прошёл. Не обходи их через `clone()`, `unwrap()` или `unsafe` без объяснения.
- На вопрос «как сделать» — объясни и покажи подход. На просьбу «сделай» — сделай и кратко поясни неочевидные решения.
- Предпочитай простой идиоматичный код продвинутым приёмам, если они не нужны для задачи.

## Стек

Сейчас (десктоп):

- Rust edition 2024, Tauri 2
- Фронтенд: React 19, TypeScript 6, Vite 8
- UI: shadcn/ui (база Base UI, стиль `base-nova`), Tailwind CSS 4
- Данные и навигация: TanStack Query 5, React Router 8
- Язык интерфейса: i18next + react-i18next (en, ru)
- Настройки: `@tauri-apps/plugin-store` (в браузере — `localStorage`)
- Проверки: ESLint 10 (`eslint-plugin-boundaries` держит границы слоёв)
- Пакетный менеджер: pnpm

Целевой бекенд (после переезда): axum, tokio, sqlx + SQLite, reqwest, scraper, argon2, tower-http. Полный список крейтов — в плане.

## Структура (текущая)

```
src/
  app/                     запуск: index.tsx (initSettings -> initI18n -> render), AppProvider, AppRouter, styles/index.css
  shell/                   оболочка окна: layout/, navbar/ (шапка, язык, тема), sidebar/ (меню разделов)
  pages/                   экраны, по папке на экран, файл экрана — index.tsx: home, network, apps, utils, system, changelog, settings
  api/                     связь с Rust: client.ts (call), <область>.ts (хуки команд), index.ts (объект api)
  ui/                      shadcn/ и общие компоненты: PageHeader.tsx
  lib/                     utils.ts (cn), settings.ts (настройки), i18n/ (переводы и useT)
index.html                 контейнер #root для React
components.json            настройки shadcn CLI
eslint.config.js           границы слоёв и правила линтера
vite.config.ts             порт 1420 (strictPort); сейчас совпадает с devUrl, после переезда — proxy на axum; алиас @
src-tauri/
  src/lib.rs               run() — настройка приложения; команды Tauri, когда появятся
  src/main.rs              точка входа, только вызывает run()
  build.rs                 иконка exe: icons/cat.ico для dev, icons/cat_w.ico для release
  Cargo.toml               зависимости Rust
  tauri.conf.json          имя, окна, сборка, безопасность
  capabilities/default.json  разрешения для окна main: ядро, тема окна, store
  gen/schemas/             генерируется, в git не попадает
```

## Команды

Запускать из корня репозитория. `Cargo.toml` лежит в `src-tauri/`, поэтому cargo нужен `--manifest-path`. Команды относятся к текущей десктопной версии; после переезда cargo-команды будут работать с `server/Cargo.toml`, а `pnpm tauri *` исчезнут.

```powershell
pnpm install                                 # зависимости фронтенда
pnpm tauri dev                               # запуск с hot reload
pnpm build                                   # проверка типов (tsc) и сборка фронтенда
pnpm lint                                    # ESLint, в том числе границы слоёв
pnpm tauri build                             # релизная сборка с установщиками
pnpm tauri build --debug --no-bundle         # быстрая проверка полной сборки
pnpm exec shadcn add <компонент>             # добавить компонент shadcn

cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

Перед завершением задачи с Rust-кодом прогоняй `test`, `fmt --check` и `clippy`, с фронтендом — `pnpm lint` и `pnpm build`.

## Команды разработки

- `/planner <задача>` — фазовый план с quality gates и DRY-контрактом.
- `/isplan [файл]` — adversarial review плана (по умолчанию `docs/planned-now.md`).
- `/isgood [diff]` — adversarial review диффа и fix-loop.
- `/sync-docs` — сверка ссылок в документации с репозиторием.
- `/ship` — pre-flight перед merge: typecheck, lint, Rust-тесты, fmt/clippy, скан, циклы, build.
- `/notes` — changelog и commit-сообщение.

Правила для ревью — `.claude/rules/`, механика — `.claude/scripts/`, профили — `.claude/agents/`. Гейт после каждой фазы: `typecheck -> lint -> test -> check:rust -> /isgood -> /notes`.

## Как добавлять команды Tauri (до переезда)

> После переезда команда Tauri заменяется HTTP-эндпоинтом axum, а `call` в `src/api/client.ts` — на `fetch`. Правило слоёв сохраняется: HTTP-вызовы живут только в `src/api`.

Команда — это Rust-функция с `#[tauri::command]`. Вся логика живёт в Rust, а в `src/api/` для неё есть только хук с именем команды и типами.

- Добавь хук в `src/api/<область>.ts`, новую область подключи в `src/api/index.ts`. Страница вызывает его как `api.<область>.use<Имя>()`. Типы пишутся вручную, при правке сигнатуры в Rust меняй их тоже.
- Регистрируй все команды в одном `tauri::generate_handler![...]`: при повторном вызове `invoke_handler` работает только последний.
- В `lib.rs` команда не может быть `pub`, в отдельном модуле — не может быть приватной (обычно `pub`).
- Аргументы в JS пишутся в camelCase, в Rust — в snake_case (`userName` → `user_name`).
- Всё, что возвращает команда, включая ошибки, должно реализовывать `serde::Serialize`. Для ошибок используй `Result<T, E>` и не паникуй.
- `async`-команда с заимствованными аргументами требует особого обращения: для `&str` бери владеющий тип (`String`) или возвращай `Result`; для `State<'_, T>` подходит только `Result`.
- Плагинам нужны разрешения в `capabilities/default.json`.

Логику держи в обычных функциях без типов Tauri, а команды делай тонкой обёрткой над ними: такую логику проще тестировать через `#[cfg(test)]`.

## Фронтенд

Раскладка и правила импортов — в `docs/architecture.md`. Кто кого может импортировать — таблица там же, это проверяет `pnpm lint`.

- Все экраны лежат в `src/pages/<имя>/`. Экран импортируется только из роутера `src/app/AppRouter.tsx`.
- `invoke` есть только в `src/api/client.ts`; после переезда там будет `fetch`. `src/api` импортируют `pages`, `shell` и `app`, но не `ui`.
- Методы `api.*` — хуки: вызывай их только в начале компонента. Линтер не поймает `api.x.useY()` внутри `if`.

- Компоненты shadcn не ставятся как библиотека: их код копируется в `src/ui/shadcn/` и принадлежит проекту. Перед написанием своего компонента проверь, нет ли готового в shadcn.
- Цвета бери из токенов темы (`bg-primary`, `text-muted-foreground`), а не задавай вручную: так работает смена темы.
- Алиас `@/` задан в трёх местах: `paths` в `tsconfig.json`, `resolve.alias` в `vite.config.ts` и `aliases` в `components.json`. Меняй их вместе, иначе `shadcn add` без ошибки создаст файлы не в той папке.
- `tsconfig.json` один, `baseUrl` не нужен: в TypeScript 6 он устарел. Документация shadcn для Vite описывает другую схему (`tsconfig.app.json`, `baseUrl`), её не применяй.
- Вызов бэкенда может завершиться ошибкой, всегда показывай её. Хуки `api.*` отдают `Error`, поэтому текст берётся из `error.message`.
- Строки интерфейса не пиши в компонентах: добавляй ключи в `lib/i18n/locales/en.ts` и `ru.ts` и бери их через `useT()`. Настройки читай через `useSetting()`, пиши через `settings.set()`.
- Пункты меню и маршруты задаются одной таблицей `shell/sidebar/sections.ts`; новый экран без записи в ней в роутер не попадёт.
- `Button` из Base UI без `type` рендерится как `type="button"` и форму не отправляет: кнопке отправки нужен явный `type="submit"`.
- У `Button` вместо `asChild` проп `render`. Если он рендерит не `<button>`, добавь `nativeButton={false}`. Ссылку так не делай: она получит `role="button"`. Стилизуй её напрямую: `<a className={buttonVariants()} href="...">`.

## Подводные камни (текущая десктопная версия)

- **Не запускай `create-tauri-app --force` в этом репозитории.** Флаг удаляет существующие файлы (проверено: пропал `LICENSE`, заменились `README.md` и `.gitignore`); уцелел только `.git`.
- `src-tauri/tauri.conf.json` содержит `"csp": null`, то есть Content Security Policy отключена. Для веба CSP обязателен: задавай заголовками от сервера и не переноси `null` в прод.
- `bundle.targets: "all"` включает MSI, а для него в Windows нужен компонент VBSCRIPT.
- `pnpm tauri dev` перезаписывает `src-tauri/target/debug/*.exe` dev-версией, которой нужен запущенный Vite. Чтобы проверить собранное приложение, сначала выполни `pnpm tauri build --debug --no-bundle`.
- `typescript-eslint` требует TypeScript ниже 6.1: не обновляй `typescript` до 7, пока этот диапазон не расширят.
- `Cargo.lock` и `pnpm-lock.yaml` коммитятся. `node_modules`, `dist`, `src-tauri/target` и `src-tauri/gen/schemas` игнорируются.

После переезда Tauri-пункты (CSP, MSI, `tauri dev`, capabilities) уходят; общими остаются требования к версиям TypeScript и коммит lock-файлов.

## Происхождение

Проект начинался как десктопный шаблон `cion-winflow`. Теперь это продукт `cion-reader`; инструкции «как использовать репозиторий как шаблон» больше не поддерживаются, история осталась в git.

## Документация для ИИ

- Этот файл — точка входа: только то, что нужно почти в каждой задаче. Держи его коротким.
- Подробности выноси в `docs/` и ставь сюда ссылку. Папку и файлы создавай, когда появляется содержимое:
  - `docs/architecture.md` — устройство фронтенда и текущего десктопного слоя;
  - `docs/planned-now.md` — целевая архитектура и порядок переезда;
  - `docs/decisions/NNNN-название.md` — принятые решения и их причины;
  - `docs/learning/` — конспекты по Rust;
  - `.claude/rules/` — правила ревью и навигации; `.claude/commands/` — рабочие команды.
- Лог версий для пользователей (`docs/versions/release.md`) пишет только команда `/notes`, руками его не правь. Порядок релиза — в `.claude/commands/notes.md`.
- Если изменение делает документацию неверной, обновляй её в той же задаче.
- Записывай только проверенные факты и не описывай то, чего ещё нет в коде.
