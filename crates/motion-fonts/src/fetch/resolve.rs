//! [`Fetcher`]: from the faces a scene asks for to a [`Lock`].

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;

use motion_format::{Diagnostic, Kind, Result};
use motion_text::FontSet;
use serde::Serialize;

use super::http::Http;
use super::releases::{self, Release, RemoteFile};
use super::sources::{Source, Sources};
use crate::{Cache, FontFile, Judge, Lock, LockedFont, Origin, Score, Sha256};

/// What to resolve.
pub struct Request<'a> {
    /// The PostScript names of the faces wanted.
    pub fonts: Vec<String>,
    /// Evidence of the files a project was made with; without it, each
    /// family takes the newest release that has its faces.
    pub judge: Option<&'a mut dyn Judge>,
    /// Only releases whose tag or commit starts with this, if given.
    pub release: Option<String>,
}

/// What a resolution found.
#[derive(Debug, Clone)]
pub struct Resolution {
    /// The faces found and their files.
    pub lock: Lock,
    /// The release each family takes, and why.
    pub choices: Vec<Choice>,
    /// The faces no open source has.
    pub unresolved: Vec<Unresolved>,
}

/// The release one family takes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choice {
    /// The family, by the prefix of its PostScript names.
    pub family: String,
    /// The release, as its source shows it.
    pub release: String,
    /// The candidates (a flavour of a release with the faces) considered.
    pub candidates: usize,
    /// Its score against the evidence, where there was evidence.
    pub score: Option<Score>,
}

/// A face that was not found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Unresolved {
    /// Its PostScript name.
    pub postscript: String,
    /// Why.
    pub why: String,
}

/// Finds font files and keeps them in a [`Cache`].
pub struct Fetcher {
    http: Http,
    cache: Cache,
    sources: Sources,
    progress: Box<dyn FnMut(&str)>,
}

/// A face found in a downloaded file.
#[derive(Clone)]
struct Found {
    file: RemoteFile,
    sha256: Sha256,
    bytes: Arc<Vec<u8>>,
    family: String,
    version: String,
}

/// A flavour of a release, downloaded: the faces it holds.
#[derive(Clone)]
struct Candidate {
    release: Arc<Release>,
    faces: BTreeMap<String, Found>,
}

impl Candidate {
    /// Its folder and extension, as the report shows it.
    fn flavour(&self) -> String {
        let path = self
            .faces
            .values()
            .next()
            .map_or("", |f| f.file.path.as_str());
        let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
        let extension = name.rsplit_once('.').map_or("", |(_, e)| e);
        match folder.is_empty() {
            true => extension.to_owned(),
            false => format!("{folder}/*.{extension}"),
        }
    }

    fn locked(&self) -> Vec<LockedFont> {
        self.faces
            .iter()
            .map(|(postscript, found)| LockedFont {
                postscript: postscript.clone(),
                family: found.family.clone(),
                version: found.version.clone(),
                licence: self.release.licence,
                source: Origin {
                    repository: self.release.repository.clone(),
                    reference: self.release.reference.clone(),
                    path: found.file.path.clone(),
                },
                sha256: found.sha256,
                size: found.bytes.len() as u64,
            })
            .collect()
    }
}

/// One family being resolved: its sources and releases, read as far as
/// needed, and the candidates found so far.
struct FamilyWork {
    prefix: String,
    names: Vec<String>,
    sources: Vec<Source>,
    /// Releases listed but not yet downloaded, in order.
    pending: VecDeque<Arc<Release>>,
    candidates: Vec<Candidate>,
    /// The candidates' files, to leave out a candidate with an earlier one's.
    seen: BTreeSet<Vec<(String, Sha256)>>,
    chosen: Option<usize>,
}

impl Fetcher {
    /// A fetcher that keeps files in `cache` and finds them in `sources`.
    pub fn new(cache: Cache, sources: Sources) -> Self {
        Self {
            http: Http::new(cache.clone()),
            cache,
            sources,
            progress: Box::new(|_| {}),
        }
    }

    /// Calls `progress` with a line for each step that takes time.
    #[must_use]
    pub fn with_progress(mut self, progress: impl FnMut(&str) + 'static) -> Self {
        self.progress = Box::new(progress);
        self
    }

    /// How many files this fetcher has downloaded.
    pub fn downloaded(&self) -> usize {
        self.http.downloaded.load(Ordering::Relaxed)
    }

    /// Makes sure the cache has every file `lock` names, downloading the
    /// missing ones from their origin.
    ///
    /// # Errors
    /// A diagnostic when a file cannot be downloaded or its bytes do not
    /// have the hash the lock pins.
    pub fn install(&mut self, lock: &Lock) -> Result<()> {
        let mut done = BTreeSet::new();
        for font in &lock.fonts {
            if !done.insert(font.sha256) || self.cache.get(&font.sha256)?.is_some() {
                continue;
            }
            let url = releases::url(&font.source);
            (self.progress)(&format!("downloading {url}"));
            let bytes = self
                .http
                .get(&url)?
                .ok_or_else(|| Diagnostic::new(Kind::Missing, format!("{url} is gone")))?;
            self.http.downloaded.fetch_add(1, Ordering::Relaxed);
            let found = Sha256::of(&bytes);
            if found != font.sha256 {
                return Err(Diagnostic::new(
                    Kind::Invalid,
                    format!(
                        "{url} hashes to {found}, not to {} as the lock pins for `{}`",
                        font.sha256, font.postscript
                    ),
                ));
            }
            self.cache.put(&bytes)?;
        }
        Ok(())
    }

    /// Resolves `request`: for each family, the release whose files have
    /// its faces and, with a judge, best match the evidence.
    ///
    /// Releases are read and downloaded only as far as needed: without a
    /// judge, up to the first candidate with every face; with one, until a
    /// release's best candidate sets every glyph the evidence stores as it
    /// was stored, or there are no more.
    ///
    /// # Errors
    /// A diagnostic when a source cannot be listed or a file downloaded, or
    /// one from the judge.
    pub fn resolve(&mut self, request: Request<'_>) -> Result<Resolution> {
        let Request {
            fonts,
            judge,
            release,
        } = request;
        let mut by_family: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for name in fonts {
            by_family
                .entry(Sources::prefix(&name))
                .or_default()
                .insert(name);
        }
        let mut unresolved = Vec::new();
        let mut work = Vec::new();
        for (prefix, names) in by_family {
            let names: Vec<String> = names.into_iter().collect();
            let sources = self.sources.of(&prefix, &self.http)?;
            if sources.is_empty() {
                unresolved.extend(names.iter().map(|name| Unresolved {
                    postscript: name.clone(),
                    why: "no open-licensed source has this family".into(),
                }));
                continue;
            }
            let mut family = FamilyWork {
                prefix,
                names,
                sources,
                pending: VecDeque::new(),
                candidates: Vec::new(),
                seen: BTreeSet::new(),
                chosen: None,
            };
            // Until the evidence says otherwise: the first candidate with every face,
            // else the first with the most.
            while !family
                .candidates
                .iter()
                .any(|c| c.faces.len() == family.names.len())
                && self.more(&mut family, release.as_deref())?
            {}
            let most = family.candidates.iter().map(|c| c.faces.len()).max();
            family.chosen =
                most.and_then(|most| family.candidates.iter().position(|c| c.faces.len() == most));
            if family.chosen.is_none() {
                unresolved.extend(family.names.iter().map(|name| Unresolved {
                    postscript: name.clone(),
                    why: "no release of the family's sources has this face".into(),
                }));
                continue;
            }
            work.push(family);
        }
        let mut scores = vec![None; work.len()];
        if let Some(judge) = judge {
            for i in 0..work.len() {
                let (best, score) = self.best(&mut work, i, judge, release.as_deref())?;
                work[i].chosen = Some(best);
                scores[i] = Some(score);
            }
        }
        let mut locked = Vec::new();
        let mut choices = Vec::new();
        for (family, score) in work.iter().zip(scores) {
            let Some(candidate) = family.chosen.map(|i| &family.candidates[i]) else {
                continue;
            };
            locked.extend(candidate.locked());
            unresolved.extend(
                family
                    .names
                    .iter()
                    .filter(|name| !candidate.faces.contains_key(*name))
                    .map(|name| Unresolved {
                        postscript: name.clone(),
                        why: format!(
                            "{} has the family, but not this face",
                            candidate.release.label
                        ),
                    }),
            );
            choices.push(Choice {
                family: family.prefix.clone(),
                release: candidate.release.label.clone(),
                candidates: family.candidates.len(),
                score,
            });
        }
        Ok(Resolution {
            lock: Lock::new(locked),
            choices,
            unresolved,
        })
    }

    /// The candidate of family `i` the judge scores best, the others' kept
    /// as chosen, and its score. Reads releases until a release's best
    /// candidate is exact, or sets every stored glyph as stored.
    fn best(
        &mut self,
        work: &mut [FamilyWork],
        i: usize,
        judge: &mut dyn Judge,
        only: Option<&str>,
    ) -> Result<(usize, Score)> {
        let mut best: Option<(usize, Score)> = None;
        let mut scored = 0;
        loop {
            // Whether the release just read has a candidate that sets every stored glyph.
            let mut glyphs_held = false;
            let read = work[i].candidates.len() - scored;
            while scored < work[i].candidates.len() {
                let score = self.score(work, i, scored, judge)?;
                glyphs_held |= score.holds_glyphs();
                if best.as_ref().is_none_or(|(_, b)| score.rank(b).is_lt()) {
                    best = Some((scored, score));
                }
                scored += 1;
            }
            // Releases whose glyphs match the evidence come in a run, from the
            // version it was made with to the next that renumbered glyphs: read on
            // until the run ends, then keep its best.
            let run_ended =
                read > 0 && !glyphs_held && best.as_ref().is_some_and(|(_, b)| b.holds_glyphs());
            let exact = best.as_ref().is_some_and(|(_, b)| b.is_exact());
            if exact || run_ended || !self.more(&mut work[i], only)? {
                break;
            }
        }
        Ok(best.expect("a family being judged has a candidate"))
    }

    /// The judge's score of candidate `c` of family `i`, with the other
    /// families' chosen candidates.
    fn score(
        &mut self,
        work: &[FamilyWork],
        i: usize,
        c: usize,
        judge: &mut dyn Judge,
    ) -> Result<Score> {
        let candidate = &work[i].candidates[c];
        let mut files: Vec<(String, Arc<Vec<u8>>)> = Vec::new();
        let mut seen = BTreeSet::new();
        let others = work
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .filter_map(|(_, other)| other.chosen.map(|c| &other.candidates[c]));
        for chosen in std::iter::once(candidate).chain(others) {
            for found in chosen.faces.values() {
                if seen.insert(found.sha256) {
                    let name = format!("{} {}", chosen.release.label, found.file.path);
                    files.push((name, found.bytes.clone()));
                }
            }
        }
        let files: Vec<FontFile<'_>> = files
            .iter()
            .map(|(name, bytes)| FontFile {
                name,
                bytes: bytes.as_slice(),
            })
            .collect();
        let score = Score::of(&judge.judge(&files)?, &work[i].names);
        (self.progress)(&format!(
            "{} {}: {} of {} stored glyphs differ, {} of {} lines off, {:.2} px",
            candidate.release.label,
            candidate.flavour(),
            score.lines.glyphs_differ,
            score.lines.glyphs,
            score.lines.lines_failing,
            score.lines.lines,
            score.lines.off
        ));
        Ok(score)
    }

    /// Reads the family's next release, listing its next source first when
    /// none is pending, and adds its candidates. False when there are no
    /// more releases.
    fn more(&mut self, family: &mut FamilyWork, only: Option<&str>) -> Result<bool> {
        let release = loop {
            if let Some(release) = family.pending.pop_front() {
                break release;
            }
            if family.sources.is_empty() {
                return Ok(false);
            }
            let source = family.sources.remove(0);
            (self.progress)(&format!(
                "listing the releases of {} in {source}",
                family.prefix
            ));
            family.pending.extend(
                releases::releases(&self.http, &source)?
                    .into_iter()
                    .filter(|r| only.is_none_or(|only| r.reference.starts_with(only)))
                    .map(Arc::new),
            );
        };
        let picks = Release::candidates(&release.files(&self.http)?, &family.names);
        // Every file of the release once, all at once: downloads overlap.
        let mut files: Vec<&RemoteFile> = picks.iter().flatten().map(|(_, f)| f).collect();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files.dedup_by(|a, b| a.path == b.path);
        let read: Vec<Result<Option<Read>>> = thread::scope(|scope| {
            let handles: Vec<_> = files
                .iter()
                .map(|&file| {
                    let (http, cache, release) = (&self.http, &self.cache, &release);
                    scope.spawn(move || read(http, cache, release, file))
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("a download thread does not panic"))
                .collect()
        });
        let mut by_path = BTreeMap::new();
        for (file, read) in files.iter().zip(read) {
            if let Some(read) = read? {
                by_path.insert(file.path.clone(), read);
            }
        }
        for pick in &picks {
            let mut faces = BTreeMap::new();
            for (name, file) in pick {
                let Some(read) = by_path.get(&file.path) else {
                    continue;
                };
                if let Some((family, version)) = read
                    .faces
                    .iter()
                    .find(|(n, ..)| n == name)
                    .map(|(_, f, v)| (f, v))
                {
                    faces.insert(
                        name.clone(),
                        Found {
                            file: file.clone(),
                            sha256: read.sha256,
                            bytes: read.bytes.clone(),
                            family: family.clone(),
                            version: version.clone(),
                        },
                    );
                }
            }
            let identity: Vec<(String, Sha256)> =
                faces.iter().map(|(n, f)| (n.clone(), f.sha256)).collect();
            if !faces.is_empty() && family.seen.insert(identity) {
                family.candidates.push(Candidate {
                    release: release.clone(),
                    faces,
                });
            }
        }
        for line in self.http.take_notes() {
            (self.progress)(&line);
        }
        Ok(true)
    }
}

/// A downloaded font file: its bytes, hash and faces.
struct Read {
    bytes: Arc<Vec<u8>>,
    sha256: Sha256,
    /// Each face's PostScript name, family and version.
    faces: Vec<(String, String, String)>,
}

/// `file` of `release`, from the cache or downloaded into it, or none when
/// it is gone or not a font.
fn read(http: &Http, cache: &Cache, release: &Release, file: &RemoteFile) -> Result<Option<Read>> {
    let origin = Origin {
        repository: release.repository.clone(),
        reference: release.reference.clone(),
        path: file.path.clone(),
    };
    let url = releases::url(&origin);
    let Some(bytes) = download(http, cache, &url, file.sha256)? else {
        return Ok(None);
    };
    let mut set = FontSet::new();
    if set.add_bytes(bytes.to_vec(), &url).is_err() {
        http.tell(format!("{url} is not a font; passed over"));
        return Ok(None);
    }
    let faces = set
        .faces()
        .iter()
        .map(|f| {
            (
                f.postscript_name.clone(),
                f.family.clone(),
                f.version.clone(),
            )
        })
        .collect();
    Ok(Some(Read {
        sha256: Sha256::of(&bytes),
        bytes,
        faces,
    }))
}

/// The bytes at `url`, from the cache when the listing's hash or the
/// file's entity tag names a file it has.
fn download(
    http: &Http,
    cache: &Cache,
    url: &str,
    listed: Option<Sha256>,
) -> Result<Option<Arc<Vec<u8>>>> {
    let mut known = listed.or_else(|| http.recall(url)?.parse().ok());
    let mut etag = None;
    if known.is_none() {
        let Some(head) = http.head_immutable(url)? else {
            return Ok(None);
        };
        etag = head.etag.map(|tag| format!("etag {tag}"));
        known = etag
            .as_deref()
            .and_then(|key| http.recall(key)?.parse().ok());
    }
    if let Some(hash) = known
        && let Some(bytes) = cache.get(&hash)?
    {
        return Ok(Some(Arc::new(bytes)));
    }
    let Some(bytes) = http.get(url)? else {
        return Ok(None);
    };
    http.downloaded.fetch_add(1, Ordering::Relaxed);
    http.tell(format!("downloaded {url}"));
    let hash = cache.put(&bytes)?;
    if listed.is_some_and(|listed| listed != hash) {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!("{url} hashes to {hash}, not to what its listing says"),
        ));
    }
    http.note(url, &hash.to_string())?;
    if let Some(key) = etag {
        http.note(&key, &hash.to_string())?;
    }
    Ok(Some(Arc::new(bytes)))
}
