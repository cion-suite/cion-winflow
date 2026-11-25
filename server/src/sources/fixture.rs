//! Источник-заглушка для интеграционных тестов. Компилируется только
//! в debug-профиле: release-бинарник его не содержит.

use std::sync::atomic::{AtomicUsize, Ordering};

use reqwest::Url;

use crate::sources::config::{HostAllowlist, url_allowed};
use crate::sources::descriptor::{Capability, SourceDescriptor};
use crate::sources::dto::{
    ChapterKind, ContentKind, ExternalId, MediaKind, SearchPage, SearchRequest, SourceChapter,
    SourceContent, SourceCovers, SourcePage, SourceSearchItem, SourceText, SourceTitle,
};
use crate::sources::error::SourceError;
use crate::sources::http::AdapterCtx;

static CALLS: AtomicUsize = AtomicUsize::new(0);

/// Сколько объектов запрошено у адаптера; тесты проверяют, что страница
/// тайтла читает только БД.
pub fn call_count() -> usize {
    CALLS.load(Ordering::SeqCst)
}

pub fn reset_calls() {
    CALLS.store(0, Ordering::SeqCst);
}

pub const CODE: &str = "fixture";

/// Второй внешний id нужен тестам политики «один entry на источник».
pub const ALT_TITLE_ID: &str = "fixture-title-2";

fn known_title(external_id: &ExternalId) -> bool {
    matches!(external_id.as_str(), "fixture-title" | ALT_TITLE_ID)
}

fn title_name(external_id: &ExternalId) -> &'static str {
    if external_id.as_str() == ALT_TITLE_ID {
        "Fixture Title 2"
    } else {
        "Fixture Title"
    }
}

pub const DESCRIPTOR: SourceDescriptor = SourceDescriptor {
    code: CODE,
    name: "Fixture",
    version: 1,
    kinds: &[MediaKind::Manga],
    default_kind: MediaKind::Manga,
    capabilities: &[Capability::Search, Capability::Chapters, Capability::Images],
    config: &[],
    filters: &[],
};

#[derive(Clone)]
pub struct Fixture {
    ctx: AdapterCtx,
    allowlist: HostAllowlist,
}

impl Fixture {
    pub fn new(ctx: AdapterCtx) -> Result<Self, SourceError> {
        let host = ctx
            .base_url
            .host_str()
            .ok_or_else(|| SourceError::Config("fixture base_url has no host".to_owned()))?;
        let allowlist = HostAllowlist::new([host.to_owned()]).map_err(SourceError::Config)?;
        Ok(Self { ctx, allowlist })
    }

    pub async fn search(&self, request: SearchRequest) -> Result<SearchPage, SourceError> {
        CALLS.fetch_add(1, Ordering::SeqCst);
        let query = request.query.trim();
        if query.is_empty() {
            return Err(SourceError::BadFilter("query is empty".to_owned()));
        }

        let item = SourceSearchItem {
            external_id: ExternalId::new("fixture-title")?,
            kind: MediaKind::Manga,
            name: "Fixture Title".to_owned(),
            alt_names: vec!["Fixture Alt".to_owned()],
            cover_ref: Some("/cover.jpg".to_owned()),
            year: Some(2024),
            status: Some(crate::sources::dto::TitleStatus::Ongoing),
            external_url: None,
        };

        Ok(SearchPage {
            items: vec![item],
            next_cursor: None,
            total: Some(1),
        })
    }

    pub async fn title(
        &self,
        _kind: MediaKind,
        external_id: &ExternalId,
    ) -> Result<SourceTitle, SourceError> {
        CALLS.fetch_add(1, Ordering::SeqCst);
        if !known_title(external_id) {
            return Err(SourceError::NotFound);
        }

        Ok(SourceTitle {
            external_id: external_id.clone(),
            kind: MediaKind::Manga,
            name: title_name(external_id).to_owned(),
            alt_names: vec!["Fixture Alt".to_owned()],
            description: Some("Fixture description".to_owned()),
            covers: SourceCovers {
                mid: Some("/cover.jpg".to_owned()),
                ..SourceCovers::default()
            },
            authors: vec!["Fixture Author".to_owned()],
            genres: vec!["Action".to_owned()],
            status: Some(crate::sources::dto::TitleStatus::Ongoing),
            year: Some(2024),
            rating: Some(7.5),
            lang: Some("ru".to_owned()),
            external_url: None,
            raw_json: None,
        })
    }

    pub async fn chapters(
        &self,
        _kind: MediaKind,
        external_id: &ExternalId,
    ) -> Result<Vec<SourceChapter>, SourceError> {
        CALLS.fetch_add(1, Ordering::SeqCst);
        if !known_title(external_id) {
            return Err(SourceError::NotFound);
        }

        let chapter = |number: &str, locked: bool| SourceChapter {
            external_id: ExternalId::new(format!("chapter-{number}")).expect("fixture id"),
            chapter_key: format!("c{number}"),
            number_text: Some(number.to_owned()),
            number_milli: number.parse::<i64>().ok().map(|value| value * 1000),
            volume: None,
            chapter_kind: ChapterKind::Normal,
            content_kind: Some(ContentKind::Images),
            name: Some(format!("Chapter {number}")),
            group_name: None,
            language: Some("ru".to_owned()),
            is_locked: locked,
            external_url: None,
            published_at: None,
            raw_json: None,
        };

        Ok(vec![
            chapter("1", false),
            chapter("2", false),
            chapter("3", true),
        ])
    }

    pub async fn content(
        &self,
        _kind: MediaKind,
        chapter: &SourceChapter,
    ) -> Result<SourceContent, SourceError> {
        CALLS.fetch_add(1, Ordering::SeqCst);
        if chapter.is_locked {
            return Err(SourceError::Locked);
        }

        let base = self.ctx.base_url.as_str().trim_end_matches('/');
        let pages = (0..2)
            .map(|index| SourcePage {
                index,
                image_ref: format!("{base}/img/{index}.png"),
                image_url: Some(format!("{base}/img/{index}.png")),
                width: Some(10),
                height: Some(20),
            })
            .collect();

        Ok(SourceContent::Images { pages })
    }

    pub async fn text(
        &self,
        _kind: MediaKind,
        _chapter: &SourceChapter,
    ) -> Result<SourceText, SourceError> {
        Err(SourceError::UnsupportedCapability)
    }

    pub fn resolve_image(&self, image_ref: &str) -> Result<Url, SourceError> {
        self.resolve(image_ref)
    }

    pub fn resolve_cover(&self, cover_ref: &str) -> Result<Url, SourceError> {
        self.resolve(cover_ref)
    }

    fn resolve(&self, reference: &str) -> Result<Url, SourceError> {
        let url = if reference.starts_with("http://") || reference.starts_with("https://") {
            Url::parse(reference).map_err(|_| SourceError::InvalidExternalId)?
        } else {
            self.ctx
                .base_url
                .join(reference)
                .map_err(|_| SourceError::InvalidExternalId)?
        };
        if url_allowed(&url, &self.allowlist) {
            Ok(url)
        } else {
            Err(SourceError::Config("host is not allowed".to_owned()))
        }
    }
}
