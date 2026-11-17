//! Registries of animatable properties: each kind of object (a layer, a shape
//! group, a rectangle, a fill, an effect) has a [`Registry`] that lists its
//! properties. Names live only in the registries; code refers to a property by
//! its row, a [`Prop`], and to a property or one of its components by a [`Target`].

use kurbo::Rect;

use crate::shorthand::{Axis, Position};
use crate::table::{self, Named};
use crate::value::{Value, ValueType};

/// Where a property rests when nothing animates it and the object sets no value.
#[derive(Debug, Clone, PartialEq)]
pub enum Rest {
    /// A fixed value.
    Value(Value),
    /// The point the layer's `at` names on the canvas.
    At,
    /// The centre of the layer's box, in the layer's own coordinates (a text
    /// layer's box may start left of or above its origin).
    BoxCentre,
    /// The zoom of the default camera: a 50 mm lens on 36 mm film, a 39.6°
    /// horizontal view of the box `at` resolves in, `width · 50/36`, 2,666.7
    /// px for 1920.
    Zoom,
    /// The depth of a layer's position: 0, and on a camera minus its zoom,
    /// which [`Layer::rest`](crate::Layer::rest) gives, so a camera that sets
    /// no depth sees a plane at depth 0 at 100 %.
    Depth,
    /// The layer's own time: a layer that neither sets nor animates the
    /// property shows its content at its own time, so the value its track
    /// holds, 0, is never read.
    OwnTime,
}

/// The default camera's focal length over its film width: a 50 mm lens on
/// 36 mm film, a 39.6° horizontal view.
pub const DEFAULT_LENS: f64 = 50.0 / 36.0;

/// The zoom of the default camera for a canvas `width` px wide.
pub fn default_zoom(width: f64) -> f64 {
    width * DEFAULT_LENS
}

/// One animatable property of an object.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertySpec {
    /// The name used in scene files.
    pub name: &'static str,
    /// What sort of value it holds.
    pub ty: ValueType,
    /// Where it rests when nothing says otherwise.
    pub rest: Rest,
    /// The names of its components, which tracks can animate one at a time;
    /// empty for a number.
    pub components: &'static [&'static str],
    /// The allowed values of each component, inclusive, or `None` when
    /// unbounded; an infinite end leaves that side open.
    pub range: Option<(f64, f64)>,
    /// A lone number sets every component (a uniform `scale`).
    pub broadcast: bool,
    /// The value moves along a path in space: one curve times the whole move,
    /// and keys may bend the path with tangents.
    pub spatial: bool,
    /// What the property means, for people and agents.
    pub doc: &'static str,
}

impl PropertySpec {
    /// The resting value on a layer at `at`, which resolves in `extent` (its
    /// parent's box, or the canvas), whose own box is `own`.
    pub fn rest(&self, at: &Position, extent: Rect, own: Rect) -> Value {
        match &self.rest {
            Rest::Value(value) => value.clone(),
            Rest::At => {
                let size = (extent.width(), extent.height());
                Value::Point([
                    extent.x0 + at.resolve(Axis::X, size),
                    extent.y0 + at.resolve(Axis::Y, size),
                ])
            }
            Rest::BoxCentre => Value::Point([own.center().x, own.center().y]),
            Rest::Zoom => Value::Number(default_zoom(extent.width())),
            Rest::Depth | Rest::OwnTime => Value::Number(0.0),
        }
    }
}

impl Named for PropertySpec {
    fn name(&self) -> &str {
        self.name
    }
}

/// The properties of one kind of object, in order.
#[derive(Debug)]
pub struct Registry {
    /// What the object is, for messages: `layer`, `rect`, `fill`.
    pub kind: &'static str,
    /// Its properties.
    pub rows: &'static [PropertySpec],
}

impl Registry {
    /// The property called `name`, if the registry has one.
    pub fn prop(&'static self, name: &str) -> Option<Prop> {
        table::position(self.rows, name).map(|index| Prop {
            registry: self,
            index,
        })
    }

    /// Every property, in order.
    pub fn props(&'static self) -> impl Iterator<Item = Prop> + Clone {
        (0..self.rows.len()).map(move |index| Prop {
            registry: self,
            index,
        })
    }

    /// The property or component called `name`, if the registry has one.
    pub fn target(&'static self, name: &str) -> Option<Target> {
        self.prop(name).map(Target::whole).or_else(|| {
            self.props().find_map(|prop| {
                let at = prop.spec().components.iter().position(|c| *c == name)?;
                Some(Target {
                    prop,
                    component: Some(at),
                })
            })
        })
    }

    /// The property names, in order.
    pub fn names(&'static self) -> impl Iterator<Item = &'static str> + Clone {
        self.rows.iter().map(|spec| spec.name)
    }

    /// Every name a track may animate: properties, then their components.
    pub fn target_names(&'static self) -> impl Iterator<Item = &'static str> + Clone {
        self.names().chain(
            self.rows
                .iter()
                .flat_map(|spec| spec.components.iter().copied()),
        )
    }
}

/// The transform every layer and shape group has, as a row set: position,
/// anchor, scale, rotation and opacity, with where position and anchor rest,
/// then any `extra` rows.
macro_rules! transform {
    ($position:expr, $anchor:expr, $anchor_doc:expr $(, $extra:expr)* $(,)?) => {
        [
            PropertySpec {
                name: "position",
                ty: ValueType::Point,
                rest: $position,
                components: &["x", "y"],
                range: None,
                broadcast: false,
                spatial: true,
                doc: "where the anchor sits in the parent, in px",
            },
            PropertySpec {
                name: "anchor",
                ty: ValueType::Point,
                rest: $anchor,
                components: &["anchor-x", "anchor-y"],
                range: None,
                broadcast: false,
                spatial: true,
                doc: $anchor_doc,
            },
            PropertySpec {
                name: "scale",
                ty: ValueType::Point,
                rest: Rest::Value(Value::Point([1.0, 1.0])),
                components: &["scale-x", "scale-y"],
                range: None,
                broadcast: true,
                spatial: false,
                doc: "size factor on each axis; one number scales both; negative values mirror",
            },
            PropertySpec {
                name: "rotate",
                ty: ValueType::Number,
                rest: Rest::Value(Value::Number(0.0)),
                components: &[],
                range: None,
                broadcast: false,
                spatial: false,
                doc: "rotation in degrees, clockwise",
            },
            PropertySpec {
                name: "opacity",
                ty: ValueType::Number,
                rest: Rest::Value(Value::Number(1.0)),
                components: &[],
                range: Some((0.0, 1.0)),
                broadcast: false,
                spatial: false,
                doc: "how opaque it is, 0 (invisible) to 1 (solid)",
            },
                $($extra,)*
        ]
    };
}

const fn number_row(
    name: &'static str,
    rest: f64,
    range: Option<(f64, f64)>,
    doc: &'static str,
) -> PropertySpec {
    PropertySpec {
        name,
        ty: ValueType::Number,
        rest: Rest::Value(Value::Number(rest)),
        components: &[],
        range,
        broadcast: false,
        spatial: false,
        doc,
    }
}

/// A layer's rows: its transform, the gain of the sound it plays, then the
/// depth of a 3D layer and what a camera sees ([D58]), then the time of its
/// content ([D62]).
///
/// [D58]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
/// [D62]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d62-a-layer-may-remap-the-time-of-its-content
const LAYER_ROWS: [PropertySpec; 16] = transform!(
    Rest::At,
    Rest::BoxCentre,
    "the point of the layer that sits at `position` and that it scales and turns about, \
     in px of its own box; the box centre by default",
    PropertySpec {
        name: "volume",
        ty: ValueType::Number,
        rest: Rest::Value(Value::Number(1.0)),
        components: &[],
        range: Some((0.0, 4.0)),
        broadcast: false,
        spatial: false,
        doc: "the gain of the sound an audio or `comp` layer plays: 1 as recorded, 0 silent, \
              up to 4 (+12 dB)",
    },
    PropertySpec {
        name: "z",
        ty: ValueType::Number,
        rest: Rest::Depth,
        components: &[],
        range: None,
        broadcast: false,
        spatial: false,
        doc: "the depth of `position` on a 3D layer or camera, in px, away from the viewer as \
              it grows; a camera rests at minus its zoom",
    },
    number_row(
        "anchor-z",
        0.0,
        None,
        "the depth of `anchor` on a 3D layer, in px",
    ),
    number_row(
        "scale-z",
        1.0,
        None,
        "the size factor along depth of a 3D layer, which its children's depths feel",
    ),
    number_row(
        "rotate-x",
        0.0,
        None,
        "the turn of a 3D layer about its X axis, in degrees",
    ),
    number_row(
        "rotate-y",
        0.0,
        None,
        "the turn of a 3D layer about its Y axis, in degrees",
    ),
    PropertySpec {
        name: "orient",
        ty: ValueType::Orientation,
        rest: Rest::Value(Value::Orientation([0.0, 0.0, 0.0])),
        components: &["orient-x", "orient-y", "orient-z"],
        range: None,
        broadcast: false,
        spatial: false,
        doc: "how a 3D layer or camera is turned, three angles in degrees applied after its \
              rotations; its keys turn the shortest way between them",
    },
    PropertySpec {
        name: "zoom",
        ty: ValueType::Number,
        rest: Rest::Zoom,
        components: &[],
        range: Some((1.0, f64::INFINITY)),
        broadcast: false,
        spatial: false,
        doc: "a camera's distance in px at which a plane facing it shows at 100 %; by \
              default a 50 mm lens on 36 mm film, `width · 50/36`",
    },
    PropertySpec {
        name: "target",
        ty: ValueType::Point,
        rest: Rest::At,
        components: &["target-x", "target-y"],
        range: None,
        broadcast: false,
        spatial: true,
        doc: "the point a camera turns to look at, in its parent's px, when the camera sets \
              or animates it",
    },
    number_row(
        "target-z",
        0.0,
        None,
        "the depth of a camera's `target`, in px",
    ),
    PropertySpec {
        name: "remap",
        ty: ValueType::Number,
        rest: Rest::OwnTime,
        components: &[],
        range: None,
        broadcast: false,
        spatial: false,
        doc: "the time a `comp` or video layer shows of its content, in seconds, at each moment \
              of its own time; without it, the layer shows its content at its own time",
    },
);

const GROUP_ROWS: [PropertySpec; 5] = transform!(
    Rest::Value(Value::Point([0.0, 0.0])),
    Rest::Value(Value::Point([0.0, 0.0])),
    "the point of the group that sits at `position` and that it scales and turns about, \
     in px of the shape's coordinates",
);

/// The properties of a layer: its transform and opacity. `position` rests
/// where `at` puts it.
pub static LAYER: Registry = Registry {
    kind: "layer",
    rows: &LAYER_ROWS,
};

/// The properties of a shape group: the same transform, resting at the origin.
pub static GROUP: Registry = Registry {
    kind: "group",
    rows: &GROUP_ROWS,
};

const TRANSFORM_EFFECT_ROWS: [PropertySpec; 5] = transform!(
    Rest::Value(Value::Point([0.0, 0.0])),
    Rest::Value(Value::Point([0.0, 0.0])),
    "the point of the layer's content that sits at `position` and that it scales and turns \
     about, in px of the layer's own coordinates",
);

/// The properties of the transform effect: a group's transform, applied to
/// everything the layer draws.
pub static TRANSFORM_EFFECT: Registry = Registry {
    kind: "transform effect",
    rows: &TRANSFORM_EFFECT_ROWS,
};

/// Every layer property, in registry order.
pub const PROPERTIES: &[PropertySpec] = &LAYER_ROWS;

/// A property: its registry and the index of its row there, so every `Prop`
/// names a row.
///
/// The constants are the layer properties whose meaning code depends on, such
/// as geometry; the registry stays the source of truth for which exist.
#[derive(Debug, Clone, Copy)]
pub struct Prop {
    registry: &'static Registry,
    index: usize,
}

impl PartialEq for Prop {
    fn eq(&self, other: &Prop) -> bool {
        std::ptr::eq(self.registry, other.registry) && self.index == other.index
    }
}

impl Eq for Prop {}

impl std::hash::Hash for Prop {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.registry.kind.hash(state);
        self.index.hash(state);
    }
}

impl Prop {
    /// Where the anchor sits in the parent.
    pub const POSITION: Prop = Prop::layer(0);
    /// The point the layer turns and scales about.
    pub const ANCHOR: Prop = Prop::layer(1);
    /// Size factor on each axis.
    pub const SCALE: Prop = Prop::layer(2);
    /// Rotation in degrees, clockwise.
    pub const ROTATE: Prop = Prop::layer(3);
    /// Opacity, 0 (invisible) to 1 (solid).
    pub const OPACITY: Prop = Prop::layer(4);
    /// The gain of the sound a layer plays, 1 as recorded.
    pub const VOLUME: Prop = Prop::layer(5);
    /// The depth of `position` on a 3D layer or camera.
    pub const Z: Prop = Prop::layer(6);
    /// The depth of `anchor` on a 3D layer.
    pub const ANCHOR_Z: Prop = Prop::layer(7);
    /// The size factor along depth.
    pub const SCALE_Z: Prop = Prop::layer(8);
    /// The turn about X, in degrees.
    pub const ROTATE_X: Prop = Prop::layer(9);
    /// The turn about Y, in degrees.
    pub const ROTATE_Y: Prop = Prop::layer(10);
    /// The orientation, three angles in degrees.
    pub const ORIENT: Prop = Prop::layer(11);
    /// A camera's zoom, in px.
    pub const ZOOM: Prop = Prop::layer(12);
    /// The point a camera with `aim` looks at.
    pub const TARGET: Prop = Prop::layer(13);
    /// The depth of that point.
    pub const TARGET_Z: Prop = Prop::layer(14);
    /// The time a layer shows of its content, in seconds.
    pub const REMAP: Prop = Prop::layer(15);

    /// The rows only a 3D layer or a camera may set or animate.
    pub const DEPTH: [Prop; 6] = [
        Prop::Z,
        Prop::ANCHOR_Z,
        Prop::SCALE_Z,
        Prop::ROTATE_X,
        Prop::ROTATE_Y,
        Prop::ORIENT,
    ];

    /// The rows only a camera may set or animate.
    pub const CAMERA: [Prop; 3] = [Prop::ZOOM, Prop::TARGET, Prop::TARGET_Z];

    const fn layer(index: usize) -> Prop {
        Prop::of(&LAYER, index)
    }

    /// Row `index` of `registry`, for the constants of other registries.
    pub(crate) const fn of(registry: &'static Registry, index: usize) -> Prop {
        assert!(index < registry.rows.len(), "a row of the registry");
        Prop { registry, index }
    }

    /// The layer property called `name`, if there is one.
    pub fn named(name: &str) -> Option<Prop> {
        LAYER.prop(name)
    }

    /// Every layer property, in registry order.
    pub fn all() -> impl Iterator<Item = Prop> {
        LAYER.props()
    }

    /// The property's position in its registry, and in every list kept parallel to it.
    pub fn index(self) -> usize {
        self.index
    }

    /// The registry it belongs to.
    pub fn registry(self) -> &'static Registry {
        self.registry
    }

    /// The property's row.
    pub fn spec(self) -> &'static PropertySpec {
        &self.registry.rows[self.index]
    }

    /// The property's name, as scene files write it.
    pub fn name(self) -> &'static str {
        self.spec().name
    }
}

/// What a track or a preset animates: a whole property, or one component of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Target {
    /// The property.
    pub prop: Prop,
    /// The component, an index into the row's `components`, or `None` for the whole value.
    pub component: Option<usize>,
}

impl Target {
    /// The whole of `prop`.
    pub const fn whole(prop: Prop) -> Target {
        Target {
            prop,
            component: None,
        }
    }

    /// The layer property or component called `name`, if there is one.
    pub fn named(name: &str) -> Option<Target> {
        LAYER.target(name)
    }

    /// The name scene files write: the property's, or its component's.
    pub fn name(self) -> &'static str {
        match self.component {
            None => self.prop.name(),
            Some(i) => self.prop.spec().components[i],
        }
    }

    /// The type of value the target holds: a component is a number.
    pub fn value_type(self) -> ValueType {
        match self.component {
            None => self.prop.spec().ty,
            Some(_) => ValueType::Number,
        }
    }

    /// Whether this target and `other` write the same numbers, so two tracks
    /// on them would fight.
    pub fn overlaps(self, other: Target) -> bool {
        self.prop == other.prop
            && match (self.component, other.component) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    }

    /// The value it rests at, out of the property's resting value.
    pub fn rest_of(self, whole: Value) -> Value {
        match self.component {
            None => whole,
            Some(i) => Value::Number(whole.components()[i]),
        }
    }

    /// Whether a lone number sets every component of the target.
    pub fn broadcasts(self) -> bool {
        self.component.is_none() && self.prop.spec().broadcast
    }

    /// Whether the target moves along a path: a spatial property animated whole.
    pub fn moves_in_space(self) -> bool {
        self.component.is_none() && self.prop.spec().spatial
    }
}

/// Every name a layer track may animate: properties, then their components.
pub fn target_names() -> impl Iterator<Item = &'static str> + Clone {
    LAYER.target_names()
}

/// The layer property names in registry order.
pub fn property_names() -> impl Iterator<Item = &'static str> + Clone {
    LAYER.names()
}

#[cfg(test)]
#[path = "../tests/unit/property.rs"]
mod tests;
