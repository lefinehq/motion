//! [`Sha256`]: the content address of a font file.

use std::fmt;
use std::str::FromStr;

use motion_format::{Diagnostic, Kind};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::Digest;

/// The SHA-256 of a file's bytes: what a lock pins and where the cache keeps
/// it. Written as 64 lowercase hexadecimal digits.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sha256([u8; 32]);

impl Sha256 {
    /// The hash of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(sha2::Sha256::digest(bytes).into())
    }

    /// The hash with these 32 bytes.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Its 32 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|byte| write!(f, "{byte:02x}"))
    }
}

impl fmt::Debug for Sha256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sha256({self})")
    }
}

impl FromStr for Sha256 {
    type Err = Diagnostic;

    fn from_str(text: &str) -> Result<Self, Diagnostic> {
        let invalid = || {
            Diagnostic::new(
                Kind::Invalid,
                format!("`{text}` is not a SHA-256: it takes 64 hexadecimal digits"),
            )
        };
        if text.len() != 64 || !text.is_ascii() {
            return Err(invalid());
        }
        let mut bytes = [0; 32];
        for (byte, pair) in bytes.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
            let pair = std::str::from_utf8(pair).map_err(|_| invalid())?;
            *byte = u8::from_str_radix(pair, 16).map_err(|_| invalid())?;
        }
        Ok(Self(bytes))
    }
}

impl Serialize for Sha256 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Sha256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse()
            .map_err(|e: Diagnostic| serde::de::Error::custom(e.message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_as_the_standard_says() {
        // FIPS 180-2, appendix B.1.
        assert_eq!(
            Sha256::of(b"abc").to_string(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn reads_what_it_writes() {
        let hash = Sha256::of(b"motion");
        assert_eq!(hash.to_string().parse::<Sha256>().unwrap(), hash);
    }

    #[test]
    fn rejects_what_is_not_a_hash() {
        for text in ["", "ab", &"g".repeat(64), &"é".repeat(32)] {
            assert!(text.parse::<Sha256>().is_err(), "{text}");
        }
    }
}
