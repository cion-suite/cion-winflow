mod probes;
mod system;

use std::net::Ipv4Addr;
use std::time::Duration;

use serde::Serialize;

use probes::{ProbeResults, TraceInfo};
use system::{ProxyConfig, SystemFacts};

const EXIT_IP: Ipv4Addr = Ipv4Addr::new(1, 1, 1, 1);
const EXIT_PORT: u16 = 443;
const EXIT_TARGET: &str = "TCP 1.1.1.1:443";
const DNS_HOST: &str = "example.com";
const TRACE_URL: &str = "https://1.1.1.1/cdn-cgi/trace";
const TRACE_FALLBACK_URL: &str = "https://www.cloudflare.com/cdn-cgi/trace";
const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Ok,
    Warning,
    Error,
    Muted,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpEntry {
    value: Option<String>,
    tone: Tone,
    country_code: Option<String>,
    alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyState {
    enabled: bool,
    host: Option<String>,
    port: Option<u16>,
    port_listening: bool,
    connect_status: Option<u16>,
    connect_latency_ms: Option<u32>,
    tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelState {
    adapter: Option<String>,
    up: bool,
    source_ip: Option<String>,
    route_via_tun: bool,
    tone: Tone,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessState {
    adapter: Option<String>,
    up: bool,
    gateway: Option<String>,
    gateway_reachable: bool,
    exit_target: String,
    exit_ok: bool,
    exit_latency_ms: Option<u32>,
    exit_via: Option<String>,
    dns_host: String,
    dns_ok: bool,
    dns_latency_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSnapshot {
    browser_ip: IpEntry,
    system_ip: IpEntry,
    local_ip: IpEntry,
    proxy: ProxyState,
    tunnel: TunnelState,
    access: AccessState,
}

#[derive(Debug, Serialize)]
pub struct NetworkError {
    code: &'static str,
    message: String,
}

impl NetworkError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for NetworkError {}

impl From<tauri::Error> for NetworkError {
    fn from(error: tauri::Error) -> Self {
        Self::new("runtime", error.to_string())
    }
}

#[tauri::command]
pub async fn network_snapshot() -> Result<NetworkSnapshot, NetworkError> {
    tokio::time::timeout(SNAPSHOT_TIMEOUT, collect_snapshot())
        .await
        .unwrap_or_else(|_| {
            Err(NetworkError::new(
                "timeout",
                "network snapshot did not finish in time",
            ))
        })
}

async fn collect_snapshot() -> Result<NetworkSnapshot, NetworkError> {
    let facts = tauri::async_runtime::spawn_blocking(system::collect)
        .await
        .map_err(NetworkError::from)??;
    let probes = probes::run(&facts).await;
    Ok(assemble(&facts, &probes))
}

fn assemble(facts: &SystemFacts, probes: &ProbeResults) -> NetworkSnapshot {
    let system_ip = ip_entry(probes.direct.as_ref());
    let browser_ip = if facts.proxy.endpoint.is_some() {
        ip_entry(probes.proxied.as_ref())
    } else if facts.proxy.enabled {
        // PAC задан, но вычислить прокси не удалось. В такой ситуации браузер
        // обычно уходит напрямую, поэтому показываем прямой IP с предупреждением.
        match probes.direct.as_ref() {
            Some(trace) => IpEntry {
                value: Some(trace.ip.clone()),
                tone: Tone::Warning,
                country_code: trace.country.clone(),
                alias: None,
            },
            None => IpEntry {
                value: None,
                tone: Tone::Warning,
                country_code: None,
                alias: None,
            },
        }
    } else {
        ip_entry(probes.direct.as_ref())
    };

    let local_ip = match &facts.physical {
        Some(adapter) => IpEntry {
            value: adapter.ipv4.map(|ip| ip.to_string()),
            tone: if adapter.ipv4.is_some() {
                Tone::Ok
            } else {
                Tone::Error
            },
            country_code: None,
            alias: Some(adapter.name.clone()),
        },
        None => IpEntry {
            value: None,
            tone: Tone::Error,
            country_code: None,
            alias: None,
        },
    };

    let exit_ok = probes.exit_ms.is_some();
    let tunnel = match &facts.tunnel {
        Some(adapter) => TunnelState {
            adapter: Some(adapter.name.clone()),
            up: adapter.up,
            source_ip: facts
                .route_source_ip
                .or(adapter.ipv4)
                .map(|ip| ip.to_string()),
            route_via_tun: true,
            tone: if adapter.up && exit_ok {
                Tone::Ok
            } else {
                Tone::Error
            },
        },
        None => TunnelState {
            adapter: None,
            up: false,
            source_ip: None,
            route_via_tun: false,
            tone: Tone::Muted,
        },
    };

    let physical = facts.physical.as_ref();
    let access = AccessState {
        adapter: physical.map(|adapter| adapter.name.clone()),
        up: physical.map(|adapter| adapter.up).unwrap_or(false),
        gateway: physical
            .and_then(|adapter| adapter.gateway)
            .map(|gateway| gateway.to_string()),
        gateway_reachable: facts.gateway_reachable,
        exit_target: EXIT_TARGET.to_string(),
        exit_ok,
        exit_latency_ms: probes.exit_ms,
        exit_via: facts
            .route_adapter
            .as_ref()
            .map(|adapter| adapter.name.clone()),
        dns_host: DNS_HOST.to_string(),
        dns_ok: probes.dns_ms.is_some(),
        dns_latency_ms: probes.dns_ms,
    };

    NetworkSnapshot {
        browser_ip,
        system_ip,
        local_ip,
        proxy: assemble_proxy(&facts.proxy, probes),
        tunnel,
        access,
    }
}

fn ip_entry(trace: Option<&TraceInfo>) -> IpEntry {
    match trace {
        Some(trace) => IpEntry {
            value: Some(trace.ip.clone()),
            tone: Tone::Ok,
            country_code: trace.country.clone(),
            alias: None,
        },
        None => IpEntry {
            value: None,
            tone: Tone::Error,
            country_code: None,
            alias: None,
        },
    }
}

fn assemble_proxy(config: &ProxyConfig, probes: &ProbeResults) -> ProxyState {
    if !config.enabled {
        return ProxyState {
            enabled: false,
            host: None,
            port: None,
            port_listening: false,
            connect_status: None,
            connect_latency_ms: None,
            tone: Tone::Muted,
        };
    }

    let endpoint = config.endpoint.as_ref();
    let host = endpoint.map(|endpoint| endpoint.host.clone());
    let port = endpoint.map(|endpoint| endpoint.port);
    let port_listening = probes.proxy_tcp_ms.is_some();
    let connect_status = probes.proxy_status;
    let connect_latency_ms = probes.proxy_latency_ms;

    // Зелёный тон означает, что через прокси получен реальный ответ trace,
    // а не только установлено TCP-соединение: иначе бейдж и IP расходятся.
    let tone = match endpoint {
        None if config.pac_configured => Tone::Warning,
        None => Tone::Error,
        Some(_) if !port_listening => Tone::Error,
        Some(_) if probes.proxied.is_some() => Tone::Ok,
        Some(_) => match connect_status {
            Some(code) if (200..300).contains(&code) => Tone::Warning,
            Some(407) => Tone::Warning,
            Some(_) => Tone::Error,
            None => Tone::Warning,
        },
    };

    ProxyState {
        enabled: true,
        host,
        port,
        port_listening,
        connect_status,
        connect_latency_ms,
        tone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use system::{AdapterInfo, ProxyEndpoint, ProxyKind};

    fn adapter(name: &str, ip: &str, gateway: Option<&str>, if_type: u32) -> AdapterInfo {
        AdapterInfo {
            index: 1,
            name: name.to_string(),
            description: String::new(),
            if_type,
            up: true,
            ipv4: Some(ip.parse().unwrap()),
            gateway: gateway.map(|value| value.parse().unwrap()),
            has_mac: true,
            metric: 10,
        }
    }

    fn facts() -> SystemFacts {
        SystemFacts {
            physical: Some(adapter("Ethernet", "192.168.1.42", Some("192.168.1.1"), 6)),
            route_adapter: Some(adapter("Ethernet", "192.168.1.42", Some("192.168.1.1"), 6)),
            route_source_ip: Some("192.168.1.42".parse().unwrap()),
            tunnel: None,
            proxy: ProxyConfig::default(),
            gateway_reachable: true,
        }
    }

    fn trace(ip: &str, country: Option<&str>) -> TraceInfo {
        TraceInfo {
            ip: ip.to_string(),
            country: country.map(str::to_string),
        }
    }

    #[test]
    fn proxy_ok_uses_proxied_browser_ip() {
        let mut facts = facts();
        facts.proxy = ProxyConfig {
            enabled: true,
            endpoint: Some(ProxyEndpoint {
                host: "127.0.0.1".to_string(),
                port: 2080,
                kind: ProxyKind::Http,
            }),
            pac_configured: false,
        };
        let probes = ProbeResults {
            direct: Some(trace("212.57.103.70", Some("RU"))),
            proxied: Some(trace("185.42.12.8", Some("NL"))),
            proxy_tcp_ms: Some(1),
            proxy_status: Some(200),
            proxy_latency_ms: Some(41),
            exit_ms: Some(34),
            dns_ms: Some(12),
        };

        let snapshot = assemble(&facts, &probes);

        assert_eq!(snapshot.proxy.tone, Tone::Ok);
        assert_eq!(snapshot.browser_ip.value.as_deref(), Some("185.42.12.8"));
        assert_eq!(snapshot.browser_ip.country_code.as_deref(), Some("NL"));
        assert_eq!(snapshot.system_ip.value.as_deref(), Some("212.57.103.70"));
        assert_eq!(snapshot.local_ip.alias.as_deref(), Some("Ethernet"));
    }

    #[test]
    fn dead_proxy_is_error_and_hides_browser_ip() {
        let mut facts = facts();
        facts.proxy = ProxyConfig {
            enabled: true,
            endpoint: Some(ProxyEndpoint {
                host: "127.0.0.1".to_string(),
                port: 2080,
                kind: ProxyKind::Http,
            }),
            pac_configured: false,
        };
        let probes = ProbeResults {
            direct: Some(trace("212.57.103.70", Some("RU"))),
            ..ProbeResults::default()
        };

        let snapshot = assemble(&facts, &probes);

        assert_eq!(snapshot.proxy.tone, Tone::Error);
        assert_eq!(snapshot.browser_ip.tone, Tone::Error);
        assert_eq!(snapshot.browser_ip.value, None);
    }

    #[test]
    fn unresolved_pac_is_warning() {
        let mut facts = facts();
        facts.proxy = ProxyConfig {
            enabled: true,
            endpoint: None,
            pac_configured: true,
        };

        let snapshot = assemble(&facts, &ProbeResults::default());

        assert_eq!(snapshot.proxy.tone, Tone::Warning);
        assert_eq!(snapshot.proxy.host, None);
        assert_eq!(snapshot.browser_ip.tone, Tone::Warning);
        assert_eq!(snapshot.browser_ip.value, None);
    }

    #[test]
    fn unresolved_pac_falls_back_to_direct_browser_ip() {
        let mut facts = facts();
        facts.proxy = ProxyConfig {
            enabled: true,
            endpoint: None,
            pac_configured: true,
        };
        let probes = ProbeResults {
            direct: Some(trace("212.57.103.70", Some("RU"))),
            ..ProbeResults::default()
        };

        let snapshot = assemble(&facts, &probes);

        assert_eq!(snapshot.browser_ip.value.as_deref(), Some("212.57.103.70"));
        assert_eq!(snapshot.browser_ip.tone, Tone::Warning);
    }

    #[test]
    fn http_connect_without_trace_is_warning() {
        let mut facts = facts();
        facts.proxy = ProxyConfig {
            enabled: true,
            endpoint: Some(ProxyEndpoint {
                host: "127.0.0.1".to_string(),
                port: 2080,
                kind: ProxyKind::Http,
            }),
            pac_configured: false,
        };
        let probes = ProbeResults {
            direct: Some(trace("212.57.103.70", Some("RU"))),
            proxy_tcp_ms: Some(1),
            proxy_status: Some(200),
            ..ProbeResults::default()
        };

        let snapshot = assemble(&facts, &probes);

        assert_eq!(snapshot.proxy.tone, Tone::Warning);
        assert_eq!(snapshot.browser_ip.tone, Tone::Error);
    }

    #[test]
    fn working_socks_proxy_is_ok() {
        let mut facts = facts();
        facts.proxy = ProxyConfig {
            enabled: true,
            endpoint: Some(ProxyEndpoint {
                host: "127.0.0.1".to_string(),
                port: 1080,
                kind: ProxyKind::Socks,
            }),
            pac_configured: false,
        };
        let probes = ProbeResults {
            direct: Some(trace("212.57.103.70", Some("RU"))),
            proxied: Some(trace("185.42.12.8", Some("NL"))),
            proxy_tcp_ms: Some(1),
            ..ProbeResults::default()
        };

        let snapshot = assemble(&facts, &probes);

        assert_eq!(snapshot.proxy.tone, Tone::Ok);
    }

    #[test]
    fn tunnel_without_exit_is_error() {
        let mut facts = facts();
        facts.tunnel = Some(AdapterInfo {
            index: 2,
            name: "neko-tun".to_string(),
            description: "Wintun Userspace Tunnel".to_string(),
            if_type: 53,
            up: true,
            ipv4: Some("172.19.0.1".parse().unwrap()),
            gateway: None,
            has_mac: false,
            metric: 1,
        });
        facts.route_source_ip = Some("172.19.0.1".parse().unwrap());

        let snapshot = assemble(&facts, &ProbeResults::default());

        assert_eq!(snapshot.tunnel.tone, Tone::Error);
        assert!(snapshot.tunnel.route_via_tun);
        assert_eq!(snapshot.tunnel.adapter.as_deref(), Some("neko-tun"));
    }

    #[test]
    fn offline_has_muted_proxy_and_tunnel() {
        let snapshot = assemble(&SystemFacts::default(), &ProbeResults::default());

        assert_eq!(snapshot.proxy.tone, Tone::Muted);
        assert_eq!(snapshot.tunnel.tone, Tone::Muted);
        assert_eq!(snapshot.browser_ip.tone, Tone::Error);
        assert_eq!(snapshot.system_ip.tone, Tone::Error);
        assert_eq!(snapshot.local_ip.tone, Tone::Error);
    }

    #[test]
    fn serializes_contract_keys() {
        let snapshot = assemble(&SystemFacts::default(), &ProbeResults::default());
        let value = serde_json::to_value(&snapshot).unwrap();
        let object = value.as_object().unwrap();

        for key in [
            "browserIp",
            "systemIp",
            "localIp",
            "proxy",
            "tunnel",
            "access",
        ] {
            assert!(object.contains_key(key), "missing {key}");
        }

        let proxy = object["proxy"].as_object().unwrap();
        for key in ["portListening", "connectStatus", "connectLatencyMs"] {
            assert!(proxy.contains_key(key), "missing proxy.{key}");
        }

        let tunnel = object["tunnel"].as_object().unwrap();
        for key in ["sourceIp", "routeViaTun"] {
            assert!(tunnel.contains_key(key), "missing tunnel.{key}");
        }

        let access = object["access"].as_object().unwrap();
        for key in [
            "gatewayReachable",
            "exitTarget",
            "exitOk",
            "exitLatencyMs",
            "exitVia",
            "dnsHost",
            "dnsOk",
            "dnsLatencyMs",
        ] {
            assert!(access.contains_key(key), "missing access.{key}");
        }

        assert_eq!(object["browserIp"]["value"], serde_json::Value::Null);
        assert_eq!(serde_json::to_value(Tone::Muted).unwrap(), "muted");
    }
}
