//! Чистые парсеры Mangamir. Сеть сюда не входит: HTML-фикстуры держат
//! селекторы и JSON-LD, поэтому редизайн сайта ломает тест, а не прод.

use reqwest::Url;
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;

use crate::sources::dto::{
    ChapterKind, ContentKind, ExternalId, SearchPage, SourceChapter, SourceContent, SourceCovers,
    SourcePage, SourceSearchItem, SourceTitle, TitleKind, TitleStatus,
};
use crate::sources::error::SourceError;
use crate::sources::keys::{build_chapter_key, normalize_number, number_milli};
use crate::sources::text::{dedupe, detect_chapter_kind, normalize_ws, truncate_chars};

const MAX_NAME_CHARS: usize = 300;
const MAX_LIST_ITEMS: usize = 50;
const MAX_DESCRIPTION_CHARS: usize = 20_000;
const MAX_CHAPTER_NAME_CHARS: usize = 300;
const MAX_CHAPTERS: usize = 5000;

fn parse_error(stage: &'static str, detail: impl Into<String>) -> SourceError {
    SourceError::Parse {
        stage,
        detail: detail.into(),
    }
}

fn selector(css: &str) -> Result<Selector, SourceError> {
    Selector::parse(css).map_err(|_| parse_error("html", format!("invalid selector `{css}`")))
}

fn text_of(element: ElementRef<'_>) -> String {
    normalize_ws(&element.text().collect::<String>())
}

fn attr_of(element: ElementRef<'_>, name: &str) -> Option<String> {
    element
        .value()
        .attr(name)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn absolute(site: &Url, href: &str) -> Result<Url, SourceError> {
    site.join(href.trim())
        .map_err(|_| parse_error("html", "invalid link"))
}

/// JSON-LD — самый устойчивый слой страниц Mangamir: имя, описание, жанры,
/// рейтинг и список глав переживают смену вёрстки. HTML остаётся запасным
/// вариантом для полей, которых в разметке нет.
fn json_ld(document: &Html) -> Vec<Value> {
    let Ok(script_selector) = Selector::parse("script[type=\"application/ld+json\"]") else {
        return Vec::new();
    };

    let mut objects = Vec::new();
    for script in document.select(&script_selector) {
        let raw = script.text().collect::<String>();
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        match value.get("@graph").and_then(Value::as_array) {
            Some(graph) => objects.extend(graph.iter().cloned()),
            None => objects.push(value),
        }
    }
    objects
}

fn find_type<'a>(objects: &'a [Value], kind: &str) -> Option<&'a Value> {
    objects
        .iter()
        .find(|object| object.get("@type").and_then(Value::as_str) == Some(kind))
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(normalize_ws)
        .filter(|text| !text.is_empty())
}

fn string_list(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(normalize_ws)
                .filter(|item| !item.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// `/manga/<slug>` -> `<slug>`; остальные ссылки не считаются тайтлом.
fn slug_from_url(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    let mut segments = url.path_segments()?;
    if segments.next()? != "manga" {
        return None;
    }
    let slug = segments.next()?;
    (!slug.is_empty() && !slug.contains('.')).then(|| slug.to_owned())
}

fn slug_from_href(site: &Url, href: &str) -> Option<String> {
    slug_from_url(absolute(site, href).ok()?.as_str())
}

/// Полный постер `.../X.jpeg` превращается в три размера сайта.
fn cover_variants(url: &str) -> SourceCovers {
    let url = url.trim();
    if url.is_empty() {
        return SourceCovers::default();
    }

    fn with_suffix(url: &str, suffix: &str) -> String {
        let Some(dot) = url.rfind('.') else {
            return url.to_owned();
        };
        let (stem, extension) = url.split_at(dot);
        let stem = stem
            .strip_suffix("_sm")
            .or_else(|| stem.strip_suffix("_md"))
            .unwrap_or(stem);
        format!("{stem}{suffix}{extension}")
    }

    SourceCovers {
        low: Some(with_suffix(url, "_sm")),
        mid: Some(with_suffix(url, "_md")),
        high: Some(url.to_owned()),
    }
}

fn search_cover(url: Option<&str>) -> Option<String> {
    url.and_then(|url| cover_variants(url).low)
}

pub fn parse_search(raw: &str, site: &Url) -> Result<SearchPage, SourceError> {
    let document = Html::parse_document(raw);
    let objects = json_ld(&document);

    let mut items = Vec::new();
    let mut total = None;

    if let Some(list) = find_type(&objects, "ItemList") {
        total = list.get("numberOfItems").and_then(Value::as_u64);
        if let Some(elements) = list.get("itemListElement").and_then(Value::as_array) {
            for element in elements {
                let Some(name) = string_field(element, "name") else {
                    continue;
                };
                let Some(external_id) = element
                    .get("url")
                    .and_then(Value::as_str)
                    .and_then(slug_from_url)
                    .and_then(|slug| ExternalId::new(slug).ok())
                else {
                    continue;
                };
                let cover_ref = search_cover(element.get("image").and_then(Value::as_str));
                let external_url = element
                    .get("url")
                    .and_then(Value::as_str)
                    .and_then(|url| Url::parse(url).ok())
                    .map(|url| url.to_string());

                items.push(SourceSearchItem {
                    external_id,
                    kind: None,
                    name: truncate_chars(&name, MAX_NAME_CHARS),
                    alt_names: Vec::new(),
                    cover_ref,
                    year: None,
                    status: None,
                    external_url,
                });
            }
        }
    }

    if items.is_empty() {
        parse_search_cards(&document, site, &mut items)?;
    }

    Ok(SearchPage {
        items,
        next_cursor: None,
        total,
    })
}

fn parse_search_cards(
    document: &Html,
    site: &Url,
    items: &mut Vec<SourceSearchItem>,
) -> Result<(), SourceError> {
    let poster_selector = selector("a[data-card-link-type=\"poster\"]")?;
    let image_selector = selector("img")?;

    for poster in document.select(&poster_selector) {
        let Some(href) = attr_of(poster, "href") else {
            continue;
        };
        let Some(slug) = slug_from_href(site, &href) else {
            continue;
        };
        let Ok(external_id) = ExternalId::new(slug) else {
            continue;
        };

        let name = attr_of(poster, "title")
            .or_else(|| {
                poster
                    .select(&image_selector)
                    .next()
                    .and_then(|image| attr_of(image, "alt"))
            })
            .map(|name| name.trim_end_matches(" обложка").trim().to_owned())
            .filter(|name| !name.is_empty());
        let Some(name) = name else {
            continue;
        };

        let cover_ref = poster
            .select(&image_selector)
            .next()
            .and_then(|image| attr_of(image, "src"))
            .and_then(|url| cover_variants(&url).low);
        let external_url = absolute(site, &href).ok().map(|url| url.to_string());

        items.push(SourceSearchItem {
            external_id,
            kind: None,
            name: truncate_chars(&name, MAX_NAME_CHARS),
            alt_names: Vec::new(),
            cover_ref,
            year: None,
            status: None,
            external_url,
        });
    }

    Ok(())
}

fn query_value(href: &str, key: &str) -> Option<String> {
    let url = Url::parse(&format!("https://placeholder.invalid{href}")).ok()?;
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

fn link_query_value(document: &Html, css: &str, key: &str) -> Result<Option<String>, SourceError> {
    let link_selector = selector(css)?;
    Ok(document
        .select(&link_selector)
        .filter_map(|link| link.value().attr("href"))
        .find_map(|href| query_value(href, key)))
}

fn kind_from_source(value: &str) -> TitleKind {
    match value.trim().to_ascii_lowercase().as_str() {
        "manga" => TitleKind::Manga,
        "manhwa" => TitleKind::Manhwa,
        "manhua" => TitleKind::Manhua,
        "comic" | "western comic" => TitleKind::WesternComic,
        _ => TitleKind::Other,
    }
}

fn status_from_source(value: &str) -> Option<TitleStatus> {
    match value.trim().to_ascii_lowercase().as_str() {
        "ongoing" => Some(TitleStatus::Ongoing),
        "completed" => Some(TitleStatus::Completed),
        "hiatus" => Some(TitleStatus::Hiatus),
        "cancelled" | "canceled" => Some(TitleStatus::Cancelled),
        _ => None,
    }
}

fn parse_year(value: &str) -> Option<i16> {
    value
        .trim()
        .parse::<i16>()
        .ok()
        .filter(|year| (1800..=2200).contains(year))
}

fn parse_rating(value: &Value) -> Option<f32> {
    let rating = value.get("ratingValue")?.as_f64()? as f32;
    (rating.is_finite() && (0.0..=10.0).contains(&rating)).then_some(rating)
}

fn parse_alt_names(document: &Html) -> Result<Vec<String>, SourceError> {
    let heading_selector = selector("h2")?;
    let mut names = Vec::new();
    for heading in document.select(&heading_selector) {
        let text = text_of(heading);
        let Some(rest) = text.strip_prefix("Другие названия:") else {
            continue;
        };
        for part in rest.split('/') {
            let part = normalize_ws(part);
            if !part.is_empty() {
                names.push(part);
            }
        }
    }
    dedupe(&mut names);
    names.truncate(MAX_LIST_ITEMS);
    Ok(names)
}

fn cover_url(document: &Html, series: Option<&Value>) -> Option<String> {
    if let Some(image) = series.and_then(|series| string_field(series, "image")) {
        return Some(image);
    }

    let selector = Selector::parse("img[src*=\"/posters/\"]").ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|image| attr_of(image, "src"))
}

pub fn parse_title(
    raw: &str,
    site: &Url,
    external_id: &ExternalId,
    language: &str,
) -> Result<SourceTitle, SourceError> {
    let document = Html::parse_document(raw);
    let objects = json_ld(&document);
    let series = find_type(&objects, "ComicSeries");

    let heading_selector = selector("h1")?;
    let name = document
        .select(&heading_selector)
        .next()
        .map(text_of)
        .filter(|name| !name.is_empty())
        .or_else(|| series.and_then(|series| string_field(series, "name")))
        .ok_or_else(|| parse_error("title", "missing title name"))?;

    let kind = link_query_value(&document, "a[href*=\"type[0]=\"]", "type[0]")?
        .map(|value| kind_from_source(&value))
        .unwrap_or(TitleKind::Other);
    let status = link_query_value(&document, "a[href*=\"status[0]=\"]", "status[0]")?
        .and_then(|value| status_from_source(&value));
    let year = link_query_value(&document, "a[href*=\"year[0]=\"]", "year[0]")?
        .as_deref()
        .and_then(parse_year);

    let description = series
        .and_then(|series| string_field(series, "description"))
        .or_else(|| {
            let selector = Selector::parse("meta[property=\"og:description\"]").ok()?;
            document
                .select(&selector)
                .next()
                .and_then(|meta| meta.value().attr("content"))
                .map(normalize_ws)
                .filter(|text| !text.is_empty())
        })
        .map(|text| truncate_chars(&text, MAX_DESCRIPTION_CHARS));

    let mut genres = series
        .map(|series| string_list(series, "genre"))
        .unwrap_or_default();
    dedupe(&mut genres);
    genres.truncate(MAX_LIST_ITEMS);

    let alt_names = parse_alt_names(&document)?;
    let covers =
        cover_url(&document, series).map_or_else(SourceCovers::default, |url| cover_variants(&url));
    let rating = find_type(&objects, "AggregateRating").and_then(parse_rating);

    let external_url = absolute(site, &format!("/manga/{}", external_id.as_str()))?;

    Ok(SourceTitle {
        external_id: external_id.clone(),
        kind,
        name: truncate_chars(&name, MAX_NAME_CHARS),
        alt_names,
        description,
        covers,
        authors: Vec::new(),
        genres,
        status,
        year,
        rating,
        lang: Some(language.to_owned()),
        external_url: Some(external_url.to_string()),
        raw_json: None,
    })
}

/// «Том 1 Глава 12 Название» -> том, номер, название. Номер может быть
/// дробным; без «Глава» остаток считается названием.
fn parse_chapter_title(raw: &str) -> (Option<u16>, Option<String>, Option<String>, ChapterKind) {
    let text = normalize_ws(raw);
    let kind = detect_chapter_kind(&text);
    let mut rest = text.as_str();
    let mut volume = None;

    if let Some(after) = rest.strip_prefix("Том ") {
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(value) = digits.parse::<u16>() {
            volume = Some(value);
        }
        rest = after[digits.len()..].trim_start();
    }

    let mut number = None;
    if let Some(after) = rest.strip_prefix("Глава ") {
        let raw_number: String = after
            .chars()
            .take_while(|character| {
                character.is_ascii_digit()
                    || *character == '.'
                    || *character == ','
                    || *character == '-'
            })
            .collect();
        let normalized = normalize_number(&raw_number);
        if !normalized.is_empty() {
            number = Some(normalized);
        }
        rest = after[raw_number.len()..].trim();
    }

    let name = (!rest.is_empty()).then(|| truncate_chars(rest, MAX_CHAPTER_NAME_CHARS));
    (volume, number, name, kind)
}

fn parse_datetime(raw: &str) -> Option<i64> {
    time::OffsetDateTime::parse(raw.trim(), &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|value| value.unix_timestamp())
}

fn row_is_locked(row: ElementRef<'_>) -> bool {
    let class = row.value().attr("class").unwrap_or_default();
    if class.contains("locked") || class.contains("premium") {
        return true;
    }
    let lower = text_of(row).to_lowercase();
    lower.contains("купить") || lower.contains("закрыта") || lower.contains("недоступна")
}

/// Livewire-компонент списка глав. По умолчанию сайт рендерит только первые
/// и последние главы; полный список приходит в ответ на `showAllChapters()`.
pub struct LivewireChapters {
    /// CSRF-токен из `livewireScriptConfig` (или `<meta name="csrf-token">`).
    pub token: String,
    pub update_url: Url,
    /// Снимок компонента как есть: Livewire проверяет по нему checksum.
    pub snapshot: String,
    /// Имя острова, которому адресован вызов.
    pub island: Option<String>,
    /// Страница уже отдала полный список — вызов не нужен.
    pub showing_all: bool,
}

/// `livewireScriptConfig = {...}` — единственный стабильный источник адреса
/// update-эндпоинта: хэш в пути меняется при деплое сайта.
fn livewire_script(raw: &str) -> Option<Value> {
    let marker = raw.find("livewireScriptConfig")?;
    let rest = &raw[marker..];
    let open = rest.find('{')?;
    serde_json::Deserializer::from_str(&rest[open..])
        .into_iter::<Value>()
        .next()?
        .ok()
}

fn meta_content(document: &Html, name: &str) -> Option<String> {
    let css = format!("meta[name=\"{name}\"]");
    let selector = Selector::parse(&css).ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|meta| attr_of(meta, "content"))
}

/// Находит снимок компонента `pages::book.show` среди всех `wire:snapshot`.
/// Снимок возвращается строкой: сериализовать его заново нельзя, checksum
/// считается по исходному JSON.
fn livewire_book_snapshot(
    document: &Html,
) -> Result<Option<(bool, Option<String>, String)>, SourceError> {
    let element_selector = selector("*")?;
    for element in document.select(&element_selector) {
        let Some(snapshot) = element.value().attr("wire:snapshot") else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(snapshot) else {
            continue;
        };
        if value
            .get("memo")
            .and_then(|memo| memo.get("name"))
            .and_then(Value::as_str)
            != Some("pages::book.show")
        {
            continue;
        }

        let showing_all = value
            .get("data")
            .and_then(|data| data.get("showingAllChapters"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let island = value
            .get("memo")
            .and_then(|memo| memo.get("islands"))
            .and_then(Value::as_array)
            .and_then(|islands| islands.first())
            .and_then(|island| island.get("name"))
            .and_then(Value::as_str)
            .map(str::to_owned);

        return Ok(Some((showing_all, island, snapshot.to_owned())));
    }
    Ok(None)
}

/// Данные для запроса полного списка глав; `None`, если Livewire на странице
/// нет (тогда остаётся обычный разбор HTML).
pub fn livewire_chapters(raw: &str, site: &Url) -> Result<Option<LivewireChapters>, SourceError> {
    let document = Html::parse_document(raw);
    let Some((showing_all, island, snapshot)) = livewire_book_snapshot(&document)? else {
        return Ok(None);
    };

    let config = livewire_script(raw);
    let token = config
        .as_ref()
        .and_then(|config| config.get("csrf"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| meta_content(&document, "csrf-token"));
    let update_url = config
        .as_ref()
        .and_then(|config| config.get("uri"))
        .and_then(Value::as_str)
        .and_then(|uri| site.join(uri).ok());

    let (Some(token), Some(update_url)) = (token, update_url) else {
        return Ok(None);
    };

    Ok(Some(LivewireChapters {
        token,
        update_url,
        snapshot,
        island,
        showing_all,
    }))
}

/// Свёрнутый список глав без Livewire-компонента: отдавать его как полный
/// нельзя. Метод `showAllChapters` встречается только в разметке кнопки.
pub fn page_is_collapsed(raw: &str) -> bool {
    raw.contains("showAllChapters")
}

/// HTML-фрагменты из ответа Livewire: `effects.islandFragments` (islands)
/// или `effects.partials` (morph).
pub fn livewire_fragments(raw: &str) -> Vec<String> {
    let Ok(payload) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    let Some(components) = payload.get("components").and_then(Value::as_array) else {
        return Vec::new();
    };

    let mut fragments = Vec::new();
    for effects in components
        .iter()
        .filter_map(|component| component.get("effects"))
    {
        if let Some(islands) = effects.get("islandFragments").and_then(Value::as_array) {
            fragments.extend(islands.iter().filter_map(Value::as_str).map(str::to_owned));
        }
        if let Some(partials) = effects.get("partials").and_then(Value::as_array) {
            fragments.extend(
                partials
                    .iter()
                    .filter_map(|partial| partial.get("html").and_then(Value::as_str))
                    .map(str::to_owned),
            );
        }
    }
    fragments
}

/// Разбирает фрагменты Livewire как один список глав.
pub fn parse_chapter_fragments(
    fragments: &[String],
    site: &Url,
    language: &str,
) -> Result<Vec<SourceChapter>, SourceError> {
    if fragments.is_empty() {
        return Ok(Vec::new());
    }
    let html = format!("<html><body>{}</body></html>", fragments.concat());
    parse_chapters(&html, site, language)
}

pub fn parse_chapters(
    raw: &str,
    site: &Url,
    language: &str,
) -> Result<Vec<SourceChapter>, SourceError> {
    let document = Html::parse_document(raw);
    let row_selector = selector("li[data-chapter-slug]")?;
    let link_selector = selector("a[href]")?;
    let time_selector = selector("time[datetime]")?;

    let mut chapters = Vec::new();
    for row in document.select(&row_selector) {
        let Some(slug) = attr_of(row, "data-chapter-slug") else {
            continue;
        };
        let Ok(external_id) = ExternalId::new(slug) else {
            continue;
        };
        let Some(link) = row.select(&link_selector).next() else {
            continue;
        };
        let Some(href) = attr_of(link, "href") else {
            continue;
        };

        let title_text = attr_of(link, "title").unwrap_or_else(|| text_of(link));
        let (volume, number_text, name, chapter_kind) = parse_chapter_title(&title_text);
        let number_milli = number_text.as_deref().and_then(number_milli);
        let chapter_key =
            build_chapter_key(volume, chapter_kind, number_text.as_deref(), &external_id);
        let published_at = row
            .select(&time_selector)
            .next()
            .and_then(|element| attr_of(element, "datetime"))
            .and_then(|value| parse_datetime(&value));
        let external_url = absolute(site, &href)?;

        chapters.push(SourceChapter {
            external_id,
            chapter_key,
            number_text,
            number_milli,
            volume,
            chapter_kind,
            content_kind: Some(ContentKind::Images),
            name,
            group_name: None,
            language: Some(language.to_owned()),
            is_locked: row_is_locked(row),
            external_url: Some(external_url.to_string()),
            published_at,
            raw_json: None,
        });

        if chapters.len() > MAX_CHAPTERS {
            return Err(parse_error("chapters", "too many chapters"));
        }
    }

    Ok(chapters)
}

pub fn parse_content(raw: &str) -> Result<SourceContent, SourceError> {
    let document = Html::parse_document(raw);
    let image_selector = selector("img[src*=\"/pages/\"]")?;
    let section_selector = selector(".chapter-section")?;

    // Страницы лежат внутри .chapter-section; если вёрстка сменится и класса
    // не будет, остаётся поиск по всему документу.
    let images: Vec<_> = match document.select(&section_selector).next() {
        Some(section) => section.select(&image_selector).collect(),
        None => document.select(&image_selector).collect(),
    };

    let mut pages = Vec::new();
    for image in images {
        let Some(source) = attr_of(image, "src").or_else(|| attr_of(image, "data-src")) else {
            continue;
        };
        let width = attr_of(image, "width").and_then(|value| value.parse::<u32>().ok());
        let height = attr_of(image, "height").and_then(|value| value.parse::<u32>().ok());

        pages.push(SourcePage {
            index: u32::try_from(pages.len()).unwrap_or(u32::MAX),
            image_ref: source.clone(),
            image_url: Some(source),
            width,
            height,
        });
    }

    if pages.is_empty() {
        let lower = raw.to_lowercase();
        if lower.contains("купить") || lower.contains("закрыт") || lower.contains("недоступ")
        {
            return Err(SourceError::Locked);
        }
        return Err(SourceError::NotFound);
    }

    Ok(SourceContent::Images { pages })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: &str = include_str!("fixtures/search.html");
    const TITLE: &str = include_str!("fixtures/title.html");
    const CONTENT: &str = include_str!("fixtures/content.html");
    const LIVEWIRE_TITLE: &str = include_str!("fixtures/livewire_title.html");
    const LIVEWIRE_UPDATE: &str = include_str!("fixtures/livewire_update.json");

    fn site() -> Url {
        Url::parse("https://mangamir.com").expect("site")
    }

    #[test]
    fn parses_search_from_json_ld() {
        let page = parse_search(SEARCH, &site()).expect("search");
        assert_eq!(page.total, Some(2));
        assert_eq!(page.items.len(), 2);

        let first = &page.items[0];
        assert_eq!(first.external_id.as_str(), "vo-vseoruzhii");
        assert_eq!(first.name, "Во всеоружии");
        assert_eq!(
            first.cover_ref.as_deref(),
            Some("https://img.mangamir.com/posters/1140/poster_sm.jpeg")
        );
        assert_eq!(
            first.external_url.as_deref(),
            Some("https://mangamir.com/manga/vo-vseoruzhii")
        );
    }

    #[test]
    fn parses_title_metadata_and_normalizes_type() {
        let external_id = ExternalId::new("vo-vseoruzhii").expect("id");
        let title = parse_title(TITLE, &site(), &external_id, "ru").expect("title");

        assert_eq!(title.kind, TitleKind::Manhwa);
        assert_eq!(title.name, "Во всеоружии");
        assert_eq!(title.status, Some(TitleStatus::Ongoing));
        assert_eq!(title.year, Some(2020));
        assert!(
            title
                .rating
                .is_some_and(|rating| (rating - 9.57).abs() < 0.001)
        );
        assert!(title.genres.contains(&"Фэнтези".to_owned()));
        assert!(title.alt_names.contains(&"Temppal".to_owned()));
        assert!(
            title
                .description
                .as_deref()
                .is_some_and(|text| { text.starts_with("Грид") && !text.contains('<') })
        );
        assert_eq!(
            title.covers.low.as_deref(),
            Some("https://img.mangamir.com/posters/1140/poster_sm.jpeg")
        );
        assert_eq!(
            title.covers.mid.as_deref(),
            Some("https://img.mangamir.com/posters/1140/poster_md.jpeg")
        );
        assert_eq!(
            title.covers.high.as_deref(),
            Some("https://img.mangamir.com/posters/1140/poster.jpeg")
        );
        assert_eq!(title.lang.as_deref(), Some("ru"));
    }

    #[test]
    fn parses_chapters_with_volumes_names_and_dates() {
        let chapters = parse_chapters(TITLE, &site(), "ru").expect("chapters");
        let keys: Vec<&str> = chapters
            .iter()
            .map(|chapter| chapter.chapter_key.as_str())
            .collect();

        assert!(keys.contains(&"v1:c0"), "{keys:?}");
        assert!(keys.contains(&"v1:c1"), "{keys:?}");
        assert!(keys.contains(&"v10:c111.37"), "{keys:?}");
        assert!(
            keys.iter().any(|key| key.starts_with("v1:special:x:h")),
            "{keys:?}"
        );

        let named = chapters
            .iter()
            .find(|chapter| chapter.chapter_key == "v1:c1")
            .expect("named chapter");
        assert_eq!(named.name.as_deref(), Some("Мечта"));
        assert_eq!(named.number_milli, Some(1000));
        assert_eq!(named.published_at, Some(1_763_856_000));

        let decimal = chapters
            .iter()
            .find(|chapter| chapter.chapter_key == "v10:c111.37")
            .expect("decimal chapter");
        assert_eq!(decimal.number_milli, Some(111_370));
    }

    #[test]
    fn chapter_keys_are_stable() {
        let first = parse_chapters(TITLE, &site(), "ru").expect("chapters");
        let second = parse_chapters(TITLE, &site(), "ru").expect("chapters");
        let first_keys: Vec<&str> = first.iter().map(|c| c.chapter_key.as_str()).collect();
        let second_keys: Vec<&str> = second.iter().map(|c| c.chapter_key.as_str()).collect();
        assert_eq!(first_keys, second_keys);
    }

    #[test]
    fn parses_page_images_in_order() {
        let content = parse_content(CONTENT).expect("content");
        let SourceContent::Images { pages } = content else {
            panic!("expected images");
        };
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].index, 0);
        assert_eq!(
            pages[0].image_ref,
            "https://img.mangamir.com/pages/1140/a202a72a91b09dc368318e330b733075608786fb.webp"
        );
        assert_eq!(pages[0].width, Some(720));
        assert_eq!(pages[0].height, Some(1904));
    }

    #[test]
    fn planned_chapter_without_pages_is_not_found() {
        let raw = r#"<html><body><div class="chapter-section"></div></body></html>"#;
        assert!(matches!(parse_content(raw), Err(SourceError::NotFound)));
    }

    #[test]
    fn detects_livewire_all_chapters_request() {
        let livewire = livewire_chapters(LIVEWIRE_TITLE, &site())
            .expect("livewire parse")
            .expect("livewire present");

        assert_eq!(livewire.token, "test-csrf-token");
        assert_eq!(
            livewire.update_url.as_str(),
            "https://mangamir.com/livewire-e4136179/update"
        );
        assert_eq!(livewire.island.as_deref(), Some("ab27de34-1"));
        assert!(!livewire.showing_all);
        // Снимок передаётся строкой как есть: пересборка сломала бы checksum.
        assert!(livewire.snapshot.starts_with('{'));
        assert!(livewire.snapshot.contains("\"showingAllChapters\":false"));
    }

    #[test]
    fn plain_page_has_no_livewire_component() {
        assert!(
            livewire_chapters(TITLE, &site())
                .expect("livewire parse")
                .is_none()
        );
        assert!(!page_is_collapsed(TITLE));
    }

    #[test]
    fn detects_collapsed_chapter_list() {
        assert!(page_is_collapsed(LIVEWIRE_TITLE));
    }

    #[test]
    fn parses_chapters_from_livewire_fragments() {
        let fragments = livewire_fragments(LIVEWIRE_UPDATE);
        assert_eq!(fragments.len(), 1);

        let chapters = parse_chapter_fragments(&fragments, &site(), "ru").expect("chapters");
        let keys: Vec<&str> = chapters
            .iter()
            .map(|chapter| chapter.chapter_key.as_str())
            .collect();
        assert_eq!(keys, ["v1:c212", "v1:c5", "v1:c1"]);

        let first = &chapters[0];
        assert_eq!(first.external_id.as_str(), "tom-1-glava-212");
        assert_eq!(first.published_at, Some(1_795_645_205));
        assert!(
            first.external_url.as_deref().is_some_and(|url| {
                url.starts_with("https://mangamir.com/manga/vo-vseoruzhii/")
            })
        );
    }

    #[test]
    fn malformed_livewire_response_yields_no_fragments() {
        assert!(livewire_fragments("not json").is_empty());
        assert!(livewire_fragments(r#"{"components":[{"effects":{}}]}"#).is_empty());
    }
}
