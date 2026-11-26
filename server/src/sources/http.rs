//! Общий HTTP-клиент источников: таймауты, ретраи, circuit breaker
//! и SSRF-защита через проверяющий DNS-резолвер.

use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use reqwest::{
    Client, Response, Url,
    dns::{Addrs, Name, Resolve, Resolving},
    header::{CONTENT_TYPE, COOKIE, SET_COOKIE},
};
use tokio::sync::Semaphore;

use crate::sources::error::SourceError;

pub const METADATA_BODY_LIMIT: usize = 5 * 1024 * 1024;
pub const IMAGE_BODY_LIMIT: usize = 15 * 1024 * 1024;
const MAX_ATTEMPTS: usize = 3;
const RETRY_BASE: Duration = Duration::from_millis(400);

/// DNS-резолвер, который отбрасывает приватные и служебные адреса.
/// Соединение идёт только с проверенными IP: DNS-rebinding после проверки
/// невозможен, потому что reqwest использует уже полученный список.
#[derive(Debug)]
pub struct SafeDns {
    allow_loopback: bool,
}

impl SafeDns {
    pub fn new(allow_loopback: bool) -> Self {
        Self { allow_loopback }
    }
}

impl Resolve for SafeDns {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let allow_loopback = self.allow_loopback;
        Box::pin(async move {
            let addrs = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error + Send + Sync>)?;

            let allowed: Vec<SocketAddr> = addrs
                .filter(|addr| !is_forbidden_ip(addr.ip(), allow_loopback))
                .collect();

            if allowed.is_empty() {
                return Err("no allowed addresses".into());
            }

            let addrs: Addrs = Box::new(allowed.into_iter());
            Ok(addrs)
        })
    }
}

fn is_forbidden_ip(ip: IpAddr, allow_loopback: bool) -> bool {
    match ip {
        IpAddr::V4(ip) => is_forbidden_v4(ip, allow_loopback),
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_forbidden_v4(mapped, allow_loopback);
            }
            (ip.is_loopback() && !allow_loopback)
                || ip.is_unspecified()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
        }
    }
}

fn is_forbidden_v4(ip: Ipv4Addr, allow_loopback: bool) -> bool {
    let octets = ip.octets();
    let carrier_grade_nat = octets[0] == 100 && (64..=127).contains(&octets[1]);

    ip.is_private()
        || (ip.is_loopback() && !allow_loopback)
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || carrier_grade_nat
        || octets[0] == 0
}

/// Клиент для метаданных. Редиректы не разрешены: IP-literal хост обходит
/// кастомный DNS, а редирект от источника — это его URL, а не наш allowlist.
pub fn metadata_client(user_agent: &str, allow_loopback: bool) -> Result<Client, SourceError> {
    Client::builder()
        .user_agent(user_agent)
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .dns_resolver(Arc::new(SafeDns::new(allow_loopback)))
        .build()
        .map_err(|error| {
            tracing::error!(%error, "cannot build metadata HTTP client");
            SourceError::Network
        })
}

/// Клиент для картинок: без редиректов, отдельный пул и таймаут.
pub fn image_client(allow_loopback: bool, user_agent: &str) -> Result<Client, SourceError> {
    Client::builder()
        .user_agent(user_agent)
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .dns_resolver(Arc::new(SafeDns::new(allow_loopback)))
        .build()
        .map_err(|error| {
            tracing::error!(%error, "cannot build image HTTP client");
            SourceError::Network
        })
}

/// Простой circuit breaker на источник: после серии ошибок запросы
/// временно не идут в сеть, чтобы падение не превращалось в шторм.
#[derive(Debug)]
pub struct CircuitBreaker {
    failures: Mutex<u32>,
    open_until: Mutex<Option<Instant>>,
}

const FAILURE_THRESHOLD: u32 = 5;
const OPEN_FOR: Duration = Duration::from_secs(60);

impl CircuitBreaker {
    pub fn new() -> Self {
        Self {
            failures: Mutex::new(0),
            open_until: Mutex::new(None),
        }
    }

    pub fn allow(&self) -> bool {
        let until = *self
            .open_until
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        until.is_none_or(|until| Instant::now() >= until)
    }

    pub fn record_success(&self) {
        *self
            .failures
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = 0;
        *self
            .open_until
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = None;
    }

    pub fn record_failure(&self) {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *failures = failures.saturating_add(1);
        if *failures >= FAILURE_THRESHOLD {
            *self
                .open_until
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()) = Some(Instant::now() + OPEN_FOR);
            *failures = 0;
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

/// Всё, что нужно адаптеру для сетевых запросов. Адаптер не знает о БД.
#[derive(Clone)]
pub struct AdapterCtx {
    pub client: Arc<Client>,
    pub base_url: Url,
    pub metadata_semaphore: Arc<Semaphore>,
    pub breaker: Arc<CircuitBreaker>,
}

impl AdapterCtx {
    pub fn new(client: Arc<Client>, base_url: Url) -> Self {
        Self {
            client,
            base_url,
            metadata_semaphore: Arc::new(Semaphore::new(2)),
            breaker: Arc::new(CircuitBreaker::new()),
        }
    }

    fn endpoint(&self, path: &str) -> Result<Url, SourceError> {
        let mut url = self.base_url.clone();
        url.set_path(path);
        url.set_query(None);
        url.set_fragment(None);
        Ok(url)
    }
}

/// Тело ответа и куки сессии из `Set-Cookie`. Нужны источникам, у которых
/// следующий запрос обязан идти в той же сессии (CSRF/Livewire).
pub struct TextResponse {
    pub body: String,
    /// Пары `name=value` без атрибутов `Path`/`Expires`.
    pub cookies: Vec<String>,
}

#[derive(Clone, Copy)]
enum Payload<'a> {
    None,
    Form(&'a [(&'a str, &'a str)]),
    Json(&'a str),
}

/// Забирает текст метаданных с двумя ретраями на сеть и 5xx.
pub async fn get_text(
    ctx: &AdapterCtx,
    url: Url,
    headers: &[(&str, &str)],
) -> Result<String, SourceError> {
    Ok(send_text(ctx, &url, headers, Payload::None, &[])
        .await?
        .body)
}

/// POST-форма для источников, у которых поиск не GET (например, RuMix).
/// Ответ читается теми же лимитами, ретраями и circuit breaker.
pub async fn post_form_text(
    ctx: &AdapterCtx,
    url: Url,
    headers: &[(&str, &str)],
    form: &[(&str, &str)],
) -> Result<String, SourceError> {
    Ok(send_text(ctx, &url, headers, Payload::Form(form), &[])
        .await?
        .body)
}

/// GET, который сохраняет `Set-Cookie`: следующий POST идёт в той же сессии.
pub async fn get_text_with_cookies(
    ctx: &AdapterCtx,
    url: Url,
    headers: &[(&str, &str)],
) -> Result<TextResponse, SourceError> {
    send_text(ctx, &url, headers, Payload::None, &[]).await
}

/// JSON-POST с куками из `get_text_with_cookies` (Livewire/CSRF).
pub async fn post_json_with_cookies(
    ctx: &AdapterCtx,
    url: Url,
    headers: &[(&str, &str)],
    json: &str,
    cookies: &[String],
) -> Result<TextResponse, SourceError> {
    send_text(ctx, &url, headers, Payload::Json(json), cookies).await
}

async fn send_text(
    ctx: &AdapterCtx,
    url: &Url,
    headers: &[(&str, &str)],
    payload: Payload<'_>,
    cookies: &[String],
) -> Result<TextResponse, SourceError> {
    if !ctx.breaker.allow() {
        return Err(SourceError::CircuitOpen);
    }

    let _permit = ctx
        .metadata_semaphore
        .acquire()
        .await
        .map_err(|_| SourceError::Network)?;

    let mut last_status = None;
    for attempt in 0..MAX_ATTEMPTS {
        let mut request = match payload {
            Payload::None => ctx.client.get(url.clone()),
            Payload::Form(form) => ctx.client.post(url.clone()).form(form),
            Payload::Json(body) => ctx
                .client
                .post(url.clone())
                .header(CONTENT_TYPE, "application/json")
                .body(body.to_owned()),
        };
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        if !cookies.is_empty() {
            request = request.header(COOKIE, cookies.join("; "));
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    let cookies = collect_cookies(&response);
                    let body = read_body_capped(response, METADATA_BODY_LIMIT).await?;
                    ctx.breaker.record_success();
                    let body = String::from_utf8(body).map_err(|_| SourceError::Parse {
                        stage: "body",
                        detail: "response is not UTF-8".to_owned(),
                    })?;
                    return Ok(TextResponse { body, cookies });
                }

                // Редиректы не следуем: источник может уводить на оплату
                // или на страницу тайтла, и адаптер должен различить это.
                if status.is_redirection() {
                    let location = response
                        .headers()
                        .get(reqwest::header::LOCATION)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or_default()
                        .to_owned();
                    return Err(SourceError::Redirect { location });
                }

                let code = status.as_u16();
                last_status = Some(code);
                match code {
                    401 => return Err(SourceError::AuthRequired),
                    403 => return Err(SourceError::Blocked),
                    404 => return Err(SourceError::NotFound),
                    429 => {
                        return Err(SourceError::RateLimited {
                            retry_after_secs: retry_after(&response),
                        });
                    }
                    500..=599 if attempt + 1 < MAX_ATTEMPTS => {
                        ctx.breaker.record_failure();
                        tokio::time::sleep(RETRY_BASE * (attempt as u32 + 1)).await;
                        continue;
                    }
                    _ => break,
                }
            }
            Err(_error) if attempt + 1 < MAX_ATTEMPTS => {
                tracing::debug!("source request failed; retrying");
                ctx.breaker.record_failure();
                tokio::time::sleep(RETRY_BASE * (attempt as u32 + 1)).await;
            }
            Err(_error) => {
                tracing::debug!("source request failed");
                ctx.breaker.record_failure();
                return Err(SourceError::Network);
            }
        }
    }

    ctx.breaker.record_failure();
    Err(SourceError::Http {
        status: last_status,
    })
}

/// Только пары `name=value`: атрибуты `Path`, `Secure` и срок жизни
/// заголовка `Cookie` не нужны, повторный запрос идёт сразу за GET.
fn collect_cookies(response: &Response) -> Vec<String> {
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .map(str::trim)
        .filter(|pair| !pair.is_empty())
        .map(str::to_owned)
        .collect()
}

pub struct FetchResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// Один запрос без ретраев: статус возвращается вызывающему,
/// чтобы прокси картинок мог отличить 401/403/410 от временной ошибки.
pub async fn fetch_bytes(
    client: &Client,
    url: Url,
    headers: &[(&str, &str)],
    max_bytes: usize,
) -> Result<FetchResponse, SourceError> {
    let mut request = client.get(url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }

    let response = request.send().await.map_err(|_error| {
        tracing::debug!("image request failed");
        SourceError::Network
    })?;

    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    // Тело ошибки не нужно: вызывающему важен статус, а не чужой HTML
    let body = if (200..300).contains(&status) {
        read_body_capped(response, max_bytes).await?
    } else {
        Vec::new()
    };

    Ok(FetchResponse {
        status,
        content_type,
        body,
    })
}

async fn read_body_capped(
    mut response: Response,
    max_bytes: usize,
) -> Result<Vec<u8>, SourceError> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(SourceError::Parse {
            stage: "body",
            detail: "response body is too large".to_owned(),
        });
    }

    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_error| {
        tracing::debug!("cannot read source response body");
        SourceError::Network
    })? {
        if body.len() + chunk.len() > max_bytes {
            return Err(SourceError::Parse {
                stage: "body",
                detail: "response body is too large".to_owned(),
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn retry_after(response: &Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
}

/// Путь API относительно `base_url`; `base_url` считается origin-ом.
pub fn endpoint_url(ctx: &AdapterCtx, path: &str) -> Result<Url, SourceError> {
    ctx.endpoint(path)
}

/// Проверка адреса, который вернул адаптер, перед запросом картинки.
pub fn ensure_allowed(
    url: &Url,
    allowlist: &crate::sources::config::HostAllowlist,
) -> Result<(), SourceError> {
    if crate::sources::config::url_allowed(url, allowlist) {
        Ok(())
    } else {
        Err(SourceError::Config(format!(
            "host `{}` is not allowed",
            url.host_str().unwrap_or("<none>")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_addresses_are_rejected() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.5.4",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
        ] {
            let ip: IpAddr = ip.parse().expect("ip");
            assert!(is_forbidden_ip(ip, false), "{ip} must be rejected");
        }

        for ip in ["8.8.8.8", "1.1.1.1", "2a00:1450:4001:80b::200e"] {
            let ip: IpAddr = ip.parse().expect("ip");
            assert!(!is_forbidden_ip(ip, false), "{ip} must be allowed");
        }

        // В тестах loopback разрешён явно: локальный сервер отдаёт картинки
        assert!(!is_forbidden_ip("127.0.0.1".parse().expect("ip"), true));
    }
}
