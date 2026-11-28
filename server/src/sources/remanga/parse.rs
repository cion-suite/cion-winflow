//! Чистые парсеры Remanga v2. Сеть сюда не входит: всё проверяется
//! на фикстурах, поэтому редизайн API ломает тест, а не прод.

use serde::Deserialize;
use serde_json::Value;

use crate::sources::dto::{
    ChapterKind, ContentKind, ExternalId, LoginOutcome, SearchPage, SourceChapter, SourceContent,
    SourceCovers, SourcePage, SourceSearchItem, SourceTitle, TitleKind, TitleStatus,
};
use crate::sources::error::SourceError;
use crate::sources::keys::{build_chapter_key, normalize_number, number_milli};
use crate::sources::text::{dedupe, truncate_chars};

// Потолки совпадают с валидацией ручного создания тайтла в catalog::service.
const MAX_NAME_CHARS: usize = 300;
const MAX_LIST_ITEMS: usize = 50;
const MAX_LIST_ITEM_CHARS: usize = 200;
const MAX_DESCRIPTION_CHARS: usize = 20_000;

#[derive(Debug, Deserialize, serde::Serialize)]
struct RawNamed {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawCover {
    #[serde(default)]
    low: Option<String>,
    #[serde(default)]
    mid: Option<String>,
    #[serde(default)]
    high: Option<String>,
}

impl RawCover {
    fn covers(&self) -> SourceCovers {
        SourceCovers {
            low: clean_ref(&self.low),
            mid: clean_ref(&self.mid),
            high: clean_ref(&self.high),
        }
    }
}

fn clean_ref(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
}

#[derive(Debug, Deserialize)]
struct RawSearchMeta {
    #[serde(default)]
    total_items: Option<u64>,
    #[serde(default)]
    total_pages: Option<u64>,
    #[serde(default)]
    page: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RawSearch {
    #[serde(default)]
    meta: Option<RawSearchMeta>,
    #[serde(default)]
    results: Vec<RawSearchItem>,
}

#[derive(Debug, Deserialize)]
struct RawSearchItem {
    #[serde(default)]
    main_name: Option<String>,
    #[serde(default)]
    secondary_name: Option<String>,
    #[serde(default)]
    another_name: Option<String>,
    #[serde(default)]
    dir: Option<String>,
    #[serde(default, rename = "type")]
    title_type: Option<RawNamed>,
    #[serde(default)]
    issue_year: Option<i16>,
    #[serde(default)]
    status: Option<RawNamed>,
    #[serde(default)]
    cover: Option<RawCover>,
}

#[derive(Debug, Deserialize)]
struct RawCreator {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "type")]
    kind: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct RawBranch {
    id: i64,
    #[serde(default)]
    publishers: Vec<RawNamed>,
}

/// Ветка перевода: `branch_id` для запроса глав и имя группы для вариантов.
#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub id: i64,
    pub group_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawTitle {
    #[serde(default)]
    main_name: Option<String>,
    #[serde(default)]
    secondary_name: Option<String>,
    #[serde(default)]
    another_name: Option<String>,
    #[serde(default)]
    dir: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "type")]
    title_type: Option<RawNamed>,
    #[serde(default)]
    issue_year: Option<i16>,
    #[serde(default)]
    avg_rating: Option<Value>,
    #[serde(default)]
    status: Option<RawNamed>,
    #[serde(default)]
    cover: Option<RawCover>,
    #[serde(default)]
    creators: Vec<RawCreator>,
    #[serde(default)]
    genres: Vec<RawNamed>,
    #[serde(default)]
    categories: Vec<RawNamed>,
    #[serde(default)]
    branches: Vec<RawBranch>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
struct RawChapter {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    tome: Option<i64>,
    #[serde(default)]
    chapter: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    is_paid: Option<bool>,
    #[serde(default)]
    is_bought: Option<bool>,
    #[serde(default)]
    is_published: Option<bool>,
    #[serde(default)]
    upload_date: Option<String>,
    #[serde(default)]
    publishers: Vec<RawNamed>,
}

#[derive(Debug, Deserialize)]
struct RawChaptersPage {
    #[serde(default)]
    next: Option<Value>,
    #[serde(default)]
    results: Vec<RawChapter>,
}

#[derive(Debug, Deserialize)]
struct RawPage {
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct RawContent {
    #[serde(default)]
    is_paid: Option<bool>,
    #[serde(default)]
    content_type: Option<String>,
    /// У платных глав поле может быть null, а не пустым массивом.
    #[serde(default)]
    pages: Option<Vec<Vec<RawPage>>>,
}

pub struct ParsedChapters {
    pub items: Vec<SourceChapter>,
    pub next_cursor: Option<String>,
}

pub fn parse_search(raw: &str) -> Result<SearchPage, SourceError> {
    let parsed: RawSearch = serde_json::from_str(raw).map_err(parse_error("search"))?;

    let mut items = Vec::with_capacity(parsed.results.len());
    for item in parsed.results {
        let Some(dir) = item.dir.as_deref().filter(|dir| !dir.trim().is_empty()) else {
            continue;
        };
        let Some(name) = item.main_name.or(item.secondary_name.clone()) else {
            continue;
        };
        let external_id = ExternalId::new(dir)?;

        items.push(SourceSearchItem {
            external_id,
            kind: title_kind(item.title_type.as_ref()),
            name: truncate_chars(name.trim(), MAX_NAME_CHARS),
            alt_names: alt_names(&item.secondary_name, &item.another_name, &name),
            // В выдаче нужен самый лёгкий вариант: он показывается в диалоге добавления
            cover_ref: item
                .cover
                .as_ref()
                .and_then(|cover| cover.covers().search_ref()),
            year: item.issue_year,
            status: item
                .status
                .as_ref()
                .and_then(|status| map_status(status.name.as_deref())),
            external_url: None,
        });
    }

    let (next_cursor, total) = match &parsed.meta {
        Some(meta) => {
            let page = meta.page.unwrap_or(1);
            let next = meta
                .total_pages
                .filter(|pages| page < *pages)
                .map(|_| (page + 1).to_string());
            (next, meta.total_items)
        }
        None => (None, None),
    };

    Ok(SearchPage {
        items,
        next_cursor,
        total,
    })
}

pub fn parse_title(raw: &str) -> Result<SourceTitle, SourceError> {
    let parsed: RawTitle = serde_json::from_str(raw).map_err(parse_error("title"))?;
    let kind = title_kind(parsed.title_type.as_ref()).unwrap_or(TitleKind::Other);

    let dir = parsed
        .dir
        .as_deref()
        .filter(|dir| !dir.trim().is_empty())
        .ok_or_else(|| SourceError::Parse {
            stage: "title",
            detail: "missing dir".to_owned(),
        })?;
    let external_id = ExternalId::new(dir)?;

    let name = parsed
        .main_name
        .or(parsed.secondary_name.clone())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| SourceError::Parse {
            stage: "title",
            detail: "missing name".to_owned(),
        })?;

    let mut genres: Vec<String> = parsed
        .genres
        .iter()
        .chain(parsed.categories.iter())
        .filter_map(|named| named.name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| truncate_chars(name, MAX_LIST_ITEM_CHARS))
        .collect();
    dedupe(&mut genres);
    genres.truncate(MAX_LIST_ITEMS);

    // type 1 — автор, type 4 — художник; type 3 — издатель
    let mut authors: Vec<String> = parsed
        .creators
        .iter()
        .filter(|creator| matches!(creator.kind, Some(1 | 4) | None))
        .filter_map(|creator| creator.name.as_deref())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| truncate_chars(name, MAX_LIST_ITEM_CHARS))
        .collect();
    dedupe(&mut authors);
    authors.truncate(MAX_LIST_ITEMS);

    Ok(SourceTitle {
        external_id,
        kind,
        name: truncate_chars(&name, MAX_NAME_CHARS),
        alt_names: alt_names(&parsed.secondary_name, &parsed.another_name, &name),
        description: parsed
            .description
            .as_deref()
            .map(html_to_text)
            .map(|text| truncate_chars(&text, MAX_DESCRIPTION_CHARS))
            .filter(|text| !text.is_empty()),
        covers: parsed
            .cover
            .as_ref()
            .map(RawCover::covers)
            .unwrap_or_default(),
        authors,
        genres,
        status: parsed
            .status
            .as_ref()
            .and_then(|status| map_status(status.name.as_deref())),
        year: parsed
            .issue_year
            .filter(|year| (1800..=2200).contains(year)),
        rating: parsed.avg_rating.as_ref().and_then(parse_rating),
        // Язык подставит адаптер из конфига: parse_title не знает настроек
        lang: None,
        external_url: None,
        raw_json: serde_json::from_str(raw).ok(),
    })
}

/// Ветки перевода: главы Remanga запрашиваются по `branch_id`, который
/// есть только в ответе `title`. Возвращаем id и число глав для оценки.
pub fn parse_branches(raw: &str) -> Result<Vec<BranchInfo>, SourceError> {
    let parsed: RawTitle = serde_json::from_str(raw).map_err(parse_error("title"))?;
    let mut branches: Vec<BranchInfo> = parsed
        .branches
        .into_iter()
        .map(|branch| {
            let names: Vec<&str> = branch
                .publishers
                .iter()
                .filter_map(|publisher| publisher.name.as_deref())
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .collect();
            BranchInfo {
                id: branch.id,
                group_name: (!names.is_empty())
                    .then(|| truncate_chars(&names.join(", "), MAX_LIST_ITEM_CHARS)),
            }
        })
        .collect();
    branches.sort_by_key(|branch| branch.id);
    branches.dedup_by_key(|branch| branch.id);
    Ok(branches)
}

pub fn parse_chapters(
    raw: &str,
    language: &str,
    group_name: Option<&str>,
) -> Result<ParsedChapters, SourceError> {
    let parsed: RawChaptersPage = serde_json::from_str(raw).map_err(parse_error("chapters"))?;

    let mut items = Vec::with_capacity(parsed.results.len());
    for chapter in parsed.results {
        // Неопубликованные главы не должны просачиваться в каталог
        if chapter.is_published == Some(false) {
            continue;
        }
        let Some(id) = chapter.id else {
            continue;
        };
        let external_id = ExternalId::new(id.to_string())?;

        let chapter_publishers: Vec<&str> = chapter
            .publishers
            .iter()
            .filter_map(|publisher| publisher.name.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        let chapter_group = if chapter_publishers.is_empty() {
            group_name.map(str::to_owned)
        } else {
            Some(truncate_chars(
                &chapter_publishers.join(", "),
                MAX_LIST_ITEM_CHARS,
            ))
        };

        let number_text = chapter
            .chapter
            .as_deref()
            .map(normalize_number)
            .filter(|number| !number.is_empty());
        let volume = chapter
            .tome
            .and_then(|tome| u16::try_from(tome).ok())
            .filter(|tome| *tome > 0);
        let chapter_kind = ChapterKind::Normal;
        let chapter_key =
            build_chapter_key(volume, chapter_kind, number_text.as_deref(), &external_id);

        items.push(SourceChapter {
            external_id,
            chapter_key,
            number_text: number_text.clone(),
            number_milli: number_text.as_deref().and_then(number_milli),
            volume,
            chapter_kind,
            // Remanga — комиксы; тип известен заранее и не требует content()
            content_kind: Some(ContentKind::Images),
            name: chapter
                .name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned),
            group_name: chapter_group,
            language: Some(language.to_owned()),
            is_locked: chapter.is_paid == Some(true) && chapter.is_bought != Some(true),
            external_url: None,
            published_at: chapter.upload_date.as_deref().and_then(parse_published_at),
            raw_json: serde_json::to_value(&chapter).ok(),
        });
    }

    let next_cursor = parsed.next.and_then(|next| match next {
        Value::Number(number) => Some(number.to_string()),
        Value::String(cursor) if !cursor.is_empty() => Some(cursor),
        _ => None,
    });

    Ok(ParsedChapters { items, next_cursor })
}

pub fn parse_content(raw: &str) -> Result<SourceContent, SourceError> {
    let parsed: RawContent = serde_json::from_str(raw).map_err(parse_error("content"))?;

    if parsed.is_paid == Some(true) {
        return Err(SourceError::Locked);
    }
    match parsed.content_type.as_deref() {
        Some("comics") => {}
        Some(content_type) => {
            return Err(SourceError::Parse {
                stage: "content",
                detail: format!("unsupported content type `{content_type}`"),
            });
        }
        None => {
            return Err(SourceError::Parse {
                stage: "content",
                detail: "missing content type".to_owned(),
            });
        }
    }

    let mut pages = Vec::new();
    for row in parsed.pages.unwrap_or_default() {
        for page in row {
            let Some(link) = page.link.filter(|link| !link.trim().is_empty()) else {
                continue;
            };
            pages.push(SourcePage {
                index: u32::try_from(pages.len()).unwrap_or(u32::MAX),
                image_ref: link.clone(),
                image_url: Some(link),
                width: page.width,
                height: page.height,
            });
        }
    }

    if pages.is_empty() {
        return Err(SourceError::Parse {
            stage: "content",
            detail: "no pages".to_owned(),
        });
    }

    Ok(SourceContent::Images { pages })
}

/// Remanga отдаёт тип как `{ id, name }`; id не используем — названия
/// стабильнее для чтения и не требуют таблицы соответствий.
fn title_kind(raw: Option<&RawNamed>) -> Option<TitleKind> {
    raw.and_then(|named| named.name.as_deref())
        .and_then(TitleKind::from_source_name)
}

pub fn map_status(name: Option<&str>) -> Option<TitleStatus> {
    match name?.trim() {
        "Продолжается" | "Выходит" => Some(TitleStatus::Ongoing),
        "Закончен" | "Завершён" | "Завершен" => {
            Some(TitleStatus::Completed)
        }
        "Заморожен" | "Приостановлен" => Some(TitleStatus::Hiatus),
        "Прекращен" | "Прекращён" | "Отменён" | "Отменен" => {
            Some(TitleStatus::Cancelled)
        }
        _ => None,
    }
}

fn parse_rating(value: &Value) -> Option<f32> {
    let rating = match value {
        Value::String(text) => text.trim().parse::<f32>().ok()?,
        Value::Number(number) => number.as_f64()? as f32,
        _ => return None,
    };
    // 0.0 у Remanga означает «без оценок», а не настоящий ноль
    (rating > 0.0 && rating <= 10.0).then_some(rating)
}

fn parse_published_at(raw: &str) -> Option<i64> {
    // Remanga отдаёт время без смещения; считаем его UTC.
    crate::sources::text::parse_rfc3339(raw)
}

fn alt_names(secondary: &Option<String>, another: &Option<String>, main: &str) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(secondary) = secondary.as_deref() {
        names.push(secondary.trim().to_owned());
    }
    if let Some(another) = another.as_deref() {
        let another = another.trim();
        // « / » — разделитель списка; одиночный слэш внутри названия не режем
        let parts: Vec<&str> = if another.contains(" / ") {
            another
                .split(" / ")
                .flat_map(|part| part.split('\n'))
                .collect()
        } else {
            another.split('\n').collect()
        };

        for part in parts {
            let part = part.trim();
            if !part.is_empty() {
                names.push(truncate_chars(part, MAX_LIST_ITEM_CHARS));
            }
        }
    }
    names.retain(|name| !name.is_empty() && name != main);
    dedupe(&mut names);
    names.truncate(MAX_LIST_ITEMS);
    names
}

/// Описание источника — plain text: чужой HTML в UI не рендерится.
fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut chars = html.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            '&' if !in_tag => {
                let mut entity = String::new();
                while let Some(&next) = chars.peek() {
                    if next == ';' || entity.len() > 10 || next.is_whitespace() {
                        break;
                    }
                    entity.push(next);
                    chars.next();
                }
                if chars.peek() == Some(&';') {
                    chars.next();
                }
                text.push_str(match entity.as_str() {
                    "nbsp" => " ",
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "#39" | "apos" => "'",
                    _ => "",
                });
            }
            character if !in_tag => text.push(character),
            _ => {}
        }
    }

    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// У v1-логина конверт `{content: ...}`, в отличие от плоских ответов v2.
#[derive(Deserialize)]
struct RawLogin {
    #[serde(default)]
    content: Option<RawLoginContent>,
}

#[derive(Deserialize)]
struct RawLoginContent {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    two_factor_auth: bool,
    #[serde(default)]
    methods: Vec<String>,
}

/// Разбирает успешный ответ входа. 400 с `{msg}` и 2FA-ветку адаптер
/// обрабатывает отдельно: здесь только форма `content`.
pub fn parse_login(raw: &str) -> Result<LoginOutcome, SourceError> {
    let parsed: RawLogin = serde_json::from_str(raw).map_err(parse_error("login"))?;
    let content = parsed.content.ok_or(SourceError::Parse {
        stage: "login",
        detail: "missing content".to_owned(),
    })?;

    Ok(LoginOutcome {
        access_token: content
            .access_token
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty()),
        two_factor_auth: content.two_factor_auth,
        methods: content
            .methods
            .into_iter()
            .map(|method| method.trim().to_owned())
            .filter(|method| !method.is_empty())
            .collect(),
    })
}

fn parse_error<'a>(stage: &'static str) -> impl Fn(serde_json::Error) -> SourceError + 'a {
    move |error| {
        tracing::debug!(%error, stage, "cannot parse remanga response");
        SourceError::Parse {
            stage,
            detail: "invalid json shape".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const SEARCH: &str = include_str!("fixtures/search.json");
    const TITLE: &str = include_str!("fixtures/title.json");
    const CHAPTERS: &str = include_str!("fixtures/chapters.json");
    const CONTENT: &str = include_str!("fixtures/content.json");
    const LOGIN: &str = include_str!("fixtures/login.json");

    #[test]
    fn parses_search_items_and_cursor() {
        let page = parse_search(SEARCH).expect("search");
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.total, Some(194));
        assert_eq!(page.next_cursor.as_deref(), Some("2"));

        let first = &page.items[0];
        assert_eq!(first.external_id.as_str(), "<13.10.2026>naruto_");
        assert_eq!(first.kind, Some(TitleKind::Manga));
        assert_eq!(first.name, "Наруто");
        assert!(first.alt_names.contains(&"Naruto".to_owned()));
        // В выдаче — low-вариант, а не mid/high
        assert_eq!(
            first.cover_ref.as_deref(),
            Some("/media/titles/naruto/b73f82b8e39d93d74820a386a0778f76.jpg")
        );
        assert_eq!(first.year, Some(1999));
        assert_eq!(first.status, Some(TitleStatus::Completed));
    }

    #[test]
    fn parses_title_metadata_as_plain_text() {
        let title = parse_title(TITLE).expect("title");
        assert_eq!(title.external_id.as_str(), "<13.10.2026>naruto_");
        assert_eq!(title.kind, TitleKind::Manga);
        assert_eq!(title.name, "Наруто");
        assert_eq!(title.status, Some(TitleStatus::Completed));
        assert_eq!(title.year, Some(1999));
        assert_eq!(title.rating, Some(9.0));
        assert!(
            title
                .description
                .as_deref()
                .is_some_and(|text| !text.contains('<'))
        );
        assert!(title.authors.contains(&"Масаси Кисимото".to_owned()));
        assert!(title.genres.contains(&"Экшен".to_owned()));
        assert_eq!(
            title.covers.low.as_deref(),
            Some("/media/titles/naruto/b73f82b8e39d93d74820a386a0778f76.jpg")
        );
        assert_eq!(
            title.covers.mid.as_deref(),
            Some("/media/titles/naruto/3b90be24a27a568a15ede3e2fd123c91.jpg")
        );
        assert_eq!(
            title.covers.high.as_deref(),
            Some("/media/titles/naruto/cc0c0a4c71306e2955760cec3958f27e.jpg")
        );
        let branches = parse_branches(TITLE).expect("branches");
        assert_eq!(branches.len(), 1);
        assert_eq!(branches[0].id, 249);
    }

    #[test]
    fn chapter_keys_follow_the_plan() {
        let chapters = parse_chapters(CHAPTERS, "ru", None).expect("chapters");
        let keys: Vec<&str> = chapters
            .items
            .iter()
            .map(|c| c.chapter_key.as_str())
            .collect();

        assert!(keys.contains(&"c0"), "tome 0 means no volume: {keys:?}");
        assert!(keys.contains(&"v1:c1"));
        assert!(keys.contains(&"v1:c1.5"));
        assert!(keys.contains(&"v1:c1-2"));
        // Без номера — стабильный хеш от external_id
        assert!(keys.iter().any(|key| key.starts_with("v1:x:h")), "{keys:?}");

        let fractional = chapters
            .items
            .iter()
            .find(|chapter| chapter.chapter_key == "v1:c1.5")
            .expect("fractional");
        assert_eq!(fractional.number_milli, Some(1500));
        assert_eq!(fractional.content_kind, Some(ContentKind::Images));

        let range = chapters
            .items
            .iter()
            .find(|chapter| chapter.chapter_key == "v1:c1-2")
            .expect("range");
        assert_eq!(range.number_milli, Some(1000));

        let paid = chapters
            .items
            .iter()
            .find(|chapter| chapter.chapter_key == "v1:c2")
            .expect("paid");
        assert!(paid.is_locked);
    }

    #[test]
    fn parses_content_pages_in_reading_order() {
        let content = parse_content(CONTENT).expect("content");
        let SourceContent::Images { pages } = content else {
            panic!("expected images");
        };
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].index, 0);
        assert!(pages[0].image_ref.ends_with("1_re.jpg"));
        assert_eq!(pages[0].width, Some(812));
    }

    #[test]
    fn paid_content_is_locked() {
        let raw = r#"{"is_paid":true,"content_type":"comics","pages":[]}"#;
        assert!(matches!(parse_content(raw), Err(SourceError::Locked)));
    }

    #[test]
    fn parses_login_token_and_two_factor() {
        let outcome = parse_login(LOGIN).expect("login");
        assert_eq!(
            outcome.access_token.as_deref(),
            Some("login-token-1234567890abcdefghij")
        );
        assert!(!outcome.two_factor_auth);
        assert!(outcome.methods.is_empty());

        let two_factor =
            parse_login(r#"{"content":{"two_factor_auth":true,"methods":["email","telegram"]}}"#)
                .expect("2fa");
        assert!(two_factor.access_token.is_none());
        assert!(two_factor.two_factor_auth);
        assert_eq!(two_factor.methods, ["email", "telegram"]);
    }

    #[test]
    fn login_without_content_is_a_parse_error() {
        assert!(matches!(
            parse_login(r#"{"msg":"wrong password"}"#),
            Err(SourceError::Parse { .. })
        ));
    }

    #[test]
    fn every_chapter_key_is_unique_inside_title() {
        let chapters = parse_chapters(CHAPTERS, "ru", None).expect("chapters");
        let mut keys = HashSet::new();
        for chapter in chapters.items {
            assert!(keys.insert(chapter.chapter_key), "duplicate key");
        }
    }
}
