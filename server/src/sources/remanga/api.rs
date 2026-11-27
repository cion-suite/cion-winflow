use reqwest::Url;

use crate::source_auth::SourceAuth;
use crate::sources::config::url_allowed;
use crate::sources::dto::{
    ExternalId, SearchPage, SearchRequest, SourceChapter, SourceContent, SourceTitle,
};
use crate::sources::error::SourceError;
use crate::sources::http::{
    AdapterCtx, auth_headers as build_auth_headers, endpoint_url, get_text, header_pairs,
};

use super::config::{CODE, RemangaConfig};
use super::parse;

const MAX_PAGES: usize = 200;
const MAX_CHAPTERS: usize = 5000;
const MAX_QUERY_CHARS: usize = 200;

#[derive(Clone)]
pub struct Remanga {
    ctx: AdapterCtx,
    config: RemangaConfig,
    /// `base_url` API плюс site и CDN: один allowlist на все исходящие картинки.
    allowlist: crate::sources::config::HostAllowlist,
    /// Хосты, которым можно отдавать сессионные заголовки: API и сайт.
    /// CDN картинок сюда не входит.
    auth_hosts: crate::sources::config::HostAllowlist,
}

impl Remanga {
    pub fn new(ctx: AdapterCtx, config: RemangaConfig) -> Result<Self, SourceError> {
        let mut hosts = config.image_hosts.clone();
        if let Some(host) = ctx.base_url.host_str() {
            hosts.push(host.to_owned());
        }
        if let Some(host) = config
            .site()
            .ok()
            .and_then(|site| site.host_str().map(str::to_owned))
        {
            hosts.push(host);
        }
        let allowlist =
            crate::sources::config::HostAllowlist::new(hosts).map_err(SourceError::Config)?;

        let mut auth_hosts = Vec::new();
        if let Some(host) = ctx.base_url.host_str() {
            auth_hosts.push(host.to_owned());
        }
        if let Some(host) = config
            .site()
            .ok()
            .and_then(|site| site.host_str().map(str::to_owned))
        {
            auth_hosts.push(host);
        }
        let auth_hosts =
            crate::sources::config::HostAllowlist::new(auth_hosts).map_err(SourceError::Config)?;

        Ok(Self {
            ctx,
            config,
            allowlist,
            auth_hosts,
        })
    }

    pub fn code(&self) -> &'static str {
        CODE
    }

    /// Учётка уходит только на хосты источника: cookie Remanga не должна
    /// попадать на `*.reimg.org` и другие CDN.
    pub fn auth_headers(
        &self,
        auth: Option<&SourceAuth>,
        url: &Url,
    ) -> Vec<(&'static str, String)> {
        build_auth_headers(auth, url, &self.auth_hosts)
    }

    pub async fn search(
        &self,
        request: SearchRequest,
        auth: Option<&SourceAuth>,
    ) -> Result<SearchPage, SourceError> {
        let query = request.query.trim();
        if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
            return Err(SourceError::BadFilter(format!(
                "query must be 1..={MAX_QUERY_CHARS} characters"
            )));
        }
        let page = match request.cursor.as_deref().map(str::trim) {
            None | Some("") => 1,
            Some(cursor) => match cursor.parse::<u64>() {
                Ok(page) if page >= 1 => page,
                _ => {
                    return Err(SourceError::BadFilter(
                        "cursor must be a page number starting at 1".to_owned(),
                    ));
                }
            },
        };

        let mut url = endpoint_url(&self.ctx, "/api/v2/search/")?;
        url.query_pairs_mut()
            .append_pair("query", query)
            .append_pair("page", &page.to_string());

        let base = [("Accept", "application/json")];
        let auth = self.auth_headers(auth, &url);
        let headers = header_pairs(&base, &auth);
        let raw = get_text(&self.ctx, url, &headers).await?;
        parse::parse_search(&raw)
    }

    pub async fn title(
        &self,
        external_id: &ExternalId,
        auth: Option<&SourceAuth>,
    ) -> Result<SourceTitle, SourceError> {
        let raw = self.title_raw(external_id, auth).await?;
        let mut title = parse::parse_title(&raw)?;
        title.lang = Some(self.config.language.clone());
        Ok(title)
    }

    pub async fn chapters(
        &self,
        external_id: &ExternalId,
        auth: Option<&SourceAuth>,
    ) -> Result<Vec<SourceChapter>, SourceError> {
        let raw = self.title_raw(external_id, auth).await?;
        let branches = parse::parse_branches(&raw)?;
        if branches.is_empty() {
            // Пустой список у Remanga тоже может означать скрытый тайтл.
            return Err(if auth.is_none() {
                SourceError::AuthMaybeRequired
            } else {
                SourceError::NotFound
            });
        }

        let multi_branch = branches.len() > 1;
        let mut chapters = Vec::new();

        for branch in branches {
            let group = branch
                .group_name
                .clone()
                .or_else(|| multi_branch.then(|| format!("branch-{}", branch.id)));
            let mut cursor: Option<String> = None;
            for _ in 0..MAX_PAGES {
                let mut url = endpoint_url(&self.ctx, "/api/v2/titles/chapters/")?;
                url.query_pairs_mut()
                    .append_pair("branch_id", &branch.id.to_string());
                if let Some(cursor) = &cursor {
                    url.query_pairs_mut().append_pair("page", cursor);
                }

                let base = [("Accept", "application/json")];
                let auth_headers = self.auth_headers(auth, &url);
                let headers = header_pairs(&base, &auth_headers);
                let raw = get_text(&self.ctx, url, &headers).await?;
                let page = parse::parse_chapters(&raw, &self.config.language, group.as_deref())?;
                chapters.extend(page.items);

                if chapters.len() > MAX_CHAPTERS {
                    return Err(SourceError::Parse {
                        stage: "chapters",
                        detail: "too many chapters".to_owned(),
                    });
                }

                match page.next_cursor {
                    Some(next) => cursor = Some(next),
                    None => break,
                }
            }
        }

        Ok(chapters)
    }

    pub async fn content(
        &self,
        chapter: &SourceChapter,
        auth: Option<&SourceAuth>,
    ) -> Result<SourceContent, SourceError> {
        let mut url = endpoint_url(&self.ctx, "/api/v2/titles/chapters/")?;
        url.set_path(&format!(
            "/api/v2/titles/chapters/{}/",
            chapter.external_id.as_str()
        ));

        let base = [("Accept", "application/json")];
        let auth_headers = self.auth_headers(auth, &url);
        let headers = header_pairs(&base, &auth_headers);
        let raw = match get_text(&self.ctx, url, &headers).await {
            // Без входа Remanga может отдать 404 вместо 401 для 18+ главы.
            Err(SourceError::NotFound) if auth.is_none() => {
                return Err(SourceError::AuthMaybeRequired);
            }
            other => other?,
        };
        let content = parse::parse_content(&raw)?;

        // Известный тип не должен расходиться с фактическим ответом
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
    ) -> Result<crate::sources::dto::SourceText, SourceError> {
        Err(SourceError::UnsupportedCapability)
    }

    /// Разрешает стабильную ссылку страницы в URL с проверкой allowlist.
    pub fn resolve_image(&self, image_ref: &str) -> Result<Url, SourceError> {
        let url = self.resolve_reference(image_ref)?;
        if url_allowed(&url, &self.allowlist) {
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
        if url_allowed(&url, &self.allowlist) {
            Ok(url)
        } else {
            Err(SourceError::Config(format!(
                "host `{}` is not allowed for covers",
                url.host_str().unwrap_or("<none>")
            )))
        }
    }

    async fn title_raw(
        &self,
        external_id: &ExternalId,
        auth: Option<&SourceAuth>,
    ) -> Result<String, SourceError> {
        let mut url = endpoint_url(&self.ctx, "/api/v2/titles/")?;
        url.set_path(&format!("/api/v2/titles/{}/", external_id.as_str()));
        let base = [("Accept", "application/json")];
        let auth_headers = self.auth_headers(auth, &url);
        let headers = header_pairs(&base, &auth_headers);
        match get_text(&self.ctx, url, &headers).await {
            // 18+ и закрытые тайтлы Remanga скрывает за 404, а не 401.
            Err(SourceError::NotFound) if auth.is_none() => Err(SourceError::AuthMaybeRequired),
            other => other,
        }
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

        let site = self.config.site().map_err(SourceError::Config)?;
        site.join(reference).map_err(|_| SourceError::Parse {
            stage: "image",
            detail: "invalid image reference".to_owned(),
        })
    }
}
