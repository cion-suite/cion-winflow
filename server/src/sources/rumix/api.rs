use reqwest::Url;

use crate::sources::config::{HostAllowlist, url_allowed};
use crate::sources::dto::{
    ExternalId, SearchPage, SearchRequest, SourceChapter, SourceContent, SourceText, SourceTitle,
};
use crate::sources::error::SourceError;
use crate::sources::http::{AdapterCtx, endpoint_url, get_text, post_form_text};

use super::config::{CODE, RumixConfig};
use super::parse;

const MAX_QUERY_CHARS: usize = 200;

#[derive(Clone)]
pub struct Rumix {
    ctx: AdapterCtx,
    config: RumixConfig,
    /// Страницы и обложки: CDN сайта.
    image_allowlist: HostAllowlist,
    /// Только сам сайт: по нему проверяется URL главы из БД.
    site_allowlist: HostAllowlist,
}

impl Rumix {
    pub fn new(ctx: AdapterCtx, config: RumixConfig) -> Result<Self, SourceError> {
        let site_host = ctx
            .base_url
            .host_str()
            .ok_or_else(|| SourceError::Config("rumix base_url has no host".to_owned()))?;

        let mut hosts = config.image_hosts.clone();
        hosts.push(site_host.to_owned());

        let image_allowlist = HostAllowlist::new(hosts).map_err(SourceError::Config)?;
        let site_allowlist =
            HostAllowlist::new([site_host.to_owned()]).map_err(SourceError::Config)?;

        Ok(Self {
            ctx,
            config,
            image_allowlist,
            site_allowlist,
        })
    }

    pub fn code(&self) -> &'static str {
        CODE
    }

    pub async fn search(&self, request: SearchRequest) -> Result<SearchPage, SourceError> {
        let query = request.query.trim();
        if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
            return Err(SourceError::BadFilter(format!(
                "query must be 1..={MAX_QUERY_CHARS} characters"
            )));
        }

        let offset = match request.cursor.as_deref().map(str::trim) {
            None | Some("") => 0,
            Some(cursor) => cursor.parse::<u64>().map_err(|_| {
                SourceError::BadFilter("cursor must be a non-negative offset".to_owned())
            })?,
        };
        let offset_text = offset.to_string();

        let url = endpoint_url(&self.ctx, "/search")?;
        let form = [
            ("q", query),
            // Поиск RuMix ищет по всем «творениям»; манга — единственный
            // поддерживаемый тип, но фильтр оставляем серверу сайта.
            ("fast-filter", "CREATION"),
            ("offset", offset_text.as_str()),
        ];
        let raw = post_form_text(&self.ctx, url, &self.request_headers(), &form).await?;

        parse::parse_search(&raw, &self.ctx.base_url, offset)
    }

    pub async fn title(&self, external_id: &ExternalId) -> Result<SourceTitle, SourceError> {
        let raw = self.page(external_id).await?;
        parse::parse_title(&raw, &self.ctx.base_url, external_id, &self.config.language)
    }

    pub async fn chapters(
        &self,
        external_id: &ExternalId,
    ) -> Result<Vec<SourceChapter>, SourceError> {
        let raw = self.page(external_id).await?;
        let chapters = parse::parse_chapters(&raw, &self.ctx.base_url, &self.config.language)?;
        if chapters.is_empty() {
            return Err(SourceError::NotFound);
        }
        Ok(chapters)
    }

    pub async fn content(&self, chapter: &SourceChapter) -> Result<SourceContent, SourceError> {
        let reference = chapter
            .external_url
            .as_deref()
            .ok_or_else(|| SourceError::Parse {
                stage: "content",
                detail: "chapter has no source URL".to_owned(),
            })?;
        let mut url = Url::parse(reference).map_err(|_| SourceError::Parse {
            stage: "content",
            detail: "invalid chapter URL".to_owned(),
        })?;
        if !url_allowed(&url, &self.site_allowlist) {
            return Err(SourceError::Config(
                "chapter URL host is not allowed".to_owned(),
            ));
        }
        // 18+ главы RuMix открываются с этим параметром; cookie не нужна.
        url.query_pairs_mut().append_pair("mtr", "true");

        let raw = match get_text(&self.ctx, url, &self.request_headers()).await {
            Ok(raw) => raw,
            Err(SourceError::Redirect { location }) => {
                // Платная глава уводит на /internal/purchase, а запланированная
                // или удалённая — на страницу тайтла.
                if location.contains("/internal/purchase/") || location.contains("/purchase") {
                    return Err(SourceError::Locked);
                }
                return Err(SourceError::NotFound);
            }
            Err(error) => return Err(error),
        };
        let content = parse::parse_content(&raw)?;

        if let Some(expected) = chapter.content_kind
            && expected != content.kind()
        {
            return Err(SourceError::Parse {
                stage: "content",
                detail: format!(
                    "declared `{}` but got `{}`",
                    expected.as_str(),
                    content.kind().as_str()
                ),
            });
        }

        Ok(content)
    }

    pub async fn text(&self, _chapter: &SourceChapter) -> Result<SourceText, SourceError> {
        Err(SourceError::UnsupportedCapability)
    }

    pub fn resolve_image(&self, image_ref: &str) -> Result<Url, SourceError> {
        let url = self.resolve_reference(image_ref)?;
        if url_allowed(&url, &self.image_allowlist) {
            Ok(url)
        } else {
            Err(SourceError::Config(format!(
                "host `{}` is not allowed for images",
                url.host_str().unwrap_or("<none>")
            )))
        }
    }

    pub fn resolve_cover(&self, cover_ref: &str) -> Result<Url, SourceError> {
        let url = self.resolve_reference(cover_ref)?;
        if url_allowed(&url, &self.image_allowlist) {
            Ok(url)
        } else {
            Err(SourceError::Config(format!(
                "host `{}` is not allowed for covers",
                url.host_str().unwrap_or("<none>")
            )))
        }
    }

    async fn page(&self, external_id: &ExternalId) -> Result<String, SourceError> {
        let url = endpoint_url(&self.ctx, &format!("/{}", external_id.as_str()))?;
        get_text(&self.ctx, url, &self.request_headers()).await
    }

    /// Сайт проверяет UA и ожидает Referer со своего домена.
    fn request_headers(&self) -> [(&str, &str); 3] {
        [
            ("Accept", "text/html"),
            ("User-Agent", self.config.user_agent.as_str()),
            ("Referer", self.ctx.base_url.as_str()),
        ]
    }

    fn resolve_reference(&self, reference: &str) -> Result<Url, SourceError> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(SourceError::Parse {
                stage: "image",
                detail: "empty image reference".to_owned(),
            });
        }

        if reference.starts_with("http://") || reference.starts_with("https://") {
            return Url::parse(reference).map_err(|_| SourceError::Parse {
                stage: "image",
                detail: "invalid image URL".to_owned(),
            });
        }

        self.ctx
            .base_url
            .join(reference)
            .map_err(|_| SourceError::Parse {
                stage: "image",
                detail: "invalid image reference".to_owned(),
            })
    }
}
