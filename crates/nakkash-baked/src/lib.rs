//! What a bake makes and a render reads ([D80], [D85]).
//!
//! A bake runs learned models over a clip and writes **assets**: a
//! [`Matte`], one alpha per clip frame, or a [`Track`], the motion of an
//! object. An asset is a [`Manifest`] plus the files it names, all kept in a
//! [`Store`] by their [`Sha256`]; the asset's digest is the hash of its
//! manifest, and a scene keeps that digest in the request that asked for it.
//! The manifest names the request it answers, the clip it was made from and
//! what made it ([`Made`]), so a render can tell a bake that no longer fits
//! its scene from one that does.
//!
//! This crate links no inference runtime and reaches no network: a render
//! only reads what is here.
//!
//! [D80]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d80-learned-models-run-in-a-bake-never-in-a-render
//! [D85]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d85-baked-results-enter-the-scene-as-requests-on-video-layers

mod made;
mod manifest;
mod matte;
mod store;
mod track;

pub use made::{GraphRun, Made, Runtime};
pub use manifest::{ASSET_VERSION, Asset, Manifest};
pub use matte::{Alpha, Matte, MatteWriter};
pub use nakkash_format::Sha256;
pub use store::Store;
pub use track::{Fit, MaskStats, PointTrack, Track, TrackFrame};

/// The command that makes what a render finds missing or stale, named in
/// every such error.
pub const BAKE_COMMAND: &str = "nakkash ml bake";
