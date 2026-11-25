# 0005. Второй источник — RuMix

- **Статус:** принято, реализовано в фазе 5
- **Дата:** 2026-11-25
- **План:** [../planned-now.md](../planned-now.md)
- **Связано:** [0002-source-abstraction.md](0002-source-abstraction.md), [0003-source-selection.md](0003-source-selection.md), [0004-first-source-remanga.md](0004-first-source-remanga.md)

## Контекст

Фазе 5 нужен второй источник **другой формы**, чтобы проверить абстракцию: HTML вместо JSON API, другая нумерация глав, тома, спецвыпуски, подписанные картинки. Кандидаты проверены 2026-11-25:

| Кандидат | Результат |
|---|---|
| MangaLib (`api.mangalib.me`) | 403, API закрыт Cloudflare/подписью |
| Mangamir | домен не резолвится |
| MangaDex (`api.mangadex.org`) | 302 на `mangadex.org/edge/challenge` — JS-челлендж |
| Mangapill | HTML доступен, но список глав тайтла обрезан, источник нестабилен для полноты |
| **RuMix (`rumix.me`)** | HTML на движке Grouple: поиск, серверный список глав, тома, глава 0, «Экстра», подписанные картинки |

RuMix — русскоязычный источник, что совпадает с языком первого тайтла и позволяет проверить `source_priorities` для `manga/ru` без смешивания языков в одной записи.

## Решение

1. **Второй адаптер — RuMix, `MediaKind::Manga`.** `base_url` — origin сайта `https://rumix.me` (не API-хост, как у Remanga). Нормализация `base_url` та же: origin без пути.
2. **Транспорт.** Страницы UTF-8. Поиск — `POST /search` с form-urlencoded `q`, `fast-filter=CREATION`, `offset`; курсор — число пропущенных результатов, `next_cursor = offset + items.len()` при `consumed < total`. Сайт отвечает `402` на User-Agent без префикса `Mozilla/5.0`, поэтому конфиг содержит `user_agent` с честным совместимым значением (`Mozilla/5.0 (compatible; cion-reader/...; +self-hosted aggregator)`); Referer — origin сайта.
3. **Редиректы не следуем.** 302 на `/internal/purchase/...` — платная глава (`SourceError::Locked`), 302 на страницу тайтла — запланированная/удалённая (`SourceError::NotFound`). Это единственный способ отличить платную главу: в списке глав она ничем не помечена.
4. **Идентификаторы.** `external_id` тайтла — slug из ссылки (`arifureta__silneishii_remeslennik_v_mire`); главы — `data-id` строки. `external_url` главы сохраняется из `href` и используется для `content()`; хост проверяется по allowlist сайта.
5. **Нумерация.** `data-num` — десятые доли: `380` → `number_text="38"`, `number_milli=38000`; `0` → глава 0. `data-vol > 0` — том. Вид главы: `td.extra-chapter` → `extra`; иначе по тексту строки — «Экстра» → `extra`, «Ваншот» → `oneshot`, «Спешл/Спецвыпуск» → `special`, «Превью» → `preview`; без номера — стабильный хеш от `external_id`. `chapter_key` строит общий `sources/keys.rs`, как у Remanga. `group_name` — `None`: RuMix не отдаёт несколько групп перевода.
6. **Контент.** Глава содержит inline `rm_h.readerInit(chapterInfo, [['base','',"path",w,h],...], ...)`; парсер собирает `base+path`: `image_ref` — стабильный URL без query-подписи, `image_url` — последний подписанный URL. 18+ открывается параметром `?mtr=true`. Ссылки живут десятки минут; при `401/403/410` существующий прокси картинок до двух раз перезапрашивает `content()` и обновляет `pages`.
7. **Метаданные.** Название — `h1.names > span.name`, alt names — popover/`eng-name`/`original-name`, описание — plain text из `manga-description[itemprop=description]`, обложка — `data-thumb` (полный файл не нужен), авторы — `elem_author`/`elem_screenwriter`/`elem_illustrator`, жанры и год — соответствующие `elem_*`. Рейтинг — `.subject-rate meta[itemprop=ratingValue]` (5-балльная шкала), адаптер умножает на 2 и ограничивает 10; `.compact-rate` на странице относится к соседним тайтлам и используется только как fallback. Статус — `.badge-primary` («выпуск продолжается/завершён/…»).
8. **Конфиг:** `image_hosts` (по умолчанию `*.rumix.me`, `rumix.me`), `language = "ru"`, `user_agent`. Дескриптор объявляет `Search`, `Chapters`, `Images`, `Volumes`, `AltNames`, `SignedUrls`; фильтров поиска нет.
9. **Ограничения.** Платные главы становятся `locked` после первого открытия, а не на импорте (иначе импорт делал бы запрос на каждую главу). Ранобэ/новеллы из поиска отклоняются при импорте как `source_unsupported_kind`. Mark-and-sweep исчезнувших глав — фаза 7. Время `data-date-raw` считается UTC (сайт может отдавать MSK).

## Проверка

- Парсеры RuMix покрыты фикстурами и `contract_suite`: поиск, метаданные, тома/0/дроби/Экстра/без номера, стабильность ключей, подписанные страницы.
- Live-тест `tests/live_rumix.rs` (`#[ignore]`) проходит на реальном сайте: поиск → тайтл → главы → `content()` → allowlist картинки.
- Интеграционные тесты `tests/multi_source.rs` проверяют два `source_id`, приоритеты, preferred, fallback, alias, группы перевода, attach/detach/primary/refresh и выключение источника.

## Рассмотренные альтернативы

| Вариант | Почему отклонён |
|---|---|
| MangaLib / Mangamir | 403/JS-челлендж или недоступный домен; `probe` не гарантирует стабильность |
| MangaDex | JS-челлендж на API, локально недостижим |
| Mangapill | HTML доступен, но неполный список глав у больших тайтлов и английский контент |
| Отложить второй источник | не выполняется критерий фазы 5: два источника, alias, приоритеты и fallback |
