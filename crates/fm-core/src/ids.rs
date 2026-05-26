use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

const MAX_ID_LEN: usize = 64;

#[derive(Debug, Error)]
pub enum InvalidIdError {
    #[error("id must not be empty")]
    Empty,
    #[error("id exceeds {MAX_ID_LEN} characters (got {0})")]
    TooLong(usize),
    #[error("id contains disallowed character {0:?}; only ASCII alphanumeric and '-' allowed")]
    BadChar(char),
}

/// Stable identifier for a local user profile.
///
/// A `UserId` is more than a string: it is the routing key for the per-user
/// filesystem root, so it MUST be filesystem-safe. Construction validates the
/// invariant: ASCII alphanumeric and hyphen only, 1..=64 chars. Path-traversal
/// characters (`/`, `\`, `.`, `..`, drive letters, null bytes) cannot survive
/// this check.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UserId(String);

impl UserId {
    pub fn new(s: impl Into<String>) -> Result<Self, InvalidIdError> {
        let s = s.into();
        if s.is_empty() {
            return Err(InvalidIdError::Empty);
        }
        if s.len() > MAX_ID_LEN {
            return Err(InvalidIdError::TooLong(s.len()));
        }
        for c in s.chars() {
            if !(c.is_ascii_alphanumeric() || c == '-') {
                return Err(InvalidIdError::BadChar(c));
            }
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for UserId {
    type Err = InvalidIdError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl TryFrom<String> for UserId {
    type Error = InvalidIdError;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::new(s)
    }
}

impl From<UserId> for String {
    fn from(u: UserId) -> Self {
        u.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_alphanumeric_and_hyphen() {
        assert!(UserId::new("user-001").is_ok());
        assert!(UserId::new("asha").is_ok());
        assert!(UserId::new("USER42").is_ok());
        assert!(UserId::new("a").is_ok());
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(UserId::new(""), Err(InvalidIdError::Empty)));
    }

    #[test]
    fn rejects_too_long() {
        let s = "a".repeat(65);
        assert!(matches!(UserId::new(s), Err(InvalidIdError::TooLong(65))));
    }

    #[test]
    fn rejects_path_separators() {
        for bad in ["a/b", "a\\b", "../etc", ".", "..", "user/../other"] {
            assert!(UserId::new(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn rejects_drive_letter_and_null() {
        for bad in ["C:user", "user\0", "user with space"] {
            assert!(UserId::new(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn serde_round_trip() {
        let uid = UserId::new("user-001").unwrap();
        let json = serde_json::to_string(&uid).unwrap();
        assert_eq!(json, r#""user-001""#);
        let back: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(uid, back);
    }

    #[test]
    fn serde_rejects_bad_id() {
        let result: Result<UserId, _> = serde_json::from_str(r#""../escape""#);
        assert!(result.is_err());
    }
}
