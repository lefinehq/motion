//! Shorthand strings for layer properties, each a `winnow` grammar behind
//! `FromStr`: position, rectangle, font, behaviour and matte.

mod behaviour;
mod font;
mod matte;
mod position;
mod rect;

pub use behaviour::Behaviour;
pub use font::Font;
pub use matte::{MATTE_MODES, Matte, MatteMode};
pub use position::{Axis, Coord, Position};
pub use rect::RectShape;
