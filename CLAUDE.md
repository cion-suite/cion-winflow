# cc-tweaks

Десктопное приложение на Rust + Tauri 2. У проекта две цели:

1. **Шаблон** — от него будут стартовать другие приложения, поэтому код и конфиги остаются общими, без логики конкретного продукта.
2. **Обучение** — владелец проекта изучает Rust на практике.

Целевая платформа — десктоп (основная ОС разработки — Windows). Android/iOS не настроены.

## Режим обучения

- Когда в коде появляется новая для проекта концепция Rust (владение, заимствование, времена жизни, `Result`/`?`, трейты, `async`), коротко объясняй её на примере этого кода.
- Ошибки компилятора разбирай по сути: что он проверяет и почему код не прошёл. Не обходи их через `clone()`, `unwrap()` или `unsafe` без объяснения.
- На вопрос «как сделать» — объясни и покажи подход. На просьбу «сделай» — сделай и кратко поясни неочевидные решения.
- Предпочитай простой идиоматичный код продвинутым приёмам, если они не нужны для задачи.

## Стек

- Rust edition 2024, Tauri 2
- Фронтенд: React 19, TypeScript 6, Vite 8
- UI: shadcn/ui (база Base UI, стиль `base-nova`), Tailwind CSS 4
- Данные и навигация: TanStack Query 5, React Router 8
- Язык интерфейса: i18next + react-i18next (en, ru)
- Настройки: `@tauri-apps/plugin-store` (в браузере — `localStorage`)
- Проверки: ESLint 10 (`eslint-plugin-boundaries` держит границы слоёв)
- Пакетный менеджер: pnpm

## Структура

```
src/
  app/                     запуск: index.tsx (initSettings -> initI18n -> render), AppProvider, AppRouter, styles/index.css
  shell/                   оболочка окна: layout/, navbar/ (шапка, язык, тема), sidebar/ (меню разделов)
  pages/                   экраны, по папке на экран, файл экрана — index.tsx: home, network, changelog, settings
  api/                     связь с Rust: client.ts (call), <область>.ts (хуки команд), index.ts (объект api)
  ui/                      shadcn/ и общие компоненты: PageHeader.tsx
  lib/                     utils.ts (cn), settings.ts (настройки), i18n/ (переводы и useT)
index.html                 контейнер #root для React
components.json            настройки shadcn CLI
eslint.config.js           границы слоёв и правила линтера
vite.config.ts             порт 1420 (strictPort), должен совпадать с devUrl; плагины, алиас @
src-tauri/
  src/lib.rs               run() — настройка приложения; команды Tauri, когда появятся
  src/main.rs              точка входа, только вызывает run()
  Cargo.toml               зависимости Rust
  tauri.conf.json          имя, окна, сборка, безопасность
  capabilities/default.json  разрешения для окна main: ядро, тема окна, store
  gen/schemas/             генерируется, в git не попадает
```

## Команды

Запускать из корня репозитория. `Cargo.toml` лежит в `src-tauri/`, поэтому cargo нужен `--manifest-path`.

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

## Как добавлять команды Tauri

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
- `invoke` есть только в `src/api/client.ts`. `src/api` импортируют `pages`, `shell` и `app`, но не `ui`.
- Методы `api.*` — хуки: вызывай их только в начале компонента. Линтер не поймает `api.x.useY()` внутри `if`.

- Компоненты shadcn не ставятся как библиотека: их код копируется в `src/ui/shadcn/` и принадлежит проекту. Перед написанием своего компонента проверь, нет ли готового в shadcn.
- Цвета бери из токенов темы (`bg-primary`, `text-muted-foreground`), а не задавай вручную: так работает смена темы.
- Алиас `@/` задан в трёх местах: `paths` в `tsconfig.json`, `resolve.alias` в `vite.config.ts` и `aliases` в `components.json`. Меняй их вместе, иначе `shadcn add` без ошибки создаст файлы не в той папке.
- `tsconfig.json` один, `baseUrl` не нужен: в TypeScript 6 он устарел. Документация shadcn для Vite описывает другую схему (`tsconfig.app.json`, `baseUrl`), её не применяй.
- Вызов команды может завершиться ошибкой, всегда показывай её. Хуки `api.*` отдают `Error`, поэтому текст берётся из `error.message`.
- Строки интерфейса не пиши в компонентах: добавляй ключи в `lib/i18n/locales/en.ts` и `ru.ts` и бери их через `useT()`. Настройки читай через `useSetting()`, пиши через `settings.set()`.
- Пункты меню и маршруты задаются одной таблицей `shell/sidebar/sections.ts`; новый экран без записи в ней в роутер не попадёт.
- `Button` из Base UI без `type` рендерится как `type="button"` и форму не отправляет: кнопке отправки нужен явный `type="submit"`.
- У `Button` вместо `asChild` проп `render`. Если он рендерит не `<button>`, добавь `nativeButton={false}`. Ссылку так не делай: она получит `role="button"`. Стилизуй её напрямую: `<a className={buttonVariants()} href="...">`.

## Подводные камни

- **Не запускай `create-tauri-app --force` в этом репозитории.** Флаг удаляет существующие файлы (проверено: пропал `LICENSE`, заменились `README.md` и `.gitignore`); уцелел только `.git`.
- `src-tauri/tauri.conf.json` содержит `"csp": null`, то есть Content Security Policy отключена. Это настройка генератора, её нужно задать до реального использования приложения.
- `bundle.targets: "all"` включает MSI, а для него в Windows нужен компонент VBSCRIPT.
- `pnpm tauri dev` перезаписывает `src-tauri/target/debug/*.exe` dev-версией, которой нужен запущенный Vite. Чтобы проверить собранное приложение, сначала выполни `pnpm tauri build --debug --no-bundle`.
- `typescript-eslint` требует TypeScript ниже 6.1: не обновляй `typescript` до 7, пока этот диапазон не расширят.
- `Cargo.lock` и `pnpm-lock.yaml` коммитятся. `node_modules`, `dist`, `src-tauri/target` и `src-tauri/gen/schemas` игнорируются.

## Использование как шаблона

При создании приложения из шаблона замени имя и идентификатор:

| Файл | Что менять |
|------|------------|
| `package.json` | `name` |
| `src-tauri/Cargo.toml` | `name`, `[lib] name`, а также заглушки `description` и `authors` |
| `src-tauri/src/main.rs` | вызов `cc_tweaks_lib::run()` — имя из `[lib] name` |
| `src-tauri/tauri.conf.json` | `productName`, `identifier`, заголовок окна `title` |
| `index.html` | `<title>` |
| `src/pages/` и маршруты в `src/app/AppRouter.tsx` | экраны и их адреса: замените на свои |
| `src/shell/sidebar/sections.ts` | пункты меню: пути, ключи переводов, иконки |
| `src/api/` | области команд: пустой каталог под команды приложения |
| `src-tauri/src/lib.rs` | команды Rust и регистрация в `generate_handler!` |
| `src/lib/i18n/locales/` | переводы языков |
| `src/lib/settings.ts` | ключи настроек и их значения по умолчанию |
| `README.md`, этот файл, `docs/architecture.md` | название, описание и примеры |
| `docs/versions/release.md` | удали файл: это лог шаблона, новый создаст `/notes` |
| `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` | `version`, если новое приложение начинает не с 0.1.0 |

`identifier` должен быть уникальным для каждого приложения. Иконки лежат в `src-tauri/icons/`, новые генерируются командой `pnpm tauri icon <файл>`. Она также создаёт `icons/android/` и `icons/ios/`; для десктопа их можно удалить.

## Документация для ИИ

- Этот файл — точка входа: только то, что нужно почти в каждой задаче. Держи его коротким.
- Подробности выноси в `docs/` и ставь сюда ссылку. Папку и файлы создавай, когда появляется содержимое:
  - `docs/architecture.md` — устройство приложения;
  - `docs/decisions/NNNN-название.md` — принятые решения и их причины;
  - `docs/learning/` — конспекты по Rust.
- Лог версий для пользователей (`docs/versions/release.md`) пишет только команда `/notes`, руками его не правь. Порядок релиза — в `.claude/commands/notes.md`.
- Если изменение делает документацию неверной, обновляй её в той же задаче.
- Записывай только проверенные факты и не описывай то, чего ещё нет в коде.
