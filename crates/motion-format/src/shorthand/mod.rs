//! Shorthand strings for layer properties: position, rectangle, font and
//! behaviour.
//!
//! Part of step 1 (Read). Each parser turns one compact string from a scene
//! into a typed value, or a [`ShorthandError`](crate::ShorthandError) that
//! lists the accepted forms.

mod behaviour;
mod font;
mod position;
mod rect;

pub use behaviour::{Behaviour, Easing, Preset};
pub use font::Font;
pub use position::{Coord, Position};
pub use rect::RectShape;
