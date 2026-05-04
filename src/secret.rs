use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::ops::Deref;
use std::ptr;

#[derive(Clone, Default)]
pub struct SecretString {
    inner: Vec<u8>,
}

impl SecretString {
    pub fn new(s: String) -> Self {
        Self {
            inner: s.into_bytes(),
        }
    }

    pub fn as_str(&self) -> &str {
        // SAFETY: inner is only ever constructed from valid UTF-8 — either
        // via String::into_bytes() in new() or via serde Deserialize from
        // a JSON string — so from_utf8_unchecked is sound.
        unsafe { std::str::from_utf8_unchecked(&self.inner) }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        for byte in &mut self.inner {
            unsafe {
                ptr::write_volatile(byte, 0);
            }
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretString(\"***REDACTED***\")")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "***REDACTED***")
    }
}

impl Deref for SecretString {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Serialize for SecretString {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(SecretString::new(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_display_redacted() {
        let secret = SecretString::new("sk-12345".to_string());
        assert_eq!(format!("{}", secret), "***REDACTED***");
    }

    #[test]
    fn test_secret_debug_redacted() {
        let secret = SecretString::new("sk-12345".to_string());
        assert!(!format!("{:?}", secret).contains("sk-12345"));
        assert!(format!("{:?}", secret).contains("REDACTED"));
    }

    #[test]
    fn test_secret_as_str() {
        let secret = SecretString::new("sk-12345".to_string());
        assert_eq!(secret.as_str(), "sk-12345");
    }

    #[test]
    fn test_secret_deref() {
        let secret = SecretString::new("sk-12345".to_string());
        assert!(secret.starts_with("sk-"));
    }

    #[test]
    fn test_secret_is_empty() {
        assert!(SecretString::new(String::new()).is_empty());
        assert!(!SecretString::new("key".to_string()).is_empty());
    }

    #[test]
    fn test_secret_serde_roundtrip() {
        let secret = SecretString::new("sk-abc".to_string());
        let json = serde_json::to_string(&secret).unwrap();
        assert_eq!(json, "\"sk-abc\"");
        let back: SecretString = serde_json::from_str(&json).unwrap();
        assert_eq!(back.as_str(), "sk-abc");
    }
}
