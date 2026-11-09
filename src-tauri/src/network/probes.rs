use std::net::IpAddr;
use std::time::{Duration, Instant};

use reqwest::redirect::Policy;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpStream, lookup_host};
use tokio::time::{Instant as TokioInstant, timeout};

use super::system::{ProxyEndpoint, ProxyKind, SystemFacts};
use super::{DNS_HOST, EXIT_IP, EXIT_PORT, TRACE_FALLBACK_URL, TRACE_URL};

const TRACE_REQUEST_TIMEOUT: Duration = Duration::from_millis(3000);
const TRACE_BUDGET: Duration = Duration::from_millis(8000);
const TRACE_CONNECT_TIMEOUT: Duration = Duration::from_millis(2000);
const PROXY_CONNECT_TIMEOUT: Duration = Duration::from_millis(2000);
const CONNECT_RESPONSE_TIMEOUT: Duration = Duration::from_millis(2000);
const EXIT_TIMEOUT: Duration = Duration::from_millis(1500);
const DNS_TIMEOUT: Duration = Duration::from_millis(1500);
const TRACE_USER_AGENT: &str = "cion-winflow/0.1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TraceFormat {
    Cloudflare,
    PlainText,
    Json,
}

struct TraceProvider {
    url: &'static str,
    format: TraceFormat,
}

/// Несколько независимых сервисов: один и тот же выходной IP может быть
/// недоступен через конкретный прокси или узел, а браузер при этом работает.
const TRACE_PROVIDERS: &[TraceProvider] = &[
    // Тот же сервис, который пользователь обычно открывает в браузере.
    // Прокси с per-destination маршрутизацией может выходить разными IP
    // для разных сайтов, поэтому источник должен совпадать.
    TraceProvider {
        url: "https://api.2ip.io",
        format: TraceFormat::Json,
    },
    // IP-литерал не требует DNS — важен, если в TUN-режиме сломан резолвер.
    TraceProvider {
        url: TRACE_URL,
        format: TraceFormat::Cloudflare,
    },
    TraceProvider {
        url: TRACE_FALLBACK_URL,
        format: TraceFormat::Cloudflare,
    },
    TraceProvider {
        url: "https://api.ipify.org",
        format: TraceFormat::PlainText,
    },
    TraceProvider {
        url: "https://ipinfo.io/json",
        format: TraceFormat::Json,
    },
    TraceProvider {
        url: "https://ifconfig.me/ip",
        format: TraceFormat::PlainText,
    },
    TraceProvider {
        url: "https://icanhazip.com",
        format: TraceFormat::PlainText,
    },
    // Без TLS: последний шанс, если TLS-соединения через TUN не проходят.
    TraceProvider {
        url: "http://api.ipify.org",
        format: TraceFormat::PlainText,
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceInfo {
    pub ip: String,
    pub country: Option<String>,
}

#[derive(Debug, Default)]
pub struct ProbeResults {
    pub direct: Option<TraceInfo>,
    pub proxied: Option<TraceInfo>,
    pub proxy_tcp_ms: Option<u32>,
    pub proxy_status: Option<u16>,
    pub proxy_latency_ms: Option<u32>,
    pub exit_ms: Option<u32>,
    pub dns_ms: Option<u32>,
}

#[derive(Debug, Default)]
struct ConnectProbe {
    tcp_ms: Option<u32>,
    status: Option<u16>,
    latency_ms: Option<u32>,
}

pub async fn run(facts: &SystemFacts) -> ProbeResults {
    let endpoint = facts.proxy.endpoint.as_ref();

    let (direct, proxied, connect, exit_ms, dns_ms) = tokio::join!(
        fetch_trace(None),
        async {
            match endpoint {
                Some(proxy) => fetch_trace(Some(proxy)).await,
                None => None,
            }
        },
        async {
            match endpoint {
                Some(proxy) => probe_connect(proxy).await,
                None => ConnectProbe::default(),
            }
        },
        probe_exit(),
        probe_dns(),
    );

    ProbeResults {
        direct,
        proxied,
        proxy_tcp_ms: connect.tcp_ms,
        proxy_status: connect.status,
        proxy_latency_ms: connect.latency_ms,
        exit_ms,
        dns_ms,
    }
}

async fn fetch_trace(proxy: Option<&ProxyEndpoint>) -> Option<TraceInfo> {
    let client = trace_client(proxy)?;
    let deadline = TokioInstant::now() + TRACE_BUDGET;

    for provider in TRACE_PROVIDERS {
        let remaining = deadline.saturating_duration_since(TokioInstant::now());
        if remaining.is_zero() {
            break;
        }
        let Some(text) = fetch_text(&client, provider.url, deadline).await else {
            continue;
        };
        if let Some(info) = parse_trace(provider.format, &text) {
            return Some(info);
        }
    }
    None
}

fn trace_client(proxy: Option<&ProxyEndpoint>) -> Option<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(TRACE_CONNECT_TIMEOUT)
        .timeout(TRACE_REQUEST_TIMEOUT)
        .redirect(Policy::limited(2))
        .user_agent(TRACE_USER_AGENT);

    builder = match proxy {
        Some(proxy) => builder.proxy(reqwest::Proxy::all(proxy.url()).ok()?),
        None => builder.no_proxy(),
    };

    builder.build().ok()
}

async fn fetch_text(client: &reqwest::Client, url: &str, deadline: TokioInstant) -> Option<String> {
    let budget = deadline
        .saturating_duration_since(TokioInstant::now())
        .min(TRACE_REQUEST_TIMEOUT);
    if budget.is_zero() {
        return None;
    }
    let response = timeout(budget, client.get(url).send()).await.ok()?.ok()?;
    if !response.status().is_success() {
        return None;
    }

    // Тело запроса делят тот же общий дедлайн: медленный сервер не должен
    // выводить бюджет trace за границы команды.
    let budget = deadline
        .saturating_duration_since(TokioInstant::now())
        .min(TRACE_REQUEST_TIMEOUT);
    if budget.is_zero() {
        return None;
    }
    timeout(budget, response.text()).await.ok()?.ok()
}

async fn probe_connect(proxy: &ProxyEndpoint) -> ConnectProbe {
    let start = Instant::now();
    let connected = timeout(
        PROXY_CONNECT_TIMEOUT,
        TcpStream::connect((proxy.host.as_str(), proxy.port)),
    )
    .await;

    let mut stream = match connected {
        Ok(Ok(stream)) => stream,
        _ => return ConnectProbe::default(),
    };
    let tcp_ms = elapsed_ms(start);
    let _ = stream.set_nodelay(true);

    if proxy.kind != ProxyKind::Http {
        return ConnectProbe {
            tcp_ms: Some(tcp_ms),
            status: None,
            latency_ms: None,
        };
    }

    let request = b"CONNECT 1.1.1.1:443 HTTP/1.1\r\nHost: 1.1.1.1:443\r\nProxy-Connection: keep-alive\r\n\r\n";
    let written = timeout(Duration::from_millis(500), stream.write_all(request)).await;
    if !matches!(written, Ok(Ok(()))) {
        return ConnectProbe {
            tcp_ms: Some(tcp_ms),
            status: None,
            latency_ms: None,
        };
    }

    let mut buffer = [0u8; 8192];
    let read = timeout(CONNECT_RESPONSE_TIMEOUT, stream.read(&mut buffer)).await;
    let status = match read {
        Ok(Ok(count)) if count > 0 => parse_connect_status(&buffer[..count]),
        _ => None,
    };

    ConnectProbe {
        tcp_ms: Some(tcp_ms),
        status,
        latency_ms: Some(elapsed_ms(start)),
    }
}

async fn probe_exit() -> Option<u32> {
    let start = Instant::now();
    match timeout(EXIT_TIMEOUT, TcpStream::connect((EXIT_IP, EXIT_PORT))).await {
        Ok(Ok(_)) => Some(elapsed_ms(start)),
        _ => None,
    }
}

async fn probe_dns() -> Option<u32> {
    let start = Instant::now();
    match timeout(DNS_TIMEOUT, lookup_host((DNS_HOST, 443))).await {
        Ok(Ok(mut addresses)) => addresses.next().map(|_| elapsed_ms(start)),
        _ => None,
    }
}

fn elapsed_ms(start: Instant) -> u32 {
    start.elapsed().as_millis().min(u32::MAX as u128) as u32
}

fn parse_trace(format: TraceFormat, text: &str) -> Option<TraceInfo> {
    match format {
        TraceFormat::Cloudflare => parse_cloudflare_trace(text),
        TraceFormat::PlainText => parse_plain_ip(text),
        TraceFormat::Json => parse_json_ip(text),
    }
}

fn parse_cloudflare_trace(text: &str) -> Option<TraceInfo> {
    let mut ip = None;
    let mut country = None;

    for line in text.lines() {
        if let Some(value) = line.strip_prefix("ip=") {
            let value = value.trim();
            if value.parse::<IpAddr>().is_ok() {
                ip = Some(value.to_string());
            }
        } else if let Some(value) = line.strip_prefix("loc=") {
            let value = value.trim();
            if is_country_code(value) {
                country = Some(value.to_string());
            }
        }
    }

    Some(TraceInfo { ip: ip?, country })
}

fn parse_plain_ip(text: &str) -> Option<TraceInfo> {
    let ip = text.lines().next()?.trim();
    ip.parse::<IpAddr>().ok()?;
    Some(TraceInfo {
        ip: ip.to_string(),
        country: None,
    })
}

fn parse_json_ip(text: &str) -> Option<TraceInfo> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let ip = value.get("ip")?.as_str()?.trim();
    ip.parse::<IpAddr>().ok()?;

    let country = ["cc", "code", "country"]
        .iter()
        .find_map(|key| value.get(*key).and_then(|value| value.as_str()))
        .map(str::trim)
        .filter(|value| is_country_code(value))
        .map(str::to_string);

    Some(TraceInfo {
        ip: ip.to_string(),
        country,
    })
}

fn is_country_code(value: &str) -> bool {
    value.len() == 2
        && value.bytes().all(|byte| byte.is_ascii_alphabetic())
        && value != "XX"
        && value != "T1"
}

fn parse_connect_status(bytes: &[u8]) -> Option<u16> {
    let text = std::str::from_utf8(bytes).ok()?;
    let line = text.lines().next()?;
    let mut parts = line.split_whitespace();
    let _version = parts.next()?;
    parts.next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trace() {
        let info = parse_cloudflare_trace("ip=185.42.12.8\nloc=NL\ncolo=AMS\n").unwrap();
        assert_eq!(info.ip, "185.42.12.8");
        assert_eq!(info.country.as_deref(), Some("NL"));
    }

    #[test]
    fn ignores_unknown_country() {
        let info = parse_cloudflare_trace("ip=1.2.3.4\nloc=XX\n").unwrap();
        assert_eq!(info.country, None);
    }

    #[test]
    fn rejects_trace_without_ip() {
        assert!(parse_cloudflare_trace("loc=NL\n").is_none());
    }

    #[test]
    fn parses_plain_ip() {
        let info = parse_plain_ip("185.42.12.8\n").unwrap();
        assert_eq!(info.ip, "185.42.12.8");
        assert_eq!(info.country, None);
        assert!(parse_plain_ip("<html>blocked</html>").is_none());
    }

    #[test]
    fn parses_json_ip() {
        let info = parse_json_ip(r#"{"ip":"185.42.12.8","country":"NL"}"#).unwrap();
        assert_eq!(info.ip, "185.42.12.8");
        assert_eq!(info.country.as_deref(), Some("NL"));

        let info = parse_json_ip(r#"{"ip":"185.42.12.8","country":"Germany","cc":"DE"}"#).unwrap();
        assert_eq!(info.country.as_deref(), Some("DE"));

        // Формат api.2ip.io: полное название страны и двухбуквенный код.
        let info =
            parse_json_ip(r#"{"ip":"5.144.178.93","country":"Germany","code":"DE"}"#).unwrap();
        assert_eq!(info.ip, "5.144.178.93");
        assert_eq!(info.country.as_deref(), Some("DE"));

        assert!(parse_json_ip(r#"{"country":"NL"}"#).is_none());
    }

    #[test]
    fn parses_connect_status() {
        assert_eq!(
            parse_connect_status(b"HTTP/1.1 200 Connection established\r\n\r\n"),
            Some(200)
        );
        assert_eq!(
            parse_connect_status(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n"),
            Some(407)
        );
        assert_eq!(parse_connect_status(b"garbage"), None);
    }

    #[tokio::test]
    #[ignore = "requires real network access"]
    async fn runs_real_probes() {
        let facts = crate::network::system::collect().expect("system facts");
        let results = run(&facts).await;
        println!("{results:#?}");
    }
}
