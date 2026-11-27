# Авторизация источников

- **Дата:** 2026-11-28
- **Статус:** фазы 1–5а реализованы; 5б, 5в и 8–11 отложены.
- **Решения:** [0017](decisions/0017-per-user-source-credentials.md) (учётки per-user), [0020](decisions/0020-source-login.md) (вход по паролю).
- **Очередь работ:** [planned-now.md](planned-now.md).

Файл собирает в одном месте то, что раньше лежало приложениями плана: устройство
учёток, проверенные факты Remanga, результаты спайков, каталог вариантов и
отложенные фазы. Для выполнения текущей очереди он не обязателен, но при
работе с авторизацией его стоит прочитать целиком.

## 1. Что реализовано

### 1.1. Учётки на пользователя

- Таблица `user_source_credentials` с ключом `(user_id, source_id)`.
  Плейнтекст `{"bearer":"...","cookie":"..."}` шифруется ChaCha20-Poly1305;
  AAD `user_id:source_id:key_version`, случайный 12-байтный nonce. В БД только
  ciphertext/nonce.
- Ключ `SOURCE_CREDENTIALS_KEY` (32 байта base64) живёт только в env. Без него
  все маршруты учёток отвечают 503 `credentials_unavailable`. Потеря или
  замена ключа делает сохранённые учётки нечитаемыми.
- API не возвращает секретов: `GET /api/me/source-credentials` отдаёт
  `code`, `name`, `configured`, `login_supported`, `updated_at`,
  `last_verified_at`, `last_error`.
- Учётки общие для всех устройств: расширение нужно только чтобы получить
  учётку один раз, телефон читает под ней без установок.

### 1.2. Вход по логину и паролю (фаза 5а)

- `POST /api/me/source-credentials/{code}/login` `{ login, password }`.
  Сервер вызывает v1 `POST {base_url}/api/users/login/` `{user,password}`,
  получает `content.access_token` и сохраняет его как обычную Bearer-учётку.
- Пароль живёт в `SecretString`, тело запроса — в `Zeroizing`; пароль не
  хранится, не логируется и не возвращается API.
- Нужны оба флага дескриптора: `Capability::Auth` и `Capability::Login`.
  `login_supported` в API — их конъюнкция; форма в UI показывается только при
  `true`. Ручная вставка Bearer/Cookie остаётся серверным API (`PUT`), но из
  UI убрана: основной путь — вход по паролю.
- Успешный вход сразу ставит `last_verified_at`, поэтому помеченные главы
  доступны без первого «проверочного» запроса. Одновременно сбрасываются
  `titles.metadata_synced_at` и per-user кэш `title_source_checks`.
- Лимит: IP-лимит как у входа в приложение плюс 5 попыток / 10 минут на
  пользователя (429 + `Retry-After`); удачный вход сбрасывает окно.
- `POST /api/me/source-credentials/{code}/verify` проверяет сохранённый токен
  лёгким авторизованным запросом Remanga `users/current`. Успех ставит
  `last_verified_at` и чистит `last_error`; отклонённый токен — 502
  `source_auth_invalid` и `last_error='auth_invalid'`; без учётки — 403
  `source_auth_not_configured`. Маршрут под IP rate limit.
- 2FA в этой фазе не поддерживается: вход такому пользователю возвращает
  409 `source_login_2fa_required` (методы — в `message`).

### 1.3. Использование в запросах

- Учётка резолвится по `(user_id, source_id)` перед запросом и уходит в
  `search`, `title`, `chapters`, `content` от имени текущего пользователя.
- `Authorization`/`Cookie` добавляются только на хосты источника (API и сайт),
  но не на CDN картинок.
- Ошибки: 401 без учётки → 403 `source_auth_not_configured`; 401 с учёткой →
  502 `source_auth_invalid`; 404 Remanga без учётки → 403
  `source_auth_maybe_required`; 404 с учёткой → 404 `source_not_found`;
  403 сайта → 502 `source_blocked`. HTTP 401 наружу не отдаётся никогда.
- `source_entries.auth_required` и `chapters.auth_required` запоминают, что
  источник/глава требует вход. Первое открытие обычной главы идёт без учётки,
  при 401/404 повторяется с ней; успех с учёткой ставит флаг, успех без неё
  сбрасывает.
- Персональная auth-ошибка не меняет глобальную доступность: не пишется в
  `source_entries.last_error`, не ставит `locked`, не сдвигает
  `metadata_synced_at`.
- Картинки помеченной главы кэшируются в пространстве `user:{id}`, публичные —
  в общем кэше. 401/403/410 при скачивании не ставят глобальный `locked`.

### 1.4. UI и тесты

- В `/settings` → «Источники»: одна строка на источник — иконка, название,
  цветной статус (зелёный пульс — проверено недавно, жёлтый — не проверено
  или проверено давно, красный — `auth_invalid`, серый — не подключено).
  Справа: иконка «Аккаунт» открывает поповер с формой логин/пароль
  (`type="password"`, «Войти»), кнопка «Проверить» и «Отключить». Ручные поля
  Bearer/Cookie из UI убраны. После мутаций инвалидируются
  `source-credentials`, `title`, `library`, `chapter-content`.
- Ошибки переведены: `source_login_failed`, `source_login_2fa_required`,
  `credentials_unavailable`, `source_credentials_invalid`.
- Тесты: `server/tests/source_credentials.rs` (шифротекст, изоляция, ключ),
  `server/tests/source_auth.rs` (заголовки, коды, sync),
  `server/tests/content_auth.rs` (гейтинг контента и картинок),
  `server/tests/source_login.rs` (успех, 2FA, неверная пара, блокировка,
  лимит попыток). Двойник Remanga — `server/tests/common/remanga.rs`.

### 1.5. Что проверено вживую

- 2026-11-27: v1-логин вернул токен, 2FA на тестовом аккаунте выключен;
  токен принят всем конвейером `search/title/chapters/content`; закрытый
  тайтл открылся с токеном; первая проверенная глава была платной, поэтому
  страниц не было (для страниц брать `is_paid=false`). Флаг `adult=1`.
- 2026-11-28: вход по паре логин/пароль проверен владельцем в приложении —
  работает.
- Секреты (логин, пароль, токен) в этот файл не записываются.

## 2. Проверенные факты Remanga (2026-11-27)

### 2.1. Вход и токен

- **Вход по паролю (v1, использует сам сайт):**
  `POST https://api.remanga.org/api/users/login/` `{user, password}` →
  `{content: {access_token, ...}}`; `user` — логин или почта. Неверные данные:
  HTTP 400 `{msg: "Введен неверный логин/почта или пароль."}`.
- **Вход v2:** `POST /api/v2/users/login/` `{login, password}`; успешный формат
  без реального аккаунта не подтверждён.
- **2FA:** v1 — `{user,password,method}` отправляет код, `{user,password,code}`
  проверяет; код 6-значный. v2 — `/api/v2/users/login/2fa/issue/` и
  `/verify/`.
- **Bearer, не Django-сессия.** `sessionid`/`csrftoken` нет; только
  DDoS-Guard cookie `__ddg*`.
- **Замер:** v1-токен — 30 символов, непрозрачный; cookie `auth:token` — ~год;
  веб refresh не вызывает. Upgrade **не** отзывает v1-токен.
- **Обновление:** `POST /api/v2/users/token/refresh/` `{refresh_token}` →
  `{access_token, refresh_token, token_type, expires_in}`; `POST
  /api/v2/users/token/upgrade/` (Bearer, `{}`) → та же пара. `expires_in=900`
  (15 минут) у access-пары. Повтор старого refresh вернул 200 — ротация не
  enforced, но single-flight желателен.
- **Сессии независимы:** `/api/v2/users/sessions/`, `/logout/`,
  `/logout-all/`. Лимит устройств не документирован; смена пароля, вероятно,
  отзывает токены.
- **OAuth2-провайдер есть:** `https://remanga.org/oauth/authorize` (PKCE,
  consent), `POST /api/v2/oauth/token/`. Публичной регистрации клиента нет:
  неизвестный `client_id` → `invalid_client`; `/applications/`, `/clients/`,
  `/register/` → 404. Нужно, чтобы Remanga выдала `client_id`.
- **Соцвход:** VK, Google, Yandex, Discord, Telegram, Shikimori;
  `POST /api/users/social/` `{code, provider, redirect_uri}` →
  `content.access_token`; токен принимается как Bearer.

### 2.2. Доступ к контенту

- **18+ неоднородно:** часть тайтлов открыта анонимно, часть отдаёт 401/404
  без Bearer. `adult` — вкусовая настройка, не возрастной гейт.
- **Платные главы не обходятся:** 200 с `is_paid: true`, `is_bought: false`;
  `pages` отсутствует/пустой. У бесплатной главы `pages` — массив массивов
  ссылок.
- **Формат ответов v2:** `search` → `{results, meta}`, `title` → объект
  верхнего уровня с `branches`, `chapters` → `{results, next}`, `content` →
  `{pages, content_type, is_paid}`. Конверт `{content: ...}` — только у
  v1-логина.

### 2.3. CORS и браузер

- `api.remanga.org` отдаёт `Access-Control-Allow-Origin` только для точного
  `https://remanga.org` (и `https://demo.remanga.org`), с
  `Allow-Credentials: true`. Другой origin получает 200 **без ACAO**.
- SPA на нашем origin **не может читать** ответы Remanga; `no-cors` даёт
  opaque, JSONP/`<script>` не работают.
- Скрипт на самом `remanga.org` (юзерскрипт/букмарклет) CORS проходит и
  читает `auth:token`.
- `origin_guard` отвергает cross-site мутации → релею нужен отдельный
  ingest-роут с одноразовым bearer-токеном, а не обычная сессия.

### 2.4. DDoS-Guard

- `Server: ddos-guard`. Датацентровый IP → 403 «restricted access from your
  current network»; резидентный российский IP → 200 без cookie. Это репутация
  IP, не авторизация; клиентский токен не спасает заблокированный сервер.
- UA/Accept не дискриминатор.

### 2.5. Картинки

- `img3.reimg2.org`: `access-control-allow-origin: *`,
  `cache-control: public, max-age=31536000, immutable`; Referer не проверяет.
- Старый `img3.reimg.org` — 403 Cloudflare; переписывать на fallback-хост.
- Обложки `remanga.org/media/...` открываются с любым Referer.

## 3. Спайки

| Спайк | Результат | Вывод |
|---|---|---|
| A: egress с VPS | локально (резидентный IP) — 200; VPS не проверен | проверить на целевом хосте при деплое (фаза 7) |
| B: login/upgrade/refresh | login 200, 2FA off, `adult=1`; upgrade 200 (пара, `expires_in=900`); старый токен после upgrade — 200; refresh 200; повтор старого refresh — 200 | v1-токен долгоживущий и не отзывается upgrade; access 15 минут; ротация refresh не enforced |
| C: токен на remanga.org | 200, `auth:token` читается (30 символов), `adult=1` | клиентский фетчинг реален |
| D: полный конвейер | `search/title/chapters/content` — 200; закрытый тайтл — 200 с токеном; первая глава была платной (`is_paid=true`), поэтому `pages` не было | v1-токен годится для всего конвейера; для страниц брать `is_paid=false` |

### B. Логин → upgrade → refresh

1. `POST /api/users/login/` `{user,password}` → наличие `access_token`,
   `two_factor_auth`.
2. `GET /api/v2/users/current/` с токеном → baseline.
3. `POST /api/v2/users/token/upgrade/` (Bearer, `{}`) → пара и `expires_in`.
4. `GET /api/v2/users/current/` **старым** токеном → жив ли после upgrade.
5. `POST /api/v2/users/token/refresh/` `{refresh_token}` → новая пара.
6. Повторить refresh старым refresh.

Выводы: старый токен жив → перенос токена безопасен; upgrade не разлогинивает
телефон; при `two_factor_auth=true` нужен шаг с кодом.

### C. Токен из браузера

На `https://remanga.org` в консоли:

```js
(async () => {
  const m = document.cookie.match(/(?:^|;\s*)auth:token=([^;]+)/);
  if (!m) return console.warn('auth:token не найден');
  const t = decodeURIComponent(m[1]);
  const r = await fetch('https://api.remanga.org/api/v2/users/current/', {
    headers: { Authorization: 'Bearer ' + t }
  });
  console.log('status', r.status, 'token length', t.length, await r.json());
})();
```

Для телефона — закладка:

```text
javascript:(async()=>{const m=document.cookie.match(/(?:^|;\s*)auth:token=([^;]+)/);if(!m)return alert('auth:token не найден — войдите');const t=decodeURIComponent(m[1]);const r=await fetch('https://api.remanga.org/api/v2/users/current/',{headers:{Authorization:'Bearer '+t}});const j=await r.json();alert(r.status+' | '+(j.username||j.msg||'ok')+' | token '+t.length)})()
```

### A. Egress с VPS

```bash
curl -s https://api.ipify.org; echo
curl -sS -D - -o /dev/null 'https://api.remanga.org/api/v2/titles/?count=1' | head -n 15
```

### D. Полный конвейер

Ответы v2 плоские: `search` → `results`, `title` → объект верхнего уровня,
`chapters` → `results`, `content` → `pages`. Для проверки страниц брать главу
с `is_paid: false`.

## 4. Каталог вариантов

### 4.1. Как получить учётку

1.1 **Серверный вход по логину/паролю (выбран, фаза 5а).** Форма → v1-логин →
   токен. *Телефон:* да. *Риски:* DDoS-Guard, капча (сейчас отключена), 2FA,
   ToS. Пароль не хранить.
1.2 **OAuth2 «Войти через Remanga».** Провайдер есть, публичной регистрации
   клиента нет. *Телефон:* лучший UX, но нужен `client_id` (фаза 11).
1.3 **Перенос `auth:token` (букмарклет/юзерскрипт).** *Телефон:* да. *Риски:*
   ручной шаг, открытая вкладка, повтор при истечении (фазы 8–10).
1.4 **Соцвход на remanga.org + перенос токена.** То же, без Remanga-пароля.
1.5 **Расширение-компаньон.** *Телефон:* нет (Kiwi мёртв, Orion beta, Firefox
   Android MV2). Опциональный десктоп (фаза 10).
1.6 **Ручная вставка Bearer/Cookie.** Серверный `PUT` есть; UI убран в фазе 5а,
   остаётся запасным API-путём.
1.7 **Код привязки app-auth.** Недокументирован, низкая уверенность.

### 4.2. Egress

2.1 **Сервер, как сейчас.** 0 кода; один IP на всех.
2.2 **Install-wide прокси.** 1–2 дня; все с одного IP.
2.3 **Per-user прокси.** 1–2 недели; у каждого свой; нужен пул клиентов и
   breaker.
2.4 **Домашний relay.** 2–4 недели; NAT/туннель. Hostname-адреса
   private/CGNAT/Tailscale отсекает `SafeDns`, но IP-literal
   LAN/Tailscale проходит → нужен явный allowlist прокси.
2.5 **Клиентский фетчинг: юзерскрипт-мост.** ~2–4 недели; работает на
   телефоне; ingest + staging.
2.6 **Клиентский фетчинг: расширение-мост.** 5–7 недель; десктоп + Firefox
   Android.
2.7 **TS-порт парсеров.** 9–13 недель; drift `chapter_key`.
2.8 **Нативное приложение.** Высокая цена; подпись/сторы.
2.9 **Прямые CDN-картинки.** 2–4 дня; частичный выигрыш (фаза 6).
2.10 **Сервер на устройстве.** 1–3 дня для бинаря; ломает shared install.

### 4.3. Продление сессии

3.1 **Хранить v1-токен как веб (выбран).** Cookie ~1 год, веб refresh не
   вызывает; upgrade его не отзывает (спайк B). При 401 — повторный вход.
3.2 **Upgrade + refresh pair (резерв, фаза 5в).** Access 15 минут;
   single-flight; fallback на v1.
3.3 **Повторный логин по 401.** Нужен пароль или 2FA-код.

## 5. Отложенные фазы

Берутся только при выполнении условия; каждая заканчивается общим гейтом.

### 5б — 2FA

**Условие:** у кого-то из пользователей включён 2FA. **Важно:** до этой фазы
вход по паролю такому пользователю недоступен (409 уже отдаётся).

1. `LoginInput` получает `method: Option<String>`, `code: Option<String>`
   (код 6-значный).
2. Первый вызов без кода: при `two_factor_auth` вернуть 409
   `source_login_2fa_required`; методы из `LoginOutcome.methods` — в
   `message`, либо добавить структурное поле в формат ошибки.
3. UI: выбор метода + поле кода; второй вызов — `{login,password,method,code}`.
4. Пароль не хранить; rate limit.
5. Тест: 2FA-сценарий в двойнике.

**Готово, когда:** пользователь с 2FA входит за два шага; тесты и гейт зелёные.

### 5в — Refresh-пара как резерв

**Условие:** v1-токен начал отзываться (401) или Remanga объявила отказ от v1.

1. После логина вызвать `token/upgrade/`; сохранить `refresh` и
   `access_expires_at` в шифруемый `Payload` (расширяется без миграции).
2. Точка ретрая — обёртка вокруг `SourceKind::search/title/chapters/content`,
   а не только `resolve`: `resolve`/`for_user` лишь читают учётку; 401
   приходит из `sources/http.rs`.
3. Single-flight на пользователя: обновить пару, сохранить, повторить запрос;
   fallback — v1-токен.
4. Тест: refresh, повтор, fallback.

**Готово, когда:** при 401 пара обновляется и запрос повторяется; тесты
зелёные.

### 8 — Букмарклет + pairing

**Условие:** вход по паролю не подходит (капча/2FA/соцвход) или пользователь
не хочет давать пароль.

1. `POST /api/me/source-credentials/{code}/pairing`: одноразовый код (32 байта,
   TTL 5 минут, привязка user+source).
2. Публичный `POST /api/source-credentials/handoff` `{code, bearer}`. Нужны
   явные правки: добавить путь в `is_public` (`auth/middleware.rs`), отдельная
   Origin-политика для маршрута (разрешить отсутствие Origin;
   `chrome-extension://` нельзя добавить в `ALLOWED_ORIGINS`, `config.rs`
   принимает только http/https), `Cache-Control: no-store`, rate limit.
3. Букмарклет на remanga.org читает `auth:token` и отправляет на handoff
   (sendBeacon/no-cors).
4. UI: диалог с кодом/сниппетом и polling.
5. Тесты: одноразовость, TTL, привязка, Origin.

**Готово, когда:** токен с телефона переносится без пароля; код одноразовый;
тесты зелёные.

### 9 — Юзерскрипт-мост

**Условие:** действительно нужны разные egress-IP на пользователя. Тяжёлая
фаза; не начинать без решения.

1. `POST /api/sources/{code}/ingest` с per-user токеном; raw JSON; сервер
   парсит.
2. Staging/quarantine: клиентские данные не пишут глобальные таблицы напрямую.
3. Per-source transport policy: Remanga — с клиента, остальные — с сервера.
4. Юзерскрипт на `cion` + `remanga.org`, `GM_xmlhttpRequest`.
5. Тесты на poisoning/повтор.

**Готово, когда:** клиентский фетчинг не ломает глобальные данные; тесты
зелёные.

### 10 — Расширение (desktop, опционально)

**Условие:** нужен one-click захват на десктопе. Не для iOS.

1. Content script читает `auth:token` (cookies/webRequest не нужны).
2. Handoff по одноразовому коду.
3. MV3 для Chromium, MV2/event page для Firefox Android.

**Готово, когда:** вход по кнопке на десктопе.

### 11 — OAuth

**Условие:** Remanga выдала `client_id`.

1. `/oauth/authorize` + PKCE, обмен кода на `/api/v2/oauth/token/`.
2. Если `client_id` не дали — закрыть фазу как невозможную.

**Готово, когда:** вход через OAuth на телефоне; тесты зелёные.

## 6. Открытые вопросы

- VPS в РФ: DDoS-Guard ещё не проверен с его IP (проверка перед публичным
  запуском, фаза 7).
- TTL refresh-токена неизвестен; access после upgrade — 15 минут.
- Нужен ли вход для конкретных тайтлов пользователя: 18+ ведёт себя
  неоднородно.
- 2FA на текущем аккаунте выключен; для других пользователей шаг с кодом
  нужен (5б).
- Выдаст ли Remanga OAuth `client_id` стороннему self-hosted приложению.
- Разрешает ли ToS серверный логин и хранение токенов; robots.txt запрещает
  `/api`.
- Может ли не-admin пользователь задавать прокси (если да — SSRF/credential-leak
  review).
- Модель доверия для клиентского ingest: кто пишет глобальные таблицы, нужен
  ли staging.

## 7. Источники

- Mihon/Remanga extension: `github.com/ichinya/mihon-remanga`
- tanuki (auth/refresh): `github.com/Gabageba/tanuki/blob/master/docs/03-parsers.md`
- ReManga iOS: `github.com/XITRIX/ReManga-iOS`
- remanga.py: `github.com/vraestoren/remanga.py`
- API reference: `github.com/TheDivineLabs/remanga-api`
- Живые эндпоинты: `api.remanga.org/api/users/login/`,
  `/api/v2/users/login/`, `/api/v2/users/token/{upgrade,refresh}/`,
  `/api/v2/users/sessions/`, `/oauth/authorize`
- CORS/DDoS-Guard/CDN: живые заголовки `api.remanga.org`, `img3.reimg2.org`,
  `remanga.org` (2026-11-27)
