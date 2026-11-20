//! The brief ([D91]): one page of everything an agent may set in a
//! template, measured in the template's own fonts, and nothing else. Rules
//! and locks come first; then each section with its story role and time;
//! then a table each of texts, colours, files, and choices and numbers.
//! Also the fourth stage of an instance, *measure*, which holds the values
//! an instance gives against the same measures.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};

use nakkash_format::key::{COMP, COMPS, LAYERS, SLOTS};
use nakkash_format::units::Time;
use nakkash_format::{Address, Content, Id, Layer, Listed, Scene, SlotType};
use nakkash_text::Typesetter;
use serde::Serialize;
use serde_json::{Value as Json, json};

use crate::instance::{Level, Problem, STORY_ROLES};
use crate::{Result, Session};

/// Characters read in a second, as the check's `text_hold` reads them.
const READ_PER_SECOND: f64 = 20.0;

/// A template's brief.
#[derive(Debug, Clone, Serialize)]
pub struct Brief {
    /// The file the brief is of: a scene, or an instance.
    pub file: String,
    /// The template's file, when the brief is of an instance.
    pub template: Option<String>,
    /// The template's canonical hash.
    pub hash: String,
    /// Width and height in px.
    pub size: [f64; 2],
    /// Frames per second.
    pub fps: f64,
    /// Length in seconds.
    pub duration: f64,
    /// The template's looks.
    pub looks: Vec<String>,
    /// The look the instance chose.
    pub look: Option<String>,
    /// What is locked, as the scene writes it.
    pub locks: Vec<String>,
    /// The sections, in time order.
    pub sections: Vec<Section>,
    /// The text slots.
    pub texts: Vec<TextRow>,
    /// The colour slots.
    pub colors: Vec<ColorRow>,
    /// The file slots.
    pub media: Vec<MediaRow>,
    /// The choices.
    pub choices: Vec<ChoiceRow>,
    /// The number and point slots.
    pub numbers: Vec<NumberRow>,
}

/// One section: a `comp` layer of the template's root.
#[derive(Debug, Clone, Serialize)]
pub struct Section {
    /// The `comp` layer's id, which an instance's `sections` names.
    pub id: String,
    /// The composition it draws.
    pub comp: String,
    /// What it is for in the story.
    pub role: String,
    /// Whether the role was inferred, not given by the instance.
    pub inferred: bool,
    /// When it shows, in seconds.
    pub start: f64,
    /// When it stops showing, in seconds.
    pub end: f64,
    /// Its composition's labels, in scene seconds.
    pub labels: Vec<(String, f64)>,
}

/// One text slot.
#[derive(Debug, Clone, Serialize)]
pub struct TextRow {
    /// Its key, as an instance writes it.
    pub id: String,
    /// What it is for.
    pub role: Option<String>,
    /// Its words now.
    pub now: String,
    /// The sections it shows in.
    pub sections: Vec<String>,
    /// The size its first layer is set at, px.
    pub size: f64,
    /// About how many characters fit on a line at that size.
    pub chars_per_line: Option<usize>,
    /// The lines it may take.
    pub lines: Option<u32>,
    /// The smallest it may be set at, px.
    pub min_size: Option<f64>,
    /// About how many characters fit on a line at the floor.
    pub chars_at_floor: Option<usize>,
    /// The lines it may take at the floor, where the box holds more of
    /// them than at its size.
    pub lines_at_floor: Option<u32>,
    /// What happens to a longer text.
    pub overflow: Option<String>,
    /// How long it is on screen, seconds.
    pub shown: Option<f64>,
    /// How many characters can be read in that time.
    pub readable: Option<usize>,
    /// Other keys of the same text in other compositions.
    pub also: Vec<String>,
}

/// One colour slot.
#[derive(Debug, Clone, Serialize)]
pub struct ColorRow {
    /// Its key.
    pub id: String,
    /// Its value now, `#RRGGBB` or `#RRGGBBAA`.
    pub now: String,
    /// What it paints most: `text`, `background`, `glow`, `gradient-1`…
    pub paints: String,
    /// Everything it paints, with how many values each.
    pub uses: Vec<(String, usize)>,
    /// The slot it is read against, and the least contrast asked.
    pub pair: Option<(String, Option<f64>)>,
    /// The contrast with its pair now.
    pub contrast: Option<f64>,
    /// Other keys of the same colour in other compositions.
    pub also: Vec<String>,
}

/// One file slot.
#[derive(Debug, Clone, Serialize)]
pub struct MediaRow {
    /// Its key.
    pub id: String,
    /// What it stands for.
    pub kind: String,
    /// Its file now, if any.
    pub now: Option<String>,
    /// The proportions it is shown at.
    pub aspect: Option<String>,
    /// The least size it asks for, px.
    pub min_px: Option<[f64; 2]>,
    /// The sections it shows in.
    pub sections: Vec<String>,
    /// How it fills its box.
    pub fit: String,
    /// What it takes.
    pub accepts: Vec<String>,
}

/// One choice.
#[derive(Debug, Clone, Serialize)]
pub struct ChoiceRow {
    /// Its key.
    pub id: String,
    /// Its word now.
    pub now: String,
    /// Its words.
    pub values: Vec<String>,
}

/// One number or point slot.
#[derive(Debug, Clone, Serialize)]
pub struct NumberRow {
    /// Its key.
    pub id: String,
    /// What it is for.
    pub role: Option<String>,
    /// Its value now.
    pub now: Json,
    /// Its least value.
    pub min: Option<f64>,
    /// Its greatest value.
    pub max: Option<f64>,
}

/// A time in seconds in a composition of `fps` frames a second.
fn seconds(time: Time, fps: f64) -> f64 {
    time.to_frames(fps) / fps
}

/// The times composition `comp` shows in the scene: windows of scene
/// seconds, each with the scene time its own time 0 falls at. The root is
/// one window.
fn windows(scene: &Scene, comp: Option<&str>, depth: usize) -> Vec<(f64, f64, f64)> {
    let root = &scene.root;
    let length = seconds(root.duration, root.fps);
    let Some(comp) = comp else {
        return vec![(0.0, length, 0.0)];
    };
    if depth > 12 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let lists: Vec<(Option<&str>, &[Layer], f64)> =
        std::iter::once((None, root.layers.as_slice(), root.fps))
            .chain(
                scene
                    .comps
                    .iter()
                    .map(|c| (Some(c.id.as_str()), c.layers.as_slice(), c.fps)),
            )
            .collect();
    for (owner, layers, fps) in lists {
        for layer in layers.iter().filter(|l| !l.hidden) {
            let Content::Comp(drawn) = &layer.content else {
                continue;
            };
            if drawn.id != comp {
                continue;
            }
            let inner = scene
                .comp(comp)
                .map_or(f64::INFINITY, |c| seconds(c.duration, c.fps));
            for (ps, pe, po) in windows(scene, owner, depth + 1) {
                let (ls, le) = layer.span.map_or((0.0, f64::INFINITY), |s| {
                    (seconds(s.start, fps), seconds(s.end, fps))
                });
                let offset = po + seconds(layer.offset, fps);
                let start = (ls + po).max(ps);
                let end = (le + po).min(pe).min(offset + inner);
                if end > start {
                    out.push((start, end, offset));
                }
            }
        }
    }
    out
}

/// The scene seconds `layer` of composition `comp` shows in.
fn shown(scene: &Scene, comp: Option<&str>, layer: &Layer) -> Vec<(f64, f64)> {
    let fps = comp
        .and_then(|c| scene.comp(c))
        .map_or(scene.root.fps, |c| c.fps);
    windows(scene, comp, 0)
        .into_iter()
        .filter_map(|(ws, we, wo)| {
            let (ls, le) = layer.span.map_or((0.0, f64::INFINITY), |s| {
                (seconds(s.start, fps), seconds(s.end, fps))
            });
            let (s, e) = ((ls + wo).max(ws), (le + wo).min(we));
            (e > s).then_some((s, e))
        })
        .collect()
}

/// The words of `name` in lower case.
fn words(name: &str) -> Vec<String> {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The story role a section's names say, if any.
fn named_role(names: &[&str]) -> Option<&'static str> {
    const BY_WORD: [(&str, &[&str]); 5] = [
        (
            "hook",
            &["hook", "intro", "opening", "open", "start", "teaser"],
        ),
        (
            "problem",
            &["problem", "pain", "challenge", "before", "issue"],
        ),
        (
            "feature",
            &[
                "feature", "features", "demo", "product", "screen", "app", "showcase", "benefit",
                "benefits", "how",
            ],
        ),
        (
            "proof",
            &[
                "proof",
                "testimonial",
                "testimonials",
                "review",
                "reviews",
                "stats",
                "trust",
                "clients",
                "results",
            ],
        ),
        (
            "cta",
            &[
                "cta", "outro", "end", "ending", "final", "call", "signup", "download", "contact",
                "closing", "pack",
            ],
        ),
    ];
    let all: Vec<String> = names.iter().flat_map(|n| words(n)).collect();
    BY_WORD
        .iter()
        .find(|(_, vocabulary)| all.iter().any(|w| vocabulary.contains(&w.as_str())))
        .map(|(role, _)| *role)
}

/// A slot's key, as an instance writes it.
pub(crate) fn key_of(listed: &Listed) -> String {
    match &listed.comp {
        None => listed.slot.id.to_string(),
        Some(comp) => format!("{comp}/{}", listed.slot.id),
    }
}

/// The layers of `raw` that follow the slot of `listed`, by composition
/// and id.
fn follower_layers(raw: &Json, listed: &Listed) -> Vec<(Option<String>, String)> {
    let found = crate::instance::followers_of(raw, listed.comp.as_deref(), &listed.slot.id);
    found
        .into_iter()
        .filter_map(|(comp, index)| {
            let list = match &comp {
                None => raw.get(LAYERS),
                Some(c) => raw
                    .get(COMPS)
                    .and_then(|cs| cs.get(c))
                    .and_then(|c| c.get(LAYERS)),
            };
            let id = list?.get(index)?.get("id")?.as_str()?.to_owned();
            Some((comp, id))
        })
        .collect()
}

/// The typed layer `id` of composition `comp`.
fn layer_at<'s>(scene: &'s Scene, comp: Option<&str>, id: &str) -> Option<&'s Layer> {
    match comp {
        None => scene.root.layers.iter().find(|l| l.id == id),
        Some(c) => scene.comp(c)?.layers.iter().find(|l| l.id == id),
    }
}

/// The sections `windows` fall in, by most overlap.
fn sections_in(sections: &[Section], windows: &[(f64, f64)]) -> Vec<String> {
    sections
        .iter()
        .filter(|s| {
            windows
                .iter()
                .any(|(a, b)| a.max(s.start) < b.min(s.end) - 1e-6)
        })
        .map(|s| s.id.clone())
        .collect()
}

/// What one text measures as.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Measured {
    /// The size it is written at.
    pub size: f64,
    /// The scale its fit sets it at, 1 when it fits.
    pub scale: f64,
    /// The lines it takes.
    pub lines: usize,
    /// Its fit's width, if it has one.
    pub width: Option<f64>,
    /// Its fit's height, if it has one.
    pub height: Option<f64>,
    /// The distance from one line to the next at the size written, px.
    pub pitch: f64,
    /// The mean advance of its characters as written, px.
    pub advance: f64,
}

/// Sets the text layer `layer` of composition `comp` of `scene`.
pub(crate) fn measure(
    scene: &Scene,
    comp: Option<&str>,
    layer: &str,
    typesetter: &mut Typesetter,
) -> Option<Measured> {
    let id: Id = layer.parse().ok()?;
    let address = Address::written(comp, id);
    let found = scene.find(&address)?;
    let Content::Text(content) = &found.content else {
        return None;
    };
    let fitted = nakkash_scene::set_text(content, typesetter).ok()?;
    let written = nakkash_scene::set_text_as_written(content, typesetter).ok()?;
    let chars: usize = written.layout.lines.iter().map(|l| l.characters).sum();
    let width: f64 = written.layout.lines.iter().map(|l| l.width).sum();
    let (width_of_fit, height_of_fit) = match content.fit {
        Some(
            nakkash_format::text::TextFit::Shrink { width, height }
            | nakkash_format::text::TextFit::Wrap { width, height },
        ) => (Some(width), Some(height)),
        Some(nakkash_format::text::TextFit::ShrinkToBox) | None => (
            content.frame.map(|f| f.width()),
            content.frame.map(|f| f.height()),
        ),
    };
    let size = content.first_style().font.size;
    let pitch = match content.first_style().leading {
        nakkash_format::text::Leading::Px(px) => px,
        nakkash_format::text::Leading::Auto => {
            size * content.paragraphs.first().map_or(1.2, |p| p.auto_leading)
        }
    };
    Some(Measured {
        size,
        scale: fitted.layout.scale,
        lines: fitted.layout.lines.len(),
        width: width_of_fit,
        height: height_of_fit,
        pitch,
        advance: if chars > 0 {
            width / chars as f64
        } else {
            content.first_style().font.size * 0.5
        },
    })
}

/// What a colour is used for at `path`, the keys from the layer down to
/// the value, in a layer of `kind` (`text`, `shapes`, `rect`, …).
fn usage(kind: &str, path: &[String], fx: Option<&str>) -> String {
    let last = path.last().map(String::as_str).unwrap_or_default();
    if let Some(effect) = fx {
        return match (effect, last) {
            ("glow", _) => "glow".into(),
            ("drop-shadow" | "inner-shadow" | "long-shadow", _) => "shadow".into(),
            ("four-color", key) => key
                .strip_prefix("color-")
                .map_or("gradient".into(), |n| format!("gradient-{n}")),
            ("gradient-ramp" | "ramp", "start" | "start-color" | "from") => "gradient-start".into(),
            ("gradient-ramp" | "ramp", _) => "gradient-end".into(),
            ("tint", "black") => "grade-dark".into(),
            ("tint", "white") => "grade-light".into(),
            ("tritone", key) => format!("grade-{key}"),
            ("fill", _) => "fill".into(),
            ("stroke", _) => "stroke".into(),
            (other, _) => other.to_owned(),
        };
    }
    match (kind, last) {
        ("scene", "bg") => "background".into(),
        ("text", "fill") if path.iter().any(|p| p == "animate") => "highlight".into(),
        ("text", "fill") => "text".into(),
        ("text", "stroke") => "text-outline".into(),
        ("shapes", "stroke") | (_, "stroke") => "stroke".into(),
        ("shapes", _) => "shape".into(),
        ("rect", "fill") => "plate".into(),
        (_, other) => other.to_owned(),
    }
}

/// Every use of every colour slot in `raw`: by the slot's key, what each
/// use paints.
fn colour_uses(raw: &Json) -> BTreeMap<String, Vec<String>> {
    fn walk(
        v: &Json,
        path: &mut Vec<String>,
        kind: &str,
        fx: Option<&str>,
        comp: Option<&str>,
        declared: &dyn Fn(Option<&str>, &str) -> Option<String>,
        out: &mut BTreeMap<String, Vec<String>>,
    ) {
        match v {
            Json::String(text) => {
                let Some(rest) = text.strip_prefix('$').filter(|r| !r.starts_with('$')) else {
                    return;
                };
                let id = rest.split(' ').next().unwrap_or_default();
                if let Some(key) = declared(comp, id) {
                    out.entry(key).or_default().push(usage(kind, path, fx));
                }
            }
            Json::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    // An effect is an object whose first key is its kind.
                    let effect = match path.last().map(String::as_str) {
                        Some("fx") => item
                            .as_object()
                            .and_then(|o| o.keys().next())
                            .map(String::as_str),
                        _ => fx,
                    };
                    path.push(i.to_string());
                    walk(item, path, kind, effect, comp, declared, out);
                    path.pop();
                }
            }
            Json::Object(fields) => {
                for (k, item) in fields {
                    path.push(k.clone());
                    walk(item, path, kind, fx, comp, declared, out);
                    path.pop();
                }
            }
            _ => {}
        }
    }
    let scene_slots: Vec<String> = raw
        .get(SLOTS)
        .and_then(Json::as_object)
        .map(|s| s.keys().cloned().collect())
        .unwrap_or_default();
    let comp_slots = |comp: &str| -> Vec<String> {
        raw.get(COMPS)
            .and_then(|c| c.get(comp))
            .and_then(|c| c.get(SLOTS))
            .and_then(Json::as_object)
            .map(|s| s.keys().cloned().collect())
            .unwrap_or_default()
    };
    let declared = |comp: Option<&str>, id: &str| -> Option<String> {
        if let Some(c) = comp
            && comp_slots(c).iter().any(|s| s == id)
        {
            return Some(format!("{c}/{id}"));
        }
        scene_slots.iter().any(|s| s == id).then(|| id.to_owned())
    };
    let mut out = BTreeMap::new();
    if let Some(bg) = raw.get("bg") {
        walk(
            bg,
            &mut vec!["bg".into()],
            "scene",
            None,
            None,
            &declared,
            &mut out,
        );
    }
    let mut lists: Vec<(Option<&str>, &Json)> =
        raw.get(LAYERS).map(|l| (None, l)).into_iter().collect();
    for (key, comp) in raw
        .get(COMPS)
        .and_then(Json::as_object)
        .into_iter()
        .flatten()
    {
        if let Some(bg) = comp.get("bg") {
            walk(
                bg,
                &mut vec!["bg".into()],
                "scene",
                None,
                Some(key),
                &declared,
                &mut out,
            );
        }
        if let Some(l) = comp.get(LAYERS) {
            lists.push((Some(key.as_str()), l));
        }
    }
    for (comp, list) in lists {
        for layer in list.as_array().into_iter().flatten() {
            let kind = ["text", "shapes", "rect", "image", "video", COMP]
                .into_iter()
                .find(|k| layer.get(*k).is_some())
                .unwrap_or("layer");
            walk(
                layer,
                &mut Vec::new(),
                kind,
                None,
                comp,
                &declared,
                &mut out,
            );
        }
    }
    out
}

/// The WCAG contrast ratio between two colours.
pub(crate) fn contrast(a: [f64; 4], b: [f64; 4]) -> f64 {
    let lum = |c: [f64; 4]| {
        let lin = |v: f64| {
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
    };
    let (la, lb) = (lum(a), lum(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// `color` as `#RRGGBB`, or `#RRGGBBAA` when it is not opaque.
fn hex(color: [f64; 4]) -> String {
    let [r, g, b, a] = color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    match a {
        255 => format!("#{r:02X}{g:02X}{b:02X}"),
        a => format!("#{r:02X}{g:02X}{b:02X}{a:02X}"),
    }
}

/// Makes the brief of `session`, read from `file`; `instance` when it is
/// an instance: the template's file and hash, the instance's look and
/// sections.
///
/// # Errors
/// The session's own, when its fonts cannot be read.
pub fn brief(
    session: &Session,
    file: &str,
    instance: Option<(&str, &str, Option<&str>, &BTreeMap<String, String>)>,
) -> Result<Brief> {
    let scene = session.scene();
    let raw = session.document().raw();
    let root = &scene.root;
    let mut typesetter = Typesetter::new(session.fonts()?.clone());
    let given_roles = instance.map(|(_, _, _, s)| s.clone()).unwrap_or_default();

    // Sections: the root's `comp` layers, in time order.
    let mut sections: Vec<Section> = Vec::new();
    for layer in root.layers.iter().filter(|l| !l.hidden) {
        let Content::Comp(drawn) = &layer.content else {
            continue;
        };
        let Some(comp) = scene.comp(&drawn.id) else {
            continue;
        };
        let span = shown(scene, None, layer);
        let (Some(start), Some(end)) = (span.first().map(|s| s.0), span.last().map(|s| s.1)) else {
            continue;
        };
        let inner_end = seconds(layer.offset, root.fps) + seconds(comp.duration, comp.fps);
        let offset = seconds(layer.offset, root.fps);
        sections.push(Section {
            id: layer.id.to_string(),
            comp: drawn.id.clone(),
            role: String::new(),
            inferred: true,
            start,
            end: end.min(inner_end),
            labels: comp
                .labels
                .iter()
                .map(|(n, t)| (n.clone(), offset + seconds(*t, comp.fps)))
                .filter(|(_, t)| *t >= start && *t <= end)
                .collect(),
        });
    }
    sections.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.id.cmp(&b.id)));

    let listed = session.slots();
    let mut texts = Vec::new();
    let mut colors = Vec::new();
    let mut media = Vec::new();
    let mut choices = Vec::new();
    let mut numbers = Vec::new();
    let uses = colour_uses(raw);
    let value_of = |key: &str| {
        listed
            .iter()
            .find(|l| key_of(l) == key)
            .map(|l| l.slot.value.clone())
    };
    for l in &listed {
        let key = key_of(l);
        let followers = follower_layers(raw, l);
        let windows: Vec<(f64, f64)> = followers
            .iter()
            .filter_map(|(c, id)| {
                layer_at(scene, c.as_deref(), id).map(|layer| shown(scene, c.as_deref(), layer))
            })
            .flatten()
            .collect();
        let in_sections = sections_in(&sections, &windows);
        let c = &l.slot.contract;
        match l.slot.ty {
            SlotType::Text => {
                let first = followers.first();
                let m = first
                    .and_then(|(comp, id)| measure(scene, comp.as_deref(), id, &mut typesetter));
                let per_line = |size: f64| {
                    m.and_then(|m| Some((m.width? / (m.advance * size / m.size)).floor() as usize))
                };
                // The lines the fit's box holds at `size`, no more than the
                // slot allows.
                let lines_at = |size: f64| {
                    let allowed = c.max_lines.or(m.map(|m| m.lines as u32))?;
                    let held = m.and_then(|m| {
                        let pitch = m.pitch * size / m.size;
                        m.height
                            .filter(|_| pitch > 0.0)
                            .map(|h| ((h / pitch + 1e-6).floor() as u32).max(1))
                    });
                    Some(held.map_or(allowed, |held| held.min(allowed)))
                };
                let lines = m.and_then(|m| lines_at(m.size));
                let lines_at_floor = c
                    .min_size
                    .and_then(lines_at)
                    .filter(|&at_floor| Some(at_floor) != lines);
                let shown_for = windows
                    .iter()
                    .map(|(a, b)| b - a)
                    .fold(None, |acc: Option<f64>, d| {
                        Some(acc.map_or(d, |a| a.max(d)))
                    });
                texts.push(TextRow {
                    id: key,
                    role: l.slot.role.as_ref().map(ToString::to_string),
                    now: l.slot.value.as_str().unwrap_or_default().to_owned(),
                    sections: in_sections,
                    size: m.map_or(0.0, |m| m.size),
                    chars_per_line: m.and_then(|m| per_line(m.size)),
                    lines,
                    lines_at_floor,
                    min_size: c.min_size,
                    chars_at_floor: c.min_size.and_then(per_line),
                    overflow: c.overflow.map(|o| o.to_string()),
                    shown: shown_for,
                    readable: shown_for.map(|s| (s * READ_PER_SECOND).floor() as usize),
                    also: Vec::new(),
                });
            }
            SlotType::Color => {
                let used = uses.get(&key).cloned().unwrap_or_default();
                let mut counted: BTreeMap<String, usize> = BTreeMap::new();
                for u in &used {
                    *counted.entry(u.clone()).or_default() += 1;
                }
                let mut ranked: Vec<(String, usize)> = counted.into_iter().collect();
                ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let value = nakkash_format::color(&l.slot.value);
                let pair = c.pair.as_ref().map(|p| {
                    let pkey = match &l.comp {
                        Some(comp)
                            if listed
                                .iter()
                                .any(|o| o.comp.as_deref() == Some(comp) && o.slot.id == *p) =>
                        {
                            format!("{comp}/{p}")
                        }
                        _ => p.to_string(),
                    };
                    (pkey, c.contrast)
                });
                let ratio = pair.as_ref().and_then(|(p, _)| {
                    Some(contrast(value?, nakkash_format::color(&value_of(p)?)?))
                });
                colors.push(ColorRow {
                    id: key,
                    now: value.map(hex).unwrap_or_default(),
                    paints: ranked.first().map_or_else(
                        || {
                            l.slot
                                .role
                                .as_ref()
                                .map_or("unused".into(), ToString::to_string)
                        },
                        |r| r.0.clone(),
                    ),
                    uses: ranked.into_iter().take(4).collect(),
                    pair,
                    contrast: ratio.map(|r| (r * 10.0).round() / 10.0),
                    also: Vec::new(),
                });
            }
            SlotType::Image | SlotType::Video => {
                let first = followers
                    .first()
                    .and_then(|(c, id)| layer_at(scene, c.as_deref(), id));
                let (fit, boxed) = match first.map(|l| &l.content) {
                    Some(Content::Image(i)) => {
                        (format!("{:?}", i.fit).to_lowercase(), Some(i.frame))
                    }
                    Some(Content::Video(v)) => {
                        (format!("{:?}", v.fit).to_lowercase(), Some(v.frame))
                    }
                    _ => ("fill".into(), None),
                };
                let aspect = c.aspect.clone().or_else(|| {
                    boxed.map(|b| format!("{}:{}", b.width().round(), b.height().round()))
                });
                let mut accepts: Vec<String> = vec![l.slot.ty.name().to_owned()];
                accepts.extend(
                    c.accepts
                        .iter()
                        .filter(|t| **t != l.slot.ty)
                        .map(|t| t.name().to_owned()),
                );
                media.push(MediaRow {
                    id: key,
                    kind: c
                        .kind
                        .map(|k| k.to_string())
                        .or_else(|| l.slot.role.as_ref().map(ToString::to_string))
                        .unwrap_or_else(|| l.slot.ty.name().to_owned()),
                    now: l.slot.value.as_str().map(str::to_owned),
                    aspect,
                    min_px: c.min_px,
                    sections: in_sections,
                    fit,
                    accepts,
                });
            }
            SlotType::Choice => choices.push(ChoiceRow {
                id: key,
                now: l.slot.value.as_str().unwrap_or_default().to_owned(),
                values: c.values.clone(),
            }),
            SlotType::Number | SlotType::Point => numbers.push(NumberRow {
                id: key,
                role: l.slot.role.as_ref().map(ToString::to_string),
                now: l.slot.value.clone(),
                min: c.min,
                max: c.max,
            }),
        }
    }
    // The same slot in several compositions is one row.
    fold_texts(&mut texts);
    fold_colors(&mut colors);

    // Story roles: given, else named, else by place and content.
    let screens: Vec<&String> = media
        .iter()
        .filter(|m| matches!(m.kind.as_str(), "screen" | "video" | "image"))
        .flat_map(|m| &m.sections)
        .collect();
    let count = sections.len();
    for (i, section) in sections.iter_mut().enumerate() {
        if let Some(role) = given_roles.get(&section.id) {
            section.role = role.clone();
            section.inferred = false;
            continue;
        }
        let named = named_role(&[&section.id, &section.comp]);
        let texted = texts.iter().any(|t| t.sections.contains(&section.id));
        section.role = match named {
            Some(role) => role,
            None if i == 0 && count > 1 => "hook",
            None if i + 1 == count && count > 1 => "cta",
            None if screens.contains(&&section.id) => "feature",
            None if !texted => "feature",
            None => "unknown",
        }
        .to_owned();
        debug_assert!(STORY_ROLES.contains(&section.role.as_str()));
    }

    let looks = raw
        .get("looks")
        .and_then(Json::as_object)
        .map(|l| l.keys().cloned().collect())
        .unwrap_or_default();
    Ok(Brief {
        file: file.to_owned(),
        template: instance.map(|(t, _, _, _)| t.to_owned()),
        hash: instance.map_or_else(
            || nakkash_format::canon::hash(raw),
            |(_, h, _, _)| h.to_owned(),
        ),
        size: [f64::from(root.size.width), f64::from(root.size.height)],
        fps: root.fps,
        duration: seconds(root.duration, root.fps),
        looks,
        look: instance.and_then(|(_, _, l, _)| l.map(str::to_owned)),
        locks: raw
            .get("locks")
            .and_then(Json::as_array)
            .map(|l| {
                l.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        sections,
        texts,
        colors,
        media,
        choices,
        numbers,
    })
}

/// Text rows of the same words, role and measures in several
/// compositions, folded into the first.
fn fold_texts(rows: &mut Vec<TextRow>) {
    let mut out: Vec<TextRow> = Vec::new();
    for row in rows.drain(..) {
        let same = out.iter_mut().find(|o| {
            o.id.contains('/')
                && row.id.contains('/')
                && o.now == row.now
                && o.role == row.role
                && o.size == row.size
        });
        match same {
            Some(o) => {
                o.also.push(row.id);
                o.sections.extend(row.sections);
            }
            None => out.push(row),
        }
    }
    *rows = out;
}

/// Colour rows of composition slots of one value and one use, folded into
/// the first.
fn fold_colors(rows: &mut Vec<ColorRow>) {
    let mut out: Vec<ColorRow> = Vec::new();
    for row in rows.drain(..) {
        let base = |id: &str| {
            id.split_once('/').map(|(_, s)| {
                s.trim_end_matches(|c: char| c.is_ascii_digit() || c == '-')
                    .to_owned()
            })
        };
        let same = out.iter_mut().find(|o| {
            o.id.contains('/')
                && o.now == row.now
                && o.paints == row.paints
                && base(&o.id) == base(&row.id)
        });
        match same {
            Some(o) => o.also.push(row.id),
            None => out.push(row),
        }
    }
    *rows = out;
}

/// Seconds as the brief writes them: `3.84`.
fn s(t: f64) -> String {
    let t = (t * 100.0).round() / 100.0;
    nakkash_format::fmt_num(t)
}

/// At most `n` characters of `text`, with `…` when cut.
fn cut(text: &str, n: usize) -> String {
    let flat = text.replace('\n', " / ");
    match flat.chars().count() > n {
        true => flat.chars().take(n - 1).collect::<String>() + "…",
        false => flat,
    }
}

impl fmt::Display for Brief {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        let hash = self.hash.get(..19).unwrap_or(&self.hash);
        let _ = write!(
            out,
            "# nakkash brief · {} · {hash}… · {}x{} · {} fps · {} s",
            self.template.as_deref().unwrap_or(&self.file),
            self.size[0],
            self.size[1],
            s(self.fps),
            s(self.duration)
        );
        if !self.looks.is_empty() {
            let _ = write!(out, " · looks: {}", self.looks.join(", "));
        }
        out.push('\n');
        let target = match &self.template {
            Some(_) => format!("`slots` of {}", self.file),
            None => "an instance: `nakkash slots <template> --as-instance --out x.json`".to_owned(),
        };
        let _ = writeln!(
            out,
            "RULES write only the ids below, into {target}; everything else is locked"
        );
        let _ = writeln!(
            out,
            "      ≈ch is a guide: `nakkash check <instance>` sets the real font and is the authority"
        );
        let _ = writeln!(
            out,
            "      text read against a colour uses its pair; edit all values, then check once: it lists every problem"
        );
        if !self.locks.is_empty() {
            let _ = writeln!(out, "LOCKS {}", self.locks.join(", "));
        }
        if let Some(look) = &self.look {
            let _ = writeln!(out, "LOOK  {look}");
        }
        out.push_str("\n## sections\n| id | role | s | labels |\n");
        for sec in &self.sections {
            let role = if sec.inferred {
                format!("{}?", sec.role)
            } else {
                sec.role.clone()
            };
            let labels: Vec<String> = sec
                .labels
                .iter()
                .map(|(n, t)| format!("{n}@{}", s(*t)))
                .collect();
            let _ = writeln!(
                out,
                "| {} | {role} | {}–{} | {} |",
                sec.id,
                s(sec.start),
                s(sec.end),
                labels.join(" ")
            );
        }
        if !self.texts.is_empty() {
            out.push_str("\n## text\n| id | role | now | fits | overflow | read | in |\n");
            for t in &self.texts {
                let mut fits = match (t.chars_per_line, t.lines) {
                    (Some(c), Some(l)) => format!("≈{c} ch × {l} ln @{}", s(t.size)),
                    (Some(c), None) => format!("≈{c} ch @{}", s(t.size)),
                    _ => format!("@{}", s(t.size)),
                };
                if let (Some(floor), Some(c)) = (t.min_size, t.chars_at_floor) {
                    let _ = write!(fits, " (≥{}: ≈{c}", s(floor));
                    if let Some(l) = t.lines_at_floor {
                        let _ = write!(fits, " × {l} ln");
                    }
                    fits.push(')');
                }
                let read = t.readable.map_or_else(String::new, |r| format!("≤{r} ch"));
                let mut id = t.id.clone();
                if !t.also.is_empty() {
                    let _ = write!(id, " +{}", t.also.len());
                }
                let _ = writeln!(
                    out,
                    "| {id} | {} | {} | {fits} | {} | {read} | {} |",
                    t.role.as_deref().unwrap_or(""),
                    cut(&t.now, 32),
                    t.overflow.as_deref().unwrap_or("none"),
                    t.sections.join(" ")
                );
            }
        }
        if !self.colors.is_empty() {
            out.push_str("\n## colour\n| id | paints | now | pair | uses |\n");
            for c in &self.colors {
                let pair = match (&c.pair, c.contrast) {
                    (Some((p, min)), now) => format!(
                        "{p} {}{}",
                        now.map_or(String::new(), |n| format!("{n}:1")),
                        min.map_or(String::new(), |m| format!(" ≥{m}"))
                    ),
                    _ => String::new(),
                };
                let uses: Vec<String> = c.uses.iter().map(|(u, n)| format!("{u}×{n}")).collect();
                let mut id = c.id.clone();
                if !c.also.is_empty() {
                    let _ = write!(id, " +{} ({})", c.also.len(), c.also.join(" "));
                }
                let _ = writeln!(
                    out,
                    "| {id} | {} | {} | {pair} | {} |",
                    c.paints,
                    c.now,
                    uses.join(" ")
                );
            }
        }
        if !self.media.is_empty() {
            out.push_str("\n## media\n| id | kind | now | aspect | min px | fit | takes | in |\n");
            for m in &self.media {
                let min = m
                    .min_px
                    .map_or(String::new(), |[w, h]| format!("{}x{}", s(w), s(h)));
                let now = m.now.as_deref().map_or("empty".to_owned(), |n| {
                    cut(n.rsplit('/').next().unwrap_or(n), 24)
                });
                let _ = writeln!(
                    out,
                    "| {} | {} | {now} | {} | {min} | {} | {} | {} |",
                    m.id,
                    m.kind,
                    m.aspect.as_deref().unwrap_or(""),
                    m.fit,
                    m.accepts.join("|"),
                    m.sections.join(" ")
                );
            }
        }
        if !self.choices.is_empty() || !self.numbers.is_empty() {
            out.push_str("\n## choices\n");
            for c in &self.choices {
                let _ = writeln!(out, "{} ∈ {{{}}} now {}", c.id, c.values.join(", "), c.now);
            }
            for n in &self.numbers {
                let range = match (n.min, n.max) {
                    (Some(a), Some(b)) => format!(" {}–{}", s(a), s(b)),
                    (Some(a), None) => format!(" ≥{}", s(a)),
                    (None, Some(b)) => format!(" ≤{}", s(b)),
                    (None, None) => String::new(),
                };
                let role = n
                    .role
                    .as_deref()
                    .filter(|r| *r != n.id)
                    .map_or(String::new(), |r| format!(" ({r})"));
                let _ = writeln!(out, "{}{role}{range} now {}", n.id, n.now);
            }
        }
        out.push_str(
            "\nmore: `nakkash at <instance> <time> <layer>` · `nakkash slots <template> --json`\n",
        );
        f.write_str(&out)
    }
}

/// Stage 4 of an instance, *measure*: each text the instance gives set in
/// its real font into its fit, each colour pair's contrast, and each file
/// it gives against the slot's size and proportions.
pub(crate) fn measured(session: &Session, given: &[(String, Json)]) -> Vec<Problem> {
    let mut problems = Vec::new();
    let Ok(fonts) = session.fonts() else {
        return problems;
    };
    let mut typesetter = Typesetter::new(fonts.clone());
    let scene = session.scene();
    let raw = session.document().raw();
    let listed = session.slots();
    let value_of = |key: &str| {
        listed
            .iter()
            .find(|l| key_of(l) == key)
            .map(|l| l.slot.value.clone())
    };
    let written: Vec<&String> = given.iter().map(|(k, _)| k).collect();
    for l in &listed {
        let key = key_of(l);
        let c = &l.slot.contract;
        let path = format!("slots.{key}");
        match l.slot.ty {
            SlotType::Text if written.contains(&&key) => {
                for (comp, id) in follower_layers(raw, l).into_iter().take(1) {
                    let Some(m) = measure(scene, comp.as_deref(), &id, &mut typesetter) else {
                        continue;
                    };
                    let needed = m.size * m.scale;
                    let over_lines = c.max_lines.is_some_and(|max| m.lines as u32 > max);
                    let below = c.min_size.is_some_and(|min| needed < min - 0.05);
                    let rejected =
                        c.overflow == Some(nakkash_format::Overflow::Reject) && m.scale < 0.999;
                    let measured =
                        json!({"size_px_needed": (needed * 10.0).round() / 10.0, "lines": m.lines});
                    let limit = json!({"size_px": m.size, "min_size_px": c.min_size, "max_lines": c.max_lines});
                    let chars = l.slot.value.as_str().map_or(0, |t| t.chars().count());
                    let floor = c.min_size.unwrap_or(m.size);
                    let keep = (chars as f64 * (needed / floor).min(1.0)).floor() as usize;
                    let shorten = chars.saturating_sub(keep).max(1);
                    // The lines the box holds at the floor, where a break
                    // could help.
                    let held_at_floor = m.height.map_or(u32::MAX, |h| {
                        let pitch = m.pitch * floor / m.size;
                        if pitch > 0.0 {
                            ((h / pitch + 1e-6).floor() as u32).max(1)
                        } else {
                            u32::MAX
                        }
                    });
                    if below || over_lines || rejected {
                        let rule = if rejected {
                            "overflow"
                        } else if over_lines {
                            "max-lines"
                        } else {
                            "min-size"
                        };
                        problems.push(Problem::new(Level::Error, "text.overflow", 4, path.clone(), format!("`{key}` needs {:.0} px in {} line{} to fit", needed, m.lines, if m.lines == 1 { "" } else { "s" })).slot(key.clone()).measured(rule, measured, limit).hint(match c.max_lines {
                            Some(max) if !rejected && (m.lines as u32) < max.min(held_at_floor) => format!("shorten by about {shorten} characters, or break it with \\n into up to {} lines", max.min(held_at_floor)),
                            _ => format!("shorten by about {shorten} characters"),
                        }));
                    } else if m.scale < 0.999 {
                        problems.push(
                            Problem::new(
                                Level::Warning,
                                "text.shrunk",
                                4,
                                path.clone(),
                                format!(
                                    "`{key}` is set at {:.0} px of {} to fit",
                                    needed,
                                    s(m.size)
                                ),
                            )
                            .slot(key.clone())
                            .measured("overflow", measured, limit),
                        );
                    }
                }
            }
            SlotType::Color => {
                let Some(pair) = &c.pair else { continue };
                let pkey = match &l.comp {
                    Some(comp)
                        if listed
                            .iter()
                            .any(|o| o.comp.as_deref() == Some(comp) && o.slot.id == *pair) =>
                    {
                        format!("{comp}/{pair}")
                    }
                    _ => pair.to_string(),
                };
                if !written.contains(&&key) && !written.contains(&&pkey) {
                    continue;
                }
                let (Some(a), Some(b)) = (
                    nakkash_format::color(&l.slot.value),
                    value_of(&pkey).and_then(|v| nakkash_format::color(&v)),
                ) else {
                    continue;
                };
                let ratio = contrast(a, b);
                if let Some(min) = c.contrast.filter(|m| ratio < m - 0.005) {
                    problems.push(
                        Problem::new(
                            Level::Error,
                            "color.contrast",
                            4,
                            path,
                            format!("`{key}` against `{pkey}` is {:.2}:1", ratio),
                        )
                        .slot(key.clone())
                        .measured(
                            "contrast",
                            json!((ratio * 100.0).round() / 100.0),
                            json!(min),
                        )
                        .hint(format!(
                            "make `{key}` or `{pkey}` lighter or darker until they reach {min}:1"
                        )),
                    );
                }
            }
            SlotType::Image if written.contains(&&key) => {
                let Some(file) = l.slot.value.as_str() else {
                    continue;
                };
                let Some([w, h]) = session.picture_size(file) else {
                    continue;
                };
                if let Some([mw, mh]) = c.min_px.filter(|[mw, mh]| w < *mw || h < *mh) {
                    problems.push(
                        Problem::new(
                            Level::Warning,
                            "media.small",
                            4,
                            path.clone(),
                            format!("`{file}` is {w}x{h} px", w = s(w), h = s(h)),
                        )
                        .slot(key.clone())
                        .measured("min-px", json!([w, h]), json!([mw, mh]))
                        .hint("a larger file, or it is drawn enlarged"),
                    );
                }
                let target = c.aspect.as_deref().and_then(nakkash_format::aspect_ratio);
                if let Some(target) = target {
                    let ratio = w / h;
                    let cut = 1.0 - (ratio / target).min(target / ratio);
                    if cut > 0.02 {
                        problems.push(Problem::new(Level::Warning, "media.crop", 4, path.clone(), format!("`{file}` is {:.2}:1 where the slot shows {:.2}:1, so {:.0} % of it is cut or left empty", ratio, target, cut * 100.0)).slot(key.clone()).measured("aspect", json!((ratio * 1000.0).round() / 1000.0), json!(c.aspect)).hint("a file of the slot's proportions"));
                    }
                }
            }
            _ => {}
        }
    }
    problems
}
