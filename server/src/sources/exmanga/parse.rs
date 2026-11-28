//! Чистые парсеры ExManga. Сеть сюда не входит: всё проверяется на
//! фикстурах, поэтому изменение формата API ломает тест, а не прод.
//!
//! ExManga — пул платных глав Remanga: `dir` тайтла и числовой id главы
//! совпадают с Remanga, поэтому `chapter_key` строится теми же правилами
//! и fallback находит общую главу.

use std::collections::HashSet;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sources::dto::{
    ChapterKind, ContentKind, ExternalId, LoginOutcome, SearchPage, SourceChapter, SourceContent,
    SourceCovers, SourcePage, SourceSearchItem, SourceTitle, TitleKind,
};
use crate::sources::error::SourceError;
use crate::sources::keys::{build_chapter_key, normalize_number, number_milli};
use crate::sources::text::{dedupe, normalize_name, parse_rfc3339, truncate_chars};

const MAX_NAME_CHARS: usize = 300;
const MAX_LIST_ITEMS: usize = 50;
const MAX_LIST_ITEM_CHARS: usize = 200;

/// Ответ ExManga всегда обёрнут в `{ success, statusCode, data }`.
#[derive(Debug, Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    success: Option<bool>,
    // Option сам по себе не требует поля в JSON; `#[serde(default)]` здесь
    // добавил бы лишний bound `T: Default`.
    data: Option<T>,
}

fn envelope<T: DeserializeOwned>(raw: &str, stage: &'static str) -> Result<Option<T>, SourceError> {
    let parsed: Envelope<T> = serde_json::from_str(raw).map_err(parse_error(stage))?;
    if parsed.success == Some(false) {
        return Err(SourceError::Parse {
            stage,
            detail: "source returned success=false".to_owned(),
        });
    }
    Ok(parsed.data)
}

#[derive(Debug, Deserialize, Serialize)]
struct RawManga {
    #[serde(default)]
    dir: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    meta: Option<RawMeta>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct RawMeta {
    #[serde(default)]
    en: Option<String>,
    #[serde(default)]
    img: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    rating: Option<Value>,
    #[serde(default)]
    genres: Option<String>,
    #[serde(default)]
    categories: Option<String>,
}

/// Глава в списке: `id`, `tome`, `chapter`, `index`, `date`. Числа приходят
/// числами, но `chapter` — строкой; поля читаются через `Value`, чтобы
/// смена типа в API не роняла весь список.
#[derive(Debug, Deserialize, Serialize)]
struct RawChapter {
    #[serde(default)]
    id: Option<Value>,
    #[serde(default)]
    tome: Option<Value>,
    #[serde(default)]
    chapter: Option<Value>,
    #[serde(default)]
    index: Option<Value>,
    #[serde(default)]
    date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawContent {
    #[serde(default)]
    is_paid: Option<bool>,
    #[serde(default)]
    is_bought: Option<bool>,
    /// Готовые ссылки страниц (их читает ридер ExManga).
    #[serde(default)]
    links: Option<Value>,
    /// Страницы в формате Remanga: массив массивов объектов с `link`.
    #[serde(default)]
    pages: Option<Value>,
}

pub fn parse_search(raw: &str) -> Result<SearchPage, SourceError> {
    let items = envelope::<Vec<RawManga>>(raw, "search")?.unwrap_or_default();
    let mut out = Vec::with_capacity(items.len());

    for item in items {
        let RawManga { dir, name, meta } = item;
        let meta = meta.unwrap_or_default();
        let Some(dir) = clean(dir.as_deref()) else {
            continue;
        };
        let Some(name) = clean(name.as_deref()).or_else(|| clean(meta.en.as_deref())) else {
            continue;
        };
        let external_id = ExternalId::new(dir)?;

        out.push(SourceSearchItem {
            external_id,
            kind: meta.kind.as_deref().and_then(TitleKind::from_source_name),
            name: truncate_chars(&name, MAX_NAME_CHARS),
            alt_names: alt_names(meta.en.as_deref(), &name),
            cover_ref: clean(meta.img.as_deref()),
            year: None,
            status: None,
            external_url: None,
        });
    }

    Ok(SearchPage {
        items: out,
        next_cursor: None,
        total: None,
    })
}

/// Метаданные тайтла по `dir`. `?dir=` возвращает массив; берём точное
/// совпадение, чтобы поиск по частичному слагу не подменил карточку.
pub fn parse_title(raw: &str, requested_dir: &str) -> Result<Option<SourceTitle>, SourceError> {
    let items = envelope::<Vec<RawManga>>(raw, "title")?.unwrap_or_default();
    let Some(item) = items
        .into_iter()
        .find(|item| item.dir.as_deref().map(str::trim) == Some(requested_dir))
    else {
        return Ok(None);
    };

    let raw_json = serde_json::to_value(&item).ok();
    let RawManga { dir, name, meta } = item;
    let meta = meta.unwrap_or_default();

    let dir = clean(dir.as_deref()).ok_or_else(|| SourceError::Parse {
        stage: "title",
        detail: "missing dir".to_owned(),
    })?;
    let external_id = ExternalId::new(dir)?;
    let name = clean(name.as_deref())
        .or_else(|| clean(meta.en.as_deref()))
        .ok_or_else(|| SourceError::Parse {
            stage: "title",
            detail: "missing name".to_owned(),
        })?;

    let mut genres = Vec::new();
    for list in [meta.genres.as_deref(), meta.categories.as_deref()]
        .into_iter()
        .flatten()
    {
        for part in list.split(',') {
            let part = part.trim();
            if !part.is_empty() {
                genres.push(truncate_chars(part, MAX_LIST_ITEM_CHARS));
            }
        }
    }
    dedupe(&mut genres);
    genres.truncate(MAX_LIST_ITEMS);

    Ok(Some(SourceTitle {
        external_id,
        kind: meta
            .kind
            .as_deref()
            .and_then(TitleKind::from_source_name)
            .unwrap_or(TitleKind::Other),
        name: truncate_chars(&name, MAX_NAME_CHARS),
        alt_names: alt_names(meta.en.as_deref(), &name),
        description: None,
        // ExManga отдаёт одну обложку; кладём её в `mid`, остальные размеры
        // сервер отдаст из ближайшего сохранённого.
        covers: SourceCovers {
            low: None,
            mid: clean(meta.img.as_deref()),
            high: None,
        },
        authors: Vec::new(),
        genres,
        status: None,
        year: None,
        rating: meta.rating.as_ref().and_then(parse_rating),
        // Язык подставит адаптер из конфига.
        lang: None,
        external_url: None,
        raw_json,
    }))
}

/// Полный список глав тайтла. `GET /api/manga/history?dir=` публичный и
/// отдаёт те же id, что Remanga: ключ главы совпадает, fallback работает.
pub fn parse_chapters(raw: &str, language: &str) -> Result<Vec<SourceChapter>, SourceError> {
    let items = envelope::<Vec<RawChapter>>(raw, "chapters")?.unwrap_or_default();
    let mut out = Vec::with_capacity(items.len());
    let mut seen = HashSet::new();

    for item in items {
        let Some(id) = item.id.as_ref().and_then(value_i64) else {
            continue;
        };
        let external_id = ExternalId::new(id.to_string())?;
        if !seen.insert(external_id.as_str().to_owned()) {
            continue;
        }

        let number = item.chapter.as_ref().map(value_text).unwrap_or_default();
        let number_text = normalize_number(&number);
        let number_text = (!number_text.is_empty()).then_some(number_text);
        let volume = item
            .tome
            .as_ref()
            .and_then(value_i64)
            .and_then(|tome| u16::try_from(tome).ok())
            .filter(|tome| *tome > 0);
        // ExManga повторяет нумерацию Remanga, где все главы `Normal`;
        // одинаковый `chapter_key` — залог fallback и общего прогресса.
        let chapter_kind = ChapterKind::Normal;
        let chapter_key =
            build_chapter_key(volume, chapter_kind, number_text.as_deref(), &external_id);

        out.push(SourceChapter {
            external_id,
            chapter_key,
            number_text: number_text.clone(),
            number_milli: number_text.as_deref().and_then(number_milli),
            volume,
            chapter_kind,
            // ExManga — комиксы; тип известен заранее и не требует content().
            content_kind: Some(ContentKind::Images),
            name: None,
            group_name: None,
            language: Some(language.to_owned()),
            // Доступность решает content(): список глав публичный.
            is_locked: false,
            external_url: None,
            published_at: item.date.as_deref().and_then(parse_rfc3339),
            raw_json: serde_json::to_value(&item).ok(),
        });
    }

    Ok(out)
}

/// Контент главы. `?extra=true&history=true` возвращает объект с `links`
/// (готовые подписанные ссылки) и `pages` (формат Remanga); без `extra`
/// API отдаёт голый массив страниц. Поддерживаются оба варианта.
pub fn parse_content(raw: &str) -> Result<SourceContent, SourceError> {
    let data = envelope::<Value>(raw, "content")?.ok_or(SourceError::NotFound)?;

    let (links, pages_value, is_paid, is_bought) = match data {
        Value::Array(array) => (Some(Value::Array(array)), None, None, None),
        Value::Object(map) => {
            let content: RawContent =
                serde_json::from_value(Value::Object(map)).map_err(parse_error("content"))?;
            (
                content.links,
                content.pages,
                content.is_paid,
                content.is_bought,
            )
        }
        _ => {
            return Err(SourceError::Parse {
                stage: "content",
                detail: "unexpected payload".to_owned(),
            });
        }
    };

    let mut pages = Vec::new();
    if let Some(value) = links.as_ref() {
        collect_pages(value, &mut pages);
    }
    if pages.is_empty()
        && let Some(value) = pages_value.as_ref()
    {
        collect_pages(value, &mut pages);
    }

    if pages.is_empty() {
        if is_paid == Some(true) && is_bought != Some(true) {
            return Err(SourceError::Locked);
        }
        return Err(SourceError::Parse {
            stage: "content",
            detail: "no pages".to_owned(),
        });
    }

    dedupe_pages(&mut pages);
    Ok(SourceContent::Images { pages })
}

pub fn parse_login(raw: &str) -> Result<LoginOutcome, SourceError> {
    let data = envelope::<Value>(raw, "login")?.ok_or_else(|| SourceError::Parse {
        stage: "login",
        detail: "missing access token".to_owned(),
    })?;

    let token = match data {
        Value::String(token) => clean(Some(&token)),
        Value::Object(map) => ["token", "access_token", "accessToken"]
            .iter()
            .find_map(|key| map.get(*key).and_then(Value::as_str))
            .and_then(|token| clean(Some(token))),
        _ => None,
    }
    .ok_or_else(|| SourceError::Parse {
        stage: "login",
        detail: "missing access token".to_owned(),
    })?;

    Ok(LoginOutcome {
        access_token: Some(token),
        two_factor_auth: false,
        methods: Vec::new(),
    })
}

/// `GET /api/auth` — лёгкая проверка токена. `success=false` означает
/// отклонённую учётку, а не сетевую ошибку.
pub fn parse_verify(raw: &str) -> Result<(), SourceError> {
    let parsed: Envelope<Value> = serde_json::from_str(raw).map_err(parse_error("verify"))?;
    if parsed.success == Some(false) {
        return Err(SourceError::AuthRequired);
    }
    // Успешный ответ может быть без `data` (профиль не важен): достаточно
    // явного `success=true` или непустого payload. Голый `{}` — не ответ
    // API, а заглушка/ошибка прокси.
    if parsed.success != Some(true) && parsed.data.is_none() {
        return Err(SourceError::Parse {
            stage: "verify",
            detail: "missing user payload".to_owned(),
        });
    }
    Ok(())
}

fn alt_names(english: Option<&str>, main: &str) -> Vec<String> {
    let Some(english) = clean(english) else {
        return Vec::new();
    };
    if normalize_name(&english) == normalize_name(main) {
        return Vec::new();
    }
    let mut names = vec![truncate_chars(&english, MAX_LIST_ITEM_CHARS)];
    dedupe(&mut names);
    names
}

fn clean(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn value_i64(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64().or_else(|| {
            number
                .as_f64()
                .filter(|value| value.fract() == 0.0)
                .map(|value| value as i64)
        }),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.trim().to_owned(),
        Value::Number(number) => number.to_string(),
        _ => String::new(),
    }
}

fn parse_rating(value: &Value) -> Option<f32> {
    let rating = match value {
        Value::Number(number) => number.as_f64()? as f32,
        Value::String(text) => text.trim().parse::<f32>().ok()?,
        _ => return None,
    };
    (rating > 0.0 && rating <= 10.0 && rating.is_finite()).then_some(rating)
}

/// Collects page objects from `links` (strings) or `pages` (nested arrays of
/// objects with `link`/`url`/`src`), preserving order.
fn collect_pages(value: &Value, out: &mut Vec<SourcePage>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_pages(item, out);
            }
        }
        Value::String(link) => {
            let link = link.trim();
            if !link.is_empty() {
                out.push(page(link.to_owned(), None, None));
            }
        }
        Value::Object(map) => {
            if let Some(link) = object_link(map) {
                out.push(page(
                    link,
                    object_dimension(map, "width"),
                    object_dimension(map, "height"),
                ));
            } else if let Some(pages) = map.get("pages") {
                collect_pages(pages, out);
            }
        }
        _ => {}
    }
}

fn page(image_ref: String, width: Option<u32>, height: Option<u32>) -> SourcePage {
    SourcePage {
        index: 0,
        image_url: Some(image_ref.clone()),
        image_ref,
        width,
        height,
    }
}

fn object_link(map: &serde_json::Map<String, Value>) -> Option<String> {
    for key in ["link", "url", "src"] {
        if let Some(Value::String(link)) = map.get(key) {
            let link = link.trim();
            if !link.is_empty() {
                return Some(link.to_owned());
            }
        }
    }
    None
}

fn object_dimension(map: &serde_json::Map<String, Value>, key: &str) -> Option<u32> {
    map.get(key)
        .and_then(value_i64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn dedupe_pages(pages: &mut Vec<SourcePage>) {
    let mut seen = HashSet::new();
    pages.retain(|item| seen.insert(item.image_ref.clone()));
    for (index, item) in pages.iter_mut().enumerate() {
        item.index = u32::try_from(index).unwrap_or(u32::MAX);
    }
}

fn parse_error<'a>(stage: &'static str) -> impl Fn(serde_json::Error) -> SourceError + 'a {
    move |error| {
        tracing::debug!(%error, stage, "cannot parse exmanga response");
        SourceError::Parse {
            stage,
            detail: "invalid json shape".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_maps_metadata_and_cover() {
        let title = parse_title(include_str!("fixtures/title.json"), "fixture-dir")
            .expect("title fixture")
            .expect("title present");

        assert_eq!(title.external_id.as_str(), "fixture-dir");
        assert_eq!(title.kind, TitleKind::Manhua);
        assert_eq!(title.name, "Фикстура");
        assert_eq!(title.alt_names, vec!["Fixture Alt".to_owned()]);
        assert_eq!(
            title.covers.mid.as_deref(),
            Some("/media/titles/fixture/cover.jpg")
        );
        assert_eq!(title.rating, Some(9.4));
        assert!(title.genres.contains(&"Экшен".to_owned()));
        assert!(title.genres.contains(&"Веб".to_owned()));
    }

    #[test]
    fn search_skips_items_without_dir_or_name() {
        let page = parse_search(include_str!("fixtures/search.json")).expect("search fixture");
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].external_id.as_str(), "fixture-dir");
        assert_eq!(page.items[0].kind, Some(TitleKind::Manhua));
        assert_eq!(page.items[1].external_id.as_str(), "second");
        assert!(page.items[1].alt_names.is_empty());
    }

    #[test]
    fn chapters_share_remanga_keys() {
        let chapters =
            parse_chapters(include_str!("fixtures/chapters.json"), "ru").expect("chapters fixture");
        assert_eq!(chapters.len(), 3);
        assert_eq!(chapters[0].external_id.as_str(), "2827338");
        assert_eq!(chapters[0].volume, Some(3));
        assert_eq!(chapters[0].number_text.as_deref(), Some("936"));
        assert_eq!(chapters[0].chapter_key, "v3:c936");
        assert_eq!(chapters[1].chapter_key, "v3:c18.5");
        // Глава без номера получает стабильный ключ от external_id, который
        // совпадает с Remanga для того же id.
        assert!(chapters[2].chapter_key.starts_with("v1:x:h"));
        assert!(chapters[2].number_text.is_none());
        assert_eq!(chapters[0].published_at, Some(1_795_822_740));
    }

    #[test]
    fn content_prefers_links_and_reports_locked() {
        let content =
            parse_content(include_str!("fixtures/content.json")).expect("content fixture");
        let SourceContent::Images { pages } = content else {
            panic!("expected images");
        };
        assert_eq!(pages.len(), 2);
        assert_eq!(
            pages[0].image_ref,
            "https://exmanga.org/storage/a/1.webp?sign=x"
        );
        assert_eq!(pages[1].index, 1);

        let locked = parse_content(
            r#"{"success":true,"data":{"is_paid":true,"is_bought":false,"links":[]}}"#,
        )
        .expect_err("locked chapter");
        assert!(matches!(locked, SourceError::Locked));
    }

    #[test]
    fn content_accepts_bare_page_array() {
        let content = parse_content(
            r#"{"success":true,"data":[[{"link":"/storage/a/1.webp","width":800,"height":1200}]]}"#,
        )
        .expect("bare pages");
        let SourceContent::Images { pages } = content else {
            panic!("expected images");
        };
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].width, Some(800));
        assert_eq!(pages[0].image_ref, "/storage/a/1.webp");
    }

    #[test]
    fn login_reads_plain_and_wrapped_tokens() {
        let plain = parse_login(include_str!("fixtures/login.json")).expect("login fixture");
        assert_eq!(
            plain.access_token.as_deref(),
            Some("header.payload.signature")
        );

        let wrapped =
            parse_login(r#"{"success":true,"data":{"token":"wrapped"}}"#).expect("wrapped token");
        assert_eq!(wrapped.access_token.as_deref(), Some("wrapped"));
    }

    #[test]
    fn verify_rejects_success_false() {
        assert!(matches!(
            parse_verify(r#"{"success":false,"data":null}"#),
            Err(SourceError::AuthRequired)
        ));
        assert!(parse_verify(r#"{"success":true,"data":{"level":1}}"#).is_ok());
    }
}
