//! AEAD-обёртка учётных данных источника.
//!
//! Схема: ChaCha20-Poly1305, случайный 12-байтный nonce, AAD
//! `user_id:source_id:key_version`. AAD привязывает шифротекст к владельцу и
//! источнику: строку нельзя перенести другому пользователю или источнику.
//! Ключ живёт только в env (`SOURCE_CREDENTIALS_KEY`, 32 байта base64) и в
//! памяти `AppState`; в БД и логи не попадает.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use zeroize::Zeroizing;

use crate::{error::AppError, source_auth::SourceAuth};

/// Длина ключа ChaCha20-Poly1305.
pub const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
/// Версия схемы шифрования. Сейчас одна; поле в БД зарезервировано под
/// ротацию ключа.
pub const KEY_VERSION: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("SOURCE_CREDENTIALS_KEY must be base64")]
    Base64,
    #[error("SOURCE_CREDENTIALS_KEY must decode to 32 bytes")]
    Length,
}

/// Разбирает ключ из env на этапе конфига: ошибка формата — отказ старта,
/// а не тихо отключённая функция.
pub fn decode_key(raw: &str) -> Result<[u8; KEY_LEN], KeyError> {
    let decoded = STANDARD.decode(raw.trim()).map_err(|_| KeyError::Base64)?;
    decoded.try_into().map_err(|_| KeyError::Length)
}

/// Проверенный ключ из env. Отдельный тип фиксирует инвариант: `Config`
/// содержит только успешно разобранный ключ, поэтому `AppState` не решает,
/// что делать с битым.
#[derive(Clone)]
pub struct SourceCredentialsKey([u8; KEY_LEN]);

impl SourceCredentialsKey {
    pub fn from_base64(raw: &str) -> Result<Self, KeyError> {
        decode_key(raw).map(Self)
    }

    pub fn cipher(&self) -> CredentialCipher {
        CredentialCipher::new(self.0)
    }
}

// Ключ не должен появиться в логах через Debug.
impl std::fmt::Debug for SourceCredentialsKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SourceCredentialsKey([REDACTED])")
    }
}

/// Шифротекст вместе с nonce и версией ключа: ровно то, что пишется в БД.
pub struct SealedCredential {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
    pub key_version: i64,
}

#[derive(Clone)]
pub struct CredentialCipher {
    key: [u8; KEY_LEN],
    key_version: i64,
}

// Ключ не должен появиться в логах через Debug.
impl std::fmt::Debug for CredentialCipher {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialCipher")
            .field("key_version", &self.key_version)
            .finish_non_exhaustive()
    }
}

impl CredentialCipher {
    pub fn new(key: [u8; KEY_LEN]) -> Self {
        Self {
            key,
            key_version: KEY_VERSION,
        }
    }

    pub fn key_version(&self) -> i64 {
        self.key_version
    }

    /// Шифрует пару учётных данных для конкретного пользователя и источника.
    pub fn seal(
        &self,
        user_id: i64,
        source_id: i64,
        auth: &SourceAuth,
    ) -> Result<SealedCredential, AppError> {
        let plaintext = Zeroizing::new(auth.to_payload_json().map_err(|error| {
            tracing::error!(%error, "cannot serialize source credential");
            AppError::Internal
        })?);

        let key = chacha20poly1305::Key::from(self.key);
        let cipher = ChaCha20Poly1305::new(&key);

        let mut nonce_bytes = [0_u8; NONCE_LEN];
        getrandom::fill(&mut nonce_bytes).map_err(|_| AppError::Random)?;

        let aad = aad(user_id, source_id, self.key_version);
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: plaintext.as_slice(),
                    aad: &aad,
                },
            )
            .map_err(|_| {
                tracing::error!("cannot encrypt source credential");
                AppError::Internal
            })?;

        Ok(SealedCredential {
            ciphertext,
            nonce: nonce_bytes.to_vec(),
            key_version: self.key_version,
        })
    }

    /// Расшифровывает строку из БД. Неверный ключ, версия или AAD дают
    /// `credentials_unavailable`: это ошибка конфигурации сервера, а не
    /// «учётка не подключена».
    pub fn open(
        &self,
        user_id: i64,
        source_id: i64,
        key_version: i64,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<SourceAuth, AppError> {
        if key_version != self.key_version || nonce.len() != NONCE_LEN {
            tracing::warn!(
                user_id,
                source_id,
                key_version,
                "stored source credential uses an unsupported key version or nonce"
            );
            return Err(credentials_unavailable());
        }

        let key = chacha20poly1305::Key::from(self.key);
        let cipher = ChaCha20Poly1305::new(&key);
        let aad = aad(user_id, source_id, key_version);
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| {
                tracing::warn!(
                    user_id,
                    source_id,
                    "cannot decrypt source credential: wrong key or tampered row"
                );
                credentials_unavailable()
            })?;

        let plaintext = Zeroizing::new(plaintext);
        SourceAuth::from_payload_json(&plaintext).map_err(|error| {
            tracing::error!(%error, user_id, source_id, "stored source credential is malformed");
            credentials_unavailable()
        })
    }
}

fn aad(user_id: i64, source_id: i64, key_version: i64) -> Vec<u8> {
    format!("{user_id}:{source_id}:{key_version}").into_bytes()
}

pub fn credentials_unavailable() -> AppError {
    AppError::service_unavailable(
        "credentials_unavailable",
        "source credentials are not available on this server",
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_auth::SourceAuth;

    fn cipher() -> CredentialCipher {
        CredentialCipher::new([7_u8; KEY_LEN])
    }

    fn auth() -> SourceAuth {
        SourceAuth::new(
            Some("bearer-token".to_owned()),
            Some("sid=abc; theme=dark".to_owned()),
        )
    }

    #[test]
    fn round_trip_preserves_fields() {
        let sealed = cipher().seal(1, 2, &auth()).expect("seal");
        assert_ne!(sealed.ciphertext, b"bearer-token");
        let opened = cipher()
            .open(1, 2, sealed.key_version, &sealed.nonce, &sealed.ciphertext)
            .expect("open");
        assert_eq!(opened.bearer(), Some("bearer-token"));
        assert_eq!(opened.cookie(), Some("sid=abc; theme=dark"));
    }

    #[test]
    fn wrong_key_is_a_server_configuration_error() {
        let sealed = cipher().seal(1, 2, &auth()).expect("seal");
        let other = CredentialCipher::new([9_u8; KEY_LEN]);
        let error = other
            .open(1, 2, sealed.key_version, &sealed.nonce, &sealed.ciphertext)
            .expect_err("wrong key must fail");
        assert!(matches!(
            error,
            AppError::ServiceUnavailable {
                code: "credentials_unavailable",
                ..
            }
        ));
    }

    #[test]
    fn aad_binds_credential_to_user_and_source() {
        let sealed = cipher().seal(1, 2, &auth()).expect("seal");
        let cipher = cipher();
        assert!(
            cipher
                .open(3, 2, sealed.key_version, &sealed.nonce, &sealed.ciphertext)
                .is_err(),
            "another user must not decrypt the credential"
        );
        assert!(
            cipher
                .open(1, 9, sealed.key_version, &sealed.nonce, &sealed.ciphertext)
                .is_err(),
            "another source must not decrypt the credential"
        );
    }

    #[test]
    fn empty_fields_normalize_to_none() {
        let auth = SourceAuth::new(Some("   ".to_owned()), None);
        assert!(auth.is_empty());
        let sealed = cipher().seal(1, 2, &auth).expect("seal");
        let opened = cipher()
            .open(1, 2, sealed.key_version, &sealed.nonce, &sealed.ciphertext)
            .expect("open");
        assert!(opened.is_empty());
    }

    #[test]
    fn key_must_be_32_base64_bytes() {
        assert!(decode_key("not base64!").is_err());
        assert!(decode_key("c2hvcnQ=").is_err());
        let valid = STANDARD.encode([1_u8; KEY_LEN]);
        assert!(decode_key(&valid).is_ok());
    }
}
