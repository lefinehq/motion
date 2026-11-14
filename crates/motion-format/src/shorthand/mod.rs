//! Shorthand strings for layer properties: position, rectangle, font and
//! behaviour.
//!
//! Part of step 1 (Read). Each type implements `FromStr` with a `winnow`
//! grammar that turns one compact string from a scene into a typed value, or
//! a [`Diagnostic`](motion_diagnostic::Diagnostic) that lists the accepted
//! forms. The pieces the grammars share with the units and curves live in
//! `motion_grammar`.

mod behaviour;
mod font;
mod position;
mod rect;

pub use behaviour::Behaviour;
pub use font::Font;
pub use position::{Axis, Coord, Position};
pub use rect::RectShape;
