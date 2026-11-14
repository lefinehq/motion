//! Finding font files ([D39]): the sources of each family, the releases
//! they hold, the download of candidates into the [`Cache`](crate::Cache),
//! and the choice of a release per family.
//!
//! Nothing here runs during a render: it is the `fetch` feature, which only
//! the CLI turns on.
//!
//! [D39]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d39-fonts-are-resolved-before-a-render

mod http;
mod releases;
mod resolve;
mod sources;

pub use resolve::{Choice, Fetcher, Request, Resolution, Unresolved};
pub use sources::{Family, Source, Sources};
