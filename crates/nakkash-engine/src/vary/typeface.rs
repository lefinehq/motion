//! The type axis ([D78]): faces paired by their class, and copy of any
//! length fitted into the master's text boxes.
//!
//! The headline is the text the `headline` role names, else the one set
//! largest. A pairing gives it a family of another class (`contrast`) or
//! of its own (`same`), and the rest of the text a family that pairs with
//! it. Under a `typefaces` lock only the master's families are used: the
//! headline and the rest trade families, or take other weights.
//!
//! New text is measured as set, against the box the master's text covers:
//! wider, it is set smaller, down to half its size; past that it is broken
//! into balanced lines, the fewest that fit, and set smaller again.
//!
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use nakkash_format::shorthand::Font;
use nakkash_format::{Result, RoleTarget};
use nakkash_scene::{ContentState, walk};
use nakkash_text::{Class, FontSet};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as Json, json};

use super::TypeSpec;
use super::places::{self, Held, Place, Visit};
use super::rng::Rng;
use crate::session::Session;

/// The least a text is set at, against its size in the master.
const SMALLEST: f64 = 0.5;

/// The most lines a point text is broken into, or twice its master's
/// lines when it has more.
const MOST_LINES: usize = 4;

/// A family and the weights it has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Family {
    /// Its name, as a scene's `font` writes it.
    pub name: String,
    /// Its class.
    pub class: String,
    /// The weights it has, upright.
    pub weights: Vec<u16>,
    /// The folder or file it comes from when the scene does not load it.
    pub source: Option<PathBuf>,
}

/// The type step of a recipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypeStep {
    /// The headline's new family and weight.
    pub headline: Option<(String, u16)>,
    /// The other texts' new family; each keeps the nearest weight.
    pub body: Option<String>,
    /// The new copy: `long`, or the text itself.
    pub copy: Option<String>,
    /// Fonts to add to the scene's `fonts` for the new families.
    pub fonts: Vec<PathBuf>,
    /// The families the step draws on, with their weights.
    pub families: Vec<Family>,
}

impl fmt::Display for TypeStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "type")?;
        if let Some((family, weight)) = &self.headline {
            write!(f, " {family} {weight}")?;
        }
        if let Some(body) = &self.body {
            write!(f, " / {body}")?;
        }
        if let Some(copy) = &self.copy {
            match copy.as_str() {
                "long" => write!(f, ", long copy")?,
                _ => write!(f, ", new copy")?,
            }
        }
        Ok(())
    }
}

/// Every family the run may draw on: those the scene loads, those in the
/// spec's folders, narrowed to the spec's list when it has one; sorted by
/// name.
pub(crate) fn candidates(master: &Session, spec: &TypeSpec, base: &Path) -> Result<Vec<Family>> {
    let mut set = master.fonts()?.clone();
    let mut from: BTreeMap<String, PathBuf> = BTreeMap::new();
    for folder in &spec.fonts {
        let path = base.join(folder);
        let before = set.faces().len();
        match path.is_dir() {
            true => set.add_folder(&path)?,
            false => set.add_file(&path)?,
        };
        for face in &set.faces()[before..] {
            from.entry(face.family.clone())
                .or_insert_with(|| path.clone());
        }
    }
    let mut families: BTreeMap<String, Family> = BTreeMap::new();
    for face in set.faces() {
        if face.slant != nakkash_text::Slant::Normal {
            continue;
        }
        let entry = families
            .entry(face.family.clone())
            .or_insert_with(|| Family {
                name: face.family.clone(),
                class: face.metrics().class.to_string(),
                weights: Vec::new(),
                source: from.get(&face.family).cloned(),
            });
        let weight = (face.weight / 100.0).round() as u16 * 100;
        if (100..=900).contains(&weight) && !entry.weights.contains(&weight) {
            entry.weights.push(weight);
        }
    }
    let wanted: BTreeSet<&str> = spec.families.iter().map(String::as_str).collect();
    Ok(families
        .into_values()
        .filter(|f| !f.weights.is_empty())
        .filter(|f| wanted.is_empty() || wanted.contains(f.name.as_str()))
        .map(|mut f| {
            f.weights.sort_unstable();
            f
        })
        .collect())
}

/// The texts of the master: each layer's place, its font as written, and
/// whether it is the headline.
fn texts(master: &Session) -> Vec<(Place, Font, bool)> {
    let scene = master.scene();
    let raw = master.document().raw();
    let headline: Option<Place> =
        scene
            .roles
            .iter()
            .find_map(|role| match (role.id.as_ref(), &role.target) {
                ("headline", RoleTarget::Layer(address)) => {
                    let ids: Vec<String> = address.layers.iter().map(ToString::to_string).collect();
                    places::resolve(raw, None, &ids)
                }
                _ => None,
            });
    let mut found = Vec::new();
    let mut copy = raw.clone();
    places::each_mut(&mut copy, |visit| {
        if let Visit::Layer { place, layer } = visit
            && layer.contains_key("text")
            && let Some(font) = layer.get("font").and_then(Json::as_str)
            && let Ok(font) = font.parse::<Font>()
        {
            found.push((place.clone(), font, false));
        }
    });
    let lead = headline.or_else(|| {
        found
            .iter()
            .max_by(|a, b| a.1.size.total_cmp(&b.1.size).then(b.0.cmp(&a.0)))
            .map(|t| t.0.clone())
    });
    for t in &mut found {
        t.2 = Some(&t.0) == lead.as_ref();
    }
    found
}

/// The family a font as written belongs to in `set`: its own name when
/// it names a family, else the family of the face its PostScript name is.
fn family_of(font: &Font, set: &FontSet) -> String {
    set.named(&font.family)
        .map_or_else(|| font.family.clone(), |face| face.family.clone())
}

/// Draws a type step: new families for the headline and the rest, and the
/// copy the spec asks for.
pub(crate) fn draw(
    master: &Session,
    spec: &TypeSpec,
    families: &[Family],
    locked: bool,
    rng: &mut Rng,
) -> Result<TypeStep> {
    let set = master.fonts()?;
    let texts = texts(master);
    let used: BTreeSet<String> = texts.iter().map(|t| family_of(&t.1, set)).collect();
    let lead = texts.iter().find(|t| t.2);
    let lead_family = lead.map(|t| family_of(&t.1, set));
    let lead_weight = lead.map_or(400, |t| t.1.weight);
    let class_of = |name: &str| {
        families
            .iter()
            .find(|f| f.name == name)
            .map_or_else(|| Class::Sans.to_string(), |f| f.class.clone())
    };
    let pool: Vec<&Family> = families
        .iter()
        .filter(|f| !locked || used.contains(&f.name))
        .collect();
    let mut headline = None;
    let mut body = None;
    if let Some(lead_family) = &lead_family {
        let own = class_of(lead_family);
        let contrast = match spec.pairs.as_deref() {
            Some("contrast") => true,
            Some("same") => false,
            _ => rng.unit() < 0.5,
        };
        let choices: Vec<&&Family> = pool
            .iter()
            .filter(|f| &f.name != lead_family && !matches!(f.class.as_str(), "mono" | "script"))
            .filter(|f| (f.class != own) == contrast)
            .collect();
        let choices: Vec<&&Family> = match choices.is_empty() {
            // No family of that kind: any other.
            true => pool.iter().filter(|f| &f.name != lead_family).collect(),
            false => choices,
        };
        if let Some(family) = rng.pick(&choices) {
            headline = Some((family.name.clone(), nearest(&family.weights, lead_weight)));
            // The rest pairs with the new headline: a sans under a serif or a
            // display face, a serif under a sans, from the master's own
            // families first.
            let wants = match family.class.as_str() {
                "sans" => "serif",
                _ => "sans",
            };
            let partners: Vec<&&Family> = pool
                .iter()
                .filter(|f| f.class == wants && f.name != family.name)
                .collect();
            let own_partner = partners.iter().find(|f| used.contains(&f.name));
            body = own_partner
                .or_else(|| rng.pick(&partners))
                .map(|f| f.name.clone());
        } else if locked && used.len() == 1 {
            // One family, locked: another weight of it for the headline.
            if let Some(family) = families.iter().find(|f| &f.name == lead_family) {
                let others: Vec<u16> = family
                    .weights
                    .iter()
                    .copied()
                    .filter(|w| *w != lead_weight)
                    .collect();
                if let Some(w) = rng.pick(&others) {
                    headline = Some((family.name.clone(), *w));
                }
            }
        }
    }
    let mut fonts: Vec<PathBuf> = families
        .iter()
        .filter(|f| {
            Some(&f.name) == headline.as_ref().map(|h| &h.0) || Some(&f.name) == body.as_ref()
        })
        .filter_map(|f| f.source.clone())
        .collect();
    fonts.sort();
    fonts.dedup();
    let fonts = fonts.into_iter().map(|f| relative_to(master, &f)).collect();
    Ok(TypeStep {
        headline,
        body,
        copy: spec.copy.clone(),
        fonts,
        families: families.to_vec(),
    })
}

/// `path` as a scene in the master's folder names it.
fn relative_to(master: &Session, path: &Path) -> PathBuf {
    let folder = master
        .path()
        .and_then(Path::parent)
        .unwrap_or(Path::new(""));
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let folder = std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf());
    pathdiff::diff_paths(&absolute, &folder).unwrap_or(absolute)
}

/// The weight of `weights` nearest `want`, the heavier on a tie.
pub(crate) fn nearest(weights: &[u16], want: u16) -> u16 {
    weights
        .iter()
        .copied()
        .min_by_key(|w| (w.abs_diff(want), u16::MAX - w))
        .unwrap_or(want)
}

/// `text` about twice as long, from its own words.
pub(crate) fn long_copy(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return text.to_owned();
    }
    let target = text.chars().count() * 2;
    let mut out = String::new();
    let mut i = 0;
    while out.chars().count() < target {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(words[i % words.len()]);
        i += 1;
    }
    out
}

/// What the master's text covers, by place: its box in its own
/// coordinates, the widest it is at any instance and frame sampled.
fn boxes(session: &Session) -> Result<BTreeMap<Place, (f64, f64)>> {
    let graph = session.graph()?;
    let raw = session.document().raw();
    let frames = session.frames()?;
    let n = frames.len().max(1) as u32;
    let mut out: BTreeMap<Place, (f64, f64)> = BTreeMap::new();
    for k in 0..=8 {
        let frame = frames.start + (n - 1) * k / 8;
        let state = session.frame(f64::from(frame))?;
        for visit in walk(graph, &state) {
            if !matches!(visit.node.content, ContentState::Text { .. }) {
                continue;
            }
            let ids: Vec<String> = visit.ids.iter().map(|s| (*s).to_owned()).collect();
            let Some(place) = places::resolve(raw, None, &ids) else {
                continue;
            };
            let b = visit.node.own_box;
            let e = out.entry(place).or_insert((0.0, 0.0));
            e.0 = e.0.max(b.width());
            e.1 = e.1.max(b.height());
        }
    }
    Ok(out)
}

/// The scene `session` makes with `step` applied, fitted, and a line on
/// what it did.
pub(crate) fn apply(session: &Session, held: &Held, step: &TypeStep) -> Result<(Json, String)> {
    let set = session.fonts()?;
    let texts = texts(session);
    let before = boxes(session)?;
    let mut raw = session.document().raw().clone();
    if !step.fonts.is_empty() {
        let fonts = raw
            .as_object_mut()
            .map(|o| o.entry("fonts").or_insert_with(|| json!([])));
        if let Some(Json::Array(list)) = fonts {
            for f in &step.fonts {
                let f = json!(f.to_string_lossy());
                if !list.contains(&f) {
                    list.push(f);
                }
            }
        }
    }
    let weights = |name: &str| -> Vec<u16> {
        step.families
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.weights.clone())
            .unwrap_or_default()
    };
    // The new font of each text, before fitting.
    let mut fonts: BTreeMap<Place, (Font, bool)> = BTreeMap::new();
    // Texts with a `fit` (D77) are fitted by the renderer, not here.
    let mut sized_by_fit: BTreeSet<Place> = BTreeSet::new();
    for (place, font, lead) in &texts {
        if held.holds(place) {
            continue;
        }
        if fitted_by_renderer(session, place) {
            sized_by_fit.insert(place.clone());
        }
        let mut new = font.clone();
        match (lead, &step.headline, &step.body) {
            (true, Some((family, weight)), _) => {
                new.family.clone_from(family);
                new.weight = *weight;
            }
            (false, _, Some(family)) => {
                let w = nearest(&weights(family), font.weight);
                new.family.clone_from(family);
                new.weight = w;
            }
            // Its own font, as written.
            _ => {}
        }
        // A PostScript name says its weight; a family is asked for by one.
        let by_name = new.family == font.family && set.named(&font.family).is_some();
        fonts.insert(place.clone(), (new, !by_name));
    }
    let copy = step.copy.as_deref();
    let set_text = |layer: &mut Map<String, Json>, raw_slots: &mut Vec<(String, String)>| {
        let Some(copy) = copy else { return };
        match layer.get("text") {
            Some(Json::String(t)) if t.starts_with('$') && !t.starts_with("$$") => {
                raw_slots.push((t[1..].to_owned(), copy.to_owned()));
            }
            Some(Json::String(t)) => {
                // Long copy keeps the master's lines, as many and as even.
                let new = match copy {
                    "long" => balanced(&long_copy(&t.replace('\n', " ")), t.split('\n').count()),
                    other => other.to_owned(),
                };
                layer.insert("text".into(), json!(new));
            }
            _ => {}
        }
    };
    let mut slot_texts: Vec<(String, String)> = Vec::new();
    places::each_mut(&mut raw, |visit| {
        if let Visit::Layer { place, layer } = visit
            && let Some((font, weighted)) = fonts.get(place)
        {
            layer.insert("font".into(), json!(written(font, 1.0, *weighted)));
            set_text(layer, &mut slot_texts);
        }
    });
    // Text slots take the copy once, for every layer that follows them.
    if !slot_texts.is_empty()
        && let Some(slots) = raw.get_mut("slots").and_then(Json::as_object_mut)
    {
        for (id, value) in &slot_texts {
            if held.holds_slot(None, id) {
                continue;
            }
            if let Some(slot) = slots.get_mut(id) {
                let old = slot
                    .get("text")
                    .and_then(Json::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let new = match value.as_str() {
                    "long" => {
                        balanced(&long_copy(&old.replace('\n', " ")), old.split('\n').count())
                    }
                    other => other.to_owned(),
                };
                *slot = match slot {
                    Json::Object(o) => {
                        let mut o = o.clone();
                        o.insert("text".into(), json!(new));
                        Json::Object(o)
                    }
                    _ => json!({"text": new}),
                };
            }
        }
    }
    // Fit: measure, then set smaller or break into lines.
    let mut scales: BTreeMap<Place, f64> = fonts
        .keys()
        .filter(|p| !sized_by_fit.contains(*p))
        .map(|p| (p.clone(), 1.0))
        .collect();
    // The lines each text has in the master, which its box was measured at.
    // And the leading each sets in px, which follows its size.
    let mut first: BTreeMap<Place, usize> = BTreeMap::new();
    let mut leads: BTreeMap<Place, f64> = BTreeMap::new();
    let mut peek = session.document().raw().clone();
    places::each_mut(&mut peek, |visit| {
        if let Visit::Layer { place, layer } = visit
            && fonts.contains_key(place)
            && let Some(Json::String(t)) = layer.get("text")
            && !t.starts_with('$')
        {
            first.insert(place.clone(), t.split('\n').count());
            if let Some(lead) = layer
                .get("style")
                .and_then(|s| s.get("leading"))
                .and_then(Json::as_f64)
            {
                leads.insert(place.clone(), lead);
            }
        }
    });
    let mut lines: BTreeMap<Place, usize> = first.clone();
    let mut missing = Vec::new();
    // Each round sets a text smaller or breaks it once more, so a text
    // can take every line it may and be set to fit them.
    for _ in 0..MOST_LINES + 3 {
        let trial = session.with_raw(raw.clone())?;
        let now = match boxes(&trial) {
            Ok(b) => b,
            Err(e) => {
                missing.push(e.message.clone());
                return Err(e);
            }
        };
        let mut changed = false;
        for (place, scale) in &mut scales {
            let (Some(&(w0, h0)), Some(&(w1, h1))) = (before.get(place), now.get(place)) else {
                continue;
            };
            if w0 <= 0.0 || h0 <= 0.0 || w1 <= 0.0 {
                continue;
            }
            // The master's box at rest is the room: as wide and as tall.
            let k = lines.get(place).copied().unwrap_or(1);
            let k0 = first.get(place).copied().unwrap_or(1);
            let (wide, tall) = (w1 / w0, h1 / h0);
            let over = wide.max(tall);
            // No wider or taller than the master's box: a text that grows
            // past it may cross the canvas's edge or another layer.
            if over <= 1.0 + 1e-3 {
                continue;
            }
            let wanted = *scale / over;
            // One line more makes it about a line narrower and a line
            // taller; break it when that lets it be set clearly larger, by
            // a fifth: a label twice as long stays one line, set smaller.
            let broken = (wide * k as f64 / (k + 1) as f64).max(tall * (k + 1) as f64 / k as f64);
            if wanted < SMALLEST && k < MOST_LINES.max(2 * k0) && broken < over * 0.8 {
                lines.insert(place.clone(), k + 1);
                changed = true;
                continue;
            }
            let fitted = wanted.max(SMALLEST);
            if (fitted - *scale).abs() > 1e-3 {
                *scale = fitted;
                changed = true;
            }
        }
        if !changed {
            break;
        }
        places::each_mut(&mut raw, |visit| {
            if let Visit::Layer { place, layer } = visit
                && let Some((font, weighted)) = fonts.get(place)
            {
                let scale = scales.get(place).copied().unwrap_or(1.0);
                layer.insert("font".into(), json!(written(font, scale, *weighted)));
                if let Some(k) = lines.get(place)
                    && first.get(place) != Some(k)
                    && !layer.contains_key("box")
                    && let Some(Json::String(t)) = layer.get("text")
                    && !t.starts_with('$')
                {
                    let flat = t.replace('\n', " ");
                    layer.insert("text".into(), json!(balanced(&flat, *k)));
                }
                // A one-line master's leading never showed, and may be
                // anything: broken, it takes the face's own. Else it is
                // set as much smaller as the text.
                if let Some(lead) = leads.get(place)
                    && let Some(Json::Object(style)) = layer.get_mut("style")
                {
                    let k = lines.get(place).copied().unwrap_or(1);
                    match first.get(place).copied().unwrap_or(1) {
                        1 if k > 1 => {
                            style.remove("leading");
                        }
                        _ => {
                            style.insert(
                                "leading".into(),
                                json!((lead * scale * 100.0).round() / 100.0),
                            );
                        }
                    }
                }
            }
        });
    }
    let smallest = scales.values().fold(1.0_f64, |m, s| m.min(*s));
    let broken = lines
        .iter()
        .filter(|(p, k)| first.get(*p) != Some(*k))
        .count();
    let note = format!(
        "{step}: {} texts, {} set smaller (to {:.0} % at least), {broken} broken into lines{}",
        fonts.len(),
        scales.values().filter(|s| **s < 0.999).count(),
        smallest * 100.0,
        match missing.is_empty() {
            true => String::new(),
            false => format!("; {}", missing.join("; ")),
        }
    );
    Ok((raw, note))
}

/// Whether the layer at `place` has a `fit`, which sets its size itself.
fn fitted_by_renderer(session: &Session, place: &Place) -> bool {
    places::layers_of(session.document().raw(), place.comp.as_deref())
        .iter()
        .find(|l| l.get("id").and_then(Json::as_str) == Some(place.layer.as_str()))
        .is_some_and(|l| l.get("fit").is_some())
}

/// `font` written at `scale` of its size, with its weight when `weighted`.
fn written(font: &Font, scale: f64, weighted: bool) -> String {
    let size = (font.size * scale * 100.0).round() / 100.0;
    match weighted {
        true => format!("{} {} {}", font.family, size, font.weight),
        false => format!("{} {}", font.family, size),
    }
}

/// `text` broken at spaces into `k` lines of about one length.
fn balanced(text: &str, k: usize) -> String {
    let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
    if k <= 1 || words.len() < 2 {
        return text.to_owned();
    }
    let total: usize = words.iter().map(|w| w.chars().count() + 1).sum();
    let per = total as f64 / k as f64;
    let mut out = String::new();
    let mut run = 0usize;
    let mut breaks = 1;
    for (i, w) in words.iter().enumerate() {
        let len = w.chars().count() + 1;
        if i > 0 {
            let here = run as f64 >= per * breaks as f64 - len as f64 / 2.0;
            if here && breaks < k {
                out.push('\n');
                breaks += 1;
            } else {
                out.push(' ');
            }
        }
        out.push_str(w);
        run += len;
    }
    out
}

#[cfg(test)]
#[path = "../../tests/unit/vary/typeface.rs"]
mod tests;
