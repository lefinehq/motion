//! [`Manifest`]: the one file that names everything of a baked asset.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use nakkash_format::{Diagnostic, Kind, Result, Sha256};

use crate::{BAKE_COMMAND, Made, Store};

/// The version of the manifest format this crate reads and writes.
pub const ASSET_VERSION: u32 = 1;

/// What every baked asset says about itself, whatever it holds: the request
/// it answers, the clip it was made from and what made it ([D85]).
///
/// [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest<T> {
    /// The manifest format's version, [`ASSET_VERSION`].
    pub version: u32,
    /// What kind of asset it is: `matte` or `track`.
    pub asset: String,
    /// The request, as the scene writes it, without its `baked` digest.
    pub request: Value,
    /// The hash of the clip it was made from.
    pub clip: Sha256,
    /// The clip's frame size in pixels.
    pub size: [u32; 2],
    /// What made it.
    pub made: Made,
    /// What it holds.
    pub content: T,
}

/// A kind of baked asset: the name its manifests carry.
pub trait Asset: Serialize + for<'de> Deserialize<'de> {
    /// The kind's name, `matte` or `track`.
    const KIND: &'static str;
}

impl<T: Asset> Manifest<T> {
    /// A manifest of `content`, answering `request` for the clip hashing to
    /// `clip`, of `size`, made as `made` says.
    pub fn new(request: Value, clip: Sha256, size: [u32; 2], made: Made, content: T) -> Self {
        Self {
            version: ASSET_VERSION,
            asset: T::KIND.to_owned(),
            request,
            clip,
            size,
            made,
            content,
        }
    }

    /// Keeps the manifest in `store` and returns its digest, the asset's
    /// name in a scene.
    ///
    /// # Errors
    /// A [`Kind::Io`] diagnostic when it cannot be written.
    pub fn put(&self, store: &Store) -> Result<Sha256> {
        let bytes = serde_json::to_vec(self).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!("a manifest that does not write: {e}"),
            )
        })?;
        store.put(&bytes)
    }

    /// The manifest with `digest` in `store`, checked to be of this kind and
    /// to answer `request` for the clip hashing to `clip`.
    ///
    /// # Errors
    /// A [`Kind::Missing`] diagnostic naming [`BAKE_COMMAND`] when the store
    /// lacks it, or a [`Kind::Conflict`] one naming it when it was baked for
    /// another request or another clip; a [`Kind::Invalid`] one when it is
    /// not a manifest of this kind or its file has changed.
    pub fn open(store: &Store, digest: &Sha256, request: &Value, clip: &Sha256) -> Result<Self> {
        let bytes = store.get(digest)?.ok_or_else(|| {
            Diagnostic::new(
                Kind::Missing,
                format!(
                    "the baked {} {digest} is not in the cache at {}; run `{BAKE_COMMAND}` on the scene",
                    T::KIND,
                    store.root().display()
                ),
            )
        })?;
        let manifest: Self = serde_json::from_slice(&bytes).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!(
                    "the baked asset {digest} is not a {} manifest: {e}",
                    T::KIND
                ),
            )
        })?;
        if manifest.version != ASSET_VERSION || manifest.asset != T::KIND {
            return Err(Diagnostic::new(
                Kind::Invalid,
                format!(
                    "the baked asset {digest} is a version {} {}, not a version {ASSET_VERSION} {}",
                    manifest.version,
                    manifest.asset,
                    T::KIND
                ),
            ));
        }
        let stale = |what: &str| {
            Diagnostic::new(
                Kind::Conflict,
                format!(
                    "the baked {} {digest} was made for another {what}; run `{BAKE_COMMAND}` on the scene",
                    T::KIND
                ),
            )
        };
        if manifest.clip != *clip {
            return Err(stale("clip"));
        }
        if manifest.request != *request {
            return Err(stale("request"));
        }
        Ok(manifest)
    }
}

#[cfg(test)]
#[path = "../tests/unit/manifest.rs"]
mod tests;
