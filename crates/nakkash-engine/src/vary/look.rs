//! The look axis ([D78]): the same template as another film, designed
//! element by element rather than passed through one colour map.
//!
//! A look reads what each layer of the master draws: the **field** (its
//! generated texture: a noise and what colours it), the **title** (text,
//! a logo, and the copies and effects a template draws them with) and the
//! **ground** under them. It then makes a decision for each:
//!
//! - **Colour by role.** The ground becomes a designed gradient; each of
//!   the field's colouring curves and fills takes one of the look's field
//!   colours in turn, so layers the master coloured alike part; the
//!   title's chromatic copies take the look's split colours; text takes
//!   the look's text colour. Neutral fills, mattes and logos stay.
//! - **Material.** The noise's kind and fractal, its cells and its speed;
//!   the rings dropped, widened or echoed; grain as a layer of its own.
//! - **Effects by element.** Glow and blur on the field, glow on the
//!   title, and the title's glitch (its displacements and channel shifts)
//!   kept or switched off.
//! - **Type.** A face heavy enough for the master's strokes, its case and
//!   its tracking, filled, outlined or extruded.
//! - **Motion.** A pace through the timing axis.
//!
//! No look writes a frame-wide map: [`super::Distance::filter`] measures
//! how much of a variant no colour transform of the master explains, and
//! the gate fails a look that a filter could have made.
//!
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

use nakkash_format::item::EFFECTS;
use nakkash_format::property::Registry;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json, json};

use super::places::{self, Held, Visit};
use super::rng::Rng;
use super::typeface::{Family, TypeStep, nearest};

/// The looks a spec may name, in the order a run without names takes them.
pub const LOOKS: [&str; 3] = ["aurora", "riso", "ember"];

/// How a look treats the text.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Treatment {
    /// Filled.
    Fill,
    /// Drawn as its outline only, this many px wide at the master's size.
    Outline(f64),
    /// Filled, over a dark extrusion as deep as this share of the text's
    /// size.
    Extrude(f64),
}

/// What a look does to the rings a template cuts: layers whose masks
/// subtract one shape from another.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rings {
    /// As the master draws them.
    Keep,
    /// Not drawn.
    Off,
    /// This many px wider, or narrower when negative, at the master's size.
    Width(f64),
    /// This many more of each, each inside the last.
    Echo(u32),
}

/// The case a look sets text in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Case {
    /// As the master writes it.
    AsIs,
    /// Capitals.
    Upper,
    /// Small letters.
    Lower,
}

/// A look's colours, by the role they play.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    /// The ground: its middle, then its edges, a radial gradient.
    pub ground: [String; 2],
    /// The field's colours, given in turn to the layers that colour it.
    pub field: Vec<String>,
    /// The title's chromatic copies: what was red, green and blue.
    pub split: [String; 3],
    /// Text.
    pub text: String,
}

/// What a look does to the field, the template's generated texture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    /// The noise's kind (`block`, `linear`, `soft-linear`, `spline`), or
    /// the master's.
    pub noise: Option<String>,
    /// The noise's fractal (`basic`, `turbulent-smooth`, …), or the
    /// master's.
    pub fractal: Option<String>,
    /// How much its contrast is multiplied by.
    pub contrast: f64,
    /// How much faster it travels and evolves.
    pub speed: f64,
    /// How much wider and taller its cells are.
    pub cells: [f64; 2],
    /// How much its glows' intensity is multiplied by; 0 drops them.
    pub glow: f64,
    /// How much its blurs are multiplied by.
    pub blur: f64,
}

/// What a look does to the title: its text, logo and the copies and
/// effects they are drawn with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Title {
    /// How much its glows' intensity is multiplied by; 0 drops them.
    pub glow: f64,
    /// Whether its displacements and channel shifts stay.
    pub glitch: bool,
    /// How far its displacements move it, times the master's: below 1 the
    /// letterforms come through a texture more whole.
    #[serde(default = "one")]
    pub displace: f64,
    /// Filled, outlined or extruded.
    pub treatment: Treatment,
    /// Its case.
    pub case: Case,
    /// Tracking added, in thousandths of an em.
    pub tracking: f64,
    /// The curve its moves (position, scale, rotation, opacity) take
    /// between keys, as a scene writes one (`out-expo`, `out-back`, …), or
    /// the master's.
    pub ease: Option<String>,
}

/// One look, every value drawn: what the recipe records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LookStep {
    /// The look's name.
    pub name: String,
    /// Its colours by role.
    pub palette: Palette,
    /// The field.
    pub field: Field,
    /// The rings.
    pub rings: Rings,
    /// The title.
    pub title: Title,
    /// The opacity of a grain layer laid over the film; 0 lays none.
    pub grain: f64,
    /// The pace of its moves, through the timing axis.
    pub pace: Option<super::Pace>,
    /// The text's new face, fitted into its boxes, when the scene can load
    /// one the look asks for.
    pub face: Option<TypeStep>,
}

impl fmt::Display for LookStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "look {}", self.name)?;
        if let Some((family, weight)) = self.face.as_ref().and_then(|t| t.headline.as_ref()) {
            write!(f, ", {family} {weight}")?;
        }
        match self.title.treatment {
            Treatment::Fill => {}
            Treatment::Outline(_) => write!(f, ", outlined")?,
            Treatment::Extrude(_) => write!(f, ", extruded")?,
        }
        if let Some(noise) = &self.field.noise {
            write!(f, ", {noise} noise")?;
        }
        let [w, h] = self.field.cells;
        if (w - 1.0).abs() > 0.05 || (h - 1.0).abs() > 0.05 {
            write!(f, ", cells ×{w:.2}/×{h:.2}")?;
        }
        if self.field.glow == 0.0 {
            write!(f, ", flat")?;
        }
        if !self.title.glitch {
            write!(f, ", no glitch")?;
        }
        match self.rings {
            Rings::Keep => Ok(()),
            Rings::Off => write!(f, ", no rings"),
            Rings::Width(px) => write!(f, ", rings {px:+.0}px"),
            Rings::Echo(n) => write!(f, ", rings ×{}", n + 1),
        }
    }
}

/// The most a look moves a template's noise contrast, either way: the
/// rays and textures of a template are its noise passed through tight
/// curves and levels, so its brightness and complexity are left alone and
/// its contrast moves a tenth at most, or the shapes break up.
const NOISE_CONTRAST: (f64, f64) = (0.9, 1.1);

/// A look as it is written down, before the seed moves it a little.
struct Preset {
    name: &'static str,
    ground: [&'static str; 2],
    field: &'static [&'static str],
    split: [&'static str; 3],
    text: &'static str,
    noise: Option<&'static str>,
    fractal: Option<&'static str>,
    contrast: f64,
    speed: f64,
    cells: [f64; 2],
    field_glow: f64,
    blur: f64,
    rings: Rings,
    title_glow: f64,
    glitch: bool,
    displace: f64,
    treatment: Treatment,
    case: Case,
    tracking: f64,
    ease: Option<&'static str>,
    grain: f64,
    pace: Option<super::Pace>,
    /// Faces it prefers, in order, by family name without spaces or case.
    faces: &'static [&'static str],
}

/// The three looks, each a set of decisions, not a colour map:
///
/// - `aurora`: an indigo ground opening in the middle; a smooth silk
///   noise in teal, pink and periwinkle layer by layer, as light as each
///   other so they part by hue, glowing and streaked long; no rings; a
///   clean white capital title, widely spaced, its glitch off; a calm pace.
/// - `riso`: a warm charcoal ground; the master's blocky noise in two
///   printing inks and a yellow, flat (no glow) and barely streaked, its
///   cells tall and thin; wide rings; a yellow capital title whose split
///   copies misregister like a print; grain; a snappy pace.
/// - `ember`: a dark warm ground; a smeared noise in orange and a
///   raspberry red, with teal sparks against them, its cells long and low;
///   rings echoed; a cream title in small letters, extruded, glitch off.
const PRESETS: [Preset; 3] = [
    Preset {
        name: "aurora",
        ground: ["#1B1F5E", "#05060F"],
        field: &["#2DE2D0", "#FF6EC7", "#8FA2FF"],
        split: ["#FF6EC7", "#2DE2D0", "#8FA2FF"],
        text: "#F2F4FF",
        noise: Some("spline"),
        fractal: None,
        contrast: 0.95,
        speed: 0.8,
        cells: [1.6, 1.4],
        field_glow: 1.1,
        blur: 1.3,
        rings: Rings::Off,
        title_glow: 0.2,
        glitch: false,
        displace: 1.0,
        treatment: Treatment::Fill,
        case: Case::Upper,
        tracking: 80.0,
        ease: Some("in-out-sine"),
        grain: 0.0,
        pace: Some(super::Pace::Calm),
        faces: &["raleway", "kumbhsans"],
    },
    Preset {
        name: "riso",
        ground: ["#2A2118", "#0E0B08"],
        field: &["#FF48B0", "#00A0E0", "#FFE800"],
        split: ["#FF48B0", "#FFE800", "#F4EEDC"],
        text: "#FFE800",
        noise: None,
        fractal: None,
        contrast: 1.08,
        speed: 1.2,
        cells: [0.5, 2.2],
        field_glow: 0.0,
        blur: 0.4,
        rings: Rings::Width(6.0),
        title_glow: 0.3,
        glitch: true,
        displace: 0.4,
        treatment: Treatment::Fill,
        case: Case::Upper,
        tracking: 20.0,
        ease: Some("out-expo"),
        grain: 0.1,
        pace: Some(super::Pace::Snappy),
        faces: &["kumbhsans", "raleway"],
    },
    Preset {
        name: "ember",
        ground: ["#3A1206", "#0A0302"],
        field: &["#FF8A1F", "#FF3B5C", "#3EC7C2"],
        split: ["#FF3B5C", "#FFD36B", "#FFF1DC"],
        text: "#FFF1DC",
        noise: None,
        fractal: Some("smeary"),
        contrast: 1.0,
        speed: 0.6,
        cells: [2.6, 0.7],
        field_glow: 1.0,
        blur: 1.0,
        rings: Rings::Echo(2),
        title_glow: 0.5,
        glitch: false,
        displace: 1.0,
        treatment: Treatment::Extrude(0.03),
        case: Case::Lower,
        tracking: -10.0,
        ease: Some("out-back"),
        grain: 0.05,
        pace: None,
        faces: &["sourceserifpro", "kumbhsans"],
    },
];

/// The families a look may set text in.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Faces<'a> {
    /// Those the scene can load with the spec's folders.
    pub families: &'a [Family],
    /// Those folders.
    pub fonts: &'a [PathBuf],
    /// How thick the master's heaviest text is drawn, in ems
    /// ([`super::typeface::thickest`]).
    pub stroke: Option<f64>,
}

/// How near the master's heaviest stroke a look's family must reach to be
/// taken in its order: a tenth thinner at most.
const THICK_ENOUGH: f64 = 0.9;

/// Draws look `index` of a run: the spec's names in turn (every look when
/// it names none), each number moved by up to 8 % by the seed so two runs
/// on two seeds are two films. `faces` are the families it may set text in
/// ([`Faces`]).
pub(crate) fn draw(
    names: &[String],
    index: usize,
    faces: &Faces<'_>,
    typefaces_locked: bool,
    rng: &mut Rng,
) -> LookStep {
    let names: Vec<&str> = match names.is_empty() {
        true => LOOKS.to_vec(),
        false => names.iter().map(String::as_str).collect(),
    };
    let name = names[index % names.len()];
    let preset = PRESETS
        .iter()
        .find(|p| p.name == name)
        .unwrap_or(&PRESETS[0]);
    let mut jitter = |v: f64| (v * rng.range(0.92, 1.08) * 1000.0).round() / 1000.0;
    let field = Field {
        noise: preset.noise.map(str::to_owned),
        fractal: preset.fractal.map(str::to_owned),
        contrast: jitter(preset.contrast).clamp(NOISE_CONTRAST.0, NOISE_CONTRAST.1),
        speed: jitter(preset.speed),
        cells: [jitter(preset.cells[0]), jitter(preset.cells[1])],
        glow: jitter(preset.field_glow),
        blur: jitter(preset.blur),
    };
    let title = Title {
        glow: jitter(preset.title_glow),
        glitch: preset.glitch,
        displace: preset.displace,
        treatment: preset.treatment,
        case: preset.case,
        tracking: preset.tracking,
        ease: preset.ease.map(str::to_owned),
    };
    let key = |s: &str| s.to_lowercase().replace([' ', '-', '_'], "");
    // The look's families in its order; the first whose heaviest weight
    // draws strokes about as thick as the master's heaviest text, else the
    // first of the folders' other families that does (no mono, no
    // script), else the heaviest of all: a template that splits or
    // outlines a poster face needs strokes as thick to stay readable.
    let wanted: Vec<&Family> = preset
        .faces
        .iter()
        .filter_map(|want| {
            faces
                .families
                .iter()
                .find(|f| key(&f.name).starts_with(want))
        })
        .collect();
    let heaviest = |f: &Family| f.strokes.iter().copied().fold(0.0, f64::max);
    let chosen: Option<&Family> = match faces.stroke {
        Some(stroke) => {
            let enough = |f: &&Family| heaviest(f) >= stroke * THICK_ENOUGH;
            let others = faces
                .families
                .iter()
                .filter(|f| !matches!(f.class.as_str(), "mono" | "script"));
            wanted
                .iter()
                .copied()
                .find(enough)
                .or_else(|| others.clone().find(enough))
                .or_else(|| {
                    wanted
                        .iter()
                        .copied()
                        .chain(others)
                        .max_by(|a, b| heaviest(a).total_cmp(&heaviest(b)))
                })
        }
        None => wanted.first().copied(),
    };
    let face = (!typefaces_locked)
        .then_some(chosen)
        .flatten()
        .map(|family| {
            let weight = nearest(&family.weights, 900);
            TypeStep {
                headline: Some((family.name.clone(), weight)),
                body: Some(family.name.clone()),
                copy: None,
                fonts: faces.fonts.to_vec(),
                families: faces.families.to_vec(),
            }
        });
    let owned = |s: &[&str]| s.iter().map(|c| (*c).to_owned()).collect::<Vec<_>>();
    LookStep {
        name: preset.name.to_owned(),
        palette: Palette {
            ground: preset.ground.map(str::to_owned),
            field: owned(preset.field),
            split: preset.split.map(str::to_owned),
            text: preset.text.to_owned(),
        },
        field,
        rings: preset.rings,
        title,
        grain: preset.grain,
        pace: preset.pace,
        face,
    }
}

/// `step` with its colour and effect decisions only: no moves of the
/// noise or rings, no type, case, tracking, ease, pace or grain, so a look
/// drawn this way keeps the master's drawing where it was
/// ([`super::colour_twin`]).
pub(crate) fn colour_only(step: &LookStep) -> LookStep {
    LookStep {
        name: step.name.clone(),
        palette: step.palette.clone(),
        field: Field {
            noise: None,
            fractal: None,
            contrast: 1.0,
            speed: 1.0,
            cells: [1.0, 1.0],
            ..step.field.clone()
        },
        rings: Rings::Keep,
        title: Title {
            treatment: Treatment::Fill,
            case: Case::AsIs,
            tracking: 0.0,
            ease: None,
            displace: 1.0,
            ..step.title.clone()
        },
        grain: 0.0,
        pace: None,
        face: None,
    }
}

/// What a layer draws, for a look: the part of the film it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// The generated texture: a noise, or what is drawn from one.
    Field,
    /// Text, a logo, and the copies and effects they are drawn with.
    Title,
    /// Both, or neither: a container, a matte, an adjustment.
    Other,
}

/// The compositions that draw a noise, and those that draw text or a
/// picture, each through any depth of compositions inside them. Hidden
/// layers do not count: a noise a title is displaced by is not drawn.
#[derive(Debug, Default)]
struct Kinds {
    noise: BTreeSet<String>,
    title: BTreeSet<String>,
}

/// Reads [`Kinds`] off `raw`'s compositions.
fn kinds(raw: &Json) -> Kinds {
    let Some(Json::Object(comps)) = raw.get("comps") else {
        return Kinds::default();
    };
    let layers = |id: &str| -> Vec<&Map<String, Json>> {
        comps
            .get(id)
            .and_then(|c| c.get("layers"))
            .and_then(Json::as_array)
            .map(|l| l.iter().filter_map(Json::as_object).collect())
            .unwrap_or_default()
    };
    let mut kinds = Kinds::default();
    // Their own layers first, then whatever they hold, until nothing moves.
    for id in comps.keys() {
        for layer in layers(id).into_iter().filter(|l| !hidden(l)) {
            if draws_noise(layer) {
                kinds.noise.insert(id.clone());
            }
            if layer.contains_key("text") || layer.contains_key("image") {
                kinds.title.insert(id.clone());
            }
        }
    }
    loop {
        let mut grew = false;
        for id in comps.keys() {
            for layer in layers(id).into_iter().filter(|l| !hidden(l)) {
                let Some(inner) = layer.get("comp").and_then(Json::as_str) else {
                    continue;
                };
                if kinds.noise.contains(inner) && !kinds.noise.contains(id) {
                    kinds.noise.insert(id.clone());
                    grew = true;
                }
                if kinds.title.contains(inner) && !kinds.title.contains(id) {
                    kinds.title.insert(id.clone());
                    grew = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    kinds
}

/// Whether `layer` is hidden: drawn only as another's map or matte.
fn hidden(layer: &Map<String, Json>) -> bool {
    layer.get("hidden").and_then(Json::as_bool) == Some(true)
}

/// Whether `layer` draws a noise of its own.
fn draws_noise(layer: &Map<String, Json>) -> bool {
    layer
        .get("fx")
        .and_then(Json::as_array)
        .is_some_and(|fx| fx.iter().any(|e| e.get("fractal-noise").is_some()))
}

/// The role of `layer`, in composition `comp` (`None` for the scene's own
/// layers).
fn role(kinds: &Kinds, comp: Option<&str>, layer: &Map<String, Json>) -> Role {
    let noise = |id: &str| kinds.noise.contains(id);
    let title = |id: &str| kinds.title.contains(id);
    if layer.contains_key("text") || layer.contains_key("image") {
        return Role::Title;
    }
    if draws_noise(layer) {
        return Role::Field;
    }
    if let Some(inner) = layer.get("comp").and_then(Json::as_str) {
        return match (noise(inner), title(inner)) {
            (true, false) => Role::Field,
            (false, true) => Role::Title,
            _ => Role::Other,
        };
    }
    match comp {
        Some(c) if noise(c) && !title(c) => Role::Field,
        Some(c) if title(c) && !noise(c) => Role::Title,
        _ => Role::Other,
    }
}

/// Makes the look's decisions on every layer the locks leave free, in
/// place; says what it did.
pub(crate) fn apply(raw: &mut Json, held: &Held, step: &LookStep) -> String {
    let kinds = kinds(raw);
    let sizes = canvases(raw);
    let carriers = carriers(raw);
    let sources = sources(raw);
    let largest = largest_text(raw);
    let mut counts = Counts::default();
    let mut field_turn = 0usize;
    places::each_mut(raw, |visit| {
        let Visit::Layer { place, layer } = visit else {
            return;
        };
        // What is held, and what is drawn only to be read, stays.
        if held.holds(place) || place.comp.as_ref().is_some_and(|c| sources.contains(c)) {
            return;
        }
        let role = role(&kinds, place.comp.as_deref(), layer);
        let colour = match role {
            Role::Field => {
                let c = &step.palette.field[field_turn % step.palette.field.len().max(1)];
                c.clone()
            }
            _ => step.palette.text.clone(),
        };
        let mut coloured = false;
        // A composition another layer reads by one of its channels carries
        // data in its colours, not colour: they stay.
        let data = place.comp.as_ref().is_some_and(|c| carriers.contains(c));
        let channel = channel_of(layer);
        if let Some(Json::Array(fx)) = layer.get_mut("fx") {
            let before = fx.len();
            fx.retain(|item| keeps(item, role, step));
            counts.dropped += before - fx.len();
            for item in fx.iter_mut().filter_map(Json::as_object_mut) {
                counts.moved += move_effect(item, role, step);
                if !data && recolour(item, role, &colour, step, channel) {
                    counts.recoloured += 1;
                    coloured = true;
                }
            }
        }
        if coloured && role == Role::Field {
            field_turn += 1;
        }
        if role == Role::Title
            && let Some(ease) = &step.title.ease
        {
            counts.eased += eased(layer, ease);
        }
        if layer.contains_key("text") {
            counts.texts += usize::from(set_text(layer, step));
            // A treatment is for the headline: a smaller line outlined or
            // extruded at its size stops reading.
            let size = font_size(layer).unwrap_or(0.0);
            if size >= largest * HEADLINE {
                counts.texts += usize::from(treat(layer, step.title.treatment, size));
            }
        }
        let canvas = sizes.get(&place.comp).copied();
        if canvas.is_some_and(|c| is_ground(layer, c)) {
            counts.ground += usize::from(ground(layer, step));
        }
    });
    let rings = rings(raw, held, step.rings);
    let grain = grain(raw, step);
    format!(
        "{step}: {} field layers coloured in turn, {} colours set by role, {} effect settings \
         moved, {} effects switched off, {} texts set, {} title keys eased, {} grounds, \
         {rings} rings changed{}",
        field_turn,
        counts.recoloured,
        counts.moved,
        counts.dropped,
        counts.texts,
        counts.eased,
        counts.ground,
        if grain { ", grain laid over" } else { "" },
    )
}

/// A text at least this share of the largest text's size is a headline,
/// and takes the look's treatment.
const HEADLINE: f64 = 0.6;

/// The size a text layer's `font` is set at: its last number.
fn font_size(layer: &Map<String, Json>) -> Option<f64> {
    let font = layer.get("font")?.as_str()?;
    font.split_whitespace()
        .rev()
        .find_map(|w| w.parse::<f64>().ok())
}

/// The largest size any text of the scene is set at.
fn largest_text(raw: &Json) -> f64 {
    let mut most: f64 = 0.0;
    let mut walk = |layers: Option<&Json>| {
        for l in layers.and_then(Json::as_array).into_iter().flatten() {
            if let Some(l) = l.as_object()
                && l.contains_key("text")
            {
                most = most.max(font_size(l).unwrap_or(0.0));
            }
        }
    };
    walk(raw.get("layers"));
    if let Some(Json::Object(comps)) = raw.get("comps") {
        for c in comps.values() {
            walk(c.get("layers"));
        }
    }
    most
}

/// The channel a layer reads itself or another by (`set-matte` red, green
/// or blue), as 0, 1 or 2.
fn channel_of(layer: &Map<String, Json>) -> Option<usize> {
    layer.get("fx")?.as_array()?.iter().find_map(|item| {
        if item.get("hidden").and_then(Json::as_bool) == Some(true) {
            return None;
        }
        match item.get("set-matte")?.as_str()? {
            "red" => Some(0),
            "green" => Some(1),
            "blue" => Some(2),
            _ => None,
        }
    })
}

/// The compositions some layer reads by one of their colour channels: the
/// composition it draws, when it reads itself, or the one the layer it
/// names as its matte draws. Their colours are data.
fn carriers(raw: &Json) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut lists: Vec<&Vec<Json>> = Vec::new();
    if let Some(Json::Array(l)) = raw.get("layers") {
        lists.push(l);
    }
    if let Some(Json::Object(comps)) = raw.get("comps") {
        for c in comps.values() {
            if let Some(Json::Array(l)) = c.get("layers") {
                lists.push(l);
            }
        }
    }
    for list in lists {
        let comp_of = |id: &str| {
            list.iter()
                .filter_map(Json::as_object)
                .find(|l| l.get("id").and_then(Json::as_str) == Some(id))
                .and_then(|l| l.get("comp"))
                .and_then(Json::as_str)
                .map(str::to_owned)
        };
        for layer in list.iter().filter_map(Json::as_object) {
            let Some(Json::Array(fx)) = layer.get("fx") else {
                continue;
            };
            for item in fx {
                let channel = item
                    .get("set-matte")
                    .and_then(Json::as_str)
                    .is_some_and(|c| matches!(c, "red" | "green" | "blue"));
                if !channel {
                    continue;
                }
                let read = match item.get("matte").and_then(Json::as_str) {
                    Some(other) => comp_of(other),
                    None => layer.get("comp").and_then(Json::as_str).map(str::to_owned),
                };
                out.extend(read);
            }
        }
    }
    out
}

/// The compositions drawn only to be read: a hidden layer another layer
/// names as its matte or its map (a displacement's) draws one, and so do
/// the compositions inside it. What they draw is data (where a title shows,
/// how far it moves), not picture, so a look leaves them whole: a noise
/// restyled as rays would cut and shift the title it mattes.
fn sources(raw: &Json) -> BTreeSet<String> {
    let mut lists: Vec<&Vec<Json>> = Vec::new();
    if let Some(Json::Array(l)) = raw.get("layers") {
        lists.push(l);
    }
    let comps = raw.get("comps").and_then(Json::as_object);
    for c in comps.into_iter().flat_map(|c| c.values()) {
        if let Some(Json::Array(l)) = c.get("layers") {
            lists.push(l);
        }
    }
    let mut out = BTreeSet::new();
    for list in lists {
        for layer in list.iter().filter_map(Json::as_object) {
            let Some(Json::Array(fx)) = layer.get("fx") else {
                continue;
            };
            for name in fx
                .iter()
                .flat_map(|item| ["matte", "map"].map(|k| item.get(k)))
                .filter_map(|v| v.and_then(Json::as_str))
            {
                let source = list
                    .iter()
                    .filter_map(Json::as_object)
                    .find(|l| l.get("id").and_then(Json::as_str) == Some(name));
                if let Some(source) = source.filter(|l| hidden(l))
                    && let Some(comp) = source.get("comp").and_then(Json::as_str)
                {
                    out.insert(comp.to_owned());
                }
            }
        }
    }
    // The compositions a source draws are read with it.
    let mut todo: Vec<String> = out.iter().cloned().collect();
    while let Some(comp) = todo.pop() {
        let inner = comps
            .and_then(|c| c.get(&comp))
            .and_then(|c| c.get("layers"))
            .and_then(Json::as_array);
        for layer in inner.into_iter().flatten() {
            if let Some(c) = layer.get("comp").and_then(Json::as_str)
                && out.insert(c.to_owned())
            {
                todo.push(c.to_owned());
            }
        }
    }
    out
}

/// What [`apply`] did, counted.
#[derive(Debug, Default)]
struct Counts {
    recoloured: usize,
    moved: usize,
    dropped: usize,
    texts: usize,
    ground: usize,
    eased: usize,
}

/// Each canvas's size: the scene's (`None`) and every composition's.
fn canvases(raw: &Json) -> std::collections::BTreeMap<Option<String>, (f64, f64)> {
    let mut out = std::collections::BTreeMap::new();
    if let Some(size) = raw.get("size").and_then(size_of) {
        out.insert(None, size);
    }
    if let Some(Json::Object(comps)) = raw.get("comps") {
        for (id, comp) in comps {
            if let Some(size) = comp.get("size").and_then(size_of) {
                out.insert(Some(id.clone()), size);
            }
        }
    }
    out
}

/// A size written `WxH` or `[w, h]`.
fn size_of(value: &Json) -> Option<(f64, f64)> {
    match value {
        Json::String(s) => {
            let (w, h) = s.split_once('x')?;
            Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
        }
        Json::Array(v) if v.len() == 2 => Some((v[0].as_f64()?, v[1].as_f64()?)),
        _ => None,
    }
}

/// The effects a look switches off on a layer of `role`: glows it sets to
/// 0, and the title's glitch when it has none.
fn keeps(item: &Json, role: Role, step: &LookStep) -> bool {
    let is = |k: &str| item.get(k).is_some();
    match role {
        Role::Field => !(is("glow") && step.field.glow == 0.0),
        Role::Title => {
            let glitch = is("displacement-map") || is("turbulent-displace") || is("shift-channels");
            let dark = is("glow") && step.title.glow == 0.0;
            !dark && (!glitch || step.title.glitch)
        }
        Role::Other => true,
    }
}

/// Moves the settings of one effect of a layer of `role`; how many it
/// moved. The field's noise takes the look's material, cells and speed,
/// its glows and blurs the look's; the title's glows the title's.
fn move_effect(item: &mut Map<String, Json>, role: Role, step: &LookStep) -> usize {
    let Some(kind) = EFFECTS.iter().find(|k| item.contains_key(k.name)) else {
        return 0;
    };
    let primary = match kind.primary {
        nakkash_format::item::Primary::Property(p) => Some(p),
        _ => None,
    };
    let field = &step.field;
    let shaped = item.contains_key("scale-width") || item.contains_key("scale-height");
    let mut moves: Vec<(&str, Change)> = match (role, kind.name) {
        (Role::Field, "fractal-noise") => vec![
            ("contrast", Change::Times(field.contrast)),
            ("evolution", Change::Times(field.speed)),
            ("scale-width", Change::Times(field.cells[0])),
            ("scale-height", Change::Times(field.cells[1])),
        ],
        (Role::Field, "glow") => vec![("intensity", Change::Times(field.glow))],
        (Role::Title, "glow") => vec![("intensity", Change::Times(step.title.glow))],
        (Role::Title, "displacement-map") => vec![
            ("horizontal", Change::Times(step.title.displace)),
            ("vertical", Change::Times(step.title.displace)),
        ],
        (Role::Field, "radial-blur" | "vector-blur") => {
            vec![("amount", Change::Times(field.blur))]
        }
        _ => Vec::new(),
    };
    let mut n = 0;
    if role == Role::Field && kind.name == "fractal-noise" {
        if !shaped {
            moves.push((
                "scale",
                Change::Times((field.cells[0] * field.cells[1]).sqrt()),
            ));
        }
        for (key, value) in [("noise", &field.noise), ("fractal", &field.fractal)] {
            if let Some(v) = value
                && item.get(key) != Some(&json!(v))
            {
                item.insert(key.into(), json!(v));
                n += 1;
            }
        }
        // The noise travels by its offset's keys: faster, they run further.
        if let Some(Json::Object(anim)) = item.get_mut("anim")
            && let Some(track) = anim.get_mut("offset")
        {
            n += usize::from(faster(track, field.speed));
        }
    }
    for (name, change) in moves {
        let range = range_of(kind.registry, name);
        let key = if primary == Some(name) {
            kind.name
        } else {
            name
        };
        if let Some(value) = item.get_mut(key) {
            n += usize::from(change_value(value, change, range));
        }
        if let Some(Json::Object(anim)) = item.get_mut("anim")
            && let Some(track) = anim.get_mut(name)
        {
            n += usize::from(change_value(track, change, range));
        }
    }
    n
}

/// A colour as red, green and blue from 0 to 1, from `#RRGGBB` or the
/// format's other ways of writing one.
fn rgb(value: &Json) -> Option<[f64; 3]> {
    nakkash_format::color(value).map(|c| [c[0], c[1], c[2]])
}

/// How far `c` is from grey: its highest channel less its lowest.
fn chroma(c: [f64; 3]) -> f64 {
    c.iter().copied().fold(0.0, f64::max) - c.iter().copied().fold(1.0, f64::min)
}

/// Gives one colouring effect of a layer of `role` the look's colour for
/// it; whether it changed it. Curves that colour (their red, green and
/// blue differ) are rewritten as a ramp from black through `colour` to a
/// light of it; chromatic fills and composites take `colour`, or on the
/// title the split colour for the primary they were nearest. Neutral fills,
/// curves that only shape levels and other layers stay.
fn recolour(
    item: &mut Map<String, Json>,
    role: Role,
    colour: &str,
    step: &LookStep,
    channel: Option<usize>,
) -> bool {
    if role == Role::Other {
        return false;
    }
    // A full tint draws the title grey from the luminance of its channel
    // copies: on the title its light end takes the look's text colour. Its
    // dark end stays the master's, so the pass is no lighter than the
    // master's where the copies are dim: another layer may read it as a
    // luminance matte (a shadow drawn where the pass is light), and a
    // lighter dark end would lay that shadow over the whole of each letter.
    // A tint to black (the shadow itself) stays.
    if role == Role::Title && item.contains_key("tint") {
        let white = item.get("white").and_then(rgb).unwrap_or([1.0; 3]);
        if chroma(white) < 0.25 && white.iter().all(|c| *c > 0.5) {
            item.insert("white".into(), json!(colour));
            return true;
        }
        return false;
    }
    if item.contains_key("curves") {
        let channels: Vec<Option<&Json>> = ["red", "green", "blue"]
            .iter()
            .map(|k| item.get(*k))
            .collect();
        let colours =
            channels.iter().any(Option::is_some) && channels.windows(2).any(|w| w[0] != w[1]);
        if !colours {
            return false;
        }
        let Some(c) = rgb(&json!(colour)) else {
            return false;
        };
        for (i, key) in ["red", "green", "blue"].iter().enumerate() {
            item.insert(
                (*key).into(),
                json!(format!(
                    "M 0,0 L 0.35,{} L 0.7,{} L 1,{}",
                    round(c[i] * 0.3),
                    round(c[i] * 0.75),
                    round(c[i])
                )),
            );
            if let Some(Json::Object(anim)) = item.get_mut("anim") {
                anim.remove(*key);
            }
        }
        return true;
    }
    let key = if item.contains_key("fill") {
        "fill"
    } else if item.contains_key("solid-composite") {
        "color"
    } else {
        return false;
    };
    let Some(was) = item.get(key).and_then(rgb) else {
        return false;
    };
    if chroma(was) < 0.25 {
        // A light fill on a layer that reads one channel of the title
        // draws that channel's piece: it takes the split colour for it.
        if role == Role::Title
            && key == "fill"
            && let Some(c) = channel
            && was.iter().all(|v| *v > 0.8)
        {
            item.insert(key.into(), json!(step.palette.split[c]));
            return true;
        }
        return false;
    }
    let new = match role {
        Role::Title => {
            let primary = (0..3)
                .max_by(|&a, &b| was[a].total_cmp(&was[b]))
                .unwrap_or(0);
            step.palette.split[primary].clone()
        }
        _ => colour.to_owned(),
    };
    item.insert(key.into(), json!(new));
    true
}

/// Sets a text layer's colour, case and tracking as the look says;
/// whether it changed it. A colour or text a slot gives stays the slot's.
fn set_text(layer: &mut Map<String, Json>, step: &LookStep) -> bool {
    let mut changed = false;
    let slot = |v: Option<&Json>| v.and_then(Json::as_str).is_some_and(|s| s.starts_with('$'));
    if !slot(layer.get("fill")) {
        changed |=
            layer.insert("fill".into(), json!(step.palette.text)) != Some(json!(step.palette.text));
    }
    if !slot(layer.get("text"))
        && let Some(Json::String(text)) = layer.get_mut("text")
    {
        let new = match step.title.case {
            Case::AsIs => text.clone(),
            Case::Upper => text.to_uppercase(),
            Case::Lower => text.to_lowercase(),
        };
        changed |= *text != new;
        *text = new;
    }
    if step.title.tracking != 0.0 {
        let style = layer
            .entry("style")
            .or_insert_with(|| Json::Object(Map::new()));
        if let Json::Object(style) = style {
            let was = style.get("tracking").and_then(Json::as_f64).unwrap_or(0.0);
            style.insert("tracking".into(), json!(round(was + step.title.tracking)));
            changed = true;
        }
    }
    changed
}

/// The tracks of a layer's moves a look's ease applies to.
const MOVES: [&str; 6] = ["position", "x", "y", "scale", "rotation", "opacity"];

/// Gives every key of `layer`'s moves the curve `ease`; how many keys it
/// changed. A key written `[t, v]` or `[t, v, curve]` takes it third, one
/// written `{"t", "v"}` as its `curve`.
fn eased(layer: &mut Map<String, Json>, ease: &str) -> usize {
    let Some(Json::Object(anim)) = layer.get_mut("anim") else {
        return 0;
    };
    let mut n = 0;
    for name in MOVES {
        let Some(Json::Array(keys)) = anim.get_mut(name) else {
            continue;
        };
        let last = keys.len().saturating_sub(1);
        for key in keys.iter_mut().take(last) {
            let set = match key {
                Json::Object(fields) => {
                    fields.insert("curve".into(), json!(ease)) != Some(json!(ease))
                }
                Json::Array(parts) if parts.len() == 2 => {
                    parts.push(json!(ease));
                    true
                }
                Json::Array(parts) if parts.len() >= 3 => {
                    let was = std::mem::replace(&mut parts[2], json!(ease));
                    was != json!(ease)
                }
                _ => false,
            };
            n += usize::from(set);
        }
    }
    n
}

/// Whether `layer` is a ground: a plain dark rect as large as the canvas
/// it sits on, `canvas`, with no effects.
fn is_ground(layer: &Map<String, Json>, canvas: (f64, f64)) -> bool {
    let Some(size) = layer.get("rect").and_then(size_of) else {
        return false;
    };
    let dark = layer
        .get("fill")
        .and_then(rgb)
        .is_some_and(|c| c.iter().all(|v| *v < 0.08));
    let bare = layer
        .get("fx")
        .and_then(Json::as_array)
        .is_none_or(Vec::is_empty);
    dark && bare && size.0 >= canvas.0 * 0.98 && size.1 >= canvas.1 * 0.98
}

/// Paints a ground as the look's: its edge colour, with a radial ramp from
/// its middle colour at the centre to the edge colour at a corner.
fn ground(layer: &mut Map<String, Json>, step: &LookStep) -> bool {
    let Some((w, h)) = layer.get("rect").and_then(size_of) else {
        return false;
    };
    let [middle, edge] = &step.palette.ground;
    layer.insert("fill".into(), json!(edge));
    layer.insert(
        "fx".into(),
        json!([{
            "ramp": 0,
            "shape": "radial",
            "start": [round(w / 2.0), round(h / 2.0)],
            "start-color": middle,
            "end": [0, 0],
            "end-color": edge,
        }]),
    );
    true
}

/// Lays a grain over the film, a layer of fine noise blended `overlay` at
/// the look's opacity; whether it laid one.
fn grain(raw: &mut Json, step: &LookStep) -> bool {
    if step.grain <= 0.0 {
        return false;
    }
    let Some((w, h)) = raw.get("size").and_then(size_of) else {
        return false;
    };
    let Some(Json::Array(layers)) = raw.get_mut("layers") else {
        return false;
    };
    layers.push(json!({
        "id": "look-grain",
        "rect": format!("{}x{}", w as u32, h as u32),
        "position": [round(w / 2.0), round(h / 2.0)],
        "fill": [0.5, 0.5, 0.5, 1],
        "blend": "overlay",
        "opacity": round(step.grain),
        "fx": [{
            "fractal-noise": 0,
            "contrast": 3,
            "scale": 0.03,
            "complexity": 2,
            "anim": {"evolution": [{"t": "0s", "v": 0}, {"t": "10s", "v": 36000}]},
        }],
    }));
    true
}

/// How much smaller each echo of a ring is than the one around it.
const ECHO_SCALE: f64 = 0.62;
/// How much fainter each echo of a ring is than the one around it.
const ECHO_OPACITY: f64 = 0.7;

/// Does to every ring the locks leave free what `rings` says; how many
/// rings it changed. A ring is a layer that subtracts one of its masks
/// from another.
fn rings(raw: &mut Json, held: &Held, rings: Rings) -> usize {
    if rings == Rings::Keep {
        return 0;
    }
    let mut changed = 0;
    let mut lists: Vec<(Option<String>, &mut Vec<Json>)> = Vec::new();
    let Json::Object(root) = raw else { return 0 };
    let mut comps = root.remove("comps");
    if let Some(Json::Array(layers)) = root.get_mut("layers") {
        lists.push((None, layers));
    }
    if let Some(Json::Object(comps)) = comps.as_mut() {
        for (id, comp) in comps.iter_mut() {
            if let Some(Json::Array(layers)) = comp.get_mut("layers") {
                lists.push((Some(id.clone()), layers));
            }
        }
    }
    for (comp, layers) in lists {
        let mut out = Vec::with_capacity(layers.len());
        for mut layer in layers.drain(..) {
            let id = layer
                .get("id")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_owned();
            let place = places::Place {
                comp: comp.clone(),
                layer: id.clone(),
            };
            if !is_ring(&layer) || held.holds(&place) {
                out.push(layer);
                continue;
            }
            changed += 1;
            match rings {
                Rings::Keep => out.push(layer),
                Rings::Off => {
                    layer["hidden"] = json!(true);
                    out.push(layer);
                }
                Rings::Width(px) => {
                    if let Some(Json::Array(masks)) = layer.get_mut("mask") {
                        for mask in masks.iter_mut().filter_map(Json::as_object_mut) {
                            if mask.get("mode").and_then(Json::as_str) == Some("subtract") {
                                // The cut grows as the ring narrows.
                                let at = mask.entry("expand").or_insert(json!(0));
                                change_value(at, Change::Plus(-px), None);
                                if let Some(Json::Object(anim)) = mask.get_mut("anim")
                                    && let Some(track) = anim.get_mut("expand")
                                {
                                    change_value(track, Change::Plus(-px), None);
                                }
                            }
                        }
                    }
                    out.push(layer);
                }
                Rings::Echo(n) => {
                    // The echoes go under the ring, the smallest last.
                    let echoes: Vec<Json> = (1..=n)
                        .map(|i| {
                            let mut echo = layer.clone();
                            echo["id"] = json!(format!("{id}-echo-{i}"));
                            let k = ECHO_SCALE.powi(i as i32);
                            times_points(echo.as_object_mut().expect("a layer"), "scale", k, 1.0);
                            times_points(
                                echo.as_object_mut().expect("a layer"),
                                "opacity",
                                ECHO_OPACITY.powi(i as i32),
                                1.0,
                            );
                            echo
                        })
                        .collect();
                    out.extend(echoes.into_iter().rev());
                    out.push(layer);
                }
            }
        }
        *layers = out;
    }
    if let Some(comps) = comps {
        root.insert("comps".into(), comps);
    }
    changed
}

/// Whether `layer` is a ring: it subtracts one of its masks from another.
fn is_ring(layer: &Json) -> bool {
    let Some(Json::Array(masks)) = layer.get("mask") else {
        return false;
    };
    let subtract = |m: &Json| m.get("mode").and_then(Json::as_str) == Some("subtract");
    masks.len() >= 2 && masks.iter().any(subtract) && masks.iter().any(|m| !subtract(m))
}

/// Multiplies key `name` of `layer` by `k`, its value and its track,
/// every number of a point; `missing` is its value when it is not set.
fn times_points(layer: &mut Map<String, Json>, name: &str, k: f64, missing: f64) {
    let animated = layer.get("anim").and_then(|a| a.get(name)).is_some();
    if !animated {
        let at = layer.entry(name).or_insert(json!(missing));
        times_point(at, k);
    }
    if let Some(Json::Object(anim)) = layer.get_mut("anim")
        && let Some(track) = anim.get_mut(name)
    {
        match track {
            Json::Array(keys) => {
                for key in keys.iter_mut() {
                    match key {
                        Json::Array(parts) if parts.len() >= 2 => times_point(&mut parts[1], k),
                        Json::Object(fields) => {
                            if let Some(v) = fields.get_mut("v") {
                                times_point(v, k);
                            }
                        }
                        _ => {}
                    }
                }
            }
            other => {
                change_value(other, Change::Times(k), None);
            }
        }
    }
}

/// Multiplies a number, or every number of a point, by `k`.
fn times_point(value: &mut Json, k: f64) {
    match value {
        Json::Number(n) => {
            if let Some(v) = n.as_f64() {
                *value = json!(round(v * k));
            }
        }
        Json::Array(parts) => {
            for part in parts.iter_mut() {
                if let Some(v) = part.as_f64() {
                    *part = json!(round(v * k));
                }
            }
        }
        _ => {}
    }
}

/// Moves a track of points by `speed` about its first key: each key's
/// value goes `speed` times as far from the first's, so the motion runs
/// that much faster over the same keys.
fn faster(track: &mut Json, speed: f64) -> bool {
    let Json::Array(keys) = track else {
        return false;
    };
    let value = |key: &Json| -> Option<Vec<f64>> {
        let v = match key {
            Json::Array(parts) if parts.len() >= 2 => &parts[1],
            Json::Object(fields) => fields.get("v")?,
            _ => return None,
        };
        match v {
            Json::Array(n) => n.iter().map(Json::as_f64).collect(),
            Json::Number(n) => n.as_f64().map(|x| vec![x]),
            _ => None,
        }
    };
    let Some(first) = keys.first().and_then(value) else {
        return false;
    };
    let mut moved = false;
    for key in keys.iter_mut().skip(1) {
        let Some(v) = value(key) else { continue };
        let to: Vec<f64> = v
            .iter()
            .zip(&first)
            .map(|(x, x0)| round(x0 + (x - x0) * speed))
            .collect();
        let new = match to.len() {
            1 => json!(to[0]),
            _ => json!(to),
        };
        match key {
            Json::Array(parts) => parts[1] = new,
            Json::Object(fields) => {
                fields.insert("v".into(), new);
            }
            _ => {}
        }
        moved = true;
    }
    moved
}

/// How a look moves a number.
#[derive(Debug, Clone, Copy)]
enum Change {
    Times(f64),
    Plus(f64),
}

impl Change {
    fn of(self, v: f64, range: Option<(f64, f64)>) -> f64 {
        let to = match self {
            Change::Times(k) => v * k,
            Change::Plus(d) => v + d,
        };
        let (lo, hi) = range.unwrap_or((f64::NEG_INFINITY, f64::INFINITY));
        round(to.clamp(lo, hi))
    }
}

/// The range the format allows a property, if it bounds it.
fn range_of(registry: &Registry, name: &str) -> Option<(f64, f64)> {
    registry
        .rows
        .iter()
        .find(|r| r.name == name)
        .and_then(|r| r.range)
}

/// Moves `value`, a number, a track `a -> b @…` or a list of keys, by
/// `change`; whether it held a number.
fn change_value(value: &mut Json, change: Change, range: Option<(f64, f64)>) -> bool {
    match value {
        Json::Number(n) => {
            let Some(v) = n.as_f64() else { return false };
            *value = json!(change.of(v, range));
            true
        }
        Json::String(track) => {
            let moved: Vec<String> = track
                .split(',')
                .map(|segment| {
                    let (values, rest) = segment.split_once('@').unwrap_or((segment, ""));
                    let values: Vec<String> = values
                        .split("->")
                        .map(|v| match v.trim().parse::<f64>() {
                            Ok(x) => format!(" {} ", change.of(x, range)),
                            Err(_) => v.to_owned(),
                        })
                        .collect();
                    let joined = values.join("->");
                    match rest.is_empty() && !segment.contains('@') {
                        true => joined.trim().to_owned(),
                        false => format!("{}@{rest}", joined.trim_start()),
                    }
                })
                .collect();
            let moved = moved
                .join(",")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let changed = moved != *track;
            *track = moved;
            changed
        }
        Json::Array(keys) => {
            let mut any = false;
            for key in keys.iter_mut() {
                match key {
                    Json::Array(parts) if parts.len() >= 2 => {
                        any |= change_value(&mut parts[1], change, range);
                    }
                    Json::Object(fields) => {
                        if let Some(v) = fields.get_mut("v") {
                            any |= change_value(v, change, range);
                        }
                    }
                    _ => {}
                }
            }
            any
        }
        _ => false,
    }
}

/// Treats one text layer; whether it changed.
fn treat(layer: &mut Map<String, Json>, treatment: Treatment, size: f64) -> bool {
    match treatment {
        Treatment::Fill => false,
        Treatment::Outline(width) => {
            let fill = layer
                .get("fill")
                .filter(|f| !f.as_str().is_some_and(|s| s.starts_with('$')))
                .cloned()
                .unwrap_or(json!("#FFFFFF"));
            let style = layer
                .entry("style")
                .or_insert_with(|| Json::Object(Map::new()));
            if let Json::Object(style) = style {
                style.insert("stroke".into(), fill);
                style.insert("stroke-width".into(), json!(width));
                style.insert("paint".into(), json!("stroke-over"));
            }
            layer.insert("fill".into(), json!("#FFFFFF00"));
            true
        }
        Treatment::Extrude(share) => {
            let depth = (share * size).max(1.0);
            let fx = layer.entry("fx").or_insert_with(|| json!([]));
            if let Json::Array(fx) = fx {
                // Steps a few px apart, darkest furthest: a solid depth.
                let steps = 4;
                for i in 1..=steps {
                    let d = depth * f64::from(i) / f64::from(steps);
                    let shade = 0.10 - 0.02 * f64::from(i);
                    fx.push(json!({
                        "drop-shadow": round(d),
                        "color": [round(shade.max(0.0)), round(shade.max(0.0)), round(shade.max(0.0) * 1.1), 1],
                        "direction": 135,
                        "softness": 0,
                    }));
                }
            }
            true
        }
    }
}

/// 1, a default for serde.
fn one() -> f64 {
    1.0
}

/// `v` to four decimals.
fn round(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

#[cfg(test)]
#[path = "../../tests/unit/vary/look.rs"]
mod tests;
