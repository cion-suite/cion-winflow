# cion-reader — план работ

- **Дата:** 2026-11-28
- **История:** прежние версии плана — в git. Этот файл — исполняемая очередь.

> Выполнять сверху вниз по таблице «Очередь». Фазы 6 → 7 идут по порядку; строки с `[done]` пропускать; условные — только при выполненном условии.
> Авторизация источников (решения, проверенные факты Remanga, спайки, каталог вариантов и отложенные фазы 5б/5в/8–11) — в [authorization.md](authorization.md).

## Как пользоваться

- Следующая работа — первая строка очереди без `[done]` и без невыполненного условия.
- Перед началом сверь `git status` / `git log`: не осталось ли незакоммиченных правок плана и кода.
- После фазы: гейт, отметить `[done]` в таблице, остановиться.
- Промпт для новой сессии:

> Работаем по `docs/planned-now.md`. Прочитай план, `CLAUDE.md`, `docs/authorization.md` и решения 0017–0020. Найди первую строку очереди без `[done]` и предложи дифф-план; дождись подтверждения. После реализации прогони гейт.

## Очередь

| # | Фаза | Статус |
|---|---|---|
| 1–4 | Учётки, auth в запросах, high-water, discovery | `[done]` |
| 5а | Вход в Remanga по логину/паролю | `[done]` |
| — | ExManga как источник платных глав (вход по email/паролю) | `[done]` |
| 6 | Прямые картинки с CDN | `[ ]` |
| 7 | Egress к Remanga | `[ ]` условная: только если проверка целевого хоста дала 403 |
| 5б, 5в, 8–11 | Отложенные (2FA, refresh, pairing, userscript, расширение, OAuth) | `—` не входят в очередь, см. [authorization.md](authorization.md) |

### Что уже сделано (фазы 1–5а)

| Фаза | Результат | Ключевые файлы |
|---|---|---|
| 1 | Учётки per-user, AEAD, API, вкладка «Авторизации» | `0014_user_source_credentials.sql`, `server/src/source_auth/`, `src/pages/settings/SourceCredentialsSettings.tsx` |
| 2 | Учётка в `search/title/chapters/content`, auth-коды, гейтинг контента и картинок | `0015_source_auth_flags.sql`, `server/src/sources/http.rs`, `catalog/content.rs`, `images/` |
| 3 | High-water `930/17`, автопометка, кламп экспорта | `0016_library_imported_progress.sql`, `server/src/library/progress.rs` |
| 4 | Discovery при открытии, per-user кэш | `0017_title_source_checks.sql`, `server/src/source_discovery/` |
| 5а | Вход по логину/паролю, токен как зашифрованная Bearer-учётка, 2FA-ошибка, лимит попыток | `server/src/source_auth/`, `server/src/sources/remanga/api.rs`, `server/tests/source_login.rs` |
| — | ExManga: поиск/главы публично, контент по JWT, запрет Remanga-хостов | `server/src/sources/exmanga/`, `server/tests/exmanga.rs`, решение 0023 |

## Гейт после каждой фазы

1. `pnpm typecheck`
2. `pnpm lint`
3. `pnpm test`
4. `pnpm check:rust`
5. `/isgood`
6. `/notes`
7. Отметить `[done]` в таблице «Очередь».
8. STOP & WAIT.

## Фаза 6 — Прямые картинки с CDN

**Что:** читалка грузит страницы глав напрямую с `img3.reimg2.org`, минуя серверный прокси. Трафик и IP — читателя; серверный прокси остаётся fallback.

**Шаги:**

1. **Хосты.** Не переписывать `image_ref` (стабильная ссылка для прокси/fallback); вычислять direct-хост при выдаче DTO. `img3.reimg.org` → `img3.reimg2.org`; для неподдерживаемого старого хоста direct URL не отдавать. Уже сохранённые `pages.image_url` со старым хостом нормализовать при выдаче или разовым refresh.
2. **DTO.** `server/src/catalog/content.rs`: `ContentPageDto` уже содержит `width`/`height` — добавить только `direct_image_url`. Заполнять в **двух** местах: `replace_pages` (свежий ответ) и `load_pages` (кэш); SELECT в `load_pages` должен читать `pages.image_url`. Прокинуть `auth_required`, чтобы не отдавать direct URL закрытым главам.
3. **Читалка.** `src/api/chapters.ts`, `src/pages/reader/index.tsx`: предпочитать `direct_image_url`, на `onError` — fallback на `/pages/{id}/image`.
4. **Гейтинг.** Для `chapters.auth_required=1` direct URL не отдавать — только прокси.
5. **CSP.** В этом плане CSP нет; при её появлении добавить `img3.reimg2.org` и `remanga.org` в `img-src`. Не путать с фазой 8 (отложенной).
6. **Тесты.** `server/tests/content_direct_images.rs`: публичная глава — direct URL есть; auth-required — нет; повторное (кэшированное) открытие — direct URL есть; старый хост — fallback.

**Готово, когда:** публичные главы грузятся с CDN, закрытые — через прокси; тесты зелёные.

**Минус:** теряется серверный кэш/ETag для прямых картинок — осознанно.

## Фаза 7 — Egress к Remanga (только если проверка целевого хоста дала 403)

**Что:** сервер выходит в Remanga через резидентный прокси; парсинг и кэш не меняются. Проверка — в разделе «Перед публичным запуском» ниже. Факты DDoS-Guard и варианты egress — в [authorization.md](authorization.md).

**Шаги:**

1. `server/src/config.rs`: `SOURCE_PROXY_URL` (env).
2. **Выбрать scope явно.** По умолчанию — install-wide: прокси применяется ко всем источникам и картинкам, не только к Remanga. Если нужен только Remanga — отдельный клиент для неё, а не правка общего `metadata_client`.
3. `server/Cargo.toml`: feature `socks`, если нужен SOCKS5.
4. `server/src/sources/http.rs`: в нужных фабриках после `.no_proxy()` вызвать `.proxy(Proxy::all(url)?)`; валидировать схему/порт прокси. **Trade-off:** прокси резолвит целевой хост, поэтому `SafeDns` больше не видит IP назначения; host-allowlist источника остаётся обязательным, прокси считается доверенным egress-компонентом.
5. **Тесты.** `server/tests/source_proxy.rs`: локальный прокси — запрос действительно прошёл через него (транспорт). Пропуск DDoS-Guard локальным тестом не проверяется — только ручной `curl` с целевого хоста.
6. Документация: `.env.example`, `CLAUDE.md`.

**Готово, когда:** запросы идут через прокси; локальный транспортный тест зелёный; ручная проверка с целевого хоста возвращает 200.

## Перед публичным запуском

Выполнять **на целевом хосте**, не на dev (домашний IP даёт 200 и ничего не доказывает):

```bash
curl -s https://api.ipify.org; echo
curl -sS -D - -o /dev/null 'https://api.remanga.org/api/v2/titles/?count=1' | head -n 15
```

- `200` → можно выкладывать.
- `403` + `ddos-guard` → выполнить фазу 7 до выкладки. Клиентский токен не спасает заблокированный сервер: edge блокирует до авторизации.

## Отложенные фазы

2FA (5б), refresh-пара (5в), pairing (8), юзерскрипт-мост (9), расширение (10) и OAuth (11) не входят в очередь. Условия запуска, шаги и проверенные факты — в [authorization.md](authorization.md#5-отложенные-фазы).

## Before merge

`/ship` — typecheck + lint + Rust-тесты + fmt/clippy + скан ветки + циклы + build. Ревью в нём нет — это `/isgood` каждой фазы.
