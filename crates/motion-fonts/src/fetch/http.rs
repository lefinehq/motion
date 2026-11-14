//! [`Http`]: the network, with what never changes kept in the cache.

use std::fs;
use std::io;
use std::sync::atomic::AtomicUsize;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use motion_format::{Diagnostic, Kind, Result};
use ureq::Agent;

use crate::Cache;
use crate::Sha256;
use crate::cache::write_whole;

/// The largest response read: more than any font file.
const LIMIT: u64 = 64 * 1024 * 1024;

/// How long a GitHub API answer is reused: the API allows 60 requests an
/// hour without a token, and a family's tags and commits change seldom.
const API_REUSE: Duration = Duration::from_secs(24 * 60 * 60);

/// The variables a GitHub token is read from, in order.
pub(crate) const TOKEN_VARIABLES: &[&str] = &["GITHUB_TOKEN", "GH_TOKEN"];

/// A client that keeps responses to immutable URLs (a tag's file list, a
/// file at a commit) under `http/` of the cache, so a second fetch of the
/// same releases makes no requests for them.
pub(crate) struct Http {
    agent: Agent,
    token: Option<String>,
    cache: Cache,
    /// Lines for the progress report from threads that cannot call it.
    told: Mutex<Vec<String>>,
    /// How many files were downloaded.
    pub downloaded: AtomicUsize,
}

/// What a `HEAD` request says of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Head {
    /// The entity tag, quotes and all.
    pub etag: Option<String>,
}

impl Http {
    pub(crate) fn new(cache: Cache) -> Self {
        let config = Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent(concat!("motion-fonts/", env!("CARGO_PKG_VERSION")))
            .build();
        let token = TOKEN_VARIABLES
            .iter()
            .find_map(|name| std::env::var(name).ok().filter(|t| !t.is_empty()));
        Self {
            agent: config.into(),
            token,
            cache,
            told: Mutex::new(Vec::new()),
            downloaded: AtomicUsize::new(0),
        }
    }

    /// Keeps a line for the progress report.
    pub(crate) fn tell(&self, line: String) {
        self.told
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line);
    }

    /// The lines kept by [`Http::tell`], in order.
    pub(crate) fn take_notes(&self) -> Vec<String> {
        std::mem::take(&mut *self.told.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// The body at `url`, or none for a 404.
    pub(crate) fn get(&self, url: &str) -> Result<Option<Vec<u8>>> {
        self.request(url, false)
    }

    /// As [`Http::get`], for a URL whose answer never changes: kept in the
    /// cache, and read from it the next time.
    pub(crate) fn get_immutable(&self, url: &str) -> Result<Option<Vec<u8>>> {
        self.reuse("GET", url, None, || self.get(url))
    }

    /// As [`Http::get_immutable`], but any refusal (4xx) is none: a listing
    /// a service will not give, such as jsDelivr's of a repository over its
    /// size limit, so the caller can ask elsewhere.
    pub(crate) fn listing(&self, url: &str) -> Result<Option<Vec<u8>>> {
        self.reuse("GET", url, None, || match self.call(url, false)? {
            (200..=299, body) => Ok(Some(body)),
            (400..=499, _) => Ok(None),
            (status, body) => Err(failed(url, status, &String::from_utf8_lossy(&body))),
        })
    }

    /// The answer kept for `method` on `url`, if younger than `max_age`
    /// (or kept at all, without one), else `fetch`'s, kept when it is some.
    fn reuse(
        &self,
        method: &str,
        url: &str,
        max_age: Option<Duration>,
        fetch: impl FnOnce() -> Result<Option<Vec<u8>>>,
    ) -> Result<Option<Vec<u8>>> {
        let kept = self.kept(method, url);
        let fresh = |meta: fs::Metadata| match max_age {
            None => true,
            Some(age) => meta
                .modified()
                .ok()
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .is_some_and(|elapsed| elapsed < age),
        };
        if fs::metadata(&kept).is_ok_and(fresh) {
            match fs::read(&kept) {
                Ok(bytes) => return Ok(Some(bytes)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(Diagnostic::io("read", &kept, &e)),
            }
        }
        let body = fetch()?;
        if let Some(body) = &body {
            write_whole(&kept, body)?;
        }
        Ok(body)
    }

    /// What a `HEAD` request for an immutable `url` says, or none for a 404.
    pub(crate) fn head_immutable(&self, url: &str) -> Result<Option<Head>> {
        let kept = self.kept("HEAD", url);
        if let Ok(etag) = fs::read_to_string(&kept) {
            return Ok(Some(Head { etag: Some(etag) }));
        }
        let response = self
            .agent
            .head(url)
            .call()
            .map_err(|e| unreachable_at(url, &e))?;
        match response.status().as_u16() {
            404 => Ok(None),
            200..=299 => {
                let etag = response
                    .headers()
                    .get("etag")
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                if let Some(etag) = &etag {
                    write_whole(&kept, etag.as_bytes())?;
                }
                Ok(Some(Head { etag }))
            }
            status => Err(failed(url, status, "")),
        }
    }

    /// A value kept in the cache under `key`, such as the hash of the file
    /// an entity tag stands for.
    pub(crate) fn recall(&self, key: &str) -> Option<String> {
        fs::read_to_string(self.kept("NOTE", key)).ok()
    }

    /// Keeps `value` under `key` for [`Http::recall`].
    pub(crate) fn note(&self, key: &str, value: &str) -> Result<()> {
        write_whole(&self.kept("NOTE", key), value.as_bytes())
    }

    /// A GitHub API response as JSON, or none for a 404, reused for a day.
    /// With a token in `GITHUB_TOKEN` or `GH_TOKEN`, it is sent: GitHub
    /// allows 60 requests an hour without one.
    pub(crate) fn github(&self, path: &str) -> Result<Option<serde_json::Value>> {
        let url = format!("https://api.github.com/{path}");
        let Some(body) = self.reuse("GET", &url, Some(API_REUSE), || self.request(&url, true))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&body)
            .map(Some)
            .map_err(|e| Diagnostic::new(Kind::Invalid, format!("{url} is not JSON: {e}")))
    }

    fn request(&self, url: &str, github: bool) -> Result<Option<Vec<u8>>> {
        match self.call(url, github)? {
            (200..=299, body) => Ok(Some(body)),
            (404, _) => Ok(None),
            (status, body) => Err(failed(url, status, &String::from_utf8_lossy(&body))),
        }
    }

    /// The status and body of a `GET` of `url`.
    fn call(&self, url: &str, github: bool) -> Result<(u16, Vec<u8>)> {
        let mut request = self.agent.get(url);
        if github {
            request = request
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28");
            if let Some(token) = &self.token {
                request = request.header("Authorization", &format!("Bearer {token}"));
            }
        }
        let mut response = request.call().map_err(|e| unreachable_at(url, &e))?;
        let status = response.status().as_u16();
        let limited = github
            && matches!(status, 403 | 429)
            && response
                .headers()
                .get("x-ratelimit-remaining")
                .is_some_and(|v| v == "0");
        let body = response
            .body_mut()
            .with_config()
            .limit(LIMIT)
            .read_to_vec()
            .map_err(|e| unreachable_at(url, &e))?;
        if limited {
            return Err(Diagnostic::new(
                Kind::Io,
                format!(
                    "GitHub's limit of requests without a token is used up; set `{}` to a token, or wait an hour",
                    TOKEN_VARIABLES[0]
                ),
            ));
        }
        Ok((status, body))
    }

    /// Where the answer to `method` on `url` is kept.
    fn kept(&self, method: &str, url: &str) -> std::path::PathBuf {
        let key = Sha256::of(format!("{method} {url}").as_bytes());
        self.cache.root().join("http").join(key.to_string())
    }
}

fn unreachable_at(url: &str, error: &ureq::Error) -> Diagnostic {
    Diagnostic::new(Kind::Io, format!("cannot fetch {url}: {error}"))
}

fn failed(url: &str, status: u16, body: &str) -> Diagnostic {
    let body: String = body.chars().take(200).collect();
    Diagnostic::new(Kind::Io, format!("{url} answered {status}: {body}"))
}
