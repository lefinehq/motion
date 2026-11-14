//! The releases a [`Source`] holds, and the files in them that may be the
//! faces asked for.

use std::collections::BTreeMap;

use base64::Engine;
use motion_format::{Diagnostic, Kind, Result};
use serde::Deserialize;

use super::http::Http;
use super::sources::{GOOGLE_FONTS, Source};
use crate::{Licence, Origin, Sha256};

/// The most commits of one Google Fonts folder listed, a hundred a page.
const MAX_COMMIT_PAGES: usize = 10;

/// One release of a family: a tag of its repository, or a commit of Google
/// Fonts that touched its folder.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Release {
    /// The repository, `owner/name`.
    pub repository: String,
    /// The tag (as jsDelivr names it) or commit.
    pub reference: String,
    /// How the release is shown: the repository and the tag, or the date
    /// and the commit.
    pub label: String,
    /// The licence its fonts are under.
    pub licence: Licence,
    /// Where its font files are listed, read when the release is.
    pub listing: Listing,
}

/// Where a release's font files are listed.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Listing {
    /// jsDelivr's listing of a tag, with hashes, else GitHub's tree of it.
    Tag,
    /// The `METADATA.pb` of a Google Fonts folder at the commit.
    Metadata {
        /// The folder.
        folder: String,
    },
}

/// A font file of a release.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RemoteFile {
    /// Its path in the repository.
    pub path: String,
    /// Its hash, where the listing gives one.
    pub sha256: Option<Sha256>,
    /// The PostScript name the listing gives for it, if any; a variable
    /// font's listing names only its first instance.
    pub postscript: Option<String>,
}

impl RemoteFile {
    /// Its name without folders or extension.
    fn stem(&self) -> &str {
        let name = self.path.rsplit('/').next().unwrap_or(&self.path);
        name.rsplit_once('.').map_or(name, |(stem, _)| stem)
    }

    /// Its folder and extension: files that share them are one flavour of
    /// a release, such as its OTFs or its hinted TTFs.
    fn flavour(&self) -> (String, String) {
        let (folder, name) = self.path.rsplit_once('/').unwrap_or(("", &self.path));
        let extension = name.rsplit_once('.').map_or("", |(_, e)| e);
        (folder.to_owned(), extension.to_ascii_lowercase())
    }

    /// Whether it looks like a variable font, which may hold many faces.
    fn is_variable(&self) -> bool {
        let stem = self.stem();
        stem.contains('[') || stem.ends_with("VF") || stem.contains("Variable")
    }

    /// Whether it may hold the face `postscript`.
    fn may_hold(&self, postscript: &str) -> bool {
        if !self.is_variable() {
            return self.stem() == postscript || self.postscript.as_deref() == Some(postscript);
        }
        let italic = |name: &str| name.contains("Italic");
        italic(self.stem()) == italic(postscript)
    }
}

/// Whether `path` is a font file a render can read.
fn is_font(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".ttf") || lower.ends_with(".otf")
}

impl Release {
    /// Its font files, from its listing.
    pub(crate) fn files(&self, http: &Http) -> Result<Vec<RemoteFile>> {
        match &self.listing {
            Listing::Tag => tag_files(http, &self.repository, &self.reference),
            Listing::Metadata { folder } => metadata(http, folder, &self.reference),
        }
    }

    /// The flavours of `files` that may hold some of `wanted`, each as the
    /// file that may hold each face; static files before variable ones.
    pub(crate) fn candidates(
        files: &[RemoteFile],
        wanted: &[String],
    ) -> Vec<Vec<(String, RemoteFile)>> {
        let mut flavours: BTreeMap<(String, String), Vec<&RemoteFile>> = BTreeMap::new();
        for file in files {
            flavours.entry(file.flavour()).or_default().push(file);
        }
        flavours
            .into_values()
            .filter_map(|files| {
                let picked: Vec<(String, RemoteFile)> = wanted
                    .iter()
                    .filter_map(|name| {
                        let statics = files.iter().filter(|f| !f.is_variable());
                        let variables = files.iter().filter(|f| f.is_variable());
                        statics
                            .chain(variables)
                            .find(|f| f.may_hold(name))
                            .map(|f| (name.clone(), (*f).clone()))
                    })
                    .collect();
                (!picked.is_empty()).then_some(picked)
            })
            .collect()
    }
}

/// Where a file of `origin` is downloaded from: GitHub's raw files, at the
/// tag or commit. They are not counted against the API's limit, and their
/// entity tags follow the content, so a file unchanged between releases is
/// downloaded once.
pub(crate) fn url(origin: &Origin) -> String {
    format!(
        "https://raw.githubusercontent.com/{}/{}/{}",
        origin.repository,
        origin.reference,
        encode_path(&origin.path)
    )
}

/// `path` with every byte outside the unreserved set and `/` escaped, as
/// the brackets of `Montserrat[wght].ttf` must be.
fn encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(char::from(byte));
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The releases `source` holds, newest first.
pub(crate) fn releases(http: &Http, source: &Source) -> Result<Vec<Release>> {
    let licence = source.licence().ok_or_else(|| {
        Diagnostic::new(
            Kind::Invalid,
            format!("{source:?} is not under an open licence"),
        )
    })?;
    match source {
        Source::GitHub { github, .. } => tags(http, github, licence),
        Source::Google { google } => commits(http, google, licence),
    }
}

/// A repository's tags, from GitHub, newest first.
fn tags(http: &Http, repository: &str, licence: Licence) -> Result<Vec<Release>> {
    #[derive(Deserialize)]
    struct Tag {
        name: String,
    }
    let path = format!("repos/{repository}/tags?per_page=100");
    let Some(value) = http.github(&path)? else {
        return Ok(Vec::new());
    };
    let tags: Vec<Tag> = from_value(&path, value)?;
    Ok(tags
        .into_iter()
        .map(|tag| Release {
            repository: repository.to_owned(),
            label: format!("{repository} {}", tag.name),
            reference: tag.name,
            licence,
            listing: Listing::Tag,
        })
        .collect())
}

/// The font files of a tag: listed with their hashes by jsDelivr, or by
/// GitHub's tree of the tag where jsDelivr will not list it (a repository
/// over its size limit).
fn tag_files(http: &Http, repository: &str, tag: &str) -> Result<Vec<RemoteFile>> {
    #[derive(Deserialize)]
    struct Files {
        files: Vec<Listed>,
    }
    #[derive(Deserialize)]
    struct Listed {
        name: String,
        hash: String,
    }
    #[derive(Deserialize)]
    struct Tree {
        tree: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Entry {
        path: String,
        #[serde(rename = "type")]
        kind: String,
    }
    let url = format!("https://data.jsdelivr.com/v1/packages/gh/{repository}@{tag}?structure=flat");
    if let Some(body) = http.listing(&url)? {
        return Ok(json::<Files>(&url, &body)?
            .files
            .into_iter()
            .filter(|file| is_font(&file.name))
            .map(|file| RemoteFile {
                path: file.name.trim_start_matches('/').to_owned(),
                sha256: base64::engine::general_purpose::STANDARD
                    .decode(&file.hash)
                    .ok()
                    .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                    .map(Sha256::from_bytes),
                postscript: None,
            })
            .collect());
    }
    let path = format!("repos/{repository}/git/trees/{tag}?recursive=1");
    let Some(value) = http.github(&path)? else {
        return Ok(Vec::new());
    };
    Ok(from_value::<Tree>(&path, value)?
        .tree
        .into_iter()
        .filter(|entry| entry.kind == "blob" && is_font(&entry.path))
        .map(|entry| RemoteFile {
            path: entry.path,
            sha256: None,
            postscript: None,
        })
        .collect())
}

fn from_value<T: for<'de> Deserialize<'de>>(path: &str, value: serde_json::Value) -> Result<T> {
    serde_json::from_value(value)
        .map_err(|e| Diagnostic::new(Kind::Invalid, format!("GitHub's {path}: {e}")))
}

/// The commits of Google Fonts that touched `folder`, newest first.
fn commits(http: &Http, folder: &str, licence: Licence) -> Result<Vec<Release>> {
    #[derive(Deserialize)]
    struct Commit {
        sha: String,
        commit: Details,
    }
    #[derive(Deserialize)]
    struct Details {
        committer: Person,
    }
    #[derive(Deserialize)]
    struct Person {
        date: String,
    }
    let mut commits: Vec<Commit> = Vec::new();
    for page in 1..=MAX_COMMIT_PAGES {
        let path = format!("repos/{GOOGLE_FONTS}/commits?path={folder}&per_page=100&page={page}");
        let Some(value) = http.github(&path)? else {
            break;
        };
        let found: Vec<Commit> = from_value(&path, value)?;
        let full = found.len() == 100;
        commits.extend(found);
        if !full {
            break;
        }
    }
    Ok(commits
        .into_iter()
        .map(|commit| {
            let date = commit
                .commit
                .committer
                .date
                .get(..10)
                .unwrap_or_default()
                .to_owned();
            Release {
                repository: GOOGLE_FONTS.to_owned(),
                label: format!(
                    "{GOOGLE_FONTS} {date} {}",
                    commit.sha.get(..7).unwrap_or(&commit.sha)
                ),
                reference: commit.sha,
                licence,
                listing: Listing::Metadata {
                    folder: folder.to_owned(),
                },
            }
        })
        .collect())
}

/// The font files the `METADATA.pb` of Google Fonts' `folder` lists at
/// `commit`; none where the folder is absent.
fn metadata(http: &Http, folder: &str, commit: &str) -> Result<Vec<RemoteFile>> {
    let url =
        format!("https://raw.githubusercontent.com/{GOOGLE_FONTS}/{commit}/{folder}/METADATA.pb");
    let Some(metadata) = http.get_immutable(&url)? else {
        return Ok(Vec::new());
    };
    Ok(metadata_files(&String::from_utf8_lossy(&metadata))
        .into_iter()
        .filter(|(filename, _)| is_font(filename))
        .map(|(filename, postscript)| RemoteFile {
            path: format!("{folder}/{filename}"),
            sha256: None,
            postscript,
        })
        .collect())
}

/// The `filename` and `post_script_name` of every `fonts` block of a
/// Google Fonts `METADATA.pb` (protobuf text format).
fn metadata_files(text: &str) -> Vec<(String, Option<String>)> {
    let mut files = Vec::new();
    let mut depth = 0usize;
    let mut current: Option<(Option<String>, Option<String>)> = None;
    for line in text.lines().map(str::trim) {
        if line.ends_with('{') {
            depth += 1;
            if depth == 1 && line.trim_end_matches('{').trim() == "fonts" {
                current = Some((None, None));
            }
            continue;
        }
        if line == "}" {
            if depth == 1
                && let Some((Some(filename), postscript)) = current.take()
            {
                files.push((filename, postscript));
            }
            depth = depth.saturating_sub(1);
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_owned();
        if let Some((filename, postscript)) = current.as_mut().filter(|_| depth == 1) {
            match key.trim() {
                "filename" => *filename = Some(value),
                "post_script_name" => *postscript = Some(value),
                _ => {}
            }
        }
    }
    files
}

fn json<T: for<'de> Deserialize<'de>>(url: &str, body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|e| {
        Diagnostic::new(
            Kind::Invalid,
            format!("{url} is not what it should be: {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> RemoteFile {
        RemoteFile {
            path: path.into(),
            sha256: None,
            postscript: None,
        }
    }

    #[test]
    fn reads_the_files_of_a_metadata_file() {
        let text = r#"name: "Montserrat"
fonts {
  name: "Montserrat"
  filename: "Montserrat[wght].ttf"
  post_script_name: "Montserrat-Thin"
}
fonts {
  filename: "Montserrat-Italic[wght].ttf"
  post_script_name: "Montserrat-ThinItalic"
}
axes {
  tag: "wght"
}
source {
  files {
    filename: "not-a-font"
  }
}
"#;
        assert_eq!(
            metadata_files(text),
            [
                (
                    "Montserrat[wght].ttf".into(),
                    Some("Montserrat-Thin".into())
                ),
                (
                    "Montserrat-Italic[wght].ttf".into(),
                    Some("Montserrat-ThinItalic".into())
                ),
            ]
        );
    }

    #[test]
    fn a_flavour_offers_a_file_for_each_face() {
        let files = [
            file("otf/Montserrat-ExtraBold.otf"),
            file("otf/Montserrat-Medium.otf"),
            file("ttf/Montserrat-ExtraBold.ttf"),
            file("variable/Montserrat[wght].ttf"),
            file("variable/Montserrat-Italic[wght].ttf"),
        ];
        let wanted = [
            "Montserrat-ExtraBold".to_owned(),
            "Montserrat-MediumItalic".to_owned(),
        ];
        let paths: Vec<Vec<String>> = Release::candidates(&files, &wanted)
            .into_iter()
            .map(|c| c.into_iter().map(|(_, f)| f.path).collect())
            .collect();
        assert_eq!(
            paths,
            [
                vec!["otf/Montserrat-ExtraBold.otf".to_owned()],
                vec!["ttf/Montserrat-ExtraBold.ttf".to_owned()],
                vec![
                    "variable/Montserrat[wght].ttf".to_owned(),
                    "variable/Montserrat-Italic[wght].ttf".to_owned()
                ],
            ]
        );
    }

    #[test]
    fn urls_escape_what_paths_may_hold() {
        let origin = |repository: &str, reference: &str, path: &str| Origin {
            repository: repository.into(),
            reference: reference.into(),
            path: path.into(),
        };
        assert_eq!(
            url(&origin(
                "google/fonts",
                "abc",
                "ofl/montserrat/Montserrat[wght].ttf"
            )),
            "https://raw.githubusercontent.com/google/fonts/abc/ofl/montserrat/Montserrat%5Bwght%5D.ttf"
        );
        assert_eq!(
            url(&origin(
                "JulietaUla/Montserrat",
                "v3.100",
                "otf/Montserrat-ExtraBold.otf"
            )),
            "https://raw.githubusercontent.com/JulietaUla/Montserrat/v3.100/otf/Montserrat-ExtraBold.otf"
        );
    }
}
