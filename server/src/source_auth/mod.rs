//! Учётки источников на пользователя: шифрование, сервис и HTTP.
//!
//! Секреты не покидают сервер: API отдаёт только состояние (`configured`,
//! `last_verified_at`, `last_error`). Адаптеры получают уже расшифрованный
//! [`SourceAuth`] и не знают о БД.

mod auth;
pub mod crypto;
pub mod handlers;
pub mod service;

pub use auth::SourceAuth;
pub use crypto::{CredentialCipher, KeyError, SourceCredentialsKey};
