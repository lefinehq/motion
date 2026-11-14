//! A layer's property tree: groups (`LIST:tdgp`) of `(match name, body)`
//! pairs, whose leaves are streams (`LIST:tdbs`) holding a static value or
//! keyframes
//! ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listtdgp)).

use motion_format::{Diagnostic, Kind, Result};

use super::{Decoder, Scan, UTF8};
use crate::model::{Child, Color, Expression, Group, Property, Reason, Stream, Value};
use crate::record::{Bound, Cdat, Layout, Record, Tdb4, Tdsb, text};
use crate::rifx::{Chunk, Name, Tag};

/// A property group.
pub(super) const TDGP: Name = Name::list(b"tdgp");
/// A property stream.
const TDBS: Name = Name::list(b"tdbs");
/// The match name of the child that follows it.
const TDMN: Name = Name::chunk(b"tdmn");
/// A display name.
const TDSN: Name = Name::chunk(b"tdsn");
/// A keyframe list.
const KEYFRAMES: Name = Name::list(b"list");
/// An expression, in files older than [`crate::record::Head::WRAPPED_TEXT`].
const EXPR: Name = Name::chunk(b"expr");

/// The match name that ends a group.
const GROUP_END: &str = "ADBE Group End";
/// The display name of a group or stream that was never renamed.
const DEFAULT_NAME: &str = "-_0_/-";

/// How a child's body is read, found by the list type of its main list.
#[derive(Debug, Clone, Copy)]
enum Body {
    /// A nested group.
    Group,
    /// An effect instance: its parameters are the group inside.
    Effect,
    /// A stream whose value, if valueless, is not decoded.
    Stream,
    /// A stream inside a wrapper that holds its values, not decoded yet.
    Wrapped(Reason),
}

/// The bodies the reader knows, by list type
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listtdgp)).
/// Any other chunk in a body, such as a mask's `mkif`, is skipped.
const BODIES: [(Tag, Body); 8] = [
    (Tag(*b"tdgp"), Body::Group),
    (Tag(*b"sspc"), Body::Effect),
    (Tag(*b"tdbs"), Body::Stream),
    (Tag(*b"om-s"), Body::Wrapped(Reason::Path)),
    (Tag(*b"otst"), Body::Wrapped(Reason::Orientation)),
    (Tag(*b"btds"), Body::Wrapped(Reason::TextDocument)),
    (Tag(*b"mrst"), Body::Wrapped(Reason::Markers)),
    (Tag(*b"GCst"), Body::Wrapped(Reason::Gradient)),
];

impl Decoder {
    /// A group: its flags, its name and its children, up to `ADBE Group End`.
    pub(super) fn group(&self, mut scan: Scan<'_, '_>, match_name: String) -> Result<Group> {
        let enabled = scan.optional::<Tdsb>()?.is_none_or(|tdsb| tdsb.enabled());
        let name = self.display_name(&mut scan)?;
        let children = scan
            .children(TDMN)
            .into_iter()
            .map_while(|(tdmn, body)| {
                let match_name = text(tdmn.body);
                (match_name != GROUP_END).then_some((match_name, body))
            })
            .map(|(match_name, body)| self.child(&scan, match_name, &body))
            .collect::<Result<_>>()?;
        Ok(Group {
            match_name,
            name,
            enabled,
            children,
        })
    }

    /// One child of a group, from the chunks between its `tdmn` and the next.
    fn child(
        &self,
        parent: &Scan<'_, '_>,
        match_name: String,
        body: &[Chunk<'_>],
    ) -> Result<Child> {
        let main = body.iter().enumerate().find_map(|(index, chunk)| {
            let list = chunk.name.list?;
            let (_, kind) = BODIES.iter().find(|(tag, _)| *tag == list)?;
            Some((index, *chunk, *kind))
        });
        let others = body
            .iter()
            .enumerate()
            .filter(|(index, _)| main.is_none_or(|(main, ..)| main != *index));
        self.skip(others.map(|(_, chunk)| chunk.name));
        let Some((_, chunk, kind)) = main else {
            return Ok(Child::Property(opaque(match_name, Reason::OtherBody)));
        };
        let mut scan = parent.open(chunk, parent.child_path(&match_name))?;
        Ok(match kind {
            Body::Group => Child::Group(self.group(scan, match_name)?),
            Body::Effect => Child::Group(self.group(scan.require_list(TDGP)?, match_name)?),
            Body::Stream => Child::Property(self.stream(scan, match_name, Reason::Valueless)?),
            Body::Wrapped(reason) => {
                Child::Property(self.stream(scan.require_list(TDBS)?, match_name, reason)?)
            }
        })
    }

    /// A stream: its flags, name, metadata, value, expression and bounds
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listtdbs)).
    /// `reason` says what a valueless stream's value is.
    fn stream(
        &self,
        mut scan: Scan<'_, '_>,
        match_name: String,
        reason: Reason,
    ) -> Result<Property> {
        let enabled = scan.optional::<Tdsb>()?.is_none_or(|tdsb| tdsb.enabled());
        let name = self.display_name(&mut scan)?;
        let tdb4: Tdb4 = scan.record()?;
        let value = match scan.find(Cdat::NAME) {
            Some(cdat) => match tdb4.layout() {
                Layout::PerDimension | Layout::SingleEase => {
                    let cdat: Cdat = scan.parse(cdat, ())?;
                    value_of(&scan, &cdat.numbers, &tdb4)?
                }
                Layout::SingleEaseValueless | Layout::Valueless => Value::Opaque { reason },
            },
            None => {
                scan.require(KEYFRAMES)?;
                Value::Opaque {
                    reason: Reason::Keyframes,
                }
            }
        };
        let expression = scan
            .find(UTF8)
            .or_else(|| scan.find(EXPR))
            .map(|chunk| Expression {
                text: text(chunk.body),
                enabled: tdb4.expression_enabled(),
            });
        let mut bound = |name| {
            scan.find(name)
                .map(|chunk| scan.parse::<Bound>(chunk, ()).map(|bound| bound.value))
                .transpose()
        };
        let (min, max) = (bound(Bound::MIN)?, bound(Bound::MAX)?);
        Ok(Property {
            match_name,
            name,
            enabled,
            value: Stream::Static(value),
            expression,
            min,
            max,
        })
    }

    /// The name in the next `tdsn`, unless it is empty or the default.
    fn display_name(&self, scan: &mut Scan<'_, '_>) -> Result<Option<String>> {
        let Some(tdsn) = scan.find(TDSN) else {
            return Ok(None);
        };
        let name = if self.wrapped_text.get() {
            let mut inner = scan.open(tdsn, scan.child_path(TDSN))?;
            inner
                .find(UTF8)
                .map(|utf8| text(utf8.body))
                .unwrap_or_default()
        } else {
            text(tdsn.body)
        };
        Ok(Some(name).filter(|name| !name.is_empty() && name != DEFAULT_NAME))
    }
}

/// The value made of the first `tdb4.dims` of `numbers`.
///
/// # Errors
/// A diagnostic when there are fewer numbers than dimensions.
fn value_of(scan: &Scan<'_, '_>, numbers: &[f64], tdb4: &Tdb4) -> Result<Value> {
    let dims = usize::from(tdb4.dims);
    let value = numbers.get(..dims).ok_or_else(|| {
        Diagnostic::new(
            Kind::Invalid,
            format!(
                "a value of {dims} dimensions holds only {} numbers",
                numbers.len()
            ),
        )
        .at(scan.child_path(Cdat::NAME))
        .expected(format!("{dims} numbers"))
    })?;
    Ok(match (value, tdb4.is_color()) {
        (&[a, r, g, b], true) => Value::Color(Color {
            r: r / 255.0,
            g: g / 255.0,
            b: b / 255.0,
            a: a / 255.0,
        }),
        (&[x], _) => Value::Number(x),
        (&[x, y], _) => Value::Vector2([x, y]),
        (&[x, y, z], _) => Value::Vector3([x, y, z]),
        _ => Value::Opaque {
            reason: Reason::Dimensions,
        },
    })
}

/// A property whose value the reader does not decode, for `reason`.
fn opaque(match_name: String, reason: Reason) -> Property {
    Property {
        match_name,
        name: None,
        enabled: true,
        value: Stream::Static(Value::Opaque { reason }),
        expression: None,
        min: None,
        max: None,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/decode/property.rs"]
mod tests;
