use reqwest::Url;
use secrecy::{ExposeSecret, SecretString};
use zeroize::Zeroizing;

use crate::source_auth::SourceAuth;
use crate::sources::config::{HostAllowlist, url_allowed};
use crate::sources::descriptor::SourceDescriptor;
use crate::sources::dto::{
    ExternalId, LoginOutcome, SearchPage, SearchRequest, SourceChapter, SourceContent, SourceTitle,
};
use crate::sources::error::SourceError;
use crate::sources::http::{
    AdapterCtx, auth_headers as build_auth_headers, endpoint_url, get_text, header_pairs,
    post_json_raw,
};

use super::config::{CODE, DESCRIPTOR, ExmangaConfig, is_remanga_host};
use super::parse;

const PAGE_SIZE: usize = 20;
const MAX_QUERY_CHARS: usize = 200;

#[derive(Clone)]
pub struct Exmanga {
    ctx: AdapterCtx,
    config: ExmangaConfig,
    /// Обложки и страницы живут на сайте и его поддоменах; чужие хосты
    /// (включая Remanga CDN) не запрашиваются никогда.
    allowlist: HostAllowlist,
    /// Хосты, которым можно отдавать Bearer: API и сайт.
    auth_hosts: HostAllowlist,
}

impl Exmanga {
    pub fn new(ctx: AdapterCtx, config: ExmangaConfig) -> Result<Self, SourceError> {
        if config.image_hosts.iter().any(|host| is_remanga_host(host)) {
            return Err(SourceError::Config(
                "image_hosts must not contain Remanga hosts".to_owned(),
            ));
        }
        let mut hosts = config.image_hosts.clone();
        if let Some(host) = ctx.base_url.host_str() {
            if is_remanga_host(host) {
                return Err(SourceError::Config(
                    "base_url must not point to Remanga".to_owned(),
                ));
            }
            hosts.push(host.to_owned());
        }
        if let Some(host) = config
            .site()
            .ok()
            .and_then(|site| site.host_str().map(str::to_owned))
        {
            if is_remanga_host(&host) {
                return Err(SourceError::Config(
                    "site_url must not point to Remanga".to_owned(),
                ));
            }
            hosts.push(host);
        }
        let allowlist = HostAllowlist::new(hosts).map_err(SourceError::Config)?;

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
        let auth_hosts = HostAllowlist::new(auth_hosts).map_err(SourceError::Config)?;

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

    pub fn descriptor(&self) -> &'static SourceDescriptor {
        &DESCRIPTOR
    }

    /// Токен уходит только на хосты ExManga: чужие CDN его не получают.
    pub fn auth_headers(
        &self,
        auth: Option<&SourceAuth>,
        url: &Url,
    ) -> Vec<(&'static str, String)> {
        build_auth_headers(auth, url, &self.auth_hosts)
    }

    /// Вход по email и паролю: `POST /api/auth/login` отдаёт JWT.
    /// Пароль живёт в `SecretString` и не попадает ни в ошибки, ни в логи;
    /// тело запроса оборачивается в `Zeroizing`, чтобы буфер обнулился.
    pub async fn login(
        &self,
        login: &str,
        password: &SecretString,
    ) -> Result<LoginOutcome, SourceError> {
        let url = endpoint_url(&self.ctx, "/api/auth/login")?;
        let body = Zeroizing::new(
            serde_json::to_string(&serde_json::json!({
                "email": login,
                "password": password.expose_secret(),
            }))
            .map_err(|_| SourceError::Parse {
                stage: "login",
                detail: "cannot encode login body".to_owned(),
            })?,
        );

        let response = post_json_raw(
            &self.ctx,
            url,
            &[("Accept", "application/json")],
            body.as_str(),
        )
        .await?;

        // 400 `{data:"Неверный email или пароль"}` — неверная пара;
        // 403 JSON — отказ авторизации; 403 HTML — блокировка сайта.
        let is_json = response
            .content_type
            .as_deref()
            .is_some_and(|value| value.to_ascii_lowercase().contains("json"));

        match response.status {
            200..=299 => {
                let outcome = parse::parse_login(&response.body)?;
                if outcome.access_token.is_none() {
                    return Err(SourceError::Parse {
                        stage: "login",
                        detail: "missing access token".to_owned(),
                    });
                }
                Ok(outcome)
            }
            // Наружу HTTP 401 не отдаём: для клиента это отказ входа.
            400 | 401 => Err(SourceError::LoginFailed),
            403 if is_json => Err(SourceError::LoginFailed),
            403 => Err(SourceError::Blocked),
            404 => Err(SourceError::NotFound),
            status => Err(SourceError::Http {
                status: Some(status),
            }),
        }
    }

    /// Лёгкая проверка сохранённого токена: `GET /api/auth`.
    pub async fn verify(&self, auth: Option<&SourceAuth>) -> Result<(), SourceError> {
        let Some(auth) = auth else {
            return Err(SourceError::AuthRequired);
        };
        let url = endpoint_url(&self.ctx, "/api/auth")?;
        let base = [("Accept", "application/json")];
        let auth_headers = self.auth_headers(Some(auth), &url);
        let headers = header_pairs(&base, &auth_headers);
        let raw = get_text(&self.ctx, url, &headers)
            .await
            .map_err(|error| match error {
                // Отклонённый токен ExManga отдаёт 401.
                SourceError::NotFound => SourceError::AuthRequired,
                other => other,
            })?;
        parse::parse_verify(&raw)
    }

    /// Поиск по названию. Endpoint публичный и не требует токена: если
    /// учётка протухла, поиск всё равно должен работать.
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
        let skip = match request.cursor.as_deref().map(str::trim) {
            None | Some("") => 0_u64,
            Some(cursor) => match cursor.parse::<u64>() {
                Ok(offset) => offset,
                _ => {
                    return Err(SourceError::BadFilter(
                        "cursor must be a non-negative offset".to_owned(),
                    ));
                }
            },
        };

        let mut url = endpoint_url(&self.ctx, "/api/manga")?;
        url.query_pairs_mut()
            .append_pair("name", query)
            .append_pair("skip", &skip.to_string())
            .append_pair("take", &PAGE_SIZE.to_string());

        let raw = get_text(&self.ctx, url, &[("Accept", "application/json")]).await?;
        let mut page = parse::parse_search(&raw)?;
        if page.items.len() == PAGE_SIZE {
            page.next_cursor = Some((skip + PAGE_SIZE as u64).to_string());
        }
        Ok(page)
    }

    /// Метаданные тайтла по `dir` — тому же слагу, что у Remanga.
    pub async fn title(
        &self,
        external_id: &ExternalId,
        _auth: Option<&SourceAuth>,
    ) -> Result<SourceTitle, SourceError> {
        let mut url = endpoint_url(&self.ctx, "/api/manga")?;
        url.query_pairs_mut()
            .append_pair("dir", external_id.as_str());

        let raw = get_text(&self.ctx, url, &[("Accept", "application/json")]).await?;
        let mut title =
            parse::parse_title(&raw, external_id.as_str())?.ok_or(SourceError::NotFound)?;
        title.lang = Some(self.config.language.clone());
        Ok(title)
    }

    /// Список глав. `GET /api/manga/history?dir=` публичный и отдаёт полный
    /// список вместе с id, номерами и томами.
    pub async fn chapters(
        &self,
        external_id: &ExternalId,
        _auth: Option<&SourceAuth>,
    ) -> Result<Vec<SourceChapter>, SourceError> {
        let mut url = endpoint_url(&self.ctx, "/api/manga/history")?;
        url.query_pairs_mut()
            .append_pair("dir", external_id.as_str());

        let raw = get_text(&self.ctx, url, &[("Accept", "application/json")]).await?;
        let chapters = parse::parse_chapters(&raw, &self.config.language)?;
        if chapters.is_empty() {
            // Тайтл без глав в пуле бесполезен как источник: не привязываем
            // пустую карточку и не держим её в discovery как рабочую.
            return Err(SourceError::NotFound);
        }
        Ok(chapters)
    }

    /// Контент главы доступен только с учёткой ExManga. Без неё сразу
    /// возвращаем `AuthRequired`: лишний 401-запрос не нужен.
    pub async fn content(
        &self,
        chapter: &SourceChapter,
        auth: Option<&SourceAuth>,
    ) -> Result<SourceContent, SourceError> {
        let Some(auth) = auth else {
            return Err(SourceError::AuthRequired);
        };

        let mut url = endpoint_url(&self.ctx, "/api/chapter")?;
        url.query_pairs_mut()
            .append_pair("id", chapter.external_id.as_str())
            .append_pair("extra", "true")
            .append_pair("history", "true");

        let base = [("Accept", "application/json")];
        let auth_headers = self.auth_headers(Some(auth), &url);
        let headers = header_pairs(&base, &auth_headers);
        let raw = get_text(&self.ctx, url, &headers).await?;
        let content = parse::parse_content(&raw)?;

        // Известный тип не должен расходиться с фактическим ответом.
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

    /// Разрешает ссылку страницы в URL с проверкой allowlist. Remanga CDN
    /// и любые другие хосты отклоняются: ExManga-адаптер не ходит на Remanga.
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn adapter() -> Exmanga {
        let ctx = AdapterCtx::new(
            Arc::new(reqwest::Client::new()),
            Url::parse("https://exmanga.org").expect("base url"),
        );
        Exmanga::new(ctx, ExmangaConfig::default()).expect("adapter")
    }

    #[test]
    fn accepts_only_exmanga_image_hosts() {
        let adapter = adapter();

        assert!(
            adapter
                .resolve_image("https://exmanga.org/storage/a/1.webp?sign=x&expires=1")
                .is_ok()
        );
        assert!(adapter.resolve_image("/storage/a/1.webp").is_ok());
        assert!(
            adapter
                .resolve_image("https://img.exmanga.org/storage/a/1.webp")
                .is_ok()
        );

        // Ни Remanga API, ни её CDN не должны использоваться как источник
        // картинок ExManga.
        for denied in [
            "https://api.remanga.org/media/a.jpg",
            "https://remanga.org/media/a.jpg",
            "https://img3.reimg.org/img/1.webp",
            "https://evil.example/a.webp",
        ] {
            assert!(adapter.resolve_image(denied).is_err(), "{denied}");
        }
    }

    #[test]
    fn refuses_remanga_base_url() {
        let ctx = AdapterCtx::new(
            Arc::new(reqwest::Client::new()),
            Url::parse("https://api.remanga.org").expect("url"),
        );
        assert!(Exmanga::new(ctx, ExmangaConfig::default()).is_err());
    }

    #[test]
    fn auth_headers_never_leave_exmanga_hosts() {
        let adapter = adapter();
        let auth = SourceAuth::new(Some("token".to_owned()), None);

        let own = Url::parse("https://exmanga.org/api/chapter?id=1").expect("url");
        assert!(!adapter.auth_headers(Some(&auth), &own).is_empty());

        let foreign = Url::parse("https://img3.reimg.org/img/1.webp").expect("url");
        assert!(adapter.auth_headers(Some(&auth), &foreign).is_empty());
    }
}
