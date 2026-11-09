use std::net::Ipv4Addr;

use super::NetworkError;

const IF_TYPE_ETHERNET_CSMACD: u32 = 6;
const IF_TYPE_IEEE80211: u32 = 71;
const IF_TYPE_PPP: u32 = 23;
const IF_TYPE_PROP_VIRTUAL: u32 = 53;
const IF_TYPE_TUNNEL: u32 = 131;
const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdapterInfo {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub if_type: u32,
    pub up: bool,
    pub ipv4: Option<Ipv4Addr>,
    pub gateway: Option<Ipv4Addr>,
    pub has_mac: bool,
    pub metric: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyKind {
    Http,
    Https,
    Socks,
    Socks4,
    Socks4a,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProxyEndpoint {
    pub host: String,
    pub port: u16,
    pub kind: ProxyKind,
}

impl ProxyEndpoint {
    pub fn url(&self) -> String {
        let scheme = match self.kind {
            ProxyKind::Http => "http",
            ProxyKind::Https => "https",
            ProxyKind::Socks => "socks5h",
            ProxyKind::Socks4 => "socks4",
            ProxyKind::Socks4a => "socks4a",
        };
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!("{scheme}://{host}:{}", self.port)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProxyConfig {
    pub enabled: bool,
    pub endpoint: Option<ProxyEndpoint>,
    pub pac_configured: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SystemFacts {
    pub physical: Option<AdapterInfo>,
    pub route_adapter: Option<AdapterInfo>,
    pub route_source_ip: Option<Ipv4Addr>,
    pub tunnel: Option<AdapterInfo>,
    pub proxy: ProxyConfig,
    pub gateway_reachable: bool,
}

#[cfg(windows)]
pub fn collect() -> Result<SystemFacts, NetworkError> {
    let adapters = read_adapters()?;
    let route = best_route(super::EXIT_IP);
    let route_index = route.map(|(index, _)| index);
    let route_source_ip = route.and_then(|(_, source_ip)| source_ip);
    let physical = select_physical(&adapters, route_index);
    let route_adapter = route_index.and_then(|index| {
        adapters
            .iter()
            .find(|adapter| adapter.index == index)
            .cloned()
    });
    let tunnel = select_tunnel(&adapters, physical.as_ref(), route_index);
    let gateway_reachable = physical
        .as_ref()
        .and_then(|adapter| adapter.ipv4.zip(adapter.gateway))
        .map(|(ip, gateway)| arp_reachable(gateway, ip))
        .unwrap_or(false);
    let proxy = read_proxy_config();

    Ok(SystemFacts {
        physical,
        route_adapter,
        route_source_ip,
        tunnel,
        proxy,
        gateway_reachable,
    })
}

#[cfg(not(windows))]
pub fn collect() -> Result<SystemFacts, NetworkError> {
    Ok(SystemFacts::default())
}

fn select_physical(adapters: &[AdapterInfo], route_index: Option<u32>) -> Option<AdapterInfo> {
    let mut candidates: Vec<&AdapterInfo> = adapters
        .iter()
        .filter(|adapter| {
            adapter.up
                && adapter.ipv4.is_some()
                && !is_loopback(adapter)
                && !is_virtual(adapter)
                && is_physical(adapter)
        })
        .collect();
    candidates.sort_by_key(|adapter| adapter.metric);
    let mut with_gateway: Vec<&AdapterInfo> = candidates
        .iter()
        .copied()
        .filter(|adapter| adapter.gateway.is_some())
        .collect();
    with_gateway.sort_by_key(|adapter| adapter.metric);

    if let Some(index) = route_index
        && let Some(adapter) = with_gateway.iter().find(|adapter| adapter.index == index)
    {
        return Some((**adapter).clone());
    }
    if let Some(adapter) = with_gateway.first() {
        return Some((**adapter).clone());
    }
    if let Some(index) = route_index
        && let Some(adapter) = candidates.iter().find(|adapter| adapter.index == index)
    {
        return Some((**adapter).clone());
    }
    candidates.first().map(|adapter| (**adapter).clone())
}

fn select_tunnel(
    adapters: &[AdapterInfo],
    physical: Option<&AdapterInfo>,
    route_index: Option<u32>,
) -> Option<AdapterInfo> {
    let index = route_index?;
    if physical.map(|adapter| adapter.index) == Some(index) {
        return None;
    }

    let adapter = adapters.iter().find(|adapter| adapter.index == index)?;
    if is_tunnel_like(adapter) {
        return Some(adapter.clone());
    }
    // TAP-подобный адаптер: Ethernet-тип, но без шлюза и не выбранный физическим.
    if is_physical(adapter) && adapter.gateway.is_none() && physical.is_some() {
        return Some(adapter.clone());
    }
    None
}

fn is_physical(adapter: &AdapterInfo) -> bool {
    matches!(adapter.if_type, IF_TYPE_ETHERNET_CSMACD | IF_TYPE_IEEE80211) && adapter.has_mac
}

fn is_tunnel_like(adapter: &AdapterInfo) -> bool {
    is_virtual(adapter)
        || matches!(
            adapter.if_type,
            IF_TYPE_PPP | IF_TYPE_PROP_VIRTUAL | IF_TYPE_TUNNEL
        )
        || !adapter.has_mac
}

/// Эвристика для виртуальных/VPN-адаптеров, которые выглядят как Ethernet.
/// Windows не даёт переносимого признака «это VPN», поэтому имена драйверов
/// остаются единственным сигналом; при необходимости его можно заменить настройкой.
fn is_virtual(adapter: &AdapterInfo) -> bool {
    let text = format!("{} {}", adapter.name, adapter.description).to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "vpn",
        "virtual",
        "tap-",
        "tap ",
        "tun",
        "wireguard",
        "wintun",
        "openvpn",
        "anyconnect",
        "globalprotect",
        "fortinet",
        "juniper",
        "pulse",
        "zerotier",
        "tailscale",
    ];
    MARKERS.iter().any(|marker| text.contains(marker))
}

fn is_loopback(adapter: &AdapterInfo) -> bool {
    adapter.if_type == IF_TYPE_SOFTWARE_LOOPBACK
        || adapter.ipv4.map(|ip| ip.is_loopback()).unwrap_or(false)
}

#[cfg(windows)]
fn read_adapters() -> Result<Vec<AdapterInfo>, NetworkError> {
    use windows::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_NO_DATA, NO_ERROR};
    use windows::Win32::NetworkManagement::IpHelper::{
        GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST,
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

    let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
    let mut size: u32 = 0;
    let first = unsafe { GetAdaptersAddresses(AF_INET.0 as u32, flags, None, None, &mut size) };
    if first == ERROR_NO_DATA.0 {
        return Ok(Vec::new());
    }
    if first != ERROR_BUFFER_OVERFLOW.0 && first != NO_ERROR.0 {
        return Err(NetworkError::new(
            "adapters",
            format!("GetAdaptersAddresses failed: {first}"),
        ));
    }

    loop {
        let words = (size as usize).div_ceil(8).max(1);
        let mut buffer = vec![0u64; words];
        let result = unsafe {
            GetAdaptersAddresses(
                AF_INET.0 as u32,
                flags,
                None,
                Some(buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH),
                &mut size,
            )
        };
        if result == ERROR_BUFFER_OVERFLOW.0 {
            continue;
        }
        if result == ERROR_NO_DATA.0 {
            return Ok(Vec::new());
        }
        if result != NO_ERROR.0 {
            return Err(NetworkError::new(
                "adapters",
                format!("GetAdaptersAddresses failed: {result}"),
            ));
        }

        let mut adapters = Vec::new();
        let mut current = buffer.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !current.is_null() {
            let adapter = unsafe { &*current };
            let index = unsafe { adapter.Anonymous1.Anonymous.IfIndex };
            let description = wide_to_string(adapter.Description);
            let name = {
                let friendly = wide_to_string(adapter.FriendlyName);
                if !friendly.is_empty() {
                    friendly
                } else if !description.is_empty() {
                    description.clone()
                } else {
                    format!("adapter {index}")
                }
            };

            let mut ipv4 = None;
            let mut unicast = adapter.FirstUnicastAddress;
            while !unicast.is_null() {
                let socket_address = unsafe { (*unicast).Address };
                if !socket_address.lpSockaddr.is_null() {
                    let address = socket_address.lpSockaddr as *const SOCKADDR_IN;
                    let family = unsafe { (*address).sin_family };
                    if family == AF_INET {
                        let raw = unsafe { (*address).sin_addr.S_un.S_addr };
                        let ip = Ipv4Addr::from(raw.to_ne_bytes());
                        if !ip.is_unspecified() && !ip.is_loopback() && !ip.is_link_local() {
                            ipv4 = Some(ip);
                            break;
                        }
                    }
                }
                unicast = unsafe { (*unicast).Next };
            }

            let mut gateway = None;
            let mut gateway_address = adapter.FirstGatewayAddress;
            while !gateway_address.is_null() {
                let socket_address = unsafe { (*gateway_address).Address };
                if !socket_address.lpSockaddr.is_null() {
                    let address = socket_address.lpSockaddr as *const SOCKADDR_IN;
                    let family = unsafe { (*address).sin_family };
                    if family == AF_INET {
                        let raw = unsafe { (*address).sin_addr.S_un.S_addr };
                        gateway = Some(Ipv4Addr::from(raw.to_ne_bytes()));
                        break;
                    }
                }
                gateway_address = unsafe { (*gateway_address).Next };
            }

            adapters.push(AdapterInfo {
                index,
                name,
                description,
                if_type: adapter.IfType,
                up: adapter.OperStatus.0
                    == windows::Win32::NetworkManagement::Ndis::NET_IF_OPER_STATUS_UP.0,
                ipv4,
                gateway,
                has_mac: adapter.PhysicalAddressLength > 0,
                metric: adapter.Ipv4Metric,
            });
            current = adapter.Next;
        }

        return Ok(adapters);
    }
}

#[cfg(windows)]
fn best_route(destination: Ipv4Addr) -> Option<(u32, Option<Ipv4Addr>)> {
    use windows::Win32::Foundation::NO_ERROR;
    use windows::Win32::NetworkManagement::IpHelper::{
        GetBestInterfaceEx, GetBestRoute2, MIB_IPFORWARD_ROW2,
    };
    use windows::Win32::Networking::WinSock::{
        AF_INET, IN_ADDR, IN_ADDR_0, SOCKADDR, SOCKADDR_IN, SOCKADDR_INET,
    };

    let destination_address = SOCKADDR_IN {
        sin_family: AF_INET,
        sin_addr: IN_ADDR {
            S_un: IN_ADDR_0 {
                S_addr: u32::from_ne_bytes(destination.octets()),
            },
        },
        ..Default::default()
    };

    let mut index = 0u32;
    let best = unsafe {
        GetBestInterfaceEx(
            &destination_address as *const SOCKADDR_IN as *const SOCKADDR,
            &mut index,
        )
    };
    if best != NO_ERROR.0 {
        return None;
    }

    let destination_inet = SOCKADDR_INET {
        Ipv4: destination_address,
    };
    let mut row = MIB_IPFORWARD_ROW2::default();
    let mut source = SOCKADDR_INET::default();
    let route = unsafe {
        GetBestRoute2(
            None,
            index,
            None,
            &destination_inet,
            0,
            &mut row,
            &mut source,
        )
    };
    if route != NO_ERROR {
        return None;
    }

    let source_ip = unsafe {
        if source.si_family == AF_INET {
            Some(Ipv4Addr::from(
                source.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes(),
            ))
        } else {
            None
        }
    };
    Some((index, source_ip))
}

#[cfg(windows)]
fn arp_reachable(destination: Ipv4Addr, source: Ipv4Addr) -> bool {
    use windows::Win32::Foundation::NO_ERROR;
    use windows::Win32::NetworkManagement::IpHelper::SendARP;

    let destination = u32::from_ne_bytes(destination.octets());
    let source = u32::from_ne_bytes(source.octets());
    let mut mac = [0u8; 8];
    let mut length = 6u32;
    let result = unsafe { SendARP(destination, source, mac.as_mut_ptr().cast(), &mut length) };
    result == NO_ERROR.0
}

#[cfg(windows)]
fn wide_to_string(value: windows::core::PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }
    unsafe { value.to_string().unwrap_or_default() }
}

#[cfg(windows)]
fn read_proxy_config() -> ProxyConfig {
    use windows::Win32::Networking::WinInet::{
        PROXY_TYPE_AUTO_DETECT, PROXY_TYPE_AUTO_PROXY_URL, PROXY_TYPE_PROXY,
    };
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    // Реестр — только fallback: если ключ недоступен, WinINet всё ещё может
    // вернуть активные настройки.
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok();

    let enabled: u32 = key
        .as_ref()
        .and_then(|key| key.get_value("ProxyEnable").ok())
        .unwrap_or(0);
    let server: String = key
        .as_ref()
        .and_then(|key| key.get_value("ProxyServer").ok())
        .unwrap_or_default();
    let pac: String = key
        .as_ref()
        .and_then(|key| key.get_value("AutoConfigURL").ok())
        .unwrap_or_default();
    let auto_detect: u32 = key
        .as_ref()
        .and_then(|key| key.get_value("AutoDetect").ok())
        .unwrap_or(0);

    // AutoConfigURL и ProxyServer остаются в реестре даже когда
    // соответствующий режим выключен. Активные режимы и строки сообщает
    // WinINet — тот же API, через который Nekoray включает системный прокси.
    // Без этого выключенный PAC продолжал бы подменять рабочий прокси.
    let pac = pac.trim();
    let (pac_active, auto_detect_active, manual_active, server, pac_url) =
        match wininet_proxy_options() {
            Some(options) => (
                options.flags & PROXY_TYPE_AUTO_PROXY_URL != 0,
                options.flags & PROXY_TYPE_AUTO_DETECT != 0,
                options.flags & PROXY_TYPE_PROXY != 0,
                options.proxy_server,
                options.auto_config_url,
            ),
            None => (
                !pac.is_empty(),
                auto_detect != 0,
                enabled != 0,
                server,
                pac.to_string(),
            ),
        };

    // ProxyEnable выключает только ручной прокси. ProxyServer при этом остаётся
    // в реестре, и брать его за рабочий endpoint нельзя.
    let manual = manual_proxy(manual_active, &server);

    if manual.is_none() && !pac_active && !auto_detect_active {
        return ProxyConfig::default();
    }

    if pac_active {
        // PAC имеет приоритет над ручным прокси, как в WinINET.
        match resolve_auto_proxy(&pac_url, auto_detect_active, super::TRACE_FALLBACK_URL) {
            // PAC может явно вернуть DIRECT — тогда прокси для этого URL нет.
            AutoProxy::Resolved(endpoint) => ProxyConfig {
                enabled: endpoint.is_some(),
                endpoint,
                pac_configured: true,
            },
            // Ручной прокси не подставляем: WinINET тоже его не использует,
            // когда задан PAC.
            AutoProxy::Unresolved => ProxyConfig {
                enabled: true,
                endpoint: None,
                pac_configured: true,
            },
        }
    } else if auto_detect_active {
        match resolve_auto_proxy("", true, super::TRACE_FALLBACK_URL) {
            AutoProxy::Resolved(endpoint) => ProxyConfig {
                enabled: endpoint.is_some(),
                endpoint,
                pac_configured: true,
            },
            // WPAD не нашёл прокси — это нормальный режим, а не ошибка.
            AutoProxy::Unresolved => ProxyConfig {
                enabled: manual.is_some(),
                endpoint: manual,
                pac_configured: false,
            },
        }
    } else {
        ProxyConfig {
            enabled: true,
            endpoint: manual,
            pac_configured: false,
        }
    }
}

#[cfg(windows)]
#[derive(Debug)]
struct WinInetProxy {
    flags: u32,
    proxy_server: String,
    auto_config_url: String,
}

/// Читает активные настройки прокси для LAN из WinINet. Именно этот API
/// используют VPN-клиенты (в том числе Nekoray), поэтому он точнее реестра.
#[cfg(windows)]
fn wininet_proxy_options() -> Option<WinInetProxy> {
    use windows::Win32::Networking::WinInet::{
        INTERNET_OPTION_PER_CONNECTION_OPTION, INTERNET_PER_CONN_AUTOCONFIG_URL,
        INTERNET_PER_CONN_FLAGS, INTERNET_PER_CONN_OPTION_LISTW, INTERNET_PER_CONN_OPTIONW,
        INTERNET_PER_CONN_OPTIONW_0, INTERNET_PER_CONN_PROXY_SERVER, InternetQueryOptionW,
    };
    use windows::core::PWSTR;

    fn string_option(
        option: windows::Win32::Networking::WinInet::INTERNET_PER_CONN,
    ) -> INTERNET_PER_CONN_OPTIONW {
        INTERNET_PER_CONN_OPTIONW {
            dwOption: option,
            Value: INTERNET_PER_CONN_OPTIONW_0 {
                pszValue: PWSTR::null(),
            },
        }
    }

    let mut options = [
        INTERNET_PER_CONN_OPTIONW {
            dwOption: INTERNET_PER_CONN_FLAGS,
            Value: INTERNET_PER_CONN_OPTIONW_0 { dwValue: 0 },
        },
        string_option(INTERNET_PER_CONN_PROXY_SERVER),
        string_option(INTERNET_PER_CONN_AUTOCONFIG_URL),
    ];
    let mut list = INTERNET_PER_CONN_OPTION_LISTW {
        dwSize: std::mem::size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32,
        pszConnection: PWSTR::null(),
        dwOptionCount: options.len() as u32,
        dwOptionError: 0,
        pOptions: options.as_mut_ptr(),
    };
    let mut size = std::mem::size_of::<INTERNET_PER_CONN_OPTION_LISTW>() as u32;

    let result = unsafe {
        InternetQueryOptionW(
            None,
            INTERNET_OPTION_PER_CONNECTION_OPTION,
            Some((&mut list as *mut INTERNET_PER_CONN_OPTION_LISTW).cast()),
            &mut size,
        )
    };

    // Строки, выделенные WinINet, освобождаем в любом случае.
    let proxy_server = unsafe { take_global_string(options[1].Value.pszValue) };
    let auto_config_url = unsafe { take_global_string(options[2].Value.pszValue) };
    if result.is_err() {
        return None;
    }

    Some(WinInetProxy {
        flags: unsafe { options[0].Value.dwValue },
        proxy_server,
        auto_config_url,
    })
}

#[cfg(windows)]
unsafe fn take_global_string(value: windows::core::PWSTR) -> String {
    if value.is_null() {
        return String::new();
    }
    let text = unsafe { value.to_string() }.unwrap_or_default();
    unsafe { free_global(value) };
    text
}

fn manual_proxy(active: bool, server: &str) -> Option<ProxyEndpoint> {
    if active {
        parse_proxy_server(server)
    } else {
        None
    }
}

#[cfg(windows)]
enum AutoProxy {
    Resolved(Option<ProxyEndpoint>),
    Unresolved,
}

/// Вычисляет прокси для URL по PAC/WPAD через WinHTTP — тот же механизм,
/// которым пользуется системный стек. Без этого PAC-сценарии не видны.
#[cfg(windows)]
fn resolve_auto_proxy(pac_url: &str, auto_detect: bool, url: &str) -> AutoProxy {
    use windows::Win32::Networking::WinHttp::{
        WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_AUTO_DETECT_TYPE_DHCP,
        WINHTTP_AUTO_DETECT_TYPE_DNS_A, WINHTTP_AUTOPROXY_AUTO_DETECT,
        WINHTTP_AUTOPROXY_CONFIG_URL, WINHTTP_AUTOPROXY_OPTIONS, WINHTTP_PROXY_INFO,
        WinHttpCloseHandle, WinHttpGetProxyForUrl, WinHttpOpen, WinHttpSetTimeouts,
    };
    use windows::core::{HSTRING, PCWSTR};

    let agent = HSTRING::from("cion-winflow");
    let target = HSTRING::from(url);
    let pac = (!pac_url.is_empty()).then(|| HSTRING::from(pac_url));

    let session = unsafe {
        WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        )
    };
    if session.is_null() {
        return AutoProxy::Unresolved;
    }

    // Разрешение PAC ограничено по времени: команда опрашивается каждые 30 с
    // и не должна зависать на недоступном PAC-сервере.
    unsafe {
        let _ = WinHttpSetTimeouts(session, 1000, 1000, 1000, 2000);
    }

    let mut options = WINHTTP_AUTOPROXY_OPTIONS::default();
    if let Some(pac) = &pac {
        options.dwFlags |= WINHTTP_AUTOPROXY_CONFIG_URL;
        options.lpszAutoConfigUrl = PCWSTR(pac.as_ptr());
    }
    if auto_detect {
        options.dwFlags |= WINHTTP_AUTOPROXY_AUTO_DETECT;
        options.dwAutoDetectFlags = WINHTTP_AUTO_DETECT_TYPE_DHCP | WINHTTP_AUTO_DETECT_TYPE_DNS_A;
    }
    options.fAutoLogonIfChallenged = true.into();

    let mut info = WINHTTP_PROXY_INFO::default();
    let resolved =
        unsafe { WinHttpGetProxyForUrl(session, PCWSTR(target.as_ptr()), &mut options, &mut info) }
            .is_ok();

    let endpoint = if resolved {
        AutoProxy::Resolved(proxy_from_info(&info))
    } else {
        AutoProxy::Unresolved
    };

    unsafe {
        free_global(info.lpszProxy);
        free_global(info.lpszProxyBypass);
        let _ = WinHttpCloseHandle(session);
    }

    endpoint
}

#[cfg(windows)]
fn proxy_from_info(
    info: &windows::Win32::Networking::WinHttp::WINHTTP_PROXY_INFO,
) -> Option<ProxyEndpoint> {
    use windows::Win32::Networking::WinHttp::WINHTTP_ACCESS_TYPE_NO_PROXY;

    if info.dwAccessType == WINHTTP_ACCESS_TYPE_NO_PROXY || info.lpszProxy.is_null() {
        return None;
    }
    let raw = unsafe { info.lpszProxy.to_string() }.ok()?;
    parse_proxy_list(&raw)
}

/// WinHTTP возвращает строку вида `host:port`, `http=...;https=...` или `DIRECT`.
/// Разбираем её тем же кодом, что и ручной прокси из реестра.
#[cfg(any(windows, test))]
fn parse_proxy_list(raw: &str) -> Option<ProxyEndpoint> {
    let parts: Vec<&str> = raw
        .split(|character: char| character == ';' || character.is_ascii_whitespace())
        .map(str::trim)
        .filter(|part| {
            !part.is_empty()
                && !part.eq_ignore_ascii_case("direct")
                && !part.eq_ignore_ascii_case("proxy")
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    parse_proxy_server(&parts.join(";"))
}

#[cfg(windows)]
unsafe fn free_global(value: windows::core::PWSTR) {
    use windows::Win32::Foundation::{GlobalFree, HGLOBAL};

    if value.is_null() {
        return;
    }
    let _ = unsafe { GlobalFree(Some(HGLOBAL(value.0.cast()))) };
}

fn parse_proxy_server(raw: &str) -> Option<ProxyEndpoint> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let mut bare = None;
    let mut https = None;
    let mut http = None;
    let mut socks = None;

    for part in raw.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((scheme, value)) = part.split_once('=') {
            match scheme.trim().to_ascii_lowercase().as_str() {
                "https" => https = parse_endpoint(value, ProxyKind::Http),
                "http" => http = parse_endpoint(value, ProxyKind::Http),
                "socks" | "socks5" => socks = parse_endpoint(value, ProxyKind::Socks),
                "socks4" => socks = parse_endpoint(value, ProxyKind::Socks4),
                "socks4a" => socks = parse_endpoint(value, ProxyKind::Socks4a),
                _ => {}
            }
        } else if bare.is_none() {
            bare = parse_endpoint(part, ProxyKind::Http);
        }
    }

    https.or(http).or(socks).or(bare)
}

fn parse_endpoint(value: &str, default_kind: ProxyKind) -> Option<ProxyEndpoint> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    // В WinINET `https=host:port` — это прокси для HTTPS-целей, а не TLS
    // до самого прокси. TLS включается только явной схемой в значении.
    let (value, kind) = if let Some(rest) = value.strip_prefix("https://") {
        (rest, ProxyKind::Https)
    } else if let Some(rest) = value.strip_prefix("http://") {
        (rest, ProxyKind::Http)
    } else if let Some(rest) = value
        .strip_prefix("socks5h://")
        .or_else(|| value.strip_prefix("socks5://"))
    {
        (rest, ProxyKind::Socks)
    } else if let Some(rest) = value.strip_prefix("socks4a://") {
        (rest, ProxyKind::Socks4a)
    } else if let Some(rest) = value.strip_prefix("socks4://") {
        (rest, ProxyKind::Socks4)
    } else {
        (value, default_kind)
    };

    let (host, port) = if let Some(rest) = value.strip_prefix('[') {
        let (host, rest) = rest.split_once(']')?;
        let port = match rest.strip_prefix(':') {
            Some(port) => port.parse().ok()?,
            None => default_port(kind),
        };
        (host.to_string(), port)
    } else if value.matches(':').count() > 1 {
        // IPv6 без квадратных скобок — неоднозначно для host:port.
        return None;
    } else if let Some((host, port)) = value.rsplit_once(':') {
        (host.to_string(), port.parse().ok()?)
    } else {
        (value.to_string(), default_port(kind))
    };

    if host.is_empty() {
        return None;
    }
    Some(ProxyEndpoint { host, port, kind })
}

fn default_port(kind: ProxyKind) -> u16 {
    match kind {
        ProxyKind::Http => 80,
        ProxyKind::Https => 443,
        ProxyKind::Socks | ProxyKind::Socks4 | ProxyKind::Socks4a => 1080,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bare_endpoint() {
        let endpoint = parse_proxy_server("127.0.0.1:2080").unwrap();
        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(endpoint.port, 2080);
        assert_eq!(endpoint.kind, ProxyKind::Http);
    }

    #[test]
    fn https_entry_keeps_http_transport() {
        let endpoint = parse_proxy_server("http=proxy:8080;https=secure:8443").unwrap();
        assert_eq!(endpoint.host, "secure");
        assert_eq!(endpoint.port, 8443);
        assert_eq!(endpoint.kind, ProxyKind::Http);
        assert_eq!(endpoint.url(), "http://secure:8443");
    }

    #[test]
    fn explicit_https_prefix_is_tls() {
        let endpoint = parse_proxy_server("https=https://secure:8443").unwrap();
        assert_eq!(endpoint.kind, ProxyKind::Https);
        assert_eq!(endpoint.url(), "https://secure:8443");
    }

    #[test]
    fn parses_ipv6_endpoint() {
        let endpoint = parse_proxy_server("[::1]:8080").unwrap();
        assert_eq!(endpoint.host, "::1");
        assert_eq!(endpoint.port, 8080);
        assert_eq!(endpoint.url(), "http://[::1]:8080");
    }

    #[test]
    fn applies_scheme_default_ports() {
        let endpoint = parse_proxy_server("socks=127.0.0.1").unwrap();
        assert_eq!(endpoint.port, 1080);
        assert_eq!(endpoint.kind, ProxyKind::Socks);
    }

    #[test]
    fn rejects_empty_proxy_string() {
        assert!(parse_proxy_server("   ").is_none());
    }

    #[test]
    fn parses_winhttp_proxy_list() {
        let endpoint = parse_proxy_list("172.16.10.102:8080").unwrap();
        assert_eq!(endpoint.host, "172.16.10.102");
        assert_eq!(endpoint.port, 8080);
        assert_eq!(endpoint.kind, ProxyKind::Http);

        assert!(parse_proxy_list("DIRECT").is_none());
        assert!(parse_proxy_list("  ").is_none());
    }

    #[test]
    fn keeps_first_bare_proxy() {
        let endpoint = parse_proxy_server("10.0.0.1:8080;10.0.0.2:8080").unwrap();
        assert_eq!(endpoint.host, "10.0.0.1");
    }

    #[test]
    fn parses_space_separated_proxy_list() {
        let endpoint = parse_proxy_list("10.0.0.1:8080 10.0.0.2:8080").unwrap();
        assert_eq!(endpoint.host, "10.0.0.1");
    }

    #[test]
    fn disabled_manual_proxy_is_ignored() {
        assert!(manual_proxy(false, "127.0.0.1:2080").is_none());
        let endpoint = manual_proxy(true, "127.0.0.1:2080").unwrap();
        assert_eq!(endpoint.port, 2080);
    }

    #[test]
    fn keeps_socks4_scheme() {
        let endpoint = parse_proxy_server("socks4=127.0.0.1").unwrap();
        assert_eq!(endpoint.kind, ProxyKind::Socks4);
        assert_eq!(endpoint.url(), "socks4://127.0.0.1:1080");

        let endpoint = parse_proxy_server("socks4a://127.0.0.1:1081").unwrap();
        assert_eq!(endpoint.kind, ProxyKind::Socks4a);
        assert_eq!(endpoint.url(), "socks4a://127.0.0.1:1081");
    }

    fn adapter(
        index: u32,
        name: &str,
        description: &str,
        gateway: Option<&str>,
        metric: u32,
    ) -> AdapterInfo {
        AdapterInfo {
            index,
            name: name.to_string(),
            description: description.to_string(),
            if_type: IF_TYPE_ETHERNET_CSMACD,
            up: true,
            ipv4: Some(format!("192.168.1.{index}").parse().unwrap()),
            gateway: gateway.map(|value| value.parse().unwrap()),
            has_mac: true,
            metric,
        }
    }

    #[test]
    fn virtual_route_adapter_is_tunnel_not_physical() {
        let ethernet = adapter(1, "Ethernet", "Intel Ethernet", Some("192.168.1.1"), 35);
        let vpn = adapter(
            2,
            "VPN",
            "Cisco AnyConnect Virtual Miniport",
            Some("10.0.0.1"),
            1,
        );
        let adapters = vec![vpn, ethernet];

        let physical = select_physical(&adapters, Some(2)).unwrap();
        assert_eq!(physical.index, 1);
        let tunnel = select_tunnel(&adapters, Some(&physical), Some(2)).unwrap();
        assert_eq!(tunnel.index, 2);
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "requires the real Windows network stack"]
    fn collects_real_system_facts() {
        println!("wininet proxy={:?}", wininet_proxy_options());
        let facts = collect().expect("system facts");
        println!("{facts:#?}");
        assert!(facts.physical.is_some());
    }
}
