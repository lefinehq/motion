//! A layer's property tree: groups (`LIST:tdgp`) of `(match name, body)`
//! pairs, whose leaves are streams (`LIST:tdbs`) holding a static value or
//! keyframes
//! ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listtdgp)).

use std::num::NonZeroU32;

use motion_format::{Diagnostic, Kind, Result};

use super::{Decoder, Scan, UTF8};
use crate::model::{
    Child, Color, Ease, Expression, Group, Keyframe, Path, Property, Reason, Spatial, Stream,
    Value, Vertex,
};
use crate::record::{
    Bound, Cdat, KeyBody, KeyItem, Layout, Lhd3, OneEase, Points, Record, Shph, Tdb4, Tdsb, text,
};
use crate::rifx::{Chunk, Name, Tag};

/// A property group.
pub(super) const TDGP: Name = Name::list(b"tdgp");
/// A property stream.
const TDBS: Name = Name::list(b"tdbs");
/// The match name of the child that follows it.
const TDMN: Name = Name::chunk(b"tdmn");
/// A display name.
const TDSN: Name = Name::chunk(b"tdsn");
/// A keyframe list: an `lhd3` and an `ldat`.
const KEYFRAMES: Name = Name::list(b"list");
/// The items of a keyframe list.
const LDAT: Name = Name::chunk(b"ldat");
/// An expression, in files older than [`crate::record::Head::WRAPPED_TEXT`].
const EXPR: Name = Name::chunk(b"expr");
/// The paths of a path stream, one per keyframe.
const OMKS: Name = Name::list(b"omks");
/// One path.
const SHAP: Name = Name::list(b"shap");

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
    /// A path stream, with its paths next to it.
    Path,
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
    (Tag(*b"om-s"), Body::Path),
    (Tag(*b"otst"), Body::Wrapped(Reason::Orientation)),
    (Tag(*b"btds"), Body::Wrapped(Reason::TextDocument)),
    (Tag(*b"mrst"), Body::Wrapped(Reason::Markers)),
    (Tag(*b"GCst"), Body::Wrapped(Reason::Gradient)),
];

/// Where a valueless stream's values live: in a wrapper the reader does not
/// decode yet, or in the paths read next to the stream, one per keyframe.
#[derive(Debug)]
enum Values {
    /// Not decoded, for this reason.
    Opaque(Reason),
    /// One path per keyframe, or one for a static stream.
    Paths(Vec<Path>),
}

impl Values {
    /// The value of keyframe `index`, or of the static stream at 0.
    ///
    /// # Errors
    /// A [`Kind::Missing`] diagnostic, at `scan`, when there is no path for it.
    fn get(&self, index: usize, scan: &Scan<'_, '_>) -> Result<Value> {
        match self {
            Values::Opaque(reason) => Ok(Value::Opaque { reason: *reason }),
            Values::Paths(paths) => paths.get(index).cloned().map(Value::Path).ok_or_else(|| {
                Diagnostic::new(Kind::Missing, format!("no path for value {index}"))
                    .at(scan.path.clone())
                    .expected("one `LIST:shap` per keyframe in `LIST:omks`")
            }),
        }
    }
}

/// How a layer's keyframe times map to composition seconds
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#interpreting-keyframes)).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Timing {
    /// Where the layer's time 0 falls, in composition seconds.
    pub start: f64,
    /// The layer's stretch: 2 runs its time at half speed.
    pub stretch: f64,
}

impl Timing {
    /// The composition time of a keyframe stored at `time` units of
    /// `time_base` a second. A reversed layer's time 0 sits a 3000th of its
    /// stretch before its start.
    fn seconds(self, time: i32, time_base: NonZeroU32) -> f64 {
        let layer_seconds = f64::from(time) / f64::from(time_base.get());
        self.start + layer_seconds * self.stretch + self.stretch.min(0.0) / 3000.0
    }
}

impl Decoder {
    /// A group: its flags, its name and its children, up to `ADBE Group End`.
    pub(super) fn group(
        &self,
        mut scan: Scan<'_, '_>,
        match_name: String,
        timing: Timing,
    ) -> Result<Group> {
        let enabled = scan.optional::<Tdsb>()?.is_none_or(|tdsb| tdsb.enabled());
        let name = self.display_name(&mut scan)?;
        let children = scan
            .children(TDMN)
            .into_iter()
            .map_while(|(tdmn, body)| {
                let match_name = text(tdmn.body);
                (match_name != GROUP_END).then_some((match_name, body))
            })
            .map(|(match_name, body)| self.child(&scan, match_name, &body, timing))
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
        timing: Timing,
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
            Body::Group => Child::Group(self.group(scan, match_name, timing)?),
            Body::Effect => {
                Child::Group(self.group(scan.require_list(TDGP)?, match_name, timing)?)
            }
            Body::Stream => {
                let values = Values::Opaque(Reason::Valueless);
                Child::Property(self.stream(scan, match_name, &values, timing)?)
            }
            Body::Path => {
                let stream = scan.require_list(TDBS)?;
                let paths = match scan.list(OMKS)? {
                    Some(mut omks) => omks.lists(SHAP).map(|shap| path(shap?)).collect(),
                    None => Ok(Vec::new()),
                };
                let values = Values::Paths(paths?);
                Child::Property(self.stream(stream, match_name, &values, timing)?)
            }
            Body::Wrapped(reason) => {
                let stream = scan.require_list(TDBS)?;
                let values = Values::Opaque(reason);
                Child::Property(self.stream(stream, match_name, &values, timing)?)
            }
        })
    }

    /// A stream: its flags, name, metadata, value or keyframes, expression
    /// and bounds
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listtdbs)).
    /// `values` holds a valueless stream's values.
    fn stream(
        &self,
        mut scan: Scan<'_, '_>,
        match_name: String,
        values: &Values,
        timing: Timing,
    ) -> Result<Property> {
        let enabled = scan.optional::<Tdsb>()?.is_none_or(|tdsb| tdsb.enabled());
        let name = self.display_name(&mut scan)?;
        let tdb4: Tdb4 = scan.record()?;
        let value = match scan.find(Cdat::NAME) {
            Some(cdat) => Stream::Static(static_value(&scan, cdat, &tdb4, values)?),
            None => {
                let list = scan.require_list(KEYFRAMES)?;
                Stream::Animated(keyframes(list, &tdb4, values, timing)?)
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
            value,
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

/// A stream's static value, from its `cdat`
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#cdat)).
///
/// # Errors
/// A diagnostic when the `cdat` holds fewer numbers than the dimensions.
fn static_value(
    scan: &Scan<'_, '_>,
    cdat: Chunk<'_>,
    tdb4: &Tdb4,
    values: &Values,
) -> Result<Value> {
    if matches!(
        tdb4.layout(),
        Layout::SingleEaseValueless | Layout::Valueless
    ) {
        return values.get(0, scan);
    }
    let dims = usize::from(tdb4.dims);
    let cdat: Cdat = scan.parse(cdat, ())?;
    let numbers = cdat.numbers.get(..dims).ok_or_else(|| {
        Diagnostic::new(
            Kind::Invalid,
            format!(
                "a value of {dims} dimensions holds only {} numbers",
                cdat.numbers.len()
            ),
        )
        .at(scan.child_path(Cdat::NAME))
        .expected(format!("{dims} numbers"))
    })?;
    Ok(value_of(numbers, tdb4.is_color()))
}

/// A stream's keyframes, from the items of its `LIST:list`
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#ldat)).
///
/// # Errors
/// A diagnostic when the `ldat` holds fewer items than `lhd3` counts, an item
/// is too short for the stream's layout, or the time base is 0.
fn keyframes(
    mut list: Scan<'_, '_>,
    tdb4: &Tdb4,
    values: &Values,
    timing: Timing,
) -> Result<Vec<Keyframe>> {
    let lhd3: Lhd3 = list.record()?;
    let ldat = list.find(LDAT).map(|chunk| chunk.body).unwrap_or_default();
    let count = usize::try_from(lhd3.count).unwrap_or(usize::MAX);
    let size = usize::try_from(lhd3.item_size).unwrap_or(usize::MAX);
    let items: Vec<&[u8]> = match size {
        0 => Vec::new(),
        size => ldat.chunks_exact(size).take(count).collect(),
    };
    if items.len() < count {
        return Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "`lhd3` counts {count} keyframes of {size} bytes, but `ldat` holds {} bytes",
                ldat.len()
            ),
        )
        .at(list.child_path(LDAT))
        .expected("an `ldat` of count x item size bytes"));
    }
    let time_base = NonZeroU32::new(tdb4.time_base).ok_or_else(|| {
        Diagnostic::new(Kind::Invalid, "the keyframes have a time base of 0")
            .at(list.path.clone())
            .expected("a `tdb4` time base of at least 1")
    })?;
    let args = (usize::from(tdb4.dims), tdb4.layout());
    items
        .into_iter()
        .enumerate()
        .map(|(index, body)| {
            let item: KeyItem = list.parse(Chunk { name: LDAT, body }, args)?;
            let time = timing.seconds(item.time, time_base);
            let valueless = || values.get(index, &list);
            keyframe(item, time, tdb4, valueless)
        })
        .collect()
}

/// The keyframe at `time` that `item` describes; a valueless stream's value
/// comes from `valueless`.
fn keyframe(
    item: KeyItem,
    time: f64,
    tdb4: &Tdb4,
    valueless: impl FnOnce() -> Result<Value>,
) -> Result<Keyframe> {
    let (value, in_ease, out_ease, spatial) = match item.body {
        KeyBody::PerDimension {
            value,
            in_speed,
            in_influence,
            out_speed,
            out_influence,
        } => (
            value_of(&value, tdb4.is_color()),
            eases(&in_speed, &in_influence),
            eases(&out_speed, &out_influence),
            None,
        ),
        KeyBody::SingleEase {
            spatial_flags,
            ease,
            value,
            in_tangent,
            out_tangent,
        } => {
            let spatial = tdb4.is_spatial().then_some(Spatial {
                in_tangent,
                out_tangent,
                continuous: spatial_flags & KeyItem::SPATIAL_CONTINUOUS != 0,
                auto_bezier: spatial_flags & KeyItem::SPATIAL_AUTO_BEZIER != 0,
            });
            let (in_ease, out_ease) = sides(&ease);
            let color = tdb4.is_color();
            (value_of(&value, color), in_ease, out_ease, spatial)
        }
        KeyBody::SingleEaseValueless { ease } => {
            let (in_ease, out_ease) = sides(&ease);
            (valueless()?, in_ease, out_ease, None)
        }
        KeyBody::Valueless => (valueless()?, Vec::new(), Vec::new(), None),
    };
    Ok(Keyframe {
        time,
        value,
        in_interpolation: item.in_interpolation.into(),
        out_interpolation: item.out_interpolation.into(),
        in_ease,
        out_ease,
        spatial,
        continuous: item.flags & KeyItem::CONTINUOUS != 0,
        auto_bezier: item.flags & KeyItem::AUTO_BEZIER != 0,
        roving: item.flags & KeyItem::ROVING != 0,
    })
}

/// One path, from a `LIST:shap`: its box, then its points, three per vertex
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listshap)).
fn path(mut shap: Scan<'_, '_>) -> Result<Path> {
    let shph: Shph = shap.record()?;
    let mut list = shap.require_list(KEYFRAMES)?;
    let lhd3: Lhd3 = list.record()?;
    let stored = list.optional::<Points>()?.map(|ldat| ldat.points);
    let count = usize::try_from(lhd3.count).unwrap_or(usize::MAX);
    let points: Vec<[f64; 2]> = stored
        .unwrap_or_default()
        .into_iter()
        .take(count)
        .map(|point| shph.point(point))
        .collect();
    let (triples, _) = points.as_chunks::<3>();
    // The incoming control point of each vertex ends the triple before it.
    let previous = triples.iter().cycle().skip(triples.len().saturating_sub(1));
    let vertices = triples
        .iter()
        .zip(previous)
        .map(|(&[point, [out_x, out_y], _], &[_, _, [in_x, in_y]])| {
            let [x, y] = point;
            Vertex {
                point,
                in_tangent: [in_x - x, in_y - y],
                out_tangent: [out_x - x, out_y - y],
            }
        })
        .collect();
    Ok(Path {
        closed: shph.closed(),
        vertices,
    })
}

/// The value that `numbers`, one per dimension, make; a colour when `color`.
fn value_of(numbers: &[f64], color: bool) -> Value {
    match (numbers, color) {
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
    }
}

/// One ease per dimension, from the speeds and influences of each.
fn eases(speeds: &[f64], influences: &[f64]) -> Vec<Ease> {
    speeds
        .iter()
        .zip(influences)
        .map(|(&speed, &influence)| Ease { speed, influence })
        .collect()
}

/// The in and out side of a single-ease item's ease, as one-element lists.
fn sides(ease: &OneEase) -> (Vec<Ease>, Vec<Ease>) {
    (
        eases(&[ease.in_speed], &[ease.in_influence]),
        eases(&[ease.out_speed], &[ease.out_influence]),
    )
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
