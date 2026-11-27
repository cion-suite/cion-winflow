use reqwest::Url;

use crate::source_auth::SourceAuth;
use crate::sources::config::{HostAllowlist, url_allowed};
use crate::sources::dto::{
    ExternalId, SearchPage, SearchRequest, SourceChapter, SourceContent, SourceText, SourceTitle,
};
use crate::sources::error::SourceError;
use crate::sources::http::{
    AdapterCtx, endpoint_url, ensure_allowed, get_text, get_text_with_cookies,
    post_json_with_cookies,
};

use super::config::{CODE, MangamirConfig};
use super::parse;

const MAX_QUERY_CHARS: usize = 200;

#[derive(Clone)]
pub struct Mangamir {
    ctx: AdapterCtx,
    config: MangamirConfig,
    /// Страницы и обложки: CDN сайта.
    image_allowlist: HostAllowlist,
    /// Только сам сайт: по нему проверяется URL главы из БД.
    site_allowlist: HostAllowlist,
}

impl Mangamir {
    pub fn new(ctx: AdapterCtx, config: MangamirConfig) -> Result<Self, SourceError> {
        let site_host = ctx
            .base_url
            .host_str()
            .ok_or_else(|| SourceError::Config("mangamir base_url has no host".to_owned()))?;

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

    /// Mangamir не поддерживает per-user вход: сессионных заголовков нет.
    pub fn auth_headers(
        &self,
        _auth: Option<&SourceAuth>,
        _url: &Url,
    ) -> Vec<(&'static str, String)> {
        Vec::new()
    }

    pub async fn search(
        &self,
        request: SearchRequest,
        _auth: Option<&SourceAuth>,
    ) -> Result<SearchPage, SourceError> {
        let query = request.query.trim();
        if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
            return Err(SourceError::BadFilter(format!(
                "query must be 1..={MAX_QUERY_CHARS} characters"
            )));
        }

        // Сайт ищет по GET-параметру q и отдаёт готовую страницу результатов.
        let mut url = endpoint_url(&self.ctx, "/manga")?;
        url.query_pairs_mut().append_pair("q", query);
        let raw = get_text(&self.ctx, url, &self.request_headers()).await?;

        parse::parse_search(&raw, &self.ctx.base_url)
    }

    pub async fn title(
        &self,
        external_id: &ExternalId,
        _auth: Option<&SourceAuth>,
    ) -> Result<SourceTitle, SourceError> {
        let raw = self.page(external_id).await?;
        parse::parse_title(&raw, &self.ctx.base_url, external_id, &self.config.language)
    }

    pub async fn chapters(
        &self,
        external_id: &ExternalId,
        _auth: Option<&SourceAuth>,
    ) -> Result<Vec<SourceChapter>, SourceError> {
        let url = self.title_url(external_id)?;
        let page = get_text_with_cookies(&self.ctx, url.clone(), &self.request_headers()).await?;

        match parse::livewire_chapters(&page.body, &self.ctx.base_url)? {
            Some(livewire) if livewire.showing_all => self.chapters_from(&page.body),
            Some(livewire) => {
                // Сайт рендерит только первые и последние главы; полный список
                // приходит отдельным Livewire-вызовом в той же сессии. Ошибку
                // не проглатываем: свёрнутый список, отданный как успешный
                // результат, sync запишет как полное обновление.
                let chapters = self.all_chapters(&livewire, &page.cookies, &url).await?;
                if chapters.is_empty() {
                    return Err(SourceError::Parse {
                        stage: "chapters",
                        detail: "livewire returned no chapters".to_owned(),
                    });
                }
                Ok(chapters)
            }
            // Кнопка «показать все главы» есть, а компонента нет: список
            // заведомо неполный, молча отдавать его нельзя.
            None if parse::page_is_collapsed(&page.body) => Err(SourceError::Parse {
                stage: "chapters",
                detail: "collapsed chapter list without livewire component".to_owned(),
            }),
            None => self.chapters_from(&page.body),
        }
    }

    /// Полный список глав через `showAllChapters()` Livewire-компонента.
    async fn all_chapters(
        &self,
        livewire: &parse::LivewireChapters,
        cookies: &[String],
        page_url: &Url,
    ) -> Result<Vec<SourceChapter>, SourceError> {
        ensure_allowed(&livewire.update_url, &self.site_allowlist)?;

        let mut call = serde_json::json!({
            "method": "showAllChapters",
            "params": [],
        });
        if let Some(island) = &livewire.island {
            call["metadata"] = serde_json::json!({
                "island": { "name": island, "mode": "morph" },
            });
        }
        let payload = serde_json::json!({
            "_token": &livewire.token,
            "components": [{
                "snapshot": &livewire.snapshot,
                "updates": {},
                "calls": [call],
            }],
        });

        let origin = self.ctx.base_url.origin().ascii_serialization();
        let headers = [
            ("Accept", "application/json"),
            ("User-Agent", self.config.user_agent.as_str()),
            ("Referer", page_url.as_str()),
            ("Origin", origin.as_str()),
            ("X-Livewire", "1"),
        ];
        let response = post_json_with_cookies(
            &self.ctx,
            livewire.update_url.clone(),
            &headers,
            &payload.to_string(),
            cookies,
        )
        .await?;

        parse::parse_chapter_fragments(
            &parse::livewire_fragments(&response.body),
            &self.ctx.base_url,
            &self.config.language,
        )
    }

    fn chapters_from(&self, raw: &str) -> Result<Vec<SourceChapter>, SourceError> {
        let chapters = parse::parse_chapters(raw, &self.ctx.base_url, &self.config.language)?;
        if chapters.is_empty() {
            return Err(SourceError::NotFound);
        }
        Ok(chapters)
    }

    pub async fn content(
        &self,
        chapter: &SourceChapter,
        _auth: Option<&SourceAuth>,
    ) -> Result<SourceContent, SourceError> {
        let reference = chapter
            .external_url
            .as_deref()
            .ok_or_else(|| SourceError::Parse {
                stage: "content",
                detail: "chapter has no source URL".to_owned(),
            })?;
        let url = Url::parse(reference).map_err(|_| SourceError::Parse {
            stage: "content",
            detail: "invalid chapter URL".to_owned(),
        })?;
        if !url_allowed(&url, &self.site_allowlist) {
            return Err(SourceError::Config(
                "chapter URL host is not allowed".to_owned(),
            ));
        }

        let raw = match get_text(&self.ctx, url, &self.request_headers()).await {
            Ok(raw) => raw,
            Err(SourceError::Redirect { location }) => {
                // Платные главы уводят на оплату или авторизацию; остальные
                // редиректы означают, что главы больше нет.
                if location.contains("purchase") || location.contains("login") {
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

    pub async fn text(
        &self,
        _chapter: &SourceChapter,
        _auth: Option<&SourceAuth>,
    ) -> Result<SourceText, SourceError> {
        Err(SourceError::UnsupportedCapability)
    }

    pub fn resolve_image(&self, image_ref: &str) -> Result<Url, SourceError> {
        let url = self.resolve_reference(image_ref)?;
        ensure_allowed(&url, &self.image_allowlist)?;
        Ok(url)
    }

    pub fn resolve_cover(&self, cover_ref: &str) -> Result<Url, SourceError> {
        let url = self.resolve_reference(cover_ref)?;
        ensure_allowed(&url, &self.image_allowlist)?;
        Ok(url)
    }

    fn title_url(&self, external_id: &ExternalId) -> Result<Url, SourceError> {
        endpoint_url(&self.ctx, &format!("/manga/{}", external_id.as_str()))
    }

    async fn page(&self, external_id: &ExternalId) -> Result<String, SourceError> {
        let url = self.title_url(external_id)?;
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
