use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

use axum::{extract::ConnectInfo, http::Request};
use tower_governor::{GovernorError, key_extractor::KeyExtractor};

use crate::auth::session::now_unix;

pub const USERNAME_MAX_ATTEMPTS: u32 = 10;
pub const USERNAME_WINDOW_SECS: i64 = 15 * 60;
/// Вход в источник по паролю: короче окно и меньше попыток, потому что это
/// перебор чужого пароля от имени пользователя инсталляции.
pub const SOURCE_LOGIN_MAX_ATTEMPTS: u32 = 5;
pub const SOURCE_LOGIN_WINDOW_SECS: i64 = 10 * 60;

const X_FORWARDED_FOR: &str = "x-forwarded-for";
const MAX_TRACKED_USERNAMES: usize = 4096;

struct Window {
    started_at: i64,
    attempts: u32,
}

/// Счётчик попыток входа по username: IP-лимит видит только запрос,
/// а имя пользователя становится известно после разбора JSON.
pub struct LoginLimiter {
    inner: Mutex<HashMap<String, Window>>,
    max_attempts: u32,
    window_secs: i64,
}

impl Default for LoginLimiter {
    fn default() -> Self {
        Self::new(USERNAME_MAX_ATTEMPTS, USERNAME_WINDOW_SECS)
    }
}

impl LoginLimiter {
    pub fn new(max_attempts: u32, window_secs: i64) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            max_attempts,
            window_secs,
        }
    }

    /// Учитывает попытку. Err — сколько секунд ждать до следующей.
    pub fn check_and_record(&self, username: &str) -> Result<(), u64> {
        let now = now_unix();
        let key = username.to_lowercase();
        let mut windows = match self.inner.lock() {
            Ok(windows) => windows,
            // Отравленный mutex не должен ронять вход: состояние — только счётчики
            Err(poisoned) => poisoned.into_inner(),
        };

        if windows.len() > MAX_TRACKED_USERNAMES {
            windows.retain(|_, window| now - window.started_at < self.window_secs);
        }

        let window = windows.entry(key).or_insert(Window {
            started_at: now,
            attempts: 0,
        });

        if now - window.started_at >= self.window_secs {
            window.started_at = now;
            window.attempts = 0;
        }

        if window.attempts >= self.max_attempts {
            let retry_after = (window.started_at + self.window_secs - now).max(1) as u64;
            return Err(retry_after);
        }

        window.attempts += 1;
        Ok(())
    }

    pub fn clear(&self, username: &str) {
        let mut windows = match self.inner.lock() {
            Ok(windows) => windows,
            Err(poisoned) => poisoned.into_inner(),
        };
        windows.remove(&username.to_lowercase());
    }
}

/// IP клиента для governor. За прокси берём X-Forwarded-For, только если
/// сосед — доверенный прокси: иначе клиент подделает заголовок.
#[derive(Clone, Debug)]
pub struct ClientIpKeyExtractor {
    trusted_proxies: Vec<IpAddr>,
}

impl ClientIpKeyExtractor {
    pub fn new(trusted_proxies: Vec<IpAddr>) -> Self {
        Self { trusted_proxies }
    }
}

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, request: &Request<T>) -> Result<Self::Key, GovernorError> {
        let peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|connect_info| connect_info.0.ip());

        match peer {
            Some(peer) if self.trusted_proxies.contains(&peer) => {
                Ok(forwarded_for(request.headers(), &self.trusted_proxies).unwrap_or(peer))
            }
            Some(peer) => Ok(peer),
            None => Err(GovernorError::UnableToExtractKey),
        }
    }
}

/// Берём первый справа адрес, который не является доверенным прокси:
/// цепочка X-Forwarded-For дописывается прокси слева направо.
fn forwarded_for(headers: &axum::http::HeaderMap, trusted_proxies: &[IpAddr]) -> Option<IpAddr> {
    let value = headers.get(X_FORWARDED_FOR)?.to_str().ok()?;
    let addresses: Vec<IpAddr> = value
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();

    addresses
        .iter()
        .rev()
        .find(|address| !trusted_proxies.contains(address))
        .copied()
        .or_else(|| addresses.first().copied())
}
