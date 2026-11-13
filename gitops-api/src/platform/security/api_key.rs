//! Static API keys for service-to-service bearer authentication.
//!
//! * Keys arrive via the `API_KEYS` env var (a `SealedSecret` in the cluster),
//!   comma-separated so a new key can be added before the old one is removed.
//! * Only SHA-256 digests are kept in memory after parsing; the raw keys are
//!   not stored and never printed (`Debug` shows the count only).
//! * Verification hashes the presented token and compares it to **every**
//!   digest in constant time, so neither timing nor length reveals anything
//!   about a configured key.
//! * An empty set verifies nothing: protected routes fail closed.

use std::fmt;

use sha2::{Digest, Sha256};
use subtle::{Choice, ConstantTimeEq};

pub const MIN_KEY_LEN: usize = 32;
pub const MAX_KEY_LEN: usize = 256;
pub const MAX_KEYS: usize = 8;

/// Why `API_KEYS` was rejected. Never contains key material.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApiKeyError {
    #[error("no keys given")]
    Empty,
    #[error("at most {MAX_KEYS} keys are allowed")]
    TooMany,
    #[error(
        "key #{0} must be {MIN_KEY_LEN}-{MAX_KEY_LEN} printable ASCII characters without spaces or commas"
    )]
    Malformed(usize),
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct ApiKeys {
    digests: Vec<[u8; 32]>,
}

impl ApiKeys {
    /// No keys: every verification fails.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Parse a comma-separated list of keys. Whitespace around each key is
    /// ignored.
    ///
    /// # Errors
    /// Returns [`ApiKeyError`] if the list is empty, too long, or any key is
    /// outside the allowed length / alphabet.
    pub fn parse(raw: &str) -> Result<Self, ApiKeyError> {
        let keys: Vec<&str> = raw.split(',').map(str::trim).collect();
        if keys.iter().all(|k| k.is_empty()) {
            return Err(ApiKeyError::Empty);
        }
        if keys.len() > MAX_KEYS {
            return Err(ApiKeyError::TooMany);
        }
        let digests = keys
            .iter()
            .enumerate()
            .map(|(i, key)| {
                if is_well_formed(key) {
                    Ok(digest(key))
                } else {
                    Err(ApiKeyError::Malformed(i + 1))
                }
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { digests })
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.digests.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.digests.is_empty()
    }

    /// `true` if `presented` matches any configured key. Runs in time that
    /// depends only on the number of configured keys.
    #[must_use]
    pub fn verify(&self, presented: &str) -> bool {
        let presented = digest(presented);
        self.digests
            .iter()
            .fold(Choice::from(0), |matched, key| {
                matched | key.ct_eq(&presented)
            })
            .into()
    }
}

impl fmt::Debug for ApiKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApiKeys")
            .field("count", &self.digests.len())
            .finish_non_exhaustive()
    }
}

fn is_well_formed(key: &str) -> bool {
    (MIN_KEY_LEN..=MAX_KEY_LEN).contains(&key.len())
        && key.bytes().all(|b| b.is_ascii_graphic() && b != b',')
}

fn digest(value: &str) -> [u8; 32] {
    Sha256::digest(value.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_A: &str = "key-a-0123456789abcdef0123456789abcdef";
    const KEY_B: &str = "key-b-0123456789abcdef0123456789abcdef";

    #[test]
    fn verifies_configured_keys_only() {
        let keys = ApiKeys::parse(&format!(" {KEY_A} , {KEY_B} ")).unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.verify(KEY_A));
        assert!(keys.verify(KEY_B));
        assert!(!keys.verify(""));
        assert!(!keys.verify(&KEY_A[..KEY_A.len() - 1]));
        assert!(!keys.verify(&format!("{KEY_A}x")));
        assert!(!keys.verify(&KEY_A.to_uppercase()));
    }

    #[test]
    fn empty_set_verifies_nothing() {
        let keys = ApiKeys::none();
        assert!(keys.is_empty());
        assert!(!keys.verify(""));
        assert!(!keys.verify(KEY_A));
    }

    #[test]
    fn rejects_malformed_lists() {
        assert_eq!(ApiKeys::parse(""), Err(ApiKeyError::Empty));
        assert_eq!(ApiKeys::parse(" , "), Err(ApiKeyError::Empty));
        assert_eq!(ApiKeys::parse("short"), Err(ApiKeyError::Malformed(1)));
        assert_eq!(
            ApiKeys::parse(&format!("{KEY_A},,{KEY_B}")),
            Err(ApiKeyError::Malformed(2))
        );
        assert_eq!(
            ApiKeys::parse(&format!("{KEY_A},has space 0123456789abcdef0123456789")),
            Err(ApiKeyError::Malformed(2))
        );
        assert_eq!(
            ApiKeys::parse(&"x".repeat(MAX_KEY_LEN + 1)),
            Err(ApiKeyError::Malformed(1))
        );
        assert_eq!(
            ApiKeys::parse(&[KEY_A; MAX_KEYS + 1].join(",")),
            Err(ApiKeyError::TooMany)
        );
    }

    #[test]
    fn debug_and_errors_never_show_key_material() {
        let keys = ApiKeys::parse(KEY_A).unwrap();
        let debug = format!("{keys:?}");
        assert!(!debug.contains("key-a"), "{debug}");
        assert!(debug.contains("count: 1"), "{debug}");

        let err = ApiKeys::parse(&format!("{KEY_A},bad")).unwrap_err();
        assert!(!err.to_string().contains("key-a"));
        assert!(!err.to_string().contains("bad"));
    }
}
