//! Shorthand strings for layer properties, each a `winnow` grammar behind
//! `FromStr`: position, rectangle, font, behaviour, matte and shutter.

mod behaviour;
mod font;
mod matte;
mod position;
mod rect;
mod shutter;

pub use behaviour::Behaviour;
pub use font::Font;
pub use matte::{MATTE_MODES, Matte, MatteMode};
pub use position::{Axis, Coord, Position};
pub use rect::RectShape;
pub use shutter::Shutter;
