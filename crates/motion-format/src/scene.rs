//! The typed scene model the compiler consumes, and [`Document`], which keeps
//! it with the JSON it was read from.

use crate::address::Id;
use crate::{Address, Result};
use serde_json::Value as Json;

use crate::anim::AnimTrack;
use crate::item::Item;
use crate::parse::scene_from_value;
use crate::preset::{PresetSpec, lookup, names};
use crate::property::Prop;
use crate::shorthand::{Behaviour, Matte, Position, RectShape, Shutter};
use crate::slots::Slot;
use crate::text::TextContent;
use crate::units::{Color, Size, Time, TimeSpan};
use crate::value::Value;

/// A whole scene: its root composition, which is the canvas the scene
/// draws, and what every composition shares.
#[derive(Debug, Clone, PartialEq)]
pub struct Scene {
    /// Format version; always 1 for now.
    pub version: u32,
    /// The scene's own canvas, clock and layers, a composition like any
    /// other, whose id is [`SCENE`](crate::address::SCENE). Its background
    /// is opaque black unless set.
    pub root: Comp,
    /// The scene's own presets, which hide built-ins of the same name.
    pub presets: Vec<(String, PresetSpec)>,
    /// Font files and folders, relative to the scene file, besides the built-in Inter.
    pub fonts: Vec<String>,
    /// Compositions that `comp` layers draw, in the order written.
    pub comps: Vec<Comp>,
    /// What its layers and slots do ([D50]).
    ///
    /// [D50]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d50-roles-name-what-a-masters-layers-do
    pub roles: Vec<crate::roles::Role>,
    /// What a variant may not change ([D51]).
    ///
    /// [D51]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d51-locks-are-refused-in-the-edit
    pub locks: Vec<crate::Lock>,
}

/// A composition: a canvas of its own, with its own clock and layers, that
/// any number of `comp` layers draw as their content.
#[derive(Debug, Clone, PartialEq)]
pub struct Comp {
    /// Its name in `comps`, which `comp` layers write, or
    /// [`SCENE`](crate::address::SCENE) for the scene's root.
    pub id: String,
    /// Its canvas in pixels, which is also the box of a layer that draws it.
    pub size: Size,
    /// Its frames per second: its layers' times count in its frames.
    pub fps: f64,
    /// Its length; a layer that draws it shows nothing past it.
    pub duration: Time,
    /// The colour behind its layers; clear unless set.
    pub background: Color,
    /// The shutter its layers with `mblur` blur over.
    pub shutter: Shutter,
    /// Its layers in paint order: the first is at the back.
    pub layers: Vec<Layer>,
    /// Its named moments, resolved, in the order written.
    pub labels: Vec<(String, Time)>,
    /// The named values it declares, which its layers' values may follow
    /// (D44). The values are already resolved in `layers`.
    pub slots: Vec<Slot>,
}

impl Scene {
    /// Finds a preset by name: the scene's own first, then the built-ins.
    pub fn preset(&self, name: &str) -> Option<&PresetSpec> {
        lookup(&self.presets, name)
    }

    /// Every usable preset name, the scene's own first, without duplicates.
    pub fn preset_names(&self) -> Vec<&str> {
        names(&self.presets)
    }

    /// The composition called `id`, if the scene has one.
    pub fn comp(&self, id: &str) -> Option<&Comp> {
        self.comps.iter().find(|c| c.id == id)
    }

    /// The layer `ids` lead to from the root, through the `comp` layers on
    /// the way, as it is drawn there: with their overrides set.
    pub fn layer(&self, ids: &[Id]) -> Option<&Layer> {
        let (last, outer) = ids.split_last()?;
        let mut layers = self.root.layers.as_slice();
        for id in outer {
            let layer = layers.iter().find(|l| l.id == id.as_ref())?;
            layers = self.drawn(layer)?;
        }
        layers.iter().find(|l| l.id == last.as_ref())
    }

    /// The layer `address` names, at any depth as drawn ([`Scene::layer`]),
    /// or as written in a composition: `scene.comps.card.layers[title]`
    /// ([`Address::written`]). `None` for any other address.
    pub fn find(&self, address: &Address) -> Option<&Layer> {
        match (address.layers.is_empty(), address.written_in_comp()) {
            (false, _) if address.keys.is_empty() => self.layer(&address.layers),
            (true, Some((comp, id))) => self.comp(comp)?.layers.iter().find(|l| l.id == id),
            _ => None,
        }
    }

    /// The layers a `comp` layer draws: its own instance when it overrides
    /// any, else its composition's. `None` for any other layer.
    pub fn drawn<'a>(&'a self, layer: &'a Layer) -> Option<&'a [Layer]> {
        let Content::Comp(comp) = &layer.content else {
            return None;
        };
        match &comp.instance {
            Some(layers) => Some(layers),
            None => self.comp(&comp.id).map(|c| c.layers.as_slice()),
        }
    }

    /// The address of every layer at any depth, in paint order: each layer,
    /// then the layers of the composition it draws.
    pub fn addresses(&self) -> Vec<Address> {
        fn collect(scene: &Scene, layers: &[Layer], outer: &[Id], all: &mut Vec<Address>) {
            for layer in layers {
                let Ok(id) = layer.id.parse::<Id>() else {
                    continue;
                };
                let ids: Vec<Id> = outer.iter().cloned().chain([id]).collect();
                all.push(Address::layer(ids.iter().cloned()));
                if let Some(inner) = scene.drawn(layer) {
                    collect(scene, inner, &ids, all);
                }
            }
        }
        let mut all = Vec::new();
        collect(self, &self.root.layers, &[], &mut all);
        all
    }
}

/// One thing on the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// Unique name, used to address the layer in edits.
    pub id: String,
    /// What the layer draws.
    pub content: Content,
    /// The fill colour of a rect layer, straight RGBA from 0 to 1; a text
    /// layer's colours are in its runs.
    pub fill: [f64; 4],
    /// Where the layer's anchor sits, in its parent's box or on the canvas:
    /// its centre, unless `anchor` moves it.
    pub at: Position,
    /// Resting values set on the layer, by property, in the order written.
    pub values: Vec<(Prop, Value)>,
    /// Entrance behaviour; JSON key `in`.
    pub enter: Option<Behaviour>,
    /// Free-form animation tracks.
    pub anim: Vec<AnimTrack>,
    /// The layer it moves with: its position, rotation and scale apply on top
    /// of the parent's.
    pub parent: Option<String>,
    /// When it shows, on its composition's timeline; always when `None`.
    pub span: Option<TimeSpan>,
    /// Where its own time 0 falls on its composition's timeline. Its keys and
    /// its entrance are in its own time, so moving it is one edit.
    pub offset: Time,
    /// How slowly its own time runs: 2 plays at half speed; a negative value
    /// runs backwards from `offset`, so its keys play before `offset`.
    pub stretch: f64,
    /// Masks that cut what it draws, combined in order, before its effects.
    pub mask: Vec<Item>,
    /// Effects on what it draws, applied in order.
    pub fx: Vec<Item>,
    /// Its effects apply to what is below it in its composition instead,
    /// where it draws (everywhere for a filter layer, which draws nothing),
    /// and it does not draw itself.
    pub adjust: bool,
    /// A `comp` layer whose composition's layers draw straight into the
    /// canvas it sits in, under its transform: not clipped to the
    /// composition's canvas, without its background, and with its effects
    /// run over the canvas it sits in, in that canvas's coordinates.
    pub collapse: bool,
    /// Left out of the frame, yet kept in the scene; its children still move with it.
    pub hidden: bool,
    /// The layer of its own list whose drawing decides where it shows.
    pub matte: Option<Matte>,
    /// How it blends with what is below it in its composition, one of
    /// [`BLEND_MODES`].
    pub blend: &'static str,
    /// It is blurred along its motion, over its composition's shutter.
    pub mblur: bool,
    /// It is a plane in its composition's 3D space, seen through the
    /// composition's camera ([D58]); a camera always is.
    ///
    /// [D58]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d58-25d-layers-are-planes-projected-through-a-camera
    pub three_d: bool,
}

/// The blend modes a layer takes, `normal` first, which is the default.
pub const BLEND_MODES: [&str; 17] = [
    "normal",
    "multiply",
    "screen",
    "overlay",
    "darken",
    "lighten",
    "color-dodge",
    "color-burn",
    "hard-light",
    "soft-light",
    "difference",
    "exclusion",
    "hue",
    "saturation",
    "color",
    "luminosity",
    "add",
];

impl Layer {
    /// The resting value it sets for `prop`, if any.
    pub fn value(&self, prop: Prop) -> Option<Value> {
        self.values
            .iter()
            .find(|(p, _)| *p == prop)
            .map(|(_, v)| v.clone())
    }

    /// Where `prop` rests on this layer: the value it sets, else the
    /// registry's rest, with `at` resolved in `extent` for a box of `own`.
    ///
    /// A camera that sets no depth rests at minus its zoom, so it sees a
    /// plane at depth 0 at 100 %.
    pub fn rest(&self, prop: Prop, extent: kurbo::Rect, own: kurbo::Rect) -> Value {
        if let Some(value) = self.value(prop) {
            return value;
        }
        match (&prop.spec().rest, &self.content) {
            (crate::property::Rest::Depth, Content::Camera(_)) => {
                let zoom = self.rest(Prop::ZOOM, extent, own);
                Value::Number(-zoom.as_number().expect("zoom is a number"))
            }
            _ => prop.spec().rest(&self.at, extent, own),
        }
    }
}

/// What a layer draws.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// Text, with its style.
    Text(Box<TextContent>),
    /// A rectangle, possibly with rounded corners.
    Rect(RectShape),
    /// Shape items: groups, outlines and the paints that draw them. The
    /// first item draws on top.
    Shapes(Vec<Item>),
    /// A composition, drawn in the layer's box at the layer's time.
    Comp(CompContent),
    /// A picture file stretched over the layer's box.
    Image(ImageContent),
    /// A video clip played in the layer's own time over the layer's box.
    Video(VideoContent),
    /// A sound file played in the layer's own time; it draws nothing.
    Audio(AudioContent),
    /// Nothing: a point that other layers move with.
    Pivot,
    /// Nothing: the viewpoint of its composition's 3D layers.
    Camera(CameraContent),
    /// Nothing of its own: its effects apply to what is below it in its
    /// composition, mixed in by its opacity and matte.
    Filter,
}

/// A camera: where it looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CameraContent {
    /// It turns to look at its `target`, which the layer sets or animates:
    /// a two-node camera. Otherwise it looks along its own depth axis.
    pub aim: bool,
}

/// What a `comp` layer draws: a composition, as it is or with the layer's
/// `overrides` set over its layers.
#[derive(Debug, Clone, PartialEq)]
pub struct CompContent {
    /// The composition's id under `comps`.
    pub id: String,
    /// The composition's layers with the layer's overrides set over them,
    /// validated as any composition's layers are; `None` when it writes
    /// none, and draws the composition every such layer shares.
    pub instance: Option<Vec<Layer>>,
}

/// An image layer's picture and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageContent {
    /// The PNG or JPEG file, as written: relative to the scene file's folder,
    /// or absolute.
    pub path: String,
    /// The layer's box, which the picture's pixels are stretched to fill,
    /// in the layer's own coordinates.
    pub frame: kurbo::Rect,
}

/// What a `video` layer plays: a clip file, and the layer's box its frames
/// fill.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoContent {
    /// The clip, as written: relative to the scene file's folder, or absolute.
    pub path: String,
    /// The layer's box, which the frames are stretched to fill, in the
    /// layer's own coordinates.
    pub frame: kurbo::Rect,
}

/// What an `audio` layer plays.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioContent {
    /// The sound file, as written: relative to the scene file's folder, or
    /// absolute.
    pub path: String,
}

/// A parsed scene file: the typed scene and the JSON it came from, kept so
/// that writing it back preserves what the author wrote.
///
/// [`Document::from_value`] is the only constructor, so the scene is always
/// the validated reading of the raw value: layer `i` of one is layer `i` of the other.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    raw: Json,
    scene: Scene,
}

impl Document {
    /// Validates `raw` and keeps it with the [`Scene`] it describes.
    ///
    /// # Errors
    /// A [`Diagnostic`](crate::Diagnostic) for the first part of `raw` that is
    /// not a valid scene, at its path.
    pub fn from_value(raw: Json) -> Result<Document> {
        let scene = scene_from_value(&raw)?;
        Ok(Document { raw, scene })
    }

    /// The JSON as read.
    pub fn raw(&self) -> &Json {
        &self.raw
    }

    /// The validated scene the JSON describes.
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Every slot of the scene, its own first and then each composition's,
    /// with the place of each value that follows it ([D44]).
    ///
    /// [D44]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d44-slots-are-named-values-that-layers-follow
    pub fn slots(&self) -> Vec<crate::Listed> {
        crate::slots::listed(&self.raw)
    }

    /// The JSON of the layer `ids` lead to from the root, as it is drawn
    /// there: its composition's own layer with every override on the way
    /// set over it. `None` when there is no such layer.
    pub fn layer_json(&self, ids: &[Id]) -> Option<serde_json::Map<String, Json>> {
        crate::overrides::layer_json(&self.raw, ids)
    }

    /// The JSON as read, dropping the scene.
    pub fn into_raw(self) -> Json {
        self.raw
    }
}
