//! The look axis ([D78]): the same template as another film. A look grades
//! the whole master through a gradient map of its own colours, moves the
//! settings of its effects (noise, glow, blur) inside ranges the format
//! allows and the look names, treats its text (filled, outlined or
//! extruded) and sets it in another face, fitted into its boxes as the
//! type axis fits.
//!
//! It moves the drawing too, not only its colours: the cells of the
//! template's noise (wrapped into rays, how many and how long), how fast
//! the noise travels, how heavy the glow is, the rings it cuts (kept,
//! dropped, widened or echoed inside one another) and, through the timing
//! axis, the pace of its moves.
//!
//! Every value a look writes is a scene's own: a `color-cycle` on the
//! layers that draw the master, a `fractal-noise` for grain, a number on an
//! effect clamped to its range. The gate judges a look as it judges any
//! variant, and [`super::distance`] measures how far from the master it
//! lands.
//!
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

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
pub const LOOKS: [&str; 6] = ["chrome", "acid", "amber", "paper", "duotone", "crimson"];

/// How a look treats the text.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Treatment {
    /// Filled, as the master sets it.
    Fill,
    /// Drawn as its outline only, this many px wide at the master's size.
    Outline(f64),
    /// Filled, over a dark extrusion this many px deep.
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

/// One look, every value drawn: what the recipe records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LookStep {
    /// The look's name.
    pub name: String,
    /// The gradient map: the colour each level of the master's intensity
    /// takes, `(level, #RRGGBB)` in rising level.
    pub stops: Vec<(f64, String)>,
    /// How much of the master's own colour stays, 0 to 1.
    pub blend: f64,
    /// The grey each hue of the master turns into before the map, as the
    /// `black-white` effect takes them (reds, yellows, greens, cyans,
    /// blues, magentas), so its figures stay apart from its ground
    /// (the `HUES` the look reads them with).
    pub hues: [f64; 6],
    /// Fractal noise: contrast and complexity as a share of the master's,
    /// brightness added, and how much faster it evolves and travels.
    pub noise: [f64; 4],
    /// Fractal noise cells: width and height as a share of the master's.
    /// Wrapped into rays, the width is how many rays there are and how
    /// wide (smaller is more and finer), the height how long they run.
    pub cells: [f64; 2],
    /// The rings the template cuts.
    pub rings: Rings,
    /// The pace of the moves, through the timing axis, when the look
    /// changes it.
    pub pace: Option<super::Pace>,
    /// Glow: radius and intensity as a share of the master's.
    pub glow: [f64; 2],
    /// Blurs and streaks: their amount as a share of the master's.
    pub blur: f64,
    /// Grain over the whole film: its opacity, 0 for none.
    pub grain: f64,
    /// The text's treatment.
    pub text: Treatment,
    /// The text's new face, fitted into its boxes, when the scene can load
    /// one the look asks for.
    pub face: Option<TypeStep>,
    /// The master's ground ([`Lit::ground`]): what its texts keep their
    /// contrast against through the map.
    #[serde(default)]
    pub ground: f64,
}

impl fmt::Display for LookStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "look {}", self.name)?;
        if let Some((family, weight)) = self.face.as_ref().and_then(|t| t.headline.as_ref()) {
            write!(f, ", {family} {weight}")?;
        }
        match self.text {
            Treatment::Fill => {}
            Treatment::Outline(_) => write!(f, ", outlined")?,
            Treatment::Extrude(_) => write!(f, ", extruded")?,
        }
        let [w, h] = self.cells;
        if (w - 1.0).abs() > 0.05 || (h - 1.0).abs() > 0.05 {
            write!(f, ", cells ×{w:.2}/×{h:.2}")?;
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
    stops: &'static [(f64, &'static str)],
    blend: f64,
    noise: [f64; 4],
    cells: [f64; 2],
    rings: Rings,
    pace: Option<super::Pace>,
    glow: [f64; 2],
    blur: f64,
    grain: f64,
    text: Treatment,
    /// Faces it prefers, in order, by family name without spaces or case.
    faces: &'static [&'static str],
}

/// The six looks. Each is a gradient map whose levels rise with the
/// master's, so what is lighter in the master stays lighter (or, for
/// `paper`, darker) and contrast keeps its sense; then the drawing: the
/// noise's cells and speed, the glow, the rings and the pace, so that no
/// two looks are the same film in other colours.
const PRESETS: [Preset; 6] = [
    // Mono chrome: greys with a cold steel middle; many fine long rays at
    // speed, a heavy glow, rings echoed inside one another, snappy moves,
    // text extruded in dark.
    Preset {
        name: "chrome",
        stops: &[
            (0.0, "#000000"),
            (0.3, "#1E2228"),
            (0.62, "#8E96A3"),
            (0.85, "#D9DEE6"),
            (1.0, "#FFFFFF"),
        ],
        blend: 0.0,
        noise: [1.05, 1.0, 0.0, 1.8],
        cells: [0.4, 1.8],
        rings: Rings::Echo(2),
        pace: Some(super::Pace::Snappy),
        glow: [1.3, 1.2],
        blur: 1.0,
        grain: 0.0,
        text: Treatment::Extrude(10.0),
        faces: &["kumbhsans", "raleway"],
    },
    // Acid: violet shadows, acid-green light; few wide rays broken into
    // short blocks, a thin glow and no rings: a hard digital film.
    Preset {
        name: "acid",
        stops: &[
            (0.0, "#06010F"),
            (0.28, "#3A0A6E"),
            (0.55, "#8A2BE2"),
            (0.78, "#7CFF3A"),
            (1.0, "#F2FFE0"),
        ],
        blend: 0.0,
        noise: [1.1, 1.0, 0.0, 1.5],
        cells: [3.5, 0.3],
        rings: Rings::Off,
        pace: Some(super::Pace::Snappy),
        glow: [1.0, 0.65],
        blur: 1.2,
        grain: 0.0,
        text: Treatment::Fill,
        faces: &["raleway", "kumbhsans"],
    },
    // Warm gold and amber on black, its light held below white so a
    // glowing field never burns out: long slow streaks, a warm glow,
    // thinner rings, a heavy serif.
    Preset {
        name: "amber",
        stops: &[
            (0.0, "#000000"),
            (0.3, "#2A1200"),
            (0.55, "#8A4200"),
            (0.8, "#E08A1E"),
            (1.0, "#FFD27A"),
        ],
        blend: 0.0,
        noise: [0.95, 1.0, 0.0, 0.55],
        cells: [1.0, 2.5],
        rings: Rings::Width(-15.0),
        pace: None,
        glow: [1.2, 1.0],
        blur: 1.3,
        grain: 0.0,
        text: Treatment::Fill,
        faces: &["sourceserifpro", "kumbhsans"],
    },
    // A light ground: the map runs from paper to ink, so the dark field
    // turns near-white and the bright rays turn dark; fewer rays, little
    // glow, no rings, calm moves and a fine grain, like print.
    Preset {
        name: "paper",
        stops: &[
            (0.0, "#F4F0E6"),
            (0.3, "#D9D1C2"),
            (0.6, "#7A6E60"),
            (0.85, "#2A2520"),
            (1.0, "#0E0C0A"),
        ],
        blend: 0.0,
        noise: [1.05, 1.0, 0.0, 0.8],
        cells: [2.2, 1.0],
        rings: Rings::Off,
        pace: Some(super::Pace::Calm),
        glow: [0.6, 0.3],
        blur: 0.8,
        grain: 0.06,
        text: Treatment::Fill,
        faces: &["sourceserifpro", "kumbhsans"],
    },
    // A cyan and magenta print over navy: fine short dashes, a ring
    // echoed once, grain, outlined type.
    Preset {
        name: "duotone",
        stops: &[
            (0.0, "#0A0E33"),
            (0.4, "#0090C8"),
            (0.7, "#FF2E93"),
            (1.0, "#FFF2F8"),
        ],
        blend: 0.0,
        noise: [1.1, 1.0, 0.0, 1.2],
        cells: [0.6, 0.45],
        rings: Rings::Echo(1),
        pace: None,
        glow: [0.8, 0.6],
        blur: 1.0,
        grain: 0.08,
        text: Treatment::Outline(6.0),
        faces: &["kumbhsans", "raleway"],
    },
    // Deep red on black, minimal: few rays, slow, thin rings.
    Preset {
        name: "crimson",
        stops: &[
            (0.0, "#000000"),
            (0.25, "#3A0008"),
            (0.5, "#E0102E"),
            (0.75, "#FF7A88"),
            (1.0, "#FFF0F2"),
        ],
        blend: 0.0,
        noise: [0.9, 1.0, 0.0, 0.5],
        cells: [2.8, 1.6],
        rings: Rings::Width(-25.0),
        pace: None,
        glow: [0.9, 0.8],
        blur: 0.8,
        grain: 0.0,
        text: Treatment::Fill,
        faces: &["raleway", "kumbhsans"],
    },
];

/// What a look reads off the master before it is drawn: the grey each
/// hue turns into, and the levels of its lit pixels in that grey.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Lit {
    /// The grey each hue turns into, as [`LookStep::hues`].
    pub hues: [f64; 6],
    /// The grey levels of the lit pixels, sorted.
    pub levels: Vec<f64>,
    /// The master's ground: the median grey of all its pixels.
    pub ground: f64,
}

/// The grey each hue turns into before the map: warm hues as light as
/// their brightest channel, greens and cyans darker, blues darkest. A
/// gradient map read off plain intensity puts a red title and the blue
/// field behind it on one level, so both take one colour of the look;
/// read this way the title stays lighter than its field.
const HUES: [f64; 6] = [1.0, 1.0, 0.6, 0.4, 0.15, 1.0];

/// What `frames` of the master light ([`Lit`]): its pixels' greys as
/// [`HUES`] turns them, those above [`LIT`]. A template is mostly dark,
/// its figures and type a small share of its pixels at middling levels; a
/// gradient map with stops at fixed levels would leave its upper colours
/// unused, so the stops are placed by these levels ([`placed`]).
pub(crate) fn lit(frames: &[crate::Image]) -> Lit {
    let mut all: Vec<f64> = frames
        .iter()
        .flat_map(|image| image.pixels().chunks_exact(4))
        .map(|p| {
            let a = f64::from(p[3]) / 255.0;
            grey(&HUES, [0, 1, 2].map(|c| f64::from(p[c]) / 255.0 * a))
        })
        .collect();
    all.sort_by(f64::total_cmp);
    let ground = all.get(all.len() / 2).copied().unwrap_or(0.0);
    let levels = all.into_iter().filter(|v| *v > LIT).collect();
    Lit {
        hues: HUES,
        levels,
        ground,
    }
}

/// The hue each channel is the primary of, red, green and blue.
const PRIMARY: [usize; 3] = [0, 2, 4];

/// The hue of channels `i` and `j` together: yellow, cyan or magenta.
fn secondary(i: usize, j: usize) -> usize {
    match (i.min(j), i.max(j)) {
        (0, 1) => 1,
        (1, 2) => 3,
        _ => 5,
    }
}

/// The channels of `rgb` from highest to lowest, and their levels.
fn ranked(rgb: [f64; 3]) -> ([usize; 3], [f64; 3]) {
    let mut order = [0, 1, 2];
    order.sort_by(|&i, &j| rgb[j].total_cmp(&rgb[i]));
    (order, order.map(|i| rgb[i]))
}

/// The grey of `rgb` under the hues' greys, as `black-white` draws it.
fn grey(hues: &[f64; 6], rgb: [f64; 3]) -> f64 {
    let ([high, middle, _], [h, m, l]) = ranked(rgb);
    (l + (h - m) * hues[PRIMARY[high]] + (m - l) * hues[secondary(high, middle)]).clamp(0.0, 1.0)
}

/// A pixel brighter than this intensity is lit.
const LIT: f64 = 0.03;

/// Draws look `index` of a run: the spec's names in turn (every look when
/// it names none), each setting moved by up to 8 % by the seed so two
/// runs on two seeds are two films. `lit` places its stops ([`lit`]);
/// `faces` are the families it may set text in ([`Faces`]).
pub(crate) fn draw(
    names: &[String],
    index: usize,
    lit: &Lit,
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
    let noise = [
        jitter(preset.noise[0]).clamp(NOISE_CONTRAST.0, NOISE_CONTRAST.1),
        preset.noise[1],
        preset.noise[2],
        jitter(preset.noise[3]),
    ];
    let mut glow = [jitter(preset.glow[0]), jitter(preset.glow[1])];
    // A heavy glow on a template that glows already burns its field out:
    // the two together stay under a ceiling.
    if glow[0] * glow[1] > GLOW_CEILING {
        glow[1] = round(GLOW_CEILING / glow[0]);
    }
    let cells = [jitter(preset.cells[0]), jitter(preset.cells[1])];
    let blur = jitter(preset.blur);
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
    LookStep {
        name: preset.name.to_owned(),
        stops: placed(preset.stops, &lit.levels),
        blend: preset.blend,
        hues: lit.hues,
        noise,
        cells,
        rings: preset.rings,
        pace: preset.pace,
        glow,
        blur,
        grain: preset.grain,
        text: preset.text,
        face,
        ground: lit.ground,
    }
}

/// The most a look multiplies a template's glow radius and intensity
/// together.
const GLOW_CEILING: f64 = 1.6;

/// A preset's stops placed between where they are written and the
/// master's quantiles: an interior stop at `t` lands halfway from `t` to
/// the level `t` of the lit pixels lie below, the ends at 0 and 1, each at
/// least a hundredth above the one before. All the way to the quantiles,
/// a dark template's lower stops crowd into a sliver of levels, and the
/// map turns its soft glows into steps.
fn placed(stops: &[(f64, &str)], lit: &[f64]) -> Vec<(f64, String)> {
    let mut out: Vec<(f64, String)> = Vec::with_capacity(stops.len());
    for (i, &(t, c)) in stops.iter().enumerate() {
        let interior = i > 0 && i + 1 < stops.len();
        let at = match (interior, lit.is_empty()) {
            (true, false) => {
                let k = ((t * lit.len() as f64) as usize).min(lit.len() - 1);
                (lit[k] + t) / 2.0
            }
            _ => t,
        };
        let floor = out.last().map_or(0.0, |(p, _)| p + 0.01);
        let ceiling = 1.0 - 0.01 * (stops.len() - 1 - i) as f64;
        out.push((round(at.max(floor).min(ceiling)), c.to_owned()));
    }
    out
}

/// Moves the settings of every effect the locks leave free and treats
/// every free text, in place; says what it changed.
pub(crate) fn apply(raw: &mut Json, held: &Held, step: &LookStep) -> String {
    let mut moved = 0usize;
    let mut texts = 0usize;
    let mut lifted = 0usize;
    let curve = smooth(&step.stops);
    places::each_mut(raw, |visit| {
        let Visit::Layer { place, layer } = visit else {
            return;
        };
        if held.holds(place) {
            return;
        }
        if let Some(Json::Array(fx)) = layer.get_mut("fx") {
            for item in fx.iter_mut().filter_map(Json::as_object_mut) {
                moved += move_effect(item, step);
            }
        }
        if layer.contains_key("text") {
            lifted += usize::from(keep_contrast(layer, &curve, step));
            texts += usize::from(treat(layer, step.text));
        }
    });
    let rings = rings(raw, held, step.rings);
    format!(
        "{step}: {moved} effect settings moved, {texts} texts treated, {lifted} texts lifted \
         clear of their ground, {rings} rings changed, graded through {} stops",
        step.stops.len()
    )
}

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

/// The most contrast a text is lifted to keep: WCAG's for body text.
const KEPT_CONTRAST: f64 = 4.5;

/// Sets `layer`'s plain fill, when it has one, to the grey whose colour
/// through the map stands as far from the map's colour of the master's
/// ground as the fill stood from the ground in the master (up to
/// [`KEPT_CONTRAST`]); whether it changed it. A map that crowds a range of
/// levels into dark browns would otherwise sink a mid-grey line of copy
/// into a black ground. The text moves away from the ground only, so it
/// stays on the side it was.
fn keep_contrast(
    layer: &mut Map<String, Json>,
    curve: &[(f64, [f64; 3])],
    step: &LookStep,
) -> bool {
    let Some(fill) = layer
        .get("fill")
        .filter(|f| !f.as_str().is_some_and(|s| s.starts_with('$')))
    else {
        return false;
    };
    let Some([r, g, b, a]) = nakkash_format::color(fill) else {
        return false;
    };
    let level = grey(&step.hues, [r, g, b]);
    let ground = step.ground;
    let lum = |rgb: [f64; 3]| super::colour::luminance([rgb[0], rgb[1], rgb[2], 1.0]);
    let ratio = |x: f64, y: f64| (x.max(y) + 0.05) / (x.min(y) + 0.05);
    let wanted = ratio(lum([r, g, b]), lum([ground; 3])).min(KEPT_CONTRAST);
    let back = lum(mapped(curve, ground));
    let reaches = |t: f64| ratio(lum(mapped(curve, t)), back) >= wanted;
    if reaches(level) {
        return false;
    }
    let toward = if level >= ground { 1.0 } else { 0.0 };
    let steps = 100;
    let lifted = (1..=steps)
        .map(|i| level + (toward - level) * f64::from(i) / f64::from(steps))
        .find(|t| reaches(*t))
        .unwrap_or(toward);
    layer.insert(
        "fill".into(),
        json!([round(lifted), round(lifted), round(lifted), round(a)]),
    );
    true
}

/// The colour `curve` (as [`smooth`] gives it) maps level `t` to, straight
/// between its points as the map draws it.
fn mapped(curve: &[(f64, [f64; 3])], t: f64) -> [f64; 3] {
    let Some(first) = curve.first() else {
        return [t; 3];
    };
    if t <= first.0 {
        return first.1;
    }
    for w in curve.windows(2) {
        let ((x0, c0), (x1, c1)) = (w[0], w[1]);
        if t <= x1 {
            let k = if x1 > x0 { (t - x0) / (x1 - x0) } else { 1.0 };
            return std::array::from_fn(|i| c0[i] + (c1[i] - c0[i]) * k);
        }
    }
    curve.last().map_or([t; 3], |l| l.1)
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

/// How much of a turn the grade's phase spans; see [`grade`].
const REPETITIONS: f64 = 0.998;

/// The effects the layers that draw the master take: its hues to grey,
/// the gradient map, then grain.
///
/// The map runs through the stops on a smooth curve in OKLab (monotone
/// cubic, so it never overshoots a stop), drawn as [`GRADE_POINTS`] points
/// a curve can hold: straight lines between the stops would bend at each,
/// and a glow running through a bend shows it as a band.
pub(crate) fn grade(step: &LookStep) -> Vec<Json> {
    let curve = smooth(&step.stops);
    let channel = |i: usize| {
        let points: Vec<String> = curve
            .iter()
            .map(|(at, rgb)| format!("{},{}", round(*at), round(rgb[i])))
            .collect();
        format!("M {}", points.join(" L "))
    };
    let [reds, yellows, greens, cyans, blues, magentas] = step.hues;
    let mut fx = vec![
        json!({
            "black-white": 0,
            "reds": reds, "yellows": yellows, "greens": greens,
            "cyans": cyans, "blues": blues, "magentas": magentas,
        }),
        json!({
            "color-cycle": "intensity",
            "red": channel(0),
            "green": channel(1),
            "blue": channel(2),
            // The cycle wraps its phase into 0 to 1, so pure white (phase 1)
            // would read as the first stop, black: a hair under one turn keeps
            // the brightest pixels on the last stop.
            "repetitions": REPETITIONS,
            "blend": step.blend,
        }),
    ];
    if step.grain > 0.0 {
        fx.push(json!({
            "fractal-noise": 0,
            "fractal": "turbulent-basic",
            "noise": "block",
            "mode": "overlay",
            "scale": 0.02,
            "complexity": 1,
            "contrast": 2,
            "opacity": step.grain,
            "anim": {"evolution": "0 -> 7200 @0-10s"},
        }));
    }
    fx
}

/// The most points a curve of the format holds.
const GRADE_POINTS: usize = 16;

/// The colour the map gives at each of [`GRADE_POINTS`] levels: the stops
/// themselves, and points between them where they lie widest apart, on a
/// monotone cubic through the stops in OKLab, as sRGB.
fn smooth(stops: &[(f64, String)]) -> Vec<(f64, [f64; 3])> {
    let knots: Vec<(f64, [f64; 3])> = stops
        .iter()
        .map(|(at, c)| {
            let rgb = nakkash_format::color(&json!(c)).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            (*at, nakkash_format::oklch::oklab([rgb[0], rgb[1], rgb[2]]))
        })
        .collect();
    if knots.len() < 2 {
        return knots
            .into_iter()
            .map(|(at, lab)| (at, nakkash_format::oklch::srgb(lab)))
            .collect();
    }
    let mut at: Vec<f64> = knots.iter().map(|k| k.0).collect();
    while at.len() < GRADE_POINTS {
        let widest = (0..at.len() - 1)
            .max_by(|&i, &j| (at[i + 1] - at[i]).total_cmp(&(at[j + 1] - at[j])))
            .expect("two knots");
        at.insert(widest + 1, (at[widest] + at[widest + 1]) / 2.0);
    }
    let xs: Vec<f64> = knots.iter().map(|k| k.0).collect();
    at.into_iter()
        .map(|x| {
            let lab: [f64; 3] = std::array::from_fn(|c| {
                let ys: Vec<f64> = knots.iter().map(|k| k.1[c]).collect();
                monotone(&xs, &ys, x)
            });
            (x, nakkash_format::oklch::srgb(lab))
        })
        .collect()
}

/// The monotone cubic through `(xs, ys)` at `x` (Fritsch and Carlson's
/// slopes): smooth, and between two knots never beyond either.
fn monotone(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let n = xs.len();
    let d: Vec<f64> = (0..n - 1)
        .map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]).max(1e-9))
        .collect();
    let slope = |i: usize| -> f64 {
        if i == 0 {
            return d[0];
        }
        if i == n - 1 {
            return d[n - 2];
        }
        let (a, b) = (d[i - 1], d[i]);
        match a * b <= 0.0 {
            true => 0.0,
            false => {
                let (h0, h1) = (xs[i] - xs[i - 1], xs[i + 1] - xs[i]);
                let (w0, w1) = (2.0 * h1 + h0, h1 + 2.0 * h0);
                (w0 + w1) / (w0 / a + w1 / b)
            }
        }
    };
    let i = (0..n - 1).find(|&i| x <= xs[i + 1]).unwrap_or(n - 2);
    let h = (xs[i + 1] - xs[i]).max(1e-9);
    let t = ((x - xs[i]) / h).clamp(0.0, 1.0);
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * ys[i]
        + (t3 - 2.0 * t2 + t) * h * slope(i)
        + (-2.0 * t3 + 3.0 * t2) * ys[i + 1]
        + (t3 - t2) * h * slope(i + 1)
}

/// Moves one effect's settings by `step`; how many it moved.
fn move_effect(item: &mut Map<String, Json>, step: &LookStep) -> usize {
    let Some(kind) = EFFECTS.iter().find(|k| item.contains_key(k.name)) else {
        return 0;
    };
    let primary = match kind.primary {
        nakkash_format::item::Primary::Property(p) => Some(p),
        _ => None,
    };
    // A noise with its own width and height takes the cells there; one
    // scaled alike both ways takes them on its scale.
    let shaped = item.contains_key("scale-width") || item.contains_key("scale-height");
    let mut moves: Vec<(&str, Change)> = match kind.name {
        "fractal-noise" => vec![
            ("contrast", Change::Times(step.noise[0])),
            ("complexity", Change::Times(step.noise[1])),
            ("brightness", Change::Plus(step.noise[2])),
            ("evolution", Change::Times(step.noise[3])),
            ("scale-width", Change::Times(step.cells[0])),
            ("scale-height", Change::Times(step.cells[1])),
        ],
        "glow" => vec![
            ("radius", Change::Times(step.glow[0])),
            ("intensity", Change::Times(step.glow[1])),
        ],
        "radial-blur" | "vector-blur" => vec![("amount", Change::Times(step.blur))],
        "box-blur" | "lens-blur" => vec![("radius", Change::Times(step.blur))],
        "gaussian-blur" => vec![("blurriness", Change::Times(step.blur))],
        _ => Vec::new(),
    };
    if kind.name == "fractal-noise" && !shaped {
        moves.push((
            "scale",
            Change::Times((step.cells[0] * step.cells[1]).sqrt()),
        ));
    }
    let mut n = 0;
    // The noise travels by its offset's keys: faster, they run further.
    if kind.name == "fractal-noise"
        && let Some(Json::Object(anim)) = item.get_mut("anim")
        && let Some(track) = anim.get_mut("offset")
    {
        n += usize::from(faster(track, step.noise[3]));
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
fn treat(layer: &mut Map<String, Json>, treatment: Treatment) -> bool {
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
        Treatment::Extrude(depth) => {
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

/// `v` to four decimals.
fn round(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

#[cfg(test)]
#[path = "../../tests/unit/vary/look.rs"]
mod tests;
