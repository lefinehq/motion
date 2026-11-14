//! Shorthand strings for layer properties, each a `winnow` grammar behind
//! `FromStr`: position, rectangle, font and behaviour.

mod behaviour;
mod font;
mod position;
mod rect;

pub use behaviour::Behaviour;
pub use font::Font;
pub use position::{Axis, Coord, Position};
pub use rect::RectShape;
