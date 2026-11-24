---
paths:
  - "src/**"
---

# Фронтенд

Раскладка и границы импортов - `docs/architecture.md`; их проверяет `pnpm lint` (`boundaries/dependencies`).

- **Дома.** `src/lib/` - утилиты, настройки, i18n; `src/ui/` - shadcn и общие компоненты без знания экранов; `src/api/` - единственный транспорт и хуки областей; `src/shell/` - оболочка окна; `src/pages/<экран>/` - экран и его части.
- **Экран.** `src/pages/<имя>/index.tsx`, компонент по имени экрана (`HomePage`). Импортирует экран только `src/app/AppRouter.tsx`. Пункты меню и маршруты - `src/shell/sidebar/sections.ts`.
- **Транспорт.** `fetch`/`invoke` - только `src/api/client.ts`. Хуки `api.*` вызываются в начале компонента; внутри `if` линтер не поймает.
- **Настройки.** Чтение - `useSetting(key)`, запись - `settings.set(key, value)`. Прямой `localStorage` - только `src/lib/settings.ts` и `src/shell/navbar/ThemeToggle.tsx`.
- **Строки.** Только `src/lib/i18n/locales/{en,ru}.ts` через `useT()`. В компонентах литералов интерфейса нет.
- **Тема.** Цвета - токены (`bg-primary`, `text-muted-foreground`), не hex и не `bg-[#...]`.
- **Base UI.** У `Button` нет `asChild` - есть `render`; не-`<button>` требует `nativeButton={false}`; отправке формы нужен явный `type="submit"`.
- **React Query.** Ключ `[имя команды или путь, ...аргументы]`; инвалидация в хуке через `onSuccess`, страница об этом не знает.
- **Импорт shadcn.** Код копируется в `src/ui/shadcn/` и принадлежит проекту; имена вендорных файлов не переименовываем.
- **Переезд на сервер.** `HashRouter` -> `BrowserRouter`, `QueryClient` под HTTP, `@tauri-apps/*` из `src/` уходят; транспорт остаётся в `src/api/client.ts`.
