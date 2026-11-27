//! Поиск тайтла на ещё не привязанных источниках при открытии страницы.
//!
//! Решение 0019: открытие тайтла от имени текущего пользователя ищет его на
//! включённых источниках того же языка, которых нет в привязке, и привязывает
//! точные совпадения. Поиск ленивый, с per-user кэшем и общим лимитом сети.

pub mod service;

pub(crate) use service::find_external_id;
pub use service::{DiscoveryError, DiscoveryOutcome, DiscoveryReport, discover_for_title};
