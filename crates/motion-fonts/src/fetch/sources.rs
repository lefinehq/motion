//! [`Sources`]: where each family's releases live, as data.

use motion_format::{Diagnostic, Kind, Result};
use serde::Deserialize;

use super::http::Http;
use crate::Licence;

/// The sources that ship with motion.
const BUILTIN: &str = include_str!("../sources.json");

/// Google Fonts' licence folders, in the order a family is looked for.
const GOOGLE_FOLDERS: &[(&str, Licence)] = &[
    ("ofl", Licence::Ofl),
    ("apache", Licence::Apache),
    ("ufl", Licence::Ubuntu),
];

/// Google Fonts' repository.
pub(crate) const GOOGLE_FONTS: &str = "google/fonts";

/// Where the releases of families live: a family's own repository, whose
/// tags are its releases, and Google Fonts' folder for it, whose commits
/// are.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    /// Notes for people who edit the file.
    #[serde(default)]
    pub comment: String,
    /// The families with sources of their own.
    pub families: Vec<Family>,
}

/// One family's sources.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Family {
    /// What its PostScript names start with, before the `-`: `Montserrat`
    /// for `Montserrat-ExtraBold`.
    pub prefix: String,
    /// Its sources, the one to prefer first.
    pub sources: Vec<Source>,
}

/// A place a family's releases live.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Source {
    /// A GitHub repository whose tags are releases.
    GitHub {
        /// `owner/name`.
        github: String,
        /// The licence its fonts are under.
        licence: Licence,
    },
    /// A folder of Google Fonts' repository, such as `ofl/lato`, whose
    /// commits are releases. Its first segment names the licence.
    Google {
        /// The folder.
        google: String,
    },
}

impl Source {
    /// The licence its fonts are under, or none for a Google Fonts folder
    /// outside the open licences' folders.
    pub fn licence(&self) -> Option<Licence> {
        match self {
            Source::GitHub { licence, .. } => Some(*licence),
            Source::Google { google } => {
                let first = google.split('/').next()?;
                GOOGLE_FOLDERS
                    .iter()
                    .find(|(folder, _)| *folder == first)
                    .map(|&(_, licence)| licence)
            }
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::GitHub { github, .. } => write!(f, "github.com/{github}"),
            Source::Google { google } => write!(f, "Google Fonts {google}"),
        }
    }
}

impl Sources {
    /// The sources that ship with motion.
    pub fn builtin() -> Self {
        Self::parse(BUILTIN).expect("the built-in sources are valid")
    }

    /// Parses sources from JSON.
    ///
    /// # Errors
    /// A diagnostic for JSON that is not sources, or a Google Fonts folder
    /// outside the open licences' folders.
    pub fn parse(text: &str) -> Result<Self> {
        let sources: Self = serde_json::from_str(text)
            .map_err(|e| Diagnostic::new(Kind::Invalid, format!("not font sources: {e}")))?;
        for family in &sources.families {
            if let Some(source) = family.sources.iter().find(|s| s.licence().is_none()) {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!(
                        "`{}`: {source:?} is not in an open licence's folder ({})",
                        family.prefix,
                        GOOGLE_FOLDERS
                            .iter()
                            .map(|(f, _)| *f)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                ));
            }
        }
        Ok(sources)
    }

    /// The family a PostScript name belongs to: the part before its `-`,
    /// without spaces.
    pub fn prefix(postscript: &str) -> String {
        let prefix = postscript
            .split_once('-')
            .map_or(postscript, |(prefix, _)| prefix);
        prefix.replace(' ', "")
    }

    /// The sources of the family `prefix`: those listed, else the Google
    /// Fonts folder that holds it, else none.
    pub(crate) fn of(&self, prefix: &str, http: &Http) -> Result<Vec<Source>> {
        if let Some(family) = self.families.iter().find(|f| f.prefix == prefix) {
            return Ok(family.sources.clone());
        }
        let folder = prefix.to_lowercase();
        for (licence, _) in GOOGLE_FOLDERS {
            let url = format!(
                "https://raw.githubusercontent.com/{GOOGLE_FONTS}/main/{licence}/{folder}/METADATA.pb"
            );
            if http.get(&url)?.is_some() {
                return Ok(vec![Source::Google {
                    google: format!("{licence}/{folder}"),
                }]);
            }
        }
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builtin_sources_are_valid() {
        let sources = Sources::builtin();
        assert!(sources.families.iter().any(|f| f.prefix == "Montserrat"));
        for family in &sources.families {
            assert!(!family.sources.is_empty(), "{}", family.prefix);
        }
    }

    #[test]
    fn a_folder_names_its_licence() {
        let google = |folder: &str| Source::Google {
            google: folder.into(),
        };
        assert_eq!(google("ofl/lato").licence(), Some(Licence::Ofl));
        assert_eq!(google("apache/roboto").licence(), Some(Licence::Apache));
        assert_eq!(google("ufl/ubuntu").licence(), Some(Licence::Ubuntu));
        assert_eq!(google("other/x").licence(), None);
        let bad = r#"{"families":[{"prefix":"X","sources":[{"google":"other/x"}]}]}"#;
        assert!(Sources::parse(bad).is_err());
    }

    #[test]
    fn a_postscript_name_names_its_family() {
        assert_eq!(Sources::prefix("Montserrat-ExtraBold"), "Montserrat");
        assert_eq!(Sources::prefix("RobotoMono-Bold"), "RobotoMono");
        assert_eq!(Sources::prefix("Lato"), "Lato");
        assert_eq!(Sources::prefix("Open Sans"), "OpenSans");
    }
}
