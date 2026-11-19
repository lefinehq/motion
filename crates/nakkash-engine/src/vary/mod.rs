//! Generated variants ([D78]): a master walked along palette, type,
//! content, timing and format, and improved by the check's own repairs.
//!
//! A run is planned first: [`plan`] draws every variant's [`Recipe`] from
//! the [`Spec`] and the seed, so the recipes say exactly what each variant
//! is. [`make`] then builds one variant from the master and its recipe,
//! which depends on nothing else, and [`gate`] measures it against the
//! master's report.
//!
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

mod colour;
mod content;
mod format;
mod improve;
mod look;
mod palette;
mod places;
mod rng;
mod sheet;
mod timing;
mod typeface;
mod wrap;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use nakkash_format::{Address, Diagnostic, Kind, Result};
use nakkash_query::{Measure, Report, Unit};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::session::Session;

pub use look::{LOOKS, LookStep, Treatment};
pub use palette::{SCHEMES, Scheme};
pub use sheet::{Sheet, sheet};
pub use timing::Pace;
pub use timing::{Beats, TimingStep};
pub use typeface::{Family, TypeStep};

use places::Held;
use rng::Rng;

/// What a run of `nakkash vary` asks for, as its spec file writes it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Spec {
    /// The seed every draw starts from.
    pub seed: u64,
    /// How many variants; the rows of a content file when it has more.
    pub count: Option<usize>,
    /// The palette axis.
    pub palette: Option<PaletteSpec>,
    /// The type axis.
    #[serde(rename = "type")]
    pub typeface: Option<TypeSpec>,
    /// The content axis.
    pub content: Option<ContentSpec>,
    /// The timing axis.
    pub timing: Option<TimingSpec>,
    /// The format axis.
    pub format: Option<FormatSpec>,
    /// The look axis.
    pub look: Option<LookSpec>,
    /// Whether the check's repairs run after the axes.
    pub improve: bool,
    /// Each variant takes one axis alone, the axes in turn, instead of one
    /// step of every axis: a walk along each axis apart.
    pub apart: bool,
}

/// The palette axis of a spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct PaletteSpec {
    /// The schemes to draw from ([`SCHEMES`]); all but `brand` when empty.
    pub schemes: Vec<String>,
    /// Brand colours, `#RRGGBB`, for the `brand` scheme.
    pub brand: Vec<String>,
}

/// The type axis of a spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TypeSpec {
    /// `contrast` pairs a family of another class with the headline's,
    /// `same` one of its own class; either when unset.
    pub pairs: Option<String>,
    /// Families to draw from instead of every one the scene can load.
    pub families: Vec<String>,
    /// New copy for every text: `long` makes each about twice as long, from
    /// its own words; any other string replaces the text.
    pub copy: Option<String>,
    /// Folders of fonts to draw from besides the scene's.
    pub fonts: Vec<PathBuf>,
}

/// The content axis of a spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ContentSpec {
    /// A CSV or JSON file of rows, relative to the spec; each row is a
    /// variant, each column a `$slot`, an `@role` or an `address.key`.
    pub rows: Option<PathBuf>,
}

/// The timing axis of a spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct TimingSpec {
    /// New lengths to draw from, such as `15s`, `450f`, or `67%` of the
    /// master's.
    pub durations: Vec<String>,
    /// A tempo in beats a minute, for a beat grid.
    pub bpm: Option<f64>,
    /// Where the first beat falls, such as `0.12s`.
    pub offset: Option<String>,
    /// A music track: its beats are found, and it plays at the root.
    pub music: Option<PathBuf>,
    /// `snappy` or `calm`.
    pub pace: Option<Pace>,
}

/// The format axis of a spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FormatSpec {
    /// Sizes to draw from: `9:16`, `1:1`, `4:5`, `16:9` or `1080x1350`.
    pub sizes: Vec<String>,
}

/// The look axis of a spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct LookSpec {
    /// The looks to take in turn ([`LOOKS`]); every one when empty.
    pub looks: Vec<String>,
    /// Folders of fonts the looks may set the text in, besides the scene's.
    pub fonts: Vec<PathBuf>,
}

/// How many frames of the master place a look's stops, and how tall.
const LIT_FRAMES: usize = 8;
const LIT_HEIGHT: f64 = 120.0;

/// One variant as planned: its id and the step of each axis it takes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recipe {
    /// Its id, which names its folder: `v1`.
    pub id: String,
    /// The run's seed.
    pub seed: u64,
    /// Its steps, in the order they are made.
    pub steps: Vec<Step>,
}

/// One axis's step of a recipe, with every value drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "axis")]
pub enum Step {
    /// New values from a row of the content file.
    Content {
        /// The row, counted from 1.
        row: usize,
        /// Each column's target and value.
        values: BTreeMap<String, String>,
    },
    /// New faces, and new copy fitted into the master's boxes.
    Type(typeface::TypeStep),
    /// The palette moved by a scheme.
    Palette {
        /// The scheme.
        #[serde(flatten)]
        scheme: Scheme,
    },
    /// The master retimed through a time map.
    Timing(timing::TimingStep),
    /// The master placed on a canvas of another size.
    Format {
        /// The new canvas's width, in px.
        width: u32,
        /// The new canvas's height, in px.
        height: u32,
    },
    /// The master graded and its effects and text moved into a look.
    Look(Box<look::LookStep>),
    /// The check's repairs.
    Improve,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Content { row, .. } => write!(f, "content row {row}"),
            Step::Type(step) => write!(f, "{step}"),
            Step::Palette { scheme } => write!(f, "palette {scheme}"),
            Step::Timing(step) => write!(f, "{step}"),
            Step::Format { width, height } => write!(f, "format {width}x{height}"),
            Step::Look(step) => write!(f, "{step}"),
            Step::Improve => write!(f, "improve"),
        }
    }
}

impl fmt::Display for Recipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let steps: Vec<String> = self.steps.iter().map(ToString::to_string).collect();
        write!(f, "{}", steps.join(" · "))
    }
}

/// Plans every variant of a run: each axis the spec names gives each
/// variant one step, drawn from its own stream of the seed. `base` is the
/// folder the spec's files are relative to.
///
/// # Errors
/// A diagnostic for a size, duration or brand colour that does not read,
/// or a content file that cannot be read.
pub fn plan(master: &Session, spec: &Spec, base: &Path) -> Result<Vec<Recipe>> {
    let held = Held::of(master.scene(), master.document().raw());
    let rows = match spec.content.as_ref().and_then(|c| c.rows.as_ref()) {
        Some(rows) => content::read_rows(&base.join(rows))?,
        None => Vec::new(),
    };
    let count = spec.count.unwrap_or(match rows.len() {
        0 => 4,
        n => n,
    });
    let faces = match &spec.typeface {
        Some(t) => typeface::candidates(master, t, base)?,
        None => Vec::new(),
    };
    // A look's stops are placed by what the master lights ([`look::lit`]).
    let look_lit = match &spec.look {
        Some(_) => look::lit(&samples(master, LIT_FRAMES, LIT_HEIGHT)?),
        None => look::Lit::default(),
    };
    let look_faces = match &spec.look {
        Some(l) => typeface::candidates(
            master,
            &TypeSpec {
                fonts: l.fonts.clone(),
                ..TypeSpec::default()
            },
            base,
        )?,
        None => Vec::new(),
    };
    let mut recipes = Vec::with_capacity(count);
    let axes = [
        !rows.is_empty(),
        spec.typeface.is_some(),
        spec.palette.is_some(),
        spec.timing.is_some(),
        spec.format.is_some(),
        spec.look.is_some(),
    ];
    let named: Vec<usize> = (0..axes.len()).filter(|&i| axes[i]).collect();
    for index in 0..count {
        // Apart, variant i takes the i-th axis named, in turn.
        let takes = |axis: usize| {
            axes[axis] && (!spec.apart || named.get(index % named.len().max(1)) == Some(&axis))
        };
        let mut steps = Vec::new();
        if takes(0) {
            let row = index % rows.len();
            steps.push(Step::Content {
                row: row + 1,
                values: rows[row].clone(),
            });
        }
        if let Some(t) = spec.typeface.as_ref().filter(|_| takes(1)) {
            let mut rng = Rng::new(spec.seed, index, "type");
            steps.push(Step::Type(typeface::draw(
                master,
                t,
                &faces,
                held.typefaces,
                &mut rng,
            )?));
        }
        if let Some(p) = spec.palette.as_ref().filter(|_| takes(2)) {
            for colour in &p.brand {
                nakkash_format::color(&Json::String(colour.clone())).ok_or_else(|| {
                    Diagnostic::new(Kind::Invalid, format!("`{colour}` is not a colour"))
                        .expected("a brand colour such as `#0F62FE`")
                })?;
            }
            let mut rng = Rng::new(spec.seed, index, "palette");
            steps.push(Step::Palette {
                scheme: palette::draw(&p.schemes, &p.brand, held.palette, &mut rng),
            });
        }
        if let Some(t) = spec.timing.as_ref().filter(|_| takes(3)) {
            let mut rng = Rng::new(spec.seed, index, "timing");
            steps.push(Step::Timing(timing::draw(master, t, base, &mut rng)?));
        }
        if let Some(f) = spec.format.as_ref().filter(|_| takes(4)) {
            let mut rng = Rng::new(spec.seed, index, "format");
            let (width, height) = format::draw(master, &f.sizes, &mut rng)?;
            steps.push(Step::Format { width, height });
        }
        if let Some(l) = spec.look.as_ref().filter(|_| takes(5)) {
            let mut rng = Rng::new(spec.seed, index, "look");
            // Apart, the look of a variant is its turn among the looks.
            let turn = match spec.apart {
                true => recipes
                    .iter()
                    .filter(|r: &&Recipe| r.steps.iter().any(|s| matches!(s, Step::Look(_))))
                    .count(),
                false => index,
            };
            let step = look::draw(
                &l.looks,
                turn,
                &look_lit,
                &look_faces,
                &l.fonts,
                held.typefaces,
                &mut rng,
            );
            // A look's pace goes through the timing axis: onto the
            // variant's own retime when it has one, else one of its own.
            if let Some(pace) = step.pace {
                match steps.iter_mut().find_map(|s| match s {
                    Step::Timing(t) => Some(t),
                    _ => None,
                }) {
                    Some(t) => {
                        t.pace.get_or_insert(pace);
                    }
                    None => steps.push(Step::Timing(TimingStep {
                        seconds: None,
                        beats: None,
                        pace: Some(pace),
                        music: None,
                    })),
                }
            }
            steps.push(Step::Look(Box::new(step)));
        }
        if spec.improve {
            steps.push(Step::Improve);
        }
        recipes.push(Recipe {
            id: format!("v{}", index + 1),
            seed: spec.seed,
            steps,
        });
    }
    Ok(recipes)
}

/// A variant as made: its scene, what each step did, and how its layers'
/// addresses read back to the master's.
#[derive(Debug)]
pub struct Made {
    /// The variant, ready to check and render; its file is the master's,
    /// so the files it names resolve as the master's do.
    pub session: Session,
    /// One line per step on what it changed.
    pub notes: Vec<String>,
    /// How the variant wraps the master, when timing or format did.
    wrapped: Option<wrap::Wrap>,
}

impl Made {
    /// The master's address of the layer at `address` in the variant, and
    /// how much larger it is drawn here; `None` for a layer the variant
    /// adds.
    pub fn read_back(&self, address: &Address) -> Option<(Address, f64)> {
        match &self.wrapped {
            None => Some((address.clone(), 1.0)),
            Some(wrapped) => wrapped.read_back(address),
        }
    }
}

/// Builds the variant `recipe` describes from `master`. Nothing but the
/// two is read, so the same pair always gives the same scene ([D78]).
///
/// [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error
///
/// # Errors
/// A diagnostic when a step makes a scene that does not validate, which
/// is a bug in the step, or when the scene's frames cannot be evaluated.
pub fn make(master: &Session, recipe: &Recipe, base: &Path) -> Result<Made> {
    let held = Held::of(master.scene(), master.document().raw());
    let mut raw = master.document().raw().clone();
    let mut notes = Vec::new();
    // Values in place first, on the master's own layers.
    for step in &recipe.steps {
        match step {
            Step::Content { values, .. } => {
                notes.push(content::apply(&mut raw, master, &held, values, base)?);
            }
            Step::Look(step) => {
                notes.push(look::apply(&mut raw, &held, step));
            }
            Step::Palette { scheme } => {
                let slots = master.slots();
                let palette = palette::gather(&raw, &held, &slots);
                let mapping = palette::mapping(&palette, scheme);
                let changed = palette::apply(&mut raw, &held, &slots, &mapping);
                notes.push(format!(
                    "palette {scheme}: {} of {} colours, {changed} values",
                    mapping.len(),
                    palette.len()
                ));
            }
            _ => {}
        }
    }
    let mut session = master.with_raw(raw.clone())?;
    // Type fits new faces and copy by measuring them, so it runs on the
    // scene with the values above.
    for step in &recipe.steps {
        if let Step::Type(t) = step {
            let (fitted, note) = typeface::apply(&session, &held, t)?;
            notes.push(note);
            session = master.with_raw(fitted)?;
        }
        // A look's face is fitted the same way.
        if let Step::Look(l) = step
            && let Some(face) = &l.face
        {
            let (fitted, note) = typeface::apply(&session, &held, face)?;
            notes.push(note);
            session = master.with_raw(fitted)?;
        }
    }
    let grade = recipe
        .steps
        .iter()
        .find_map(|s| match s {
            Step::Look(l) => Some(look::grade(l)),
            _ => None,
        })
        .unwrap_or_default();
    // Then time and size, which wrap the master instead of rewriting it.
    let timing = recipe.steps.iter().find_map(|s| match s {
        Step::Timing(t) => Some(t),
        _ => None,
    });
    let size = recipe.steps.iter().find_map(|s| match s {
        Step::Format { width, height } => Some((*width, *height)),
        _ => None,
    });
    let mut wrapped = None;
    if timing.is_some() || size.is_some() || !grade.is_empty() {
        let map = match timing {
            Some(t) => {
                let (map, note) = timing::time_map(&session, t)?;
                notes.push(note);
                Some(map)
            }
            None => None,
        };
        let groups = match size {
            Some((width, height)) => {
                let (groups, note) = format::layout(&session, width, height)?;
                notes.push(note);
                Some(groups)
            }
            None => None,
        };
        let music = timing.and_then(|t| t.music.clone());
        let size = &master.scene().root.size;
        let canvas = (f64::from(size.width), f64::from(size.height));
        let w = wrap::Wrap::new(canvas, map, groups, music, grade);
        session = master.with_raw(w.apply(session.document().raw()))?;
        wrapped = Some(w);
    }
    let mut made = Made {
        session,
        notes,
        wrapped,
    };
    if recipe.steps.contains(&Step::Improve) {
        let note = improve::improve(&mut made, master)?;
        made.notes.push(note);
    }
    Ok(made)
}

/// Repairs the contrast errors `made` adds to `baseline`, `master`'s
/// report, given `report`, `made`'s own, by moving each text's fill in
/// lightness; says what it did, or `None` when it added none, with the
/// report `made` has now.
///
/// # Errors
/// As [`Session::full_check`].
pub fn repair(
    made: &mut Made,
    master: &Session,
    baseline: &Report,
    report: Report,
) -> Result<(Option<String>, Report)> {
    let (note, report) = improve::repair_contrast(made, master, baseline, report)?;
    if let Some(note) = &note {
        made.notes.push(note.clone());
    }
    Ok((note, report))
}

/// What a variant's report adds to its master's.
#[derive(Debug, Clone, Default)]
pub struct Gated {
    /// The findings the master's report does not have, in the variant's
    /// own addresses.
    pub new: Report,
    /// How many of the variant's findings the master already has.
    pub known: usize,
}

impl Gated {
    /// Whether the variant passes: it adds no error ([D78]).
    ///
    /// [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error
    pub fn passed(&self) -> bool {
        !self.new.has_errors()
    }
}

/// Measures `variant`, the full report of `made`, against `master`'s: a
/// finding is known when the master has one by the same rule on the layer
/// it reads back to, measured as bad or worse once the variant's scale is
/// taken out of a measure in px. A finding on a layer the variant adds is
/// always new.
pub fn gate(master: &Report, variant: Report, made: &Made) -> Gated {
    let mut gated = Gated {
        new: Report {
            violations: Vec::new(),
            passed: variant.passed,
        },
        known: 0,
    };
    // Read once: a template drawn as many copies reports close to a
    // million findings, and asking the whole report about each took hours.
    let known = master.known();
    for v in variant.violations {
        let back = made.read_back(&v.address).map(|(address, scale)| {
            let mut read = v.clone();
            read.address = address;
            read.with = v
                .with
                .as_ref()
                .and_then(|w| made.read_back(w))
                .map(|(w, _)| w);
            read.measure = unscaled(v.measure, scale);
            read
        });
        match back {
            Some(read) if known.knows(&read) => gated.known += 1,
            _ => gated.new.violations.push(v),
        }
    }
    gated
}

/// `measure` with a scale of `scale` taken out of it when it is counted in
/// px, and the noise of measuring at another moment or size taken off in
/// its favour: 1 px of a px measure, 0.1 of a ratio (the precision both
/// are printed at). A variant shows the master's frames at moments between
/// them, and the pixel gates draw them small, so a ratio of 1.3:1 in the
/// master reads 1.2:1 in its copy without anything having changed.
fn unscaled(measure: Measure, scale: f64) -> Measure {
    let px = |unit: Unit| matches!(unit, Unit::PxPerFrame);
    match measure {
        Measure::Overshoot(v) => Measure::Overshoot((v / scale - 1.0).max(0.0)),
        Measure::Below {
            value,
            limit,
            unit: Unit::Ratio,
        } => Measure::Below {
            value: value + 0.1,
            limit,
            unit: Unit::Ratio,
        },
        Measure::Above { value, limit, unit } if px(unit) => Measure::Above {
            value: value / scale,
            limit,
            unit,
        },
        Measure::Below { value, limit, unit } if px(unit) => Measure::Below {
            value: value / scale,
            limit,
            unit,
        },
        other => other,
    }
}

/// How far a variant lands from its master as it looks, in OKLab: the
/// distance between their palettes, and the mean distance pixel by pixel.
/// Measured on frames drawn small at the same share of each film's length,
/// so "very different" is a number ([D78]).
///
/// [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Distance {
    /// Each film's colours, binned in OKLab by how much of the frames
    /// they cover: the mean distance from a colour of one to the nearest of
    /// the other, both ways, weighted by cover. 0 is the same palette; 0.1
    /// reads as another, 0.3 as far apart as black from mid grey.
    pub palette: f64,
    /// The mean OKLab distance between the two films' pixels at the same
    /// place and moment; `None` when their canvases differ in shape.
    pub frames: Option<f64>,
    /// How far the two films' shapes are apart, whatever their colours:
    /// one less the correlation of their edge maps (the gradient of OKLab
    /// lightness) at the same place and moment. 0 is the same drawing
    /// recoloured, or inverted; 1 shares no edge. `None` as for `frames`.
    pub structure: Option<f64>,
}

/// Frames of `session` for [`distance`]: `n` of them, evenly over its
/// length, each drawn `height` px tall.
///
/// # Errors
/// When a frame cannot be evaluated or drawn.
pub fn samples(session: &Session, n: usize, height: f64) -> Result<Vec<crate::Image>> {
    let frames: Vec<u32> = session.frames()?.collect();
    let (_, h) = session.canvas()?;
    let shrink = (height / f64::from(h)).min(1.0);
    (0..n.max(1))
        .map(|i| {
            let pick = ((i as f64 + 0.5) / n.max(1) as f64 * frames.len() as f64) as usize;
            let at = frames[pick.min(frames.len().saturating_sub(1))];
            crate::gates::small(session, f64::from(at), shrink)
        })
        .collect()
}

/// How far `variant`'s [`samples`] are from `master`'s.
pub fn distance(master: &[crate::Image], variant: &[crate::Image]) -> Distance {
    let lab = |px: &[u8]| {
        let a = f64::from(px[3]) / 255.0;
        let c = |v: u8| f64::from(v) / 255.0 * a;
        let lch = colour::to_lch([c(px[0]), c(px[1]), c(px[2]), 1.0]);
        let h = lch.h.to_radians();
        [lch.l, lch.c * h.cos(), lch.c * h.sin()]
    };
    let gap = |p: [f64; 3], q: [f64; 3]| {
        ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
    };
    // A palette: OKLab in bins, each its mean colour and its share.
    let palette = |images: &[crate::Image]| {
        let mut bins: BTreeMap<(i32, i32, i32), ([f64; 3], f64)> = BTreeMap::new();
        let mut total: f64 = 0.0;
        for image in images {
            for px in image.pixels().chunks_exact(4) {
                let p = lab(px);
                let key = (
                    (p[0] * 10.0).floor() as i32,
                    (p[1] * 15.0).floor() as i32,
                    (p[2] * 15.0).floor() as i32,
                );
                let bin = bins.entry(key).or_insert(([0.0; 3], 0.0));
                for (sum, v) in bin.0.iter_mut().zip(p) {
                    *sum += v;
                }
                bin.1 += 1.0;
                total += 1.0;
            }
        }
        bins.into_values()
            .filter(|(_, n)| *n / total.max(1.0) >= 0.005)
            .map(|(sum, n)| ([sum[0] / n, sum[1] / n, sum[2] / n], n))
            .collect::<Vec<_>>()
    };
    let (pa, pb) = (palette(master), palette(variant));
    let towards = |from: &[([f64; 3], f64)], to: &[([f64; 3], f64)]| {
        let weight: f64 = from.iter().map(|(_, n)| n).sum();
        from.iter()
            .map(|(c, n)| {
                n * to
                    .iter()
                    .map(|(d, _)| gap(*c, *d))
                    .fold(f64::INFINITY, f64::min)
            })
            .sum::<f64>()
            / weight.max(1.0)
    };
    let palette = match pa.is_empty() || pb.is_empty() {
        true => 0.0,
        false => (towards(&pa, &pb) + towards(&pb, &pa)) / 2.0,
    };
    let same = master.len() == variant.len()
        && master
            .iter()
            .zip(variant)
            .all(|(a, b)| a.width() == b.width() && a.height() == b.height());
    let frames = same.then(|| {
        let (mut sum, mut n) = (0.0, 0.0);
        for (a, b) in master.iter().zip(variant) {
            for (p, q) in a.pixels().chunks_exact(4).zip(b.pixels().chunks_exact(4)) {
                sum += gap(lab(p), lab(q));
                n += 1.0;
            }
        }
        sum / f64::max(n, 1.0)
    });
    let structure = same.then(|| {
        let edges = |image: &crate::Image| {
            let (w, h) = (image.width() as usize, image.height() as usize);
            let l: Vec<f64> = image
                .pixels()
                .chunks_exact(4)
                .map(|px| lab(px)[0])
                .collect();
            edges(&l, w, h)
        };
        let (a, b): (Vec<f64>, Vec<f64>) = master
            .iter()
            .zip(variant)
            .flat_map(|(m, v)| edges(m).into_iter().zip(edges(v)))
            .unzip();
        (1.0 - correlation(&a, &b)).clamp(0.0, 1.0)
    });
    let round = |v: f64| (v * 1e4).round() / 1e4;
    Distance {
        palette: round(palette),
        frames: frames.map(round),
        structure: structure.map(round),
    }
}

/// The gradient magnitude of `l`, a `w` by `h` plane, by Sobel's kernels;
/// the outermost px read their nearest neighbour inside.
fn edges(l: &[f64], w: usize, h: usize) -> Vec<f64> {
    let at = |x: isize, y: isize| {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        l[y * w + x]
    };
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h as isize {
        for x in 0..w as isize {
            let gx = at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x - 1, y)
                - at(x - 1, y + 1);
            let gy = at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x, y - 1)
                - at(x + 1, y - 1);
            out.push(gx.hypot(gy));
        }
    }
    out
}

/// Pearson's correlation of `a` and `b`; 1 when either is flat, as two
/// blank films share all the edges they have.
fn correlation(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len()) as f64;
    if n == 0.0 {
        return 1.0;
    }
    let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        ab += (x - ma) * (y - mb);
        aa += (x - ma).powi(2);
        bb += (y - mb).powi(2);
    }
    match aa > 0.0 && bb > 0.0 {
        true => ab / (aa * bb).sqrt(),
        false => 1.0,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/vary/mod.rs"]
mod tests;
