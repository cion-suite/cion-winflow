//! Нормализованные DTO источников. Общими для всех адаптеров остаются
//! только эти типы; форма ответа конкретного сайта живёт внутри адаптера.

use serde::{Deserialize, Serialize};

use crate::sources::error::SourceError;

/// Максимальная длина `external_id`: длиннее — почти наверняка ошибка парсинга.
pub const MAX_EXTERNAL_ID_CHARS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Manga,
    Anime,
    Book,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manga => "manga",
            Self::Anime => "anime",
            Self::Book => "book",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "manga" => Some(Self::Manga),
            "anime" => Some(Self::Anime),
            "book" => Some(Self::Book),
            _ => None,
        }
    }
}

impl std::fmt::Display for MediaKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentKind {
    Images,
    Video,
    Text,
}

impl ContentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Images => "images",
            Self::Video => "video",
            Self::Text => "text",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "images" => Some(Self::Images),
            "video" => Some(Self::Video),
            "text" => Some(Self::Text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChapterKind {
    Normal,
    Special,
    Extra,
    Oneshot,
    Preview,
}

impl ChapterKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Special => "special",
            Self::Extra => "extra",
            Self::Oneshot => "oneshot",
            Self::Preview => "preview",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(Self::Normal),
            "special" => Some(Self::Special),
            "extra" => Some(Self::Extra),
            "oneshot" => Some(Self::Oneshot),
            "preview" => Some(Self::Preview),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleStatus {
    Ongoing,
    Completed,
    Hiatus,
    Cancelled,
}

impl TitleStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ongoing => "ongoing",
            Self::Completed => "completed",
            Self::Hiatus => "hiatus",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextFormat {
    Html,
    Markdown,
    Plain,
}

impl TextFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Markdown => "markdown",
            Self::Plain => "plain",
        }
    }
}

/// Непрозрачный идентификатор записи источника. Валидируется один раз,
/// при создании: адаптер обязан URL-кодировать его сам при сборке запроса.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExternalId(String);

impl ExternalId {
    pub fn new(value: impl Into<String>) -> Result<Self, SourceError> {
        let value = value.into();
        let trimmed = value.trim();
        if trimmed.is_empty()
            || trimmed.chars().count() > MAX_EXTERNAL_ID_CHARS
            || trimmed.chars().any(char::is_control)
            || trimmed.contains("://")
            || trimmed.contains("..")
            // `/` ломает сборку пути: адаптер кодирует сегмент, а не иерархию
            || trimmed.contains('/')
        {
            return Err(SourceError::InvalidExternalId);
        }

        Ok(Self(trimmed.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ExternalId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Канонический ключ главы. Строит адаптер; сервер только хранит его
/// и фиксирует при первом импорте.
pub type ChapterKey = String;

/// Размер обложки. `low` показывается в поиске и диалоге добавления,
/// `mid` — в библиотеке, `high` — на странице тайтла.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverSize {
    Low,
    Mid,
    High,
}

impl CoverSize {
    pub const ALL: [Self; 3] = [Self::Low, Self::Mid, Self::High];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Mid => "mid",
            Self::High => "high",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "low" => Some(Self::Low),
            "mid" => Some(Self::Mid),
            "high" => Some(Self::High),
            _ => None,
        }
    }

    /// Порядок поиска файла: запрошенный размер, затем ближайший больший,
    /// затем меньший. Отсутствующая обложка не превращается в 404, пока
    /// у тайтла есть хоть один вариант.
    pub fn fallback_order(self) -> [Self; 3] {
        match self {
            Self::Low => [Self::Low, Self::Mid, Self::High],
            Self::Mid => [Self::Mid, Self::High, Self::Low],
            Self::High => [Self::High, Self::Mid, Self::Low],
        }
    }
}

impl std::fmt::Display for CoverSize {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Обложки источника по размерам. Пустой `Option` — варианта у сайта нет.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceCovers {
    #[serde(default)]
    pub low: Option<String>,
    #[serde(default)]
    pub mid: Option<String>,
    #[serde(default)]
    pub high: Option<String>,
}

impl SourceCovers {
    pub fn get(&self, size: CoverSize) -> Option<&str> {
        let value = match size {
            CoverSize::Low => &self.low,
            CoverSize::Mid => &self.mid,
            CoverSize::High => &self.high,
        };
        value
            .as_deref()
            .map(str::trim)
            .filter(|reference| !reference.is_empty())
    }

    /// Все непустые варианты в порядке low → mid → high.
    pub fn entries(&self) -> impl Iterator<Item = (CoverSize, &str)> {
        CoverSize::ALL
            .into_iter()
            .filter_map(|size| self.get(size).map(|reference| (size, reference)))
    }

    pub fn is_empty(&self) -> bool {
        self.entries().next().is_none()
    }

    /// Ссылка для выдачи поиска: сначала low, чтобы не тянуть тяжёлый файл.
    pub fn search_ref(&self) -> Option<String> {
        CoverSize::ALL
            .into_iter()
            .find_map(|size| self.get(size).map(str::to_owned))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceTitle {
    pub external_id: ExternalId,
    pub kind: MediaKind,
    pub name: String,
    pub alt_names: Vec<String>,
    pub description: Option<String>,
    /// Стабильные ссылки на обложки по размерам, не URL для клиента.
    #[serde(default)]
    pub covers: SourceCovers,
    pub authors: Vec<String>,
    pub genres: Vec<String>,
    pub status: Option<TitleStatus>,
    pub year: Option<i16>,
    /// Нормализованная оценка в диапазоне 0..=10.
    pub rating: Option<f32>,
    /// Язык контента источника; один тайтл — один язык в первых фазах.
    pub lang: Option<String>,
    pub external_url: Option<String>,
    pub raw_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceChapter {
    pub external_id: ExternalId,
    pub chapter_key: ChapterKey,
    /// Номер как у источника после лёгкой нормализации; у спецвыпуска без номера — None.
    pub number_text: Option<String>,
    /// Только для сортировки; у диапазона берётся начало.
    pub number_milli: Option<i64>,
    pub volume: Option<u16>,
    pub chapter_kind: ChapterKind,
    /// None — формат станет известен при первом content().
    pub content_kind: Option<ContentKind>,
    pub name: Option<String>,
    /// Команда перевода, если источник различает варианты.
    pub group_name: Option<String>,
    pub language: Option<String>,
    pub is_locked: bool,
    pub external_url: Option<String>,
    pub published_at: Option<i64>,
    pub raw_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcePage {
    pub index: u32,
    /// Стабильная ссылка источника (путь или id) — ключ кэша страницы.
    pub image_ref: String,
    /// Последний разрешённый URL; адаптер заполняет, если уже знает его.
    pub image_url: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceContent {
    Images { pages: Vec<SourcePage> },
    Video { embed_url: String },
    Text { format: TextFormat },
}

impl SourceContent {
    pub fn kind(&self) -> ContentKind {
        match self {
            Self::Images { .. } => ContentKind::Images,
            Self::Video { .. } => ContentKind::Video,
            Self::Text { .. } => ContentKind::Text,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceText {
    pub format: TextFormat,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSearchItem {
    pub external_id: ExternalId,
    pub kind: MediaKind,
    pub name: String,
    pub alt_names: Vec<String>,
    pub cover_ref: Option<String>,
    pub year: Option<i16>,
    pub status: Option<TitleStatus>,
    pub external_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchRequest {
    pub query: String,
    pub kind: MediaKind,
    /// Непрозрачный курсор; адаптер сам решает, страница это или offset.
    pub cursor: Option<String>,
    pub filters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPage {
    pub items: Vec<SourceSearchItem>,
    pub next_cursor: Option<String>,
    pub total: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_id_accepts_opaque_unicode_and_rejects_unsafe_values() {
        assert!(ExternalId::new("<13.10.2026>naruto_").is_ok());
        assert!(ExternalId::new("кириллица-42").is_ok());
        assert!(ExternalId::new("%D0%BC%D0%B0%D0%BD%D0%B3%D0%B0").is_ok());

        assert!(ExternalId::new("").is_err());
        assert!(ExternalId::new("   ").is_err());
        assert!(ExternalId::new("https://evil.example").is_err());
        assert!(ExternalId::new("../etc/passwd").is_err());
        assert!(ExternalId::new("a/b").is_err());
        assert!(ExternalId::new("bad\u{0007}id").is_err());
        assert!(ExternalId::new("x".repeat(MAX_EXTERNAL_ID_CHARS + 1)).is_err());
    }
}
