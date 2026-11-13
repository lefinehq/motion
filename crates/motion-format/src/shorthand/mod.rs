//! Shorthand strings for layer properties: position, rectangle, font and
//! behaviour.
//!
//! Part of step 1 (Read). Each parser turns one compact string from a scene
//! into a typed value, or a [`Diagnostic`](motion_diagnostic::Diagnostic) that
//! lists the accepted forms.

mod behaviour;
mod font;
mod position;
mod rect;

pub(crate) use behaviour::EXPECTED as BEHAVIOUR_EXPECTED;
pub(crate) use font::EXPECTED as FONT_EXPECTED;
pub(crate) use position::EXPECTED as POSITION_EXPECTED;
pub(crate) use rect::EXPECTED as RECT_EXPECTED;

pub use behaviour::Behaviour;
pub use font::Font;
pub use position::{Coord, Position};
pub use rect::RectShape;
