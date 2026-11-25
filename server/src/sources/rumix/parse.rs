//! Чистые парсеры RuMix. Сеть сюда не входит: HTML-фикстуры держат
//! селекторы, а редизайн сайта ломает тест, а не прод.

use reqwest::Url;
use scraper::{ElementRef, Html, Selector};

use crate::sources::dto::{
    ChapterKind, ContentKind, ExternalId, MediaKind, SearchPage, SourceChapter, SourceContent,
    SourceCovers, SourcePage, SourceSearchItem, SourceTitle, TitleStatus,
};
use crate::sources::error::SourceError;
use crate::sources::keys::build_chapter_key;
use crate::sources::text::{dedupe, normalize_ws, truncate_chars};

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

/// Ссылка на тайтл `/<slug>` -> непрозрачный `external_id`.
fn external_id_from_href(href: &str) -> Result<String, SourceError> {
    let path = href
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .trim()
        .trim_start_matches('/');

    if path.is_empty() {
        return Err(parse_error("search", "empty title link"));
    }
    Ok(path.to_owned())
}

fn absolute(site: &Url, href: &str) -> Result<Url, SourceError> {
    site.join(href.trim())
        .map_err(|_| parse_error("html", "invalid link"))
}

fn status_from_class(class: &str) -> Option<TitleStatus> {
    if class.contains("mangaCompleted") {
        Some(TitleStatus::Completed)
    } else if class.contains("mangaPublishing") {
        Some(TitleStatus::Ongoing)
    } else if class.contains("mangaOnHold") {
        Some(TitleStatus::Hiatus)
    } else if class.contains("mangaDropped") || class.contains("mangaCancelled") {
        Some(TitleStatus::Cancelled)
    } else {
        None
    }
}

fn status_from_text(raw: &str) -> Option<TitleStatus> {
    let text = raw.to_lowercase();
    if text.contains("продолжается") {
        Some(TitleStatus::Ongoing)
    } else if text.contains("заверш") {
        Some(TitleStatus::Completed)
    } else if text.contains("приостанов") || text.contains("заморож") {
        Some(TitleStatus::Hiatus)
    } else if text.contains("прекращ") || text.contains("отмен") {
        Some(TitleStatus::Cancelled)
    } else {
        None
    }
}

fn parse_rating(raw: &str) -> Option<f32> {
    let rating = raw.trim().parse::<f32>().ok()?;
    if !rating.is_finite() || rating <= 0.0 {
        return None;
    }
    // RuMix оценивает по 5-балльной шкале, DTO — по 10-балльной.
    Some((rating * 2.0).min(10.0))
}

/// Общее число результатов: сервер рисует его в inline-скрипте
/// `$(".result-count-placeholder").html('17')`.
fn parse_total(html: &str) -> Option<u64> {
    let marker = ".result-count-placeholder\").html('";
    let start = html.find(marker)? + marker.len();
    let digits: String = html[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

pub fn parse_search(
    raw: &str,
    site: &Url,
    kind: MediaKind,
    offset: u64,
) -> Result<SearchPage, SourceError> {
    let document = Html::parse_document(raw);
    let container_selector = selector("#advanced-results")?;
    if document.select(&container_selector).next().is_none() {
        return Err(parse_error("search", "results container missing"));
    }
    let tile_selector = selector("#advanced-results .tile")?;
    let link_selector = selector(".desc h3 a")?;
    let cover_selector = selector(".img img")?;
    let alt_selector = selector(".html-popover-holder h5")?;
    let status_selector = selector(".tags span")?;

    let mut items = Vec::new();
    for tile in document.select(&tile_selector) {
        let Some(link) = tile.select(&link_selector).next() else {
            continue;
        };
        let Some(href) = attr_of(link, "href") else {
            continue;
        };
        let external_id = ExternalId::new(external_id_from_href(&href)?)?;

        let name = text_of(link);
        if name.is_empty() {
            continue;
        }

        let cover_ref = tile
            .select(&cover_selector)
            .next()
            .and_then(|image| attr_of(image, "data-original").or_else(|| attr_of(image, "src")));

        let mut alt_names: Vec<String> = tile
            .select(&alt_selector)
            .map(text_of)
            .filter(|value| !value.is_empty() && value != &name)
            .collect();
        dedupe(&mut alt_names);
        alt_names.truncate(MAX_LIST_ITEMS);

        let status = tile
            .select(&status_selector)
            .filter_map(|element| element.value().attr("class"))
            .find_map(status_from_class);

        items.push(SourceSearchItem {
            external_id,
            kind,
            name: truncate_chars(&name, MAX_NAME_CHARS),
            alt_names,
            cover_ref,
            year: None,
            status,
            external_url: absolute(site, &href).ok().map(|url| url.to_string()),
        });
    }

    let total = parse_total(raw);
    let consumed = offset + (items.len() as u64);
    let next_cursor = if items.is_empty() {
        None
    } else {
        total
            .filter(|total| consumed < *total)
            .map(|_| consumed.to_string())
    };

    Ok(SearchPage {
        items,
        next_cursor,
        total,
    })
}

/// Тип произведения в заголовке RuMix: «Манга», «Манхва», «Ранобэ».
fn title_kind(raw: &str) -> MediaKind {
    let first = raw.split_whitespace().next().unwrap_or_default();
    match first {
        "Ранобэ" | "Ранобе" | "Новелла" => MediaKind::Book,
        _ => MediaKind::Manga,
    }
}

pub fn parse_title(
    raw: &str,
    site: &Url,
    external_id: &ExternalId,
    language: &str,
) -> Result<SourceTitle, SourceError> {
    let document = Html::parse_document(raw);

    let heading_selector = selector("h1.names")?;
    let heading = document
        .select(&heading_selector)
        .next()
        .ok_or_else(|| parse_error("title", "missing title heading"))?;
    let heading_text = text_of(heading);

    let kind = title_kind(&heading_text);
    if kind != MediaKind::Manga {
        return Err(SourceError::UnsupportedKind(kind.as_str().to_owned()));
    }

    let name_selector = selector("h1.names > span.name")?;
    let name = document
        .select(&name_selector)
        .next()
        .map(text_of)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| parse_error("title", "missing title name"))?;

    let alt_selector = selector(".all-names-popover .name")?;
    let mut alt_names: Vec<String> = document
        .select(&alt_selector)
        .map(text_of)
        .filter(|value| !value.is_empty() && value != &name)
        .collect();
    for extra in [".eng-name", ".original-name"] {
        let extra_selector = selector(extra)?;
        for element in document.select(&extra_selector) {
            let value = text_of(element);
            if !value.is_empty() && value != name {
                alt_names.push(value);
            }
        }
    }
    dedupe(&mut alt_names);
    alt_names.truncate(MAX_LIST_ITEMS);

    let description_selector = selector("div.manga-description[itemprop=\"description\"]")?;
    let description = document
        .select(&description_selector)
        .next()
        .map(text_of)
        .filter(|text| !text.is_empty())
        .map(|text| truncate_chars(&text, MAX_DESCRIPTION_CHARS));

    let cover_selector = selector("img[itemprop=\"image\"]")?;
    // Один <img> несёт три размера: data-thumb (маленький), src (обычный)
    // и data-full (оригинал). Раскладываем их по low/mid/high; остальные
    // размеры страхуются ближайшим доступным атрибутом.
    let (thumb, normal, full) =
        document
            .select(&cover_selector)
            .next()
            .map_or((None, None, None), |image| {
                (
                    attr_of(image, "data-thumb"),
                    attr_of(image, "src"),
                    attr_of(image, "data-full"),
                )
            });
    let covers = SourceCovers {
        low: thumb
            .clone()
            .or_else(|| normal.clone())
            .or_else(|| full.clone()),
        mid: normal
            .clone()
            .or_else(|| full.clone())
            .or_else(|| thumb.clone()),
        high: full.or(normal).or(thumb),
    };

    let people_selector = selector(".elem_author a, .elem_screenwriter a, .elem_illustrator a")?;
    let mut authors: Vec<String> = document
        .select(&people_selector)
        .map(text_of)
        .filter(|name| !name.is_empty())
        .collect();
    dedupe(&mut authors);
    authors.truncate(MAX_LIST_ITEMS);

    let genre_selector = selector(".elem_genre")?;
    let mut genres: Vec<String> = document
        .select(&genre_selector)
        .map(text_of)
        .filter(|name| !name.is_empty())
        .collect();
    dedupe(&mut genres);
    genres.truncate(MAX_LIST_ITEMS);

    let year_selector = selector(".elem_year a")?;
    let year = document
        .select(&year_selector)
        .next()
        .and_then(|element| text_of(element).parse::<i16>().ok())
        .filter(|year| (1800..=2200).contains(year));

    // Рейтинг тайтла — в subject-rate; .compact-rate встречается в блоках
    // «похожее» и относится к другим произведениям.
    let rating_meta_selector = selector(".subject-rate meta[itemprop=\"ratingValue\"]")?;
    let mut rating = document
        .select(&rating_meta_selector)
        .next()
        .and_then(|meta| meta.value().attr("content"))
        .and_then(parse_rating);
    if rating.is_none() {
        let rating_selector = selector(".compact-rate")?;
        rating = document
            .select(&rating_selector)
            .next()
            .and_then(|element| element.value().attr("title"))
            .and_then(parse_rating);
    }

    let status_selector = selector(".badge-primary")?;
    let status = document
        .select(&status_selector)
        .next()
        .and_then(|element| status_from_text(&text_of(element)));

    let external_url = absolute(site, &format!("/{}", external_id.as_str()))?;

    Ok(SourceTitle {
        external_id: external_id.clone(),
        kind,
        name: truncate_chars(&name, MAX_NAME_CHARS),
        alt_names,
        description,
        covers,
        authors,
        genres,
        status,
        year,
        rating,
        lang: Some(language.to_owned()),
        external_url: Some(external_url.to_string()),
        raw_json: None,
    })
}

pub fn parse_chapters(
    raw: &str,
    site: &Url,
    language: &str,
) -> Result<Vec<SourceChapter>, SourceError> {
    let document = Html::parse_document(raw);
    let row_selector = selector("tr.item-row[data-id]")?;
    let link_selector = selector("a.chapter-link")?;
    let date_selector = selector("td.date")?;
    let extra_selector = selector("td.extra-chapter")?;

    let mut chapters = Vec::new();
    for row in document.select(&row_selector) {
        let Some(external_id) = attr_of(row, "data-id") else {
            continue;
        };
        let external_id = ExternalId::new(external_id)?;

        let volume = attr_of(row, "data-vol")
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|volume| *volume > 0);
        let number = attr_of(row, "data-num").and_then(|value| value.parse::<i64>().ok());
        let number_text = number.map(tenths_text);
        let number_milli = number.map(|value| value.saturating_mul(100));

        let Some(link) = row.select(&link_selector).next() else {
            continue;
        };
        let Some(href) = attr_of(link, "href") else {
            continue;
        };
        let link_text = text_of(link);
        let chapter_kind = if row.select(&extra_selector).next().is_some() {
            ChapterKind::Extra
        } else {
            detect_kind(&link_text)
        };
        let chapter_key =
            build_chapter_key(volume, chapter_kind, number_text.as_deref(), &external_id);
        let external_url = absolute(site, &href)?;

        let published_at = row
            .select(&date_selector)
            .next()
            .and_then(|element| element.value().attr("data-date-raw"))
            .and_then(parse_date);

        chapters.push(SourceChapter {
            external_id,
            chapter_key,
            number_text,
            number_milli,
            volume,
            chapter_kind,
            // Сайт комиксовый: формат известен ещё до content()
            content_kind: Some(ContentKind::Images),
            name: clean_chapter_name(&link_text),
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

/// `data-num` у RuMix — десятые доли: 380 -> «38», 25 -> «2.5».
fn tenths_text(value: i64) -> String {
    if value % 10 == 0 {
        (value / 10).to_string()
    } else {
        format!("{}.{}", value / 10, value % 10)
    }
}

fn detect_kind(text: &str) -> ChapterKind {
    let lower = text.to_lowercase();
    if lower.contains("экстра") || lower.contains("extra") {
        ChapterKind::Extra
    } else if lower.contains("ваншот") || lower.contains("one shot") || lower.contains("one-shot")
    {
        ChapterKind::Oneshot
    } else if lower.contains("спешл") || lower.contains("special") || lower.contains("спецвыпуск")
    {
        ChapterKind::Special
    } else if lower.contains("превью") || lower.contains("preview") {
        ChapterKind::Preview
    } else {
        ChapterKind::Normal
    }
}

fn strip_number(value: &str) -> &str {
    value.trim_start_matches(|character: char| {
        character.is_ascii_digit() || character == '.' || character == ','
    })
}

fn trim_separators(value: &str) -> &str {
    value.trim_start_matches(|character: char| {
        character.is_whitespace()
            || character == '-'
            || character == '—'
            || character == ':'
            || character == '.'
    })
}

/// «1 - 2 Название» -> «Название»; «4 Экстра ...» -> «...».
fn clean_chapter_name(text: &str) -> Option<String> {
    let mut rest = strip_number(text.trim());
    rest = trim_separators(rest);
    if rest.starts_with(|character: char| character.is_ascii_digit()) {
        rest = trim_separators(strip_number(rest));
    }

    let lower = rest.to_lowercase();
    for word in [
        "экстра",
        "extra",
        "ваншот",
        "one shot",
        "one-shot",
        "спешл",
        "special",
        "спецвыпуск",
        "превью",
        "preview",
    ] {
        if lower.starts_with(word) {
            rest = trim_separators(&rest[word.len()..]);
            break;
        }
    }

    let name = normalize_ws(rest);
    (!name.is_empty()).then(|| truncate_chars(&name, MAX_CHAPTER_NAME_CHARS))
}

fn row_is_locked(row: ElementRef<'_>) -> bool {
    let class = row.value().attr("class").unwrap_or_default();
    if class.contains("item-paid")
        || class.contains("item-closed")
        || class.contains("item-no-access")
    {
        return true;
    }

    let lower = text_of(row).to_lowercase();
    lower.contains("закрыта") || lower.contains("недоступна") || lower.contains("купить")
}

fn parse_date(raw: &str) -> Option<i64> {
    let (date, clock) = raw.trim().split_once(' ')?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u8>().ok()?;
    let day = date_parts.next()?.parse::<u8>().ok()?;

    let mut clock_parts = clock.split(':');
    let hour = clock_parts.next()?.parse::<u8>().ok()?;
    let minute = clock_parts.next()?.parse::<u8>().ok()?;
    let second = clock_parts.next()?.split('.').next()?.parse::<u8>().ok()?;

    let date =
        time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()?;
    let time = time::Time::from_hms(hour, minute, second).ok()?;
    Some(
        time::PrimitiveDateTime::new(date, time)
            .assume_utc()
            .unix_timestamp(),
    )
}

fn extract_pics(raw: &str) -> Option<&str> {
    let marker = "rm_h.readerInit(";
    let start = raw.find(marker)? + marker.len();
    let rest = &raw[start..];
    // Второй аргумент — массив страниц; первый (chapterInfo) — объект.
    let open = rest.find('[')?;
    let mut depth = 0_i32;
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    for (index, character) in rest[open..].char_indices() {
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                in_string = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => in_string = Some(character),
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return rest.get(open..open + index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_pics(array: &str) -> Result<Vec<SourcePage>, SourceError> {
    let mut pages = Vec::new();
    let mut depth = 0_i32;
    let mut start = None;
    let mut in_string: Option<char> = None;
    let mut escaped = false;

    for (index, character) in array.char_indices() {
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == quote {
                in_string = None;
            }
            continue;
        }

        match character {
            '\'' | '"' => in_string = Some(character),
            '[' => {
                depth += 1;
                if depth == 2 {
                    start = Some(index + 1);
                }
            }
            ']' => {
                if depth == 2
                    && let Some(begin) = start.take()
                    && let Some(page) = parse_picture(&array[begin..index], pages.len())?
                {
                    pages.push(page);
                }
                depth -= 1;
            }
            _ => {}
        }
    }

    Ok(pages)
}

/// Внутри кортежа: `['https://fs1.rumix.me/','',"path",1500,2132]`.
fn parse_picture(inner: &str, index: usize) -> Result<Option<SourcePage>, SourceError> {
    let tokens = js_tokens(inner)?;
    if tokens.len() < 5 {
        return Ok(None);
    }

    let url = format!("{}{}", tokens[0], tokens[2]);
    if url.trim().is_empty() {
        return Ok(None);
    }

    // image_ref — стабильная ссылка без подписи, image_url — последний
    // подписанный URL; кэш прокси ключуется не по ним, а по главе и idx.
    let stable_path = tokens[2].split('?').next().unwrap_or_default();
    let stable = format!("{}{}", tokens[0], stable_path);

    Ok(Some(SourcePage {
        index: u32::try_from(index).unwrap_or(u32::MAX),
        image_ref: stable,
        image_url: Some(url),
        width: tokens[3].parse::<u32>().ok(),
        height: tokens[4].parse::<u32>().ok(),
    }))
}

fn js_tokens(inner: &str) -> Result<Vec<String>, SourceError> {
    let mut tokens = Vec::new();
    let mut chars = inner.trim().chars().peekable();

    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace() || *c == ',') {
            chars.next();
        }
        let Some(&first) = chars.peek() else {
            break;
        };

        if first == '\'' || first == '"' {
            chars.next();
            let mut value = String::new();
            let mut escaped = false;
            for character in chars.by_ref() {
                if escaped {
                    value.push(match character {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == first {
                    break;
                } else {
                    value.push(character);
                }
            }
            tokens.push(value);
        } else {
            let mut value = String::new();
            while let Some(&character) = chars.peek() {
                if character == ',' || character == ']' {
                    break;
                }
                value.push(character);
                chars.next();
            }
            tokens.push(value.trim().to_owned());
        }
    }

    Ok(tokens)
}

pub fn parse_content(raw: &str) -> Result<SourceContent, SourceError> {
    let pages = match extract_pics(raw) {
        Some(array) => parse_pics(array)?,
        None => {
            let lower = raw.to_lowercase();
            if lower.contains("закрыта") || lower.contains("недоступна") || lower.contains("купить")
            {
                return Err(SourceError::Locked);
            }
            return Err(SourceError::NotFound);
        }
    };

    if pages.is_empty() {
        // Планируемая глава: страниц ещё нет, fallback должен попробовать
        // другой источник, а не показывать пустую читалку.
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

    fn site() -> Url {
        Url::parse("https://rumix.me").expect("site")
    }

    #[test]
    fn parses_search_results_and_total() {
        let page = parse_search(SEARCH, &site(), MediaKind::Manga, 0).expect("search");
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total, Some(2));
        assert_eq!(page.next_cursor, None);

        let first = &page.items[0];
        assert_eq!(
            first.external_id.as_str(),
            "arifureta__silneishii_remeslennik_v_mire"
        );
        assert_eq!(first.name, "Арифурэта: Сильнейший ремесленник в мире");
        assert!(first.alt_names.contains(&"Arifureta".to_owned()));
        assert_eq!(first.status, Some(TitleStatus::Completed));
        assert_eq!(
            first.cover_ref.as_deref(),
            Some("https://static.rumix.me/uploads/pics/00/00/312_p.jpg")
        );
    }

    #[test]
    fn parses_title_metadata() {
        let external_id = ExternalId::new("arifureta__silneishii_remeslennik_v_mire").expect("id");
        let title = parse_title(TITLE, &site(), &external_id, "ru").expect("title");

        assert_eq!(title.kind, MediaKind::Manga);
        assert_eq!(title.name, "Арифурэта: Сильнейший ремесленник в мире");
        assert!(title.alt_names.contains(&"Arifureta".to_owned()));
        assert!(title.authors.contains(&"Рё Сиракомэ".to_owned()));
        assert!(title.genres.contains(&"романтика".to_owned()));
        assert_eq!(title.year, Some(2016));
        assert!(
            title
                .rating
                .is_some_and(|rating| (rating - 9.2).abs() < 0.001),
            "rating must be normalized to 0..=10"
        );
        assert_eq!(title.status, Some(TitleStatus::Ongoing));
        assert!(title.description.as_deref().is_some_and(|text| {
            text.starts_with("Хадзимэ Нагумо") && !text.contains('<')
        }));
        assert_eq!(
            title.covers.low.as_deref(),
            Some("https://static.rumix.me/uploads/pics/00/00/312_p.jpg")
        );
        assert_eq!(
            title.covers.mid.as_deref(),
            Some("https://static.rumix.me/uploads/pics/00/00/312.jpg")
        );
        assert_eq!(
            title.covers.high.as_deref(),
            Some("https://static.rumix.me/uploads/pics/00/00/312_o.jpg")
        );
        assert_eq!(title.lang.as_deref(), Some("ru"));
    }

    #[test]
    fn chapter_keys_cover_volumes_decimals_extras_and_no_numbers() {
        let chapters = parse_chapters(TITLE, &site(), "ru").expect("chapters");
        let keys: Vec<&str> = chapters
            .iter()
            .map(|chapter| chapter.chapter_key.as_str())
            .collect();

        assert!(keys.contains(&"v1:c0"), "{keys:?}");
        assert!(keys.contains(&"v1:c1"));
        assert!(keys.contains(&"v2:c1.5"));
        assert!(keys.contains(&"v4:extra:c27"));
        assert!(
            keys.iter().any(|key| key.starts_with("v3:special:x:h")),
            "{keys:?}"
        );

        let zero = chapters
            .iter()
            .find(|chapter| chapter.chapter_key == "v1:c0")
            .expect("chapter zero");
        assert_eq!(zero.number_text.as_deref(), Some("0"));
        assert_eq!(zero.number_milli, Some(0));
        assert_eq!(zero.name.as_deref(), Some("Начало"));

        let decimal = chapters
            .iter()
            .find(|chapter| chapter.chapter_key == "v2:c1.5")
            .expect("decimal");
        assert_eq!(decimal.number_milli, Some(1500));

        let special = chapters
            .iter()
            .find(|chapter| chapter.chapter_key.starts_with("v3:special:x:h"))
            .expect("no-number special");
        assert_eq!(special.number_text, None);
        assert_eq!(special.chapter_kind, ChapterKind::Special);

        assert_eq!(
            chapters
                .iter()
                .find(|chapter| chapter.chapter_key == "v4:extra:c27")
                .and_then(|chapter| chapter.name.as_deref()),
            Some("Дополнительная глава")
        );
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
    fn parses_signed_page_urls_in_order() {
        let content = parse_content(CONTENT).expect("content");
        let SourceContent::Images { pages } = content else {
            panic!("expected images");
        };
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].index, 0);
        assert!(pages[0].image_ref.contains("/fs8/00/09/00/000.jpg"));
        assert!(
            !pages[0].image_ref.contains("t="),
            "image_ref must be stable"
        );
        assert!(
            pages[0]
                .image_url
                .as_deref()
                .is_some_and(|url| url.contains("t=1795610057")),
            "image_url must keep the signed URL"
        );
        assert_eq!(pages[0].width, Some(1500));
        assert_eq!(pages[0].height, Some(2132));
    }

    #[test]
    fn planned_chapter_is_not_found() {
        let raw = r#"<html><body><script>rm_h.readerInit({}, [], false, [], true);</script></body></html>"#;
        assert!(matches!(parse_content(raw), Err(SourceError::NotFound)));
    }
}
