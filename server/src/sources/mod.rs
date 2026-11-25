//! Источники: узкий контракт, нормализованные DTO и компилируемые адаптеры.
//! HTTP, БД и API не знают особенностей конкретного сайта; добавление
//! источника — новый модуль и ветка в `SourceKind`.

pub mod config;
pub mod descriptor;
pub mod dto;
pub mod error;
pub mod http;
pub mod registry;
pub mod remanga;

#[cfg(debug_assertions)]
pub mod fixture;

#[cfg(test)]
mod contract;

pub use descriptor::{Capability, ConfigField, FieldType, SourceDescriptor};
pub use dto::{
    ChapterKind, ContentKind, ExternalId, MediaKind, SearchPage, SearchRequest, SourceChapter,
    SourceContent, SourcePage, SourceSearchItem, SourceText, SourceTitle, TextFormat, TitleStatus,
};
pub use error::SourceError;

use reqwest::Url;

#[cfg(debug_assertions)]
use crate::sources::fixture::Fixture;
use crate::sources::remanga::Remanga;

/// Единственная точка dispatch. Компилятор требует ветку для каждого
/// источника, поэтому новый адаптер нельзя забыть в методе.
#[derive(Clone)]
pub enum SourceKind {
    Remanga(Remanga),
    #[cfg(debug_assertions)]
    Fixture(Fixture),
}

impl SourceKind {
    pub fn descriptor(&self) -> &'static SourceDescriptor {
        match self {
            Self::Remanga(_) => &remanga::DESCRIPTOR,
            #[cfg(debug_assertions)]
            Self::Fixture(_) => &fixture::DESCRIPTOR,
        }
    }

    pub fn supports(&self, kind: MediaKind) -> bool {
        self.descriptor().supports(kind)
    }

    pub async fn search(&self, request: SearchRequest) -> Result<SearchPage, SourceError> {
        match self {
            Self::Remanga(source) => source.search(request).await,
            #[cfg(debug_assertions)]
            Self::Fixture(source) => source.search(request).await,
        }
    }

    pub async fn title(
        &self,
        kind: MediaKind,
        external_id: &ExternalId,
    ) -> Result<SourceTitle, SourceError> {
        match self {
            Self::Remanga(source) => source.title(kind, external_id).await,
            #[cfg(debug_assertions)]
            Self::Fixture(source) => source.title(kind, external_id).await,
        }
    }

    pub async fn chapters(
        &self,
        kind: MediaKind,
        external_id: &ExternalId,
    ) -> Result<Vec<SourceChapter>, SourceError> {
        match self {
            Self::Remanga(source) => source.chapters(kind, external_id).await,
            #[cfg(debug_assertions)]
            Self::Fixture(source) => source.chapters(kind, external_id).await,
        }
    }

    pub async fn content(
        &self,
        kind: MediaKind,
        chapter: &SourceChapter,
    ) -> Result<SourceContent, SourceError> {
        match self {
            Self::Remanga(source) => source.content(kind, chapter).await,
            #[cfg(debug_assertions)]
            Self::Fixture(source) => source.content(kind, chapter).await,
        }
    }

    pub async fn text(
        &self,
        kind: MediaKind,
        chapter: &SourceChapter,
    ) -> Result<SourceText, SourceError> {
        match self {
            Self::Remanga(source) => source.text(kind, chapter).await,
            #[cfg(debug_assertions)]
            Self::Fixture(source) => source.text(kind, chapter).await,
        }
    }

    pub fn resolve_image(&self, image_ref: &str) -> Result<Url, SourceError> {
        match self {
            Self::Remanga(source) => source.resolve_image(image_ref),
            #[cfg(debug_assertions)]
            Self::Fixture(source) => source.resolve_image(image_ref),
        }
    }

    pub fn resolve_cover(&self, cover_ref: &str) -> Result<Url, SourceError> {
        match self {
            Self::Remanga(source) => source.resolve_cover(cover_ref),
            #[cfg(debug_assertions)]
            Self::Fixture(source) => source.resolve_cover(cover_ref),
        }
    }
}
