# Устройство приложения

Серверная версия: React SPA в `src/` и сервер `server/` на axum. В разработке — два процесса (Vite и axum), в проде — один origin. Целевая архитектура и фазы — [planned-now.md](planned-now.md).

## Слои фронтенда

Код `src/` разложен по слоям. Импорты между файлами `.ts` и `.tsx` проверяет `pnpm lint`. Имена файлов линтер не проверяет, это договорённость.

| Слой | Что лежит | Сейчас в приложении |
|---|---|---|
| `app/` | запуск: `index.tsx`, провайдеры, маршрутизатор, глобальные стили | `AppProvider.tsx`, `AppRouter.tsx`, `styles/index.css` |
| `shell/` | оболочка окна: то, что не принадлежит ни одному экрану | `layout/`, `navbar/` (шапка, поиск по библиотеке, меню пользователя, язык, тема) |
| `pages/<имя>/` | экраны, по папке на экран | `title/`, `library/`, `profile/`, `admin/`, `reader/`, `watch/`, `changelog/`, `settings/`, `login/`, `register/` |
| `api/` | HTTP-транспорт и хуки областей | `client.ts`, `auth.tsx`, `catalog.ts`, `chapters.ts`, `library.ts`, `profile.ts`, `discover.ts`, `progress.ts`, `admin.ts`, `sourceCredentials.ts`, `errors.ts`, `index.ts` (области собираются здесь) |
| `ui/` | `shadcn/` и мелкие общие компоненты | `shadcn/`, `PageHeader.tsx`, `PlaceholderPage.tsx`, `AuthShell.tsx`, `TitleForm.tsx`, `CoverImage.tsx`, `LibraryCard.tsx`, `Pagination.tsx`, `Textarea.tsx`, `AccountMenu.tsx` |
| `lib/` | утилиты, хуки и типы без UI | `utils.ts`, `settings.ts`, `routes.ts`, `nav.ts`, `reader.ts`, `catalog.ts`, `apiErrors.ts`, `i18n/` |

Файл `.ts` или `.tsx` вне этих папок — ошибка линтера (`boundaries/no-unknown-files`).

## Кто кого импортирует

| Слой | Может импортировать |
|---|---|
| `app` | `shell`, `pages`, `api`, `ui`, `lib` |
| `shell` | `api`, `ui`, `lib` |
| `pages` | `api`, `ui`, `lib` |
| `api` | `lib` |
| `ui` | `lib` |
| `lib` | ничего из этих слоёв |

Внутри своей папки импортировать можно всё. Снаружи `api` доступен только через `@/api` и `@/api/client`.

## Куда положить код

- Нужен одному экрану — в папку этого экрана.
- Компонент нужен нескольким экранам — в `ui/`, данные он получает пропсами.
- Хук или утилита нужны нескольким экранам — в `lib/`.
- Логика приложения — в серверный сервис или адаптер источника.

## Экраны и меню

Экран — `pages/<имя>/index.tsx`, рядом лежат его компоненты. Файл называется `index`, а компонент — по экрану (`export function LibraryPage`), импорт выглядит так: `@/pages/library`. Импортировать экран может только `app`, экраны друг друга не импортируют.

Сайдбара нет: корневой экран `/` — библиотека. Шапка содержит только логотип со ссылкой на `/`, поиск по библиотеке и меню пользователя; его пункты и фильтр по роли задаёт `lib/nav.ts` (`MENU_NAV`, `menuItemsForRole`), администратор видит пункт «Админка», у каждого есть «Профиль» (`/profile`, чужой — `/profile/:id`). Меню пользователя — тонкая обёртка над `ui/AccountMenu.tsx`: страницы не могут импортировать `shell`, поэтому у читалки своя обёртка `pages/reader/ReaderUserMenu.tsx` с тем же меню. Статические маршруты `PAGES` перечислены в `AppRouter` явно, `/library` перенаправляет на `/`.

Параметрические маршруты (`/title/:id`, `/profile/:id`, `/reader/:titleId/:chapterKey`, `/watch/:titleId/:chapterKey`) перечислены отдельным списком `PARAM_ROUTES` в `AppRouter`. Ссылки на них собирай через `src/lib/routes.ts`: `chapterKey` содержит `:` и `.`, поэтому кодируется `encodeURIComponent`.

## Авторизация

Сервер выдаёт сессию в HttpOnly-cookie `sid`. `AuthProvider` (`api/auth.tsx`) один раз запрашивает `GET /api/me`:

- `null` — гость: 401 превращается в `null`, это не ошибка;
- `user` — вошедший пользователь.

`AppRouter` делит маршруты на публичные (`/login`, `/register`, вне `Layout`) и защищённые: `<RequireAuth>` показывает загрузку или ошибку, а гостя уводит на `/login` с `state.from`. После входа экран возвращает на `from`.

`useLoginMutation` и `useRegisterMutation` кладут ответ в кэш `["me"]`; `useLogoutMutation` очищает весь кэш React Query, чтобы данные прошлого пользователя не остались на общем браузере. 401 от любого запроса тоже очищает кэш — `RequireAuth` уводит на вход. 401 не ретраится (`AppProvider`).

Снаружи `src/api` импортируется только через `@/api` (хуки, провайдер) и `@/api/client` (`request`, `ApiError`) — это проверяет `eslint-plugin-boundaries`. Ошибки с известным `code` переводятся в i18n через `lib/apiErrors.ts` и хук `useErrorText` (`api/errors.ts`); незнакомый код показывается текстом сервера.

## Библиотека, поиск и тайтл

- **Библиотека** (`pages/library/`) — корневой экран. `GET /api/library?q=&status=&tab_id=&sort=&order=&page=&page_size=` возвращает `{ items, total, page, page_size, counts }`; у элемента есть `status`, `custom_tab_id`, `read_count` (эффективный прогресс: `max(chapter_reads, imported_read_count)`), `imported_read_count`, `available_count` (объединение доступных глав включённых источников по каноническим ключам с учётом alias) и краткая карточка тайтла. `counts` — общие счётчики для вкладок (`all` и `statuses`; встроенные статусы считаются только без `custom_tab_id`). `PUT /api/library/{title_id}` (`{ status, score, custom_tab_id }`) вставляет или обновляет запись, `DELETE` убирает. Статусы: `in_progress|planned|favorite|completed|dropped|on_hold|not_interested`, «Любимое» — обычный статус; `custom_tab_id` — своя закладка вместо встроенного статуса, `null` — встроенный. Оценка — 1..10 или `null`. Сортировка — `updated|added|title|chapters|rating` с отдельным `order=asc|desc`; `updated` — дата последней опубликованной главы, `rating` — оценка источника. Карточка показывает оценку пользователя звёздочкой в правом нижнем углу обложки и прогресс `read_count/available_count` (например, `930/17`); если доступных глав нет, показывается только числитель.
- **Свои закладки — это статусы** — не больше трёх на пользователя: `GET/POST /api/library/tabs`, `PUT/DELETE /api/library/tabs/{id}`. У тайтла ровно один статус: `library.custom_tab_id` перекрывает встроенный `status`; удаление закладки (`ON DELETE SET NULL`) возвращает сохранённый встроенный статус. Мультивыбора и `library_tab_items` больше нет (миграция 0007). `GET /api/titles/{id}` отдаёт `library.custom_tab_id`, чтобы Select показал выбранную свою закладку.
- **Добавление тайтла** — диалог на экране библиотеки: `GET /api/discover/search?q=` ищет по всем включённым источникам (ошибка одного источника возвращается в `errors`, а не ломает выдачу). Выдача сортируется по приоритету источника и дедуплицируется по нормализованному названию: на тайтл остаётся одна карточка — от самого приоритетного источника. `POST /api/discover/add` (`{ code, external_id }`) импортирует тайтл на сервер, если его там нет, и добавляет запись в библиотеку со статусом `planned`. Если тайтл с тем же нормализованным названием или alt name уже существует, источник привязывается к нему. Затем тот же поиск `source_discovery`, что работает при открытии страницы, ищет тайтл на остальных включённых источниках того же языка: сначала по основному названию, если ничего не нашлось — по альтернативным. Поиск идёт под общей блокировкой на тайтл; если открытие страницы уже ищет, добавление поиск пропускает. Ошибки отдельных источников логируются и не отменяют добавление. Тип импортированного тайтла берётся из метаданных источника. Доступно любому пользователю с сессией; маршрут `add` под rate limit, потому что ходит в чужую сеть. `in_library` и `title_id` в выдаче позволяют показать «Уже в библиотеке».
- **Поиск в шапке** (`shell/navbar/LibrarySearch.tsx`) — тот же `GET /api/library` с `q` и небольшим `page_size`; ищет только по своей библиотеке.
- **Тайтл** (`pages/title/`) — `GET /api/titles/{id}?lang=` отдаёт `{ title, library, chapters, sources, aliases, read_keys, view_lang }`. `library` — `{ status, score, custom_tab_id }` текущего пользователя или `null`; `chapters` сгруппированы по каноническому `chapter_key` и содержат выбранный вариант, `published_at` и `source_enabled`; `read_keys` — прочитанные ключи; `view_lang` — каким языком сервер реально отфильтровал главы (`null` — фильтра нет). Запрос читает только БД. Под обложкой — кнопка «Читать» с первой главы, «Продолжить» с сохранённого прогресса или «Прочитано», когда прогресс на последней главе и страница последняя/предпоследняя (`page >= page_count - 2`); в состоянии «Прочитано» это disabled-кнопка, а не ссылка. Select статуса (на месте прежней кнопки «Любимое»; после встроенных — свои закладки с разделителем) и удаление. Вкладки: «Главы» (первая), «О тайтле» и «Настройки» (только админ). Если у тайтла есть включённые источники двух и более языков, под названием вместо счётчика глав — пилюля-переключатель языка просмотра; выбор хранится в `?lang=` и в `localStorage` по id тайтла, а сервер фильтрует главы, выбранный вариант, карточку «Источники» и связки (`content` и переходы читалки несут `lang`). Язык метаданных `titles.lang` при этом не меняется. Недоступные главы (`locked`/`missing`) в списке скрыты, остальные фильтруются и сортируются по номеру/тому. Глаз — кнопка: прочитанная глава приглушает всю карточку и показывает перечёркнутый глаз; если у главы есть непрочитанные предыдущие, глаз предлагает «Только эта глава» или «Эта глава и все предыдущие» и пишет пачку через `PUT /api/titles/{id}/chapters/read`. Справа от глав на широком экране — рейтинг и информативные карточки «Источники» (иконка, название, число глав), без действий. На «О тайтле» — описание, метаданные, а у администратора ещё правка тайтла. В «Настройках» — только **связки глав выбранного языка**, одна строка на диапазон, слева связанная глава, справа каноническая, под иконкой связи — «Связано N» (а «ожидают N» показывается, только когда их больше нуля), удаление — в конце строки; клик по иконке связи открывает редактор с выбором канонической стороны и предпросмотром пар. Правая колонка на этой вкладке пустая, но занимает место: ширина не меняется между вкладками. В диалогах связок и скане участвуют только источники выбранного языка. Кнопка «Найти расхождения» ищет серии глав с одинаковым номером и устойчивым сдвигом тома и предлагает диапазоны; односторонние пропуски показываются замечаниями. Оставшиеся одиночные ручные пары — в сворачиваемом блоке «Точечные связи». Канонический ключ задаёт том и номер в списке, поэтому связка исправляет неверную нумерацию источника; при пересборке прогресс и `chapter_reads` переезжают на canonical, а новая глава входит в диапазон при следующем sync. Порядок источников задаётся в `/admin`; бейджа «Предпочтительный» на карточках нет. Привязка источников из UI убрана. При открытии страницы фронтенд вызывает `POST /api/titles/{id}/sync`; сервер от имени текущего пользователя ищет тайтл на ещё не привязанных источниках того же языка, привязывает точные совпадения и только потом, по TTL, решает, обновлять ли главы, обложки и общие метаданные. Если найден новый источник, ответ приходит с `sources_linked`, и страница инвалидируется даже при `refreshed=false`. Auth-ошибки поиска показываются подсказкой со ссылкой на настройки, а карточка «Источники» — «подключить аккаунт».
- **Админка** (`pages/admin/`) — две колонки источников («Русские»/«Англоязычные»); порядок строк = приоритет, стрелки сохраняют список через `PUT /api/admin/source-priorities`, Switch включает/выключает через `PUT /api/sources/{code}`, гаечный ключ открывает popover с `base_url`, сбросом к `default_base_url` и probe. Ссылки захардкожены в дескрипторах адаптеров; `GET /api/sources` отдаёт `site_url`, `default_base_url` и `lang`. Ручное создание тайтла (`POST /api/titles` с формой `TitleForm`) остаётся. Импорта из источника в админке нет: тайтлы добавляются через библиотеку. После создания админ переходит на карточку тайтла.
- **Обложки.** Хранятся по размерам в `title_covers`: `low` — выдача поиска и диалог добавления, `mid` — карточки библиотеки, `high` — страница тайтла. `POST /api/titles/{id}/cover?size=low|mid|high` (multipart, поле `file`, до 5 МиБ, тело до 6 МиБ) принимает JPEG/PNG/WebP/GIF, декодирует с лимитами в `spawn_blocking`, ресайзит под размер (320×480 / 640×960 / 1200×1800) и пишет WebP в `DATA_DIR/covers/{id}/{size}.webp`; `DELETE` с тем же `size` убирает размер и файл, если на него больше никто не ссылается (после миграции low и mid могли делить один файл). `GET /api/titles/{id}/cover?size=low|mid|high` отдаёт файл с `ETag` и `Cache-Control: private, max-age=86400`, а отсутствующий размер — из ближайшего сохранённого (сначала более крупный, затем мелкий); UI добавляет `v={updated_at}` как cache-buster. При импорте, привязке источника и ручном `POST /api/admin/refresh/{title_id}` сервер автоматически скачивает обложки адаптера (`SourceCovers`), но заполняет только незанятые размеры, поэтому ручную обложку не перетирает. Явная админская операция `POST /api/titles/{id}/cover/from-source` (`{ replace_manual?: boolean }`) перебирает привязанные источники в порядке preferred → primary → привязка и обновляет уже сохранённые source-размеры (обложка могла смениться у источника); ручные размеры (`title_covers.source_id IS NULL`, включая строки миграции 0005) заменяются только с `replace_manual=true`, а запись без файла на диске считается пустой и лечится. Ответ — `{ sources, imported, skipped_manual, errors }`: ошибка одного источника не отменяет остальные. `GET /api/titles/{id}` отдаёт `title.cover_sizes` и `title.manual_cover_sizes`. Оригинал не сохраняется и не отдаётся.
- **Типы и статусы** общие для формы и API — строковые константы в `lib/catalog.ts`. Тип произведения — свойство тайтла (`manga|manhwa|manhua|western_comic|ru_comic|indonesian_comic|other|original`), а не источника. Форма тайтла (`ui/TitleForm.tsx`) не знает об API: получает значения и колбэк.

## Профиль и аватары

- **Экран** (`pages/profile/`) — `/profile` показывает текущего пользователя, `/profile/:id` — любого по uid; оба маршрута в `Layout`. `useProfileQuery` берёт `GET /api/users/{id}/profile` с `status`, `page`, `page_size`. В шапке-карточке: аватар, ник, бейдж роли, дата регистрации, uid (по нему открывают профиль) и три счётчика — `stats.read_chapters`, `stats.counts.all`, `stats.counts.statuses.completed`. Ниже — те же вкладки встроенных статусов, что в библиотеке, и общая карточка `ui/LibraryCard`; `not_interested` в профиле не показывается. Вкладка и страница привязаны к uid: при переходе на другой профиль сбрасываются без эффекта.
- **Аватар** доступен только владельцу: кнопка-камера шлёт `POST /api/me/avatar` (multipart `file`), крестик — `DELETE /api/me/avatar`. Сервер обрезает картинку по центру до 512×512, кодирует WebP и кладёт в `DATA_DIR/avatars/{user_id}.webp` под per-user локом; `users.avatar_updated_at` — признак наличия, `users.avatar_version` — монотонный `ETag` и `?v=` (удаление версию не сбрасывает). `userAvatarUrl(id, avatar_updated_at, avatar_version)` показывается в профиле, шапке и меню читалки. В шапке вместо имени — только аватар, нажатие открывает меню аккаунта (`AccountMenu variant="avatar"`), пункт «Профиль» ведёт на `/profile`; в читалке тот же вид. После мутации инвалидируются `["me"]` и `["profile"]`, поэтому шапка обновляется без перезагрузки. Если файла нет, `AvatarImage` падает на `AvatarFallback` с инициалом.
- **Импорт/экспорт закладок** — в `/settings`, вкладка «Импорт/Экспорт»: одна карточка `ImportExportSettings` с иконкой Remanga. «Импортировать закладки» открывает `BookmarksImportDialog`, JSON уходит multipart-полем `file` в `POST /api/me/import/bookmarks`. Сервер отвечает заданием `{ job }` и запускает фоновую работу; `useBookmarkImportQuery` опрашивает `GET /api/me/import/bookmarks` каждые 1.5 с, пока `job.state === "running"`, и показывает счётчики и ошибки. Завершение задания инвалидирует `profile`, `library`, `library-tabs`, `title`, `chapter-content`, `progress`. Диалог смонтирован вне вкладок и продолжает опрос после закрытия. Рядом — «Скачать JSON» (`useExportBookmarksMutation` → `requestBlob`) на `GET /api/me/export/bookmarks`; браузер скачивает `bookmarks.json` в формате `{ remanga: [...] }`, ошибки сервера показываются в карточке. Под кнопками — подсказки: high-water у старых тайтлов появляется только после повторного импорта закладок, а экспорт включает только тайтлы, привязанные к Remanga.
- **Учётные данные источников** — вкладка «Источники» в `/settings` (`SourceCredentialsSettings`). `GET /api/me/source-credentials` отдаёт источники с `Capability::Auth`, признак `configured` и `login_supported` (оба флага — `Auth` и `Login`); `PUT/DELETE /api/me/source-credentials/{code}` сохраняют (`{ bearer?, cookie? }`, payload заменяется целиком) и удаляют личную учётку. `POST /api/me/source-credentials/{code}/login` (`{ login, password }`) вызывает v1-логин Remanga, сохраняет полученный токен как Bearer и сразу ставит `last_verified_at`; пароль не хранится и не логируется, 2FA отвечает 409 `source_login_2fa_required`, неверная пара — 400 `source_login_failed`, блокировка сайта — 502 `source_blocked`; на пользователя действует лимит 5 попыток за 10 минут (429 + `Retry-After`). `POST /api/me/source-credentials/{code}/verify` проверяет сохранённый токен лёгким запросом Remanga `users/current`: успех ставит `last_verified_at` и чистит `last_error`, отклонение — 502 `source_auth_invalid` + `last_error='auth_invalid'`; маршрут под IP rate limit. Секреты шифруются AEAD-ключом из env и никогда не возвращаются API; UI — одна строка на источник: иконка, название, цветной статус (зелёный пульс — проверено меньше 24 ч назад, жёлтый — не проверено или проверено давно, красный — `auth_invalid`, серый — не подключено), справа иконка «Аккаунт» с поповером логин/пароль, «Проверить» и «Отключить»; ручные поля `type="password"` убраны из UI, но `PUT` с `{ bearer?, cookie? }` остаётся серверным API. После мутации инвалидируются `source-credentials`, `title`, `library`, `chapter-content`. Сохранённая учётка используется в запросах текущего пользователя к источнику: `Authorization`/`Cookie` уходят только на хосты источника (API и сайт), но не на CDN картинок. Ошибки авторизации переводятся в `source_auth_not_configured`/`source_auth_maybe_required` (403, «подключите аккаунт») и `source_auth_invalid` (502, «учётка отклонена»); наружу никогда не отдаётся HTTP 401, чтобы фронтенд не принял его за истёкшую сессию. Карточка «Источники» показывает «подключить аккаунт», если `auth_required` и у пользователя нет учётки.
- **Аккаунт** — третья вкладка `/settings` (`AccountSettings`): аватар (загрузка/удаление, `POST/DELETE /api/me/avatar`), никнейм (`PUT /api/me/username`, после успеха `me` обновляется, `profile` инвалидируется) и пароль (`PUT /api/me/password`: текущий пароль проверяется, новый — от 10 символов; текущая сессия остаётся, остальные сессии пользователя удаляются). Шапка профиля показывает аватар и ник только для чтения; кнопок редактирования там нет.
- **Публичность** — внутри инсталляции: профиль читает любая сессия, но аноним получает 401. Пароль и сессии не отдаются, `custom_tab_id` в публичной выдаче — `null`; «Не интересно» скрыто из счётчиков и списка.

## Главы, ридер и прогресс

- **Главы** — `GET /api/titles/{title_id}/chapters/{chapter_key}/content?lang=`: `lang` ограничивает выбор варианта источниками выбранного языка (неизвестный язык игнорируется); сервер сам выбирает вариант по известной недоступности (`locked`/`missing` — в конец) → приоритет источника → preferred → `is_primary` → `ordinal_site`, делает fallback и возвращает `{ chapter_id, chapter_key, number_text, name, content_kind, source, content, fallback? }`. Для картинок `content.pages[]` содержит внутренние `image_url` вида `/pages/{id}/image`. Повторное открытие уже известной главы отдаёт страницы из БД без сетевого запроса. Глава с `auth_required` открывается только с собственной свежей учёткой пользователя (`last_verified_at` не старше 24 ч); без неё — 403 `source_auth_not_configured`, с отклонённой — 502 `source_auth_invalid`. Первое открытие обычной главы идёт без учётки, а при 401/404 повторяется с ней. `GET /api/chapters/{id}/content` — прямой вариант без fallback для отладки; правила учётки те же.
- **Ридер** (`pages/reader/`) — маршрут `/reader/:titleId/:chapterKey` живёт вне `Layout`. Верхняя панель: слева стрелка и название тайтла одной ссылкой (логотипа нет), по центру номер главы `том-номер` со стрелками переходов, справа выбор статуса (встроенные и свои закладки одним списком) и меню пользователя. На телефоне внизу страница, настройки (шторка) и выбор статуса; на широком — счётчик, настройки справа (Sheet) и выбор статуса в шапке. Клик по странице переключает панели, прокрутка вниз после небольшого хода скрывает их (на телефоне все); прокрутка вверх состояние не меняет. Ширина колонки (`readerWidth`) — `sm`…`6xl` или `full`, в настройках выбирается Slider по шагам. Высоты страниц считаются по фактическим пропорциям картинки: широкие масштабируются до колонки, узкие остаются в натуральную ширину, поэтому кадры разного размера не дают вертикальных пробелов. Виртуальное окно — по накопленным высотам; выше и ниже текущей монтируется небольшое окно, префетч идёт последовательно по одной странице за окном, после смены ширины текущая страница сохраняется. Смена главы пересоздаёт состояние через `key`. Прогресс сохраняется с дебаунсом, а при `visibilitychange`/`pagehide` и размонтировании — через `fetch(..., { keepalive: true })`.
- **Прогресс** — `GET/PUT /api/progress/{title_id}`; `PUT` принимает `chapter_key` и одно поле позиции (`page` для картинок, `seconds` для видео, `scroll` для текста). Сервер резолвит alias → canonical и проверяет поле по `content_kind` выбранного варианта. `page` — 0-based индекс `pages.idx`.
- **Изображения** — браузер не ходит на чужие сайты: `GET /api/pages/{id}/image` и `GET /api/sources/{code}/cover?ref=` резолвят стабильную ссылку через адаптер, проверяют allowlist хостов, кэшируют байты на диске и отдают с `ETag`, `nosniff` и `Cache-Control: private`. Страницы главы с `auth_required` без учётки не отдаются; полученные с личной учёткой картинки кэшируются отдельно (`user:{id}`), публичные — в общем кэше. 401/403/410 при скачивании картинки не помечают главу `locked`: это может быть персональная auth-проблема или блокировка сайта.
- **Источники** — админские `GET /api/sources`, `PUT /api/sources/{code}`, `PUT /api/sources/{code}/config`, `POST .../search`, `POST .../probe`; отдельного HTTP-импорта нет, импорт выполняют `discover/add` и привязка к тайтлу. Remanga v2 — JSON API (`external_id` тайтла — `dir`, главы — числовой `id`). RuMix — HTML на движке Grouple (`external_id` — slug, главы — `data-id`, поиск POST-формой, картинки по подписанным URL, платная глава — 302 на покупку). Mangamir — HTML (`external_id` — slug, главы — `data-chapter-slug`, поиск `GET /manga?q=`, метаданные из JSON-LD, картинки `img.mangamir.com/pages/...`; полный список глав — Livewire `showAllChapters` с CSRF-токеном и куками того же GET). Импорт идёт `search → import → chapters`, страницы — лениво при первом `/content`. Привязка к существующему тайтлу — `POST /api/titles/{id}/sources` (в UI убрана). `POST /api/titles/{id}/sync` обновляет главы, обложки и общие метаданные по приоритету не чаще раза в 10 минут; поиск недостающих источников имеет собственный per-user TTL (`title_source_checks`) и идёт независимо от этого ограничения.

## Настройки

`lib/settings.ts` хранит пары ключ-значение в `localStorage` под ключом `cion-reader:settings`. Ключи — `locale` и `readerWidth` (ширина колонки читалки, по умолчанию `3xl`). Сервер хранит аккаунт (ник, аватар, пароль), но читательские настройки пока остаются в браузере; их перенос на сервер — следующий шаг. Битый JSON логируется, а не молча заменяется значениями по умолчанию.

Порядок запуска в `app/index.tsx` важен: `initSettings()` читает сохранённые значения, `initI18n()` берёт из них язык, и только потом приложение рендерится — иначе интерфейс мигал бы языком по умолчанию. Компоненты читают настройку через `useSetting(key)` и пишут через `settings.set(key, value)`.

## Язык интерфейса

Переводы — i18next и react-i18next. Все строки под пространством имён `common` в `lib/i18n/locales/en.ts` и `ru.ts`; состав языков — `resources.ts` (`SUPPORTED_LOCALES`, `DEFAULT_LOCALE`). В компонентах `useT()` возвращает функцию перевода, `useLocale()` — текущий язык.

Выбранный язык лежит в настройке `locale`; `initI18n()` подписан на её изменение и вызывает `i18n.changeLanguage`. Переключатель — `shell/navbar/LocaleMenu.tsx`. При первом запуске берётся язык системы, если он есть в `SUPPORTED_LOCALES`.

## Тема

Тема — это класс `dark` на `<html>`. `public/theme.js` ставит его до первой отрисовки: берёт выбор из `localStorage` (ключ `theme`), а если выбора нет, то тему системы. Файл внешний, а не inline-скрипт, чтобы CSP не требовал `unsafe-inline` или хеша. `shell/navbar/ThemeToggle.tsx` переключает класс и сохраняет выбор.

## Страница «Что нового»

`pages/changelog/` импортирует `docs/versions/release.md` как текст (`?raw`) и разбирает разделы `### Версия vX.Y.Z` с буллетами (`release-notes.ts`). Отдельного запроса к бэкенду нет: страница всегда соответствует файлу на момент сборки.

## Связь с бэкендом

```
pages/<экран>/index.tsx → api/<область>.ts → api/client.ts → /api/...
```

| Файл | За что отвечает |
|---|---|
| `api/client.ts` | `request<T>(path, init)`: путь относительно префикса API; `apiUrl(path)` для `<img src>` и ссылок; `jsonMutation(body, method)` для JSON-мутаций; `ApiError` с `status` и `code` |
| `api/auth.tsx` | `AuthProvider`, `useAuth`, хуки входа/регистрации/выхода, смена ника и пароля |
| `api/catalog.ts` | Тайтл с главами и источниками, создание/правка, загрузка обложки, `titleCoverUrl` |
| `api/chapters.ts` | `useChapterContentQuery`, `pageImageUrl`, типы страниц и fallback |
| `api/library.ts` | Библиотека: список с фильтрами и счётчиками, upsert, удаление, свои закладки |
| `api/profile.ts` | Профиль по uid (статистика и библиотека), `userAvatarUrl`, загрузка и удаление своего аватара, импорт закладок (`useBookmarkImportQuery`, `useBookmarkImportMutation`), экспорт (`useExportBookmarksMutation`) |
| `api/sourceCredentials.ts` | Личные учётки источников: список, вход по логину/паролю, проверка токена и удаление (`useSourceCredentialsQuery`, `useLoginSourceCredentialMutation`, `useVerifySourceCredentialMutation`, `useDeleteSourceCredentialMutation`); ручное сохранение (`PUT`) осталось серверным API; секретов в ответах нет |
| `api/discover.ts` | Поиск по источникам и ленивый импорт (`search`, `add`), `sourceCoverUrl` |
| `api/progress.ts` | `useProgressQuery`, `useSaveProgressMutation`, `progressApi.flush` (keepalive) |
| `api/errors.ts` | `useErrorText`: `ApiError.code` -> ключ i18n (`lib/apiErrors.ts`) |
| `api/<область>.ts` | Хуки области: `useQuery` для чтения, `useMutation` для изменений |
| `api/index.ts` | Собирает области в объект `api` и реэкспортирует публичные хуки и типы |

Правила:

- **`fetch` только в `client.ts`.** Ссылки, которые браузер запрашивает сам (картинки, файлы), собираются через `apiUrl`.
- **Ошибки.** Сервер отвечает `{ code, message }` с корректным HTTP-статусом. `request` превращает его в `ApiError`; сетевой сбой — `ApiError` со `status: 0` и `code: "network_error"`. 401 не повторяется: `AuthProvider` чистит кэш, `RequireAuth` уводит на `/login`.
- **Пустой ответ.** `204` и пустое тело дают `undefined`.
- **Ключ запроса** — `[имя области или путь, ...аргументы]`. Перечитывание данных после изменения настраивается в хуке через `onSuccess` и `invalidateQueries`, а страница об этом не знает.
- **Типы** сверяются с Rust вручную: `request<T>` ничего не проверяет во время работы.
- **`QueryClient`** в `AppProvider.tsx` создаётся с умолчаниями TanStack Query — это HTTP-клиент: повторы и обновление при фокусе окна.

Автотестов фронтенда нет, поэтому после изменения `client.ts` проверь вручную в консоли браузера при запущенных `pnpm dev` и `pnpm dev:server`:

1. `await import("/src/api/client.ts").then((m) => m.request("/health"))` — объект со `status: "ok"`.
2. `await import("/src/api/client.ts").then((m) => m.request("/missing"))` — `ApiError` со `status: 404`, `code: "not_found"` и сообщением из JSON.
3. Остановить сервер и повторить тот же запрос — `ApiError` со `status: 0`, `code: "network_error"`.
4. `request("/me")` без cookie — `ApiError` со `status: 401`, без автоматического повтора.
5. Ответ `204` или пустое тело — `undefined`.

## Сервер

```
server/
  migrations/
    0001_auth.sql     users, invites, sessions
    0002_catalog.sql  sources, titles, source_entries, chapters, aliases,
                      pages, chapter_texts, library, progress, chapter_reads
    0003_source_entry_per_source.sql  один source_entry на (title_id, source_id)
    0004_library_tabs.sql  статус not_interested, favorite, library_tabs,
                           library_tab_items
    0005_title_covers.sql  обложки по размерам low/mid/high
    0006_favorite_status.sql  favorite становится статусом, колонка удалена
    0007_library_custom_status.sql  своя закладка становится статусом,
                                    library_tab_items удалена
    0008_title_types.sql  типы тайтла; kind убран из source_entries/priorities
    0009_title_metadata_sync.sql  снимок метаданных тайтла
    0010_alias_ranges.sql  chapter_alias_ranges; source_entry_id/origin у alias
    0011_user_profiles.sql  users.avatar_updated_at (файл — DATA_DIR/avatars)
    0012_avatar_version.sql  монотонный users.avatar_version для ETag
    0013_library_tab_source_ref.sql  source_ref своей закладки для импорта
    0014_user_source_credentials.sql  зашифрованные учётки источников на пользователя
    0015_source_auth_flags.sql  auth_required у source_entries и chapters
    0016_library_imported_progress.sql  imported_read_count (high-water) в library
    0017_title_source_checks.sql  per-user кэш поиска тайтла на источниках
  src/
    main.rs          конфиг, БД, DataDir, bootstrap админа, tracing, запуск
    app.rs           Router: health, auth, catalog, titles, library, discover,
                     rate limit, слои
    auth/            handlers, service, password, session, middleware, limiter
    catalog/         service (каталог, тайтл), handlers, cover; selection (приоритеты
                     и fallback), chapters (группировка глав), content (выдача главы),
                     links (attach/detach, primary, chapter_aliases),
                     alias_ranges (связки-диапазоны, материализация, прогресс)
    images/          cache (диск, single-flight), handlers (прокси страниц и обложек)
    library/         service (библиотека, встроенные и свои статусы, видимость
                     Owner/Public), progress (high-water, автопометка новых
                     доступных глав), handlers
    profile/         service (профиль по uid, статистика), handlers, avatar (multipart,
                     квадратный WebP, ETag)
    discover/        поиск по всем источникам и ленивый импорт для библиотеки
    bookmarks/       импорт закладок Remanga/ExManga: parse (статусы, слияние),
                     export (JSON-формат Remanga), service (фоновое задание,
                     прогресс), handlers
    progress.rs      чтение и запись прогресса по chapter_key + chapter_reads
    source_admin/    service (реестр, конфиг, поиск, импорт, probe, refresh),
                     priorities (kind/lang), handlers
    source_auth/     per-user учётки источников: crypto (AEAD), service
                     (upsert/login/list/delete, расшифровка), handlers
    source_discovery/ поиск тайтла на ещё не привязанных источниках при
                     открытии: кандидаты, кэш title_source_checks, бюджет, timeout
    sources/         dto, descriptor, config, error, http (SSRF-клиент, circuit breaker,
                     POST-форма), keys (общий chapter_key), registry, contract (тесты),
                     remanga/ (JSON API), rumix/ (HTML), fixture.rs (debug-заглушка)
    config.rs        env и допустимые origin
    db.rs            пул SQLite (WAL), миграции при старте
    state.rs         AppState: конфиг, пул, реестр источников, HTTP-клиенты, кэш, семафоры
    error.rs         AppError -> { code, message } (+ 404/422/502/503-варианты)
    extract.rs       ApiJson/ApiQuery/ApiPath: ошибки извлечения тоже { code, message }
    pagination.rs    page/page_size: значения по умолчанию и потолок
    static_files.rs  ServeDir, SPA fallback, заголовки кэша
  tests/http.rs      health, API 401/404, SPA fallback, /assets
  tests/auth.rs      вход, инвайты, сессии, смена ника/пароля, rate limit
  tests/catalog.rs   тайтлы, поиск, пагинация, обложки, права admin
  tests/library.rs   библиотека, оценки, изоляция пользователей
  tests/library_tabs.rs  фильтры, счётчики, встроенные и свои статусы
  tests/profile.rs  профиль по uid, статистика, «не интересно», аватары
  tests/bookmarks.rs  импорт закладок: статусы, прогресс, свои закладки, экспорт, идемпотентность
  tests/imported_progress.rs  high-water: 930/17, кламп экспорта, автопометка новых глав
  tests/discover.rs  поиск по источникам, ленивый импорт, отметка прочитанного
  tests/discover_auto_link.rs  автопривязка источников по названию (основному и альтернативному)
  tests/discover_on_open.rs  discovery при открытии: привязка, кэш, TTL ошибок
  tests/sources.rs   источники, импорт, главы, прогресс, кэш картинок, fallback
  tests/multi_source.rs  два источника, приоритеты, preferred, alias, группы, attach/detach
  tests/alias_ranges.rs  связки-диапазоны: новые главы, перекрытие, прогресс, старые alias
  tests/source_credentials.rs  учётки источников: шифротекст, изоляция, очистка, ключ
  tests/source_login.rs  вход в Remanga по паролю: токен, 2FA, коды, лимит попыток
  tests/source_auth.rs  учётки в запросах Remanga: заголовки, коды, sync, проверка
  tests/content_auth.rs  гейтинг контента и картинок по учётке пользователя
  tests/common/remanga.rs  локальный двойник Remanga API для auth-тестов
  tests/migration.rs     апгрейд базы 0007 → 0009 без потери дочерних строк
  tests/mangamir_livewire.rs  Mangamir: полный список глав через Livewire (локальный сервер)
  tests/live_remanga.rs  live-проверка API Remanga (ignored, `pnpm test -- --ignored`)
  tests/live_rumix.rs    live-проверка HTML RuMix (ignored)
```

- `GET /api/health` отдаёт `{ "status": "ok", "version": "..." }` без сессии и проверяет БД (`SELECT 1`); ошибка БД — 500.
- Публичные пути перечислены явно: `/api/health` и `/api/auth/*`. Остальной `/api` требует cookie `sid`: без сессии — JSON 401, неизвестный путь с сессией — JSON 404.
- Auth: `POST /api/auth/register` (по инвайту, сразу создаёт сессию), `POST /api/auth/login`, `POST /api/auth/logout`, `GET /api/me`; админские `GET/POST /api/admin/invites`. `user` в ответах входа и `/api/me` содержит `avatar_updated_at` (`null` — аватара нет) и `avatar_version`.
- Профиль: `GET /api/users/{id}/profile?status=&sort=&order=&page=&page_size=` — `{ user, stats, library }` любой сессии. `stats.read_chapters` — размер журнала `chapter_reads`, `stats.counts` — счётчики библиотеки; `library` — выдача `/api/library` в режиме `LibraryVisibility::Public`: `not_interested` скрыт и в счётчиках, и в списке, `tab_id` игнорируется, записи своих закладок считаются под встроенным статусом (иначе вкладки не сходились бы со списком), а `custom_tab_id` в выдаче — `null`. Неизвестный или отключённый uid — 404 `user_not_found`.
- Аватары: `POST/DELETE /api/me/avatar` — только свой, multipart `file` до 5 МиБ (JPEG/PNG/WebP/GIF), `image::Limits` в `spawn_blocking` с семафором, `resize_to_fill` до 512×512, WebP в `DATA_DIR/avatars/{user_id}.webp`, атомарная запись tmp + rename под per-user `avatar_lock` (загрузка и удаление не должны оставить версию без файла); `users.avatar_updated_at` — признак наличия, `users.avatar_version` — монотонный счётчик. `GET /api/users/{id}/avatar` отдаёт `image/webp` с `ETag` по версии, `Cache-Control: private, max-age=86400`, `nosniff`; нет строки/файла — 404 `avatar_not_found`. Тело маршрута — `DefaultBodyLimit::max(6 МиБ)`.
- Импорт закладок: `GET/POST /api/me/import/bookmarks` — только текущий пользователь. `POST` принимает multipart `file` (JSON до 3 МиБ, до 5000 записей), разбирает `{ exmanga?, remanga? }` и сразу возвращает `{ job }`, запуская фоновое задание; `GET` отдаёт последнее задание пользователя (`null`, если импортов не было). Задание: `state` (`running|completed|failed`), `total/processed/succeeded/failed`, `titles_imported`, `library_created/library_updated`, `read_marked`, `errors` (до 200) и `errors_truncated`. Одно активное задание на пользователя — 409 `import_in_progress`; внутри задания до трёх параллельных тайтлов; `429` источника повторяется после `Retry-After` (до 120 с); `POST` под отдельным IP rate limit, `GET` страница настроек опрашивает каждые 1.5 с. Статусы: `type` 1..7 Remanga и непрерывный блок из семи значений вне этого диапазона; остальные — кастомные, для них создаётся закладка `new1/new2/...` с `source_ref`. Если тайтла нет на Remanga (404 по `dir`, а затем по точному названию; 404 без учётки тоже считается отсутствием), импорт ищет точное название на остальных включённых источниках с поиском и главами — в порядке приоритета их языка — и импортирует найденное, чтобы закладка не терялась. `progress` — число прочитанных глав: первые доступные канонические главы пишутся в `chapter_reads`, `progress` ставится на последнюю из них. Число из файла сохраняется в `library.imported_read_count` (монотонный high-water), а статус перезаписывается по эффективному `max(chapter_reads, imported_read_count)`, чтобы 930 импортированных глав не выглядели как «ещё не читал».
- Экспорт закладок: `GET /api/me/export/bookmarks` — только текущий пользователь. Отдаёт `bookmarks.json` (`{ remanga: [{ type, progress, progress_total, name, dir }] }`) с `Content-Disposition: attachment` и `Cache-Control: no-store`. В файл попадают тайтлы с привязкой к Remanga; `progress` — эффективный `read_count`, но не больше `chapter_count` (high-water клампится), `progress_total` — максимум `chapter_count`, `type` — встроенный 1..7 или исходный id своей закладки из `source_ref`; ручные закладки экспортируются под встроенным статусом.
- Аккаунт: `PUT /api/me/username` (`{ username }`, те же правила, что при регистрации; занятый ник — 409 `username_taken`) возвращает `{ user }`; `PUT /api/me/password` (`{ current_password, new_password }`, новый пароль от 10 символов) проверяет текущий, хеширует Argon2 и удаляет все сессии пользователя, кроме текущей; неверный текущий пароль — 400 `invalid_password`, маршрут под rate limit.
- Учётки источников: `GET /api/me/source-credentials` (`{ credentials: [{ code, name, configured, updated_at, last_verified_at, last_error }] }`) перечисляет источники с `Capability::Auth`; `PUT /api/me/source-credentials/{code}` (`{ bearer?, cookie? }`) заменяет payload, `DELETE` удаляет. Плейнтекст — JSON `{bearer,cookie}` — шифруется ChaCha20-Poly1305 с AAD `user_id:source_id:key_version`; ключ 32 байта base64 из `SOURCE_CREDENTIALS_KEY`, в БД только ciphertext/nonce. Без ключа маршруты учёток отвечают 503 `credentials_unavailable`; неверный или неизвестный источник — 404 `source_not_found`/400 `source_credentials_invalid`. Сохранение/удаление сбрасывает `titles.metadata_synced_at` у тайтлов с этим источником, чтобы sync не ждал TTL. API никогда не возвращает секреты. Учётка используется в `search`, `title`, `chapters` и `content` Remanga от имени текущего пользователя; заголовки уходят только на хосты источника. Успешный авторизованный запрос обновляет `last_verified_at`, 401 — `last_error='auth_invalid'`. `source_entries.auth_required` и `chapters.auth_required` (миграция 0015) фиксируют, что источник/глава требует вход; auth-ошибка одного пользователя не перезаписывает глобальный `source_entries.last_error` и не двигает TTL sync.
- Каталог: `GET /api/catalog` (поиск по `name_lower`, фильтр `kind`, пагинация) остаётся серверным API, но отдельного экрана в UI нет; `GET /api/titles/{id}` отдаёт `read_keys`, `title.cover_sizes` и `title.manual_cover_sizes`; админские `POST /api/titles`, `PUT /api/titles/{id}`, `POST/DELETE /api/titles/{id}/cover?size=low|mid|high`, `POST /api/titles/{id}/cover/from-source`; `GET /api/titles/{id}/cover?size=low|mid|high` доступен любой сессии.
- Библиотека: `GET /api/library?q=&status=&tab_id=&sort=&page=&page_size=` (`{ items, total, page, page_size, counts }`), `PUT /api/library/{title_id}` (`{ status, score, custom_tab_id }`), `DELETE /api/library/{title_id}` — всегда для текущего пользователя. `favorite` — полноценный статус наравне с остальными, отдельного флага нет; `custom_tab_id` — своя закладка вместо встроенного статуса. `status` фильтрует только записи без своей закладки, `tab_id` — по своей. Свои закладки: `GET/POST /api/library/tabs`, `PUT/DELETE /api/library/tabs/{id}`; не больше трёх, имя уникально у пользователя, удаление возвращает встроенный статус. У элемента `read_count` — эффективный прогресс (`max(chapter_reads, imported_read_count)`), `available_count` — объединение доступных глав включённых источников по каноническим ключам (alias → canonical): это знаменатель карточки.
- Поиск и импорт для пользователя: `GET /api/discover/search?q=&kind=` собирает выдачу включённых источников с поиском и главами (ошибки — в `errors` вместе с `error_code`), `POST /api/discover/add` импортирует найденный тайтл при необходимости и добавляет его в библиотеку (`planned`); если тайтл уже импортирован, но обложек у него нет, `add` добирает их из источника. Импорт, автопривязка и поиск идут с учёткой текущего пользователя. Поиск на остальных источниках и поиск при открытии страницы (`source_discovery`) — один и тот же код под общей блокировкой на тайтл; результаты поиска без привязки кэшируются в `title_source_checks` на пользователя (`not_found` — 24 ч, `auth_required`/`error` — 1 ч). Импорт идёт тем же кодом, что админский, но без роли admin; маршрут `add` под rate limit.
- `PUT /api/progress/{title_id}` дополнительно пишет `chapter_reads` по каноническому ключу: карточка библиотеки показывает эффективный `read_count`, страница тайтла — отметку прочитанного. `PUT /api/titles/{title_id}/chapters/read` (`{ chapter_keys, read }`) отмечает главы вручную пачкой, не трогая прогресс, и возвращает полный `read_keys`; неизвестный ключ отклоняет весь запрос.
- Пагинация каталога и библиотеки — общая `pagination::resolve`: `page >= 1`, `page_size` по умолчанию 50, потолок 200; `page=0` — 400 `validation`.
- Обложки: multipart-поле `file` до 5 МиБ, только JPEG/PNG/WebP/GIF, декодирование с `image::Limits` в `spawn_blocking` с семафором; результат — WebP-файл запрошенного размера (`low`/`mid`/`high`) в `DATA_DIR/covers/{id}/`, запись атомарная (tmp + rename). Оригинал не отдаётся; ответ с `ETag`, `nosniff`, `Cache-Control: private`. «Получить из источника» идёт тем же путём загрузки через прокси и allowlist источника; `manual_cover_sizes` — размеры с `source_id IS NULL`.
- Пароли — Argon2id в `spawn_blocking` с семафором. Токен сессии — 32 случайных байта, в БД только SHA-256; idle 30 дней, абсолютный срок 90. Cookie `sid`: `HttpOnly`, `SameSite=Lax`, `Secure` по `COOKIE_SECURE`, `Max-Age` 30 дней.
- Мутации (`POST`/`PUT`/`PATCH`/`DELETE`) требуют разрешённый `Origin`; `Sec-Fetch-Site: cross-site` отклоняется. В dev разрешены localhost/127.0.0.1 на `:1420` и `:3000`; в release без `PUBLIC_ORIGIN`/`ALLOWED_ORIGINS` мутации запрещены.
- Rate limit: IP — `tower_governor` (login — burst 10, пополнение раз в 2 с; register — burst 5, раз в 10 с; discover add — burst 6, раз в 4 с), username — счётчик в памяти (10 попыток за 15 минут). `X-Forwarded-For` читается только от адресов из `TRUSTED_PROXIES`.
- Первый администратор — `ADMIN_USERNAME`/`ADMIN_PASSWORD` при пустой таблице `users`; повторно не создаётся.
- БД: `DATABASE_URL` (по умолчанию `sqlite://data/cion.db`), WAL, `foreign_keys=ON`, `busy_timeout=5s`, `create_if_missing`; миграции — `sqlx::migrate!` при старте. Время — unix seconds.
- `index.html` получают только навигационные запросы: `Sec-Fetch-Mode: navigate` или `Accept: text/html`. Путь вида `/reader/1/c12.5` не считается файлом по расширению и работает.
- Отсутствующий `/assets/*` — 404 даже при навигации: ошибка деплоя не маскируется.
- `dist/assets/*` отдаются с `public, max-age=31536000, immutable`, остальное — `no-cache`.
- Источники: `sources` — реестр скомпилированных адаптеров; при загрузке реестр добавляет недостающие строки с `base_url = NULL`, поэтому приоритеты видят все адаптеры. `PUT /api/sources/{code}` заводит `base_url` и `enabled` (отсутствующий `base_url` не стирает прежний), `PUT .../config` мержит и валидирует `config_json` (422 со списком полей), `POST .../search`, `POST .../probe`; отдельного HTTP-импорта нет, импорт выполняют `discover/add` и привязка к тайтлу. Адаптеры не знают о БД; `AppState.sources` — `Arc<RwLock<SourceRegistry>>`, guard не держится через `.await`. Битый конфиг отключает только свой источник и персистится в `sources.config_error`.
- Связи тайтла (admin): `POST/DELETE /api/titles/{id}/sources`, `PUT /api/titles/{id}/primary-source`, `GET/POST/DELETE /api/titles/{id}/chapter-aliases`, `POST /api/titles/{id}/chapter-aliases/bulk` (пачка до 500 одной транзакцией: проверка цепочек после вставки, перенос `progress`/`chapter_reads` на canonical; клиент дробит длинные пачки), `POST /api/admin/refresh/{title_id}`. Alias — один hop, цепочки запрещены; перепривязка alias к другому canonical — 409 `alias_exists` (сначала удалить); при отвязке висячие alias удаляются. `POST refresh` возвращает per-entry статус, сеть идёт до транзакции.
- Связки-диапазоны (admin): `GET/POST /api/titles/{id}/alias-ranges`, `PUT/DELETE .../alias-ranges/{range_id}`, `POST .../alias-ranges/apply` (`{ ranges: [{ source_entry_id, anchor_chapter_id, canonical_chapter_id }] }`, пачка одной транзакцией). Связка привязана к `source_entry_id` и якорю-главе, действует от якоря до следующей связки этого источника и типа главы. Точные пары `chapter_aliases` с `origin = 'range'` и `source_entry_id` — производная: пересобираются после refresh/import, detach и CRUD связок; ручные пары (`source_entry_id IS NULL`) не трогаются. Смещение вычисляется из координат якорей; если источник поправил нумерацию якоря, связка получает `needs_review` и не пересчитывается. Если канонической главы ещё нет, пара не создаётся («ожидает»). Старые alias сворачиваются в диапазоны при первом `GET /alias-ranges`, только непрерывные серии с одинаковым смещением. Прогресс/`chapter_reads` переносятся на новый canonical одним проходом; при удалении связки остаются на canonical.
- Приоритеты (admin): `GET/PUT /api/admin/source-priorities?lang=`; ранг 0-based, список заменяется целиком, после сохранения preferred пересчитывается у всех тайтлов. Набор языка тайтла полностью заменяет `*`. Effective enabled = `enabled AND base_url IS NOT NULL AND config_error IS NULL`; такие источники не участвуют в preferred, статистике страницы и выбранном варианте. **Ранг важнее числа глав**: preferred выбирается по `rank` → `is_primary` → `chapter_count`, а вариант главы — по доступности → `rank` → preferred. Один `source_entry` на `(title_id, source_id)`.
- Синхронизация тайтла: `POST /api/titles/{id}/sync` (любой пользователь, rate limit, TTL 10 минут). Сначала запускается `source_discovery` для ещё не привязанных включённых источников того же языка (не больше трёх источников и шести поисков на источник, общий таймаут; найденное привязывается и снимает TTL-ранний выход). Ответ содержит `sources_linked` и `discovery: { checked, skipped, errors }`; per-user auth-ошибка поиска не двигает `metadata_synced_at`. Затем обновляет главы всех entry, пересчитывает preferred, обновляет source-обложки и сливает метаданные из всех источников по приоритету: первый непустой `name`/`description`/`status`/`year`/`rating`/`lang`, первый конкретный `kind`, списки объединяются. `titles.source_metadata_json` хранит прошлый снимок; поле, отличающееся от него, считается ручной правкой и не перезаписывается. После обновления глав новые доступные главы отмечаются прочитанными по high-water каждого пользователя; позиция `progress` при этом не двигается.
- Импорт: адаптер заново получает метаданные по `external_id`, затем главы; запись — upsert в транзакции. `chapter_key` фиксируется при первом импорте (расхождение — в `source_entries.last_error`). После импорта считаются `chapter_count`/`available_count`/`locked_count`/`missing_count` и `titles.preferred_source_id`. Если у пользователя для тайтла есть `imported_read_count`, появившиеся или разблокировавшиеся главы в пределах `min(high-water, доступные)` отмечаются прочитанными; давно доступные главы, снятые вручную, не восстанавливаются.
- Выдача главы: `GET /api/titles/{title_id}/chapters/{chapter_key}/content?lang=` сам выбирает вариант (при `lang` — только из источников этого языка) и делает fallback; `locked`/`missing` кешируются по TTL (6/24 ч), временные ошибки не меняют доступность. `GET /api/chapters/{id}/content` — прямой вариант без fallback.
- Картинки: `GET /api/pages/{id}/image` и `GET /api/sources/{code}/cover?ref=`; обложка из выдачи источника доступна любому пользователю с сессией (нужна диалогу добавления), не только админу. Ключ кэша страницы — `(source_entry_id, external chapter id, idx)`, запись атомарная, одновременные запросы дедуплицируются. SQLite-статусы `401/403/410` запускают до двух перезапросов списка страниц; затем глава помечается `locked`.
- SSRF: общий DNS-резолвер отбрасывает private/loopback/link-local/CGNAT/ULA/IPv4-mapped; редиректы у HTTP-клиентов выключены, порты ограничены (loopback — только тестам). URL источника берётся из реестра, а не от клиента; `external_id` валидируется. `SSRF_ALLOW_LOOPBACK` (по умолчанию `false`) нужен только интеграционным тестам с локальным сервером картинок.
- Конфиг из env: `BIND_ADDR` (по умолчанию `127.0.0.1:3000`), `STATIC_DIR` (`dist`), `DATABASE_URL`, `DATA_DIR` (`data`), `REGISTRATION_MODE` (`invite|closed`), `PUBLIC_ORIGIN`, `ALLOWED_ORIGINS`, `COOKIE_SECURE`, `TRUSTED_PROXIES`, `SSRF_ALLOW_LOOPBACK` (только тесты), `SOURCE_CREDENTIALS_KEY` (32 байта base64; без него учётки источников отключены), `ADMIN_USERNAME`/`ADMIN_PASSWORD`, `RUST_LOG`. В dev `.env` читает `dotenvy`, пример — `.env.example`.
- Статика читается с диска при каждом запросе: обновление `dist/` не требует перезапуска сервера. `rust-embed` не используем.

## Разработка

```powershell
pnpm dev:server   # axum, 127.0.0.1:3000
pnpm dev          # Vite, 1420, прокси /api -> http://127.0.0.1:3000
```

CORS не нужен: в dev работает прокси, в проде origin один. Правка `.ts`/`.tsx`/CSS пересобирает только Vite; правка Rust требует перезапуска процесса `dev:server` (или `cargo watch`).

Проверка прод-раздачи: `pnpm build`, затем `pnpm dev:server` и http://127.0.0.1:3000. После пересборки UI сервер перезапускать не нужно — `dist/` читается с диска.
