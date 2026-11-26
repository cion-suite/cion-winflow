//! Дескриптор источника — данные, а не набор трейтов: админка и обработчики
//! узнают возможности и схему конфига без особенностей конкретного сайта.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Search,
    Chapters,
    Images,
    VideoEmbed,
    Text,
    Volumes,
    AltNames,
    SearchFilters,
    AuthCookie,
    SignedUrls,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Search => "search",
            Self::Chapters => "chapters",
            Self::Images => "images",
            Self::VideoEmbed => "video_embed",
            Self::Text => "text",
            Self::Volumes => "volumes",
            Self::AltNames => "alt_names",
            Self::SearchFilters => "search_filters",
            Self::AuthCookie => "auth_cookie",
            Self::SignedUrls => "signed_urls",
        }
    }
}

pub struct SourceDescriptor {
    /// Ключ в `sources.code`; совпадает с именем модуля адаптера.
    pub code: &'static str,
    pub name: &'static str,
    /// Растёт при смене семантики парсинга или контракта.
    pub version: u32,
    pub capabilities: &'static [Capability],
    /// Схема `config_json` для админки.
    pub config: &'static [ConfigField],
    /// Схема фильтров поиска; пусто, если фильтров нет.
    pub filters: &'static [ConfigField],
}

impl SourceDescriptor {
    pub fn has(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConfigField {
    pub key: &'static str,
    pub kind: FieldType,
    pub required: bool,
    pub secret: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum FieldType {
    Text {
        max_len: usize,
    },
    Url,
    UrlList,
    /// Список хостов: точный домен или `*.example.com`.
    HostList,
    Integer {
        min: i64,
        max: i64,
    },
    Bool,
    Select(&'static [&'static str]),
    MultiSelect(&'static [&'static str]),
    Range {
        min: i64,
        max: i64,
        step: i64,
    },
    TriState,
}

impl FieldType {
    /// Описание типа без значений: для `GET /api/sources` и документации.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Text { .. } => "text",
            Self::Url => "url",
            Self::UrlList => "url_list",
            Self::HostList => "host_list",
            Self::Integer { .. } => "integer",
            Self::Bool => "bool",
            Self::Select(_) => "select",
            Self::MultiSelect(_) => "multi_select",
            Self::Range { .. } => "range",
            Self::TriState => "tri_state",
        }
    }
}
