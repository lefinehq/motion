//! [`Lock`]: the exact font files a scene is set in.

use std::fs;
use std::path::Path;

use motion_format::{Diagnostic, Kind, Result};
use serde::{Deserialize, Serialize};

use crate::Sha256;

/// The version of the lock format this crate reads and writes.
pub const LOCK_VERSION: u32 = 1;

/// The exact font files a scene is set in, one entry per face it uses, as
/// `motion fonts fetch` resolved them. A scene names its lock in `fonts`,
/// and a render loads each file from the [`Cache`](crate::Cache) by its
/// hash, so the same lock always draws with the same bytes ([D39]).
///
/// [D39]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d39-fonts-are-resolved-before-a-render
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    /// The lock format's version, [`LOCK_VERSION`].
    pub lock: u32,
    /// The faces, ordered by PostScript name.
    pub fonts: Vec<LockedFont>,
}

/// One face of a [`Lock`] and the file it comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedFont {
    /// The face's PostScript name, as scenes ask for it: `Montserrat-ExtraBold`.
    pub postscript: String,
    /// Its family.
    pub family: String,
    /// The version its file names, such as `Version 3.100`.
    pub version: String,
    /// The licence the file is under.
    pub licence: Licence,
    /// Where the file was fetched from.
    pub source: Origin,
    /// The file's hash, under which the cache keeps it.
    pub sha256: Sha256,
    /// The file's size in bytes.
    pub size: u64,
}

/// Where a locked file comes from: a path at an exact ref of a repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    /// The GitHub repository, `owner/name`.
    pub repository: String,
    /// A tag or commit of it.
    #[serde(rename = "ref")]
    pub reference: String,
    /// The file's path in it.
    pub path: String,
}

/// The licences fonts are fetched under: open ones only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Licence {
    /// The SIL Open Font License 1.1.
    #[serde(rename = "OFL-1.1")]
    Ofl,
    /// The Apache License 2.0.
    #[serde(rename = "Apache-2.0")]
    Apache,
    /// The Ubuntu Font Licence 1.0.
    #[serde(rename = "UFL-1.0")]
    Ubuntu,
}

impl Lock {
    /// A lock of `fonts`, put in order.
    pub fn new(mut fonts: Vec<LockedFont>) -> Self {
        fonts.sort_by(|a, b| a.postscript.cmp(&b.postscript));
        fonts.dedup_by(|a, b| a.postscript == b.postscript);
        Self {
            lock: LOCK_VERSION,
            fonts,
        }
    }

    /// Reads the lock at `path`.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be read, or one for a
    /// file that is not a lock this version reads.
    pub fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|e| Diagnostic::io("read", path, &e))?;
        Self::parse(&text).map_err(|e| e.under(path.display().to_string()))
    }

    /// Parses a lock.
    ///
    /// # Errors
    /// A diagnostic for text that is not a lock this version reads.
    pub fn parse(text: &str) -> Result<Self> {
        let lock: Self = serde_json::from_str(text)
            .map_err(|e| Diagnostic::new(Kind::Invalid, format!("not a font lock: {e}")))?;
        if lock.lock != LOCK_VERSION {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "font lock version {} is not one this motion reads ({LOCK_VERSION})",
                    lock.lock
                ),
            ));
        }
        Ok(lock)
    }

    /// The lock as JSON, ending with a newline: entries in order, so a
    /// change reads well in review.
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("a lock is plain data");
        text.push('\n');
        text
    }

    /// Writes the lock to `path`.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when the file cannot be written.
    pub fn write(&self, path: &Path) -> Result<()> {
        fs::write(path, self.to_json()).map_err(|e| Diagnostic::io("write", path, &e))
    }

    /// The entry for the face `postscript`.
    pub fn get(&self, postscript: &str) -> Option<&LockedFont> {
        self.fonts.iter().find(|font| font.postscript == postscript)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn font(postscript: &str) -> LockedFont {
        LockedFont {
            postscript: postscript.into(),
            family: "Montserrat".into(),
            version: "Version 3.100".into(),
            licence: Licence::Ofl,
            source: Origin {
                repository: "JulietaUla/Montserrat".into(),
                reference: "v3.100".into(),
                path: format!("otf/{postscript}.otf"),
            },
            sha256: Sha256::of(postscript.as_bytes()),
            size: 53144,
        }
    }

    #[test]
    fn keeps_its_entries_in_order() {
        let lock = Lock::new(vec![
            font("Montserrat-Medium"),
            font("Montserrat-ExtraBold"),
            font("Montserrat-Medium"),
        ]);
        let names: Vec<_> = lock.fonts.iter().map(|f| f.postscript.as_str()).collect();
        assert_eq!(names, ["Montserrat-ExtraBold", "Montserrat-Medium"]);
    }

    #[test]
    fn reads_what_it_writes() {
        let lock = Lock::new(vec![font("Montserrat-ExtraBold")]);
        assert_eq!(Lock::parse(&lock.to_json()).unwrap(), lock);
        assert!(lock.to_json().contains(r#""ref": "v3.100""#));
        assert!(lock.to_json().contains(r#""licence": "OFL-1.1""#));
    }

    #[test]
    fn rejects_another_version_and_unknown_keys() {
        let lock = Lock::new(vec![font("Montserrat-ExtraBold")]).to_json();
        let later = lock.replace(r#""lock": 1"#, r#""lock": 2"#);
        assert!(
            Lock::parse(&later)
                .unwrap_err()
                .message
                .contains("version 2")
        );
        let extra = lock.replace(r#""lock": 1"#, r#""lock": 1, "url": "x""#);
        assert!(Lock::parse(&extra).is_err());
    }
}
