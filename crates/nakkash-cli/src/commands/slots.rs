//! `nakkash slots`: the slots of a scene and of its compositions, which an
//! agent changes to make a variant ([D44]); what the scene draws that no
//! slot holds (`--coverage`); and whether every slot changes the frame
//! (`--verify`, [D77]).
//!
//! [D44]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d44-slots-are-named-values-that-layers-follow
//! [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use nakkash_engine::oklch::Oklch;
use nakkash_engine::{
    ContentState, Diagnostic, Font, FrameState, Image, Kind, Listed, Quality, RenderOptions,
    Result, SceneGraph, Session, Severity, SlotType, color, colors, is_reference, json_at,
    key_text, resolved,
};
use serde::Serialize;
use serde_json::{Value as Json, json};

/// What `nakkash slots` does besides listing.
#[derive(Debug, Clone, Default)]
pub struct Args {
    /// Print as JSON.
    pub json: bool,
    /// Count what no slot holds.
    pub coverage: bool,
    /// Change each slot and render where it shows.
    pub verify: bool,
    /// The picture an image slot is given when verified; a test card when
    /// none.
    pub picture: Option<PathBuf>,
    /// The clip a video slot is given when verified.
    pub clip: Option<PathBuf>,
    /// With `verify`, mark each slot that changes nothing as unseen in the
    /// scene file, and clear the mark of each that changes the render.
    pub mark: bool,
}

/// Lists the slots, their coverage or their check, as `args` says.
pub fn run(path: &Path, args: &Args) -> Result<ExitCode> {
    let session = Session::open(path)?;
    if args.verify {
        let verified = verify(&session, args)?;
        print(&verified, args.json, |v| v.to_string())?;
        if args.mark {
            let marked = mark(path, &session, &verified)?;
            if !args.json {
                println!("{marked} slots marked unseen in {}", path.display());
            }
        }
        let dead = verified
            .slots
            .iter()
            .any(|s| s.outcome == Outcome::Unchanged)
            || verified
                .slots
                .iter()
                .any(|s| matches!(s.outcome, Outcome::Broken(_)))
            || verified.together.is_some();
        return Ok(if dead {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        });
    }
    if args.coverage {
        let coverage = coverage(session.document().raw());
        print(&coverage, args.json, |c| c.to_string())?;
        return Ok(ExitCode::SUCCESS);
    }
    let slots = session.slots();
    if args.json {
        let raw = session.document().raw();
        let data: Vec<_> = slots.iter().map(|listed| data(raw, listed)).collect();
        print(&data, true, |_| String::new())?;
    } else {
        print!("{}", listing(&slots));
    }
    Ok(ExitCode::SUCCESS)
}

/// Prints `value` as JSON, or as `text` gives it.
fn print<T: Serialize>(value: &T, json: bool, text: impl Fn(&T) -> String) -> Result<()> {
    if json {
        let written = serde_json::to_string_pretty(value).map_err(|e| {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot write the slots as JSON: {e}"),
            )
        })?;
        println!("{written}");
    } else {
        print!("{}", text(value));
    }
    Ok(())
}

/// One slot as `--json` lists it: its declaration, the place of each value
/// that follows it, and what a variant needs to stay inside the design.
fn data(raw: &Json, listed: &Listed) -> Json {
    let mut data = json!({
        "comp": listed.comp,
        "id": listed.slot.id.to_string(),
        "type": listed.slot.ty.name(),
        "value": listed.slot.value,
        "label": listed.slot.label,
        "role": listed.slot.role.as_deref(),
        "seen": listed.slot.seen,
        "followers": listed.followers,
    });
    let first = listed
        .followers
        .first()
        .and_then(|f| follower_layer(raw, f));
    match (listed.slot.ty, first) {
        (SlotType::Text, Some(layer)) => {
            let font = layer["font"].as_str().and_then(|f| f.parse::<Font>().ok());
            let words = listed.slot.value.as_str().unwrap_or_default();
            data["text"] = json!({
                "font": font.as_ref().map(|f| f.family.clone()),
                "size": font.as_ref().map(|f| f.size),
                "fit": layer.get("fit"),
                "characters": words.chars().count(),
                "lines": words.lines().count().max(1),
            });
        }
        (SlotType::Image | SlotType::Video, Some(layer)) => {
            data["media"] = json!({"box": layer.get("box"), "fit": layer.get("fit")});
        }
        _ => {}
    }
    let edit = listed.followers.iter().any(|f| in_edit_comp(f));
    if edit {
        data["edit"] = json!(true);
    }
    data
}

/// The layer that holds the value at `place`, a follower's place as the
/// format writes it: `title.text` or `comps.card.layers[title].text`.
fn follower_layer<'j>(raw: &'j Json, place: &str) -> Option<&'j Json> {
    let (comp, id) = layer_of(place)?;
    let layers = match comp {
        Some(comp) => &raw["comps"][comp]["layers"],
        None => &raw["layers"],
    };
    layers
        .as_array()?
        .iter()
        .find(|l| l["id"].as_str() == Some(id))
}

/// The composition and layer id a follower's place names; none for a value
/// of the scene itself, such as its `dur`.
fn layer_of(place: &str) -> Option<(Option<&str>, &str)> {
    if let Some(rest) = place.strip_prefix("comps.") {
        let (comp, rest) = rest.split_once('.')?;
        let rest = rest.strip_prefix("layers[")?;
        let (id, _) = rest.split_once(']')?;
        return Some((Some(comp), id));
    }
    let id = place.split(['.', '/', '[']).next()?;
    (!["dur", "bg"].contains(&id)).then_some((None, id))
}

/// Whether a follower's place is in a composition whose id says it is for
/// editing.
fn in_edit_comp(place: &str) -> bool {
    const EDIT: [&str; 8] = [
        "edit",
        "change",
        "replace",
        "your",
        "place",
        "placeholder",
        "insert",
        "logo",
    ];
    layer_of(place)
        .and_then(|(comp, _)| comp)
        .is_some_and(|comp| {
            comp.split('-')
                .any(|w| EDIT.contains(&w.trim_matches(|c: char| c.is_ascii_digit())))
        })
}

/// The slots as lines under the scene and each composition that has any.
fn listing(slots: &[Listed]) -> String {
    let mut text = String::new();
    let mut owner = None;
    for listed in slots {
        if owner != Some(&listed.comp) {
            owner = Some(&listed.comp);
            match &listed.comp {
                None => text.push_str("scene\n"),
                Some(comp) => text.push_str(&format!("comps.{comp}\n")),
            }
        }
        let slot = &listed.slot;
        let label = slot
            .label
            .as_deref()
            .map(|l| format!("  {l:?}"))
            .unwrap_or_default();
        let role = slot
            .role
            .as_deref()
            .map(|r| format!("  {r}"))
            .unwrap_or_default();
        let follow = match listed.followers.len() {
            1 => "1 value follows".to_owned(),
            n => format!("{n} values follow"),
        };
        let unseen = match slot.seen {
            true => "",
            false => "  unseen",
        };
        text.push_str(&format!(
            "  ${} {} {}{label}{role}  ({follow}){unseen}\n",
            slot.id, slot.ty, slot.value
        ));
    }
    if slots.is_empty() {
        text.push_str("no slots\n");
    }
    text
}

/// How much of what a scene draws its slots hold.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Coverage {
    /// Text layers with words: those following a slot, and all.
    pub text: Count,
    /// Distinct colours that can show, in eight bits a channel: those
    /// whose every use follows a slot, and all.
    pub colors: Count,
    /// Image and video layers.
    pub media: Count,
    /// Slots marked unseen (`"seen": false`), of all the slots.
    pub unseen: Count,
    /// Where each value no slot holds is.
    pub unbound: Vec<String>,
}

/// A part of a whole.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Count {
    /// How many are held.
    pub bound: usize,
    /// How many there are.
    pub total: usize,
}

impl std::fmt::Display for Count {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.total {
            0 => write!(f, "none"),
            total => write!(
                f,
                "{} of {} ({:.1} %)",
                self.bound,
                total,
                100.0 * self.bound as f64 / total as f64
            ),
        }
    }
}

impl std::fmt::Display for Coverage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "held by slots:")?;
        writeln!(f, "  text layers {}", self.text)?;
        writeln!(f, "  colours     {}", self.colors)?;
        writeln!(f, "  media       {}", self.media)?;
        if self.unseen.bound > 0 {
            writeln!(f, "unseen slots  {}", self.unseen)?;
        }
        if !self.unbound.is_empty() {
            writeln!(f, "not held:")?;
            for place in &self.unbound {
                writeln!(f, "  {place}")?;
            }
        }
        Ok(())
    }
}

/// What `raw`, a scene's JSON, draws that its slots hold.
pub fn coverage(raw: &Json) -> Coverage {
    let mut coverage = Coverage::default();
    for declared in declarations(raw) {
        for value in declared.values() {
            coverage.unseen.total += 1;
            if value.get(SEEN) == Some(&Json::Bool(false)) {
                coverage.unseen.bound += 1;
            }
        }
    }
    let reference = |value: &Json| {
        value
            .as_str()
            .is_some_and(|t| is_reference(t) && !t.starts_with("$$"))
    };
    let mut owners = vec![(String::new(), &raw["layers"])];
    if let Some(comps) = raw["comps"].as_object() {
        owners.extend(
            comps
                .iter()
                .map(|(id, c)| (format!("comps.{id}."), &c["layers"])),
        );
    }
    for (prefix, layers) in owners {
        // A layer left out draws neither words nor a file.
        let drawn = layers
            .as_array()
            .into_iter()
            .flatten()
            .filter(|l| l.get("hidden") != Some(&Json::Bool(true)));
        for layer in drawn {
            let id = layer["id"].as_str().unwrap_or_default();
            if let Some(words) = layer.get("text").filter(|t| t.as_str() != Some("")) {
                coverage.text.total += 1;
                match reference(words) {
                    true => coverage.text.bound += 1,
                    false => coverage.unbound.push(format!("{prefix}{id}.text {words}")),
                }
            }
            for key in ["image", "video"] {
                if let Some(file) = layer.get(key) {
                    coverage.media.total += 1;
                    match reference(file) {
                        true => coverage.media.bound += 1,
                        false => coverage.unbound.push(format!("{prefix}{id}.{key} {file}")),
                    }
                }
            }
        }
    }
    // A colour is held when every use of it that can show follows a slot.
    let resolved = resolved(raw).unwrap_or_else(|_| raw.clone());
    let mut all = BTreeSet::new();
    let mut loose = BTreeSet::new();
    // Whether a colour shows is read from the values the scene draws with.
    for place in colors::places(&resolved).into_iter().filter(|p| p.shown) {
        let written = json_at(raw, &place.steps);
        let value = json_at(&resolved, &place.steps).and_then(color);
        let Some(value) = value else {
            continue;
        };
        let color = value.map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8);
        all.insert(color);
        if !written.is_some_and(reference) && loose.insert(color) {
            let [r, g, b, a] = color;
            coverage.unbound.push(format!(
                "{} #{r:02X}{g:02X}{b:02X}{a:02X}",
                key_text(&place.steps)
            ));
        }
    }
    coverage.colors = Count {
        bound: all.len() - loose.len(),
        total: all.len(),
    };
    coverage
}

/// What became of one slot when it was changed.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    /// The frame changed, at `frame`, by `pixels`.
    Changed {
        /// The frame drawn.
        frame: u32,
        /// How many pixels moved.
        pixels: usize,
    },
    /// Its length changed the scene's.
    Retimed {
        /// The frames before and after.
        frames: [u32; 2],
    },
    /// No frame drawn changed.
    Unchanged,
    /// The change did not apply, or the scene would not draw: why.
    Broken(String),
    /// Not tried, and why.
    Skipped(String),
}

/// One slot's check.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Verified {
    /// The composition whose own slot it is; `None` for the scene's.
    pub comp: Option<String>,
    /// The slot.
    pub id: String,
    /// Its type.
    pub ty: &'static str,
    /// The edit made.
    pub edit: String,
    /// What it did.
    pub outcome: Outcome,
}

/// Every slot's check, and the scene's with them all changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Verification {
    /// Each slot's.
    pub slots: Vec<Verified>,
    /// Why the scene with every slot changed fails, if it does: it does
    /// not draw, or its check finds more errors than the original's.
    pub together: Option<String>,
}

impl std::fmt::Display for Verification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for slot in &self.slots {
            let what = match &slot.outcome {
                Outcome::Changed { frame, pixels } => {
                    format!("changes {pixels} pixels at frame {frame}")
                }
                Outcome::Retimed { frames: [a, b] } => format!("retimes {a} frames to {b}"),
                Outcome::Unchanged => "CHANGES NOTHING".into(),
                Outcome::Broken(why) => format!("BROKEN: {why}"),
                Outcome::Skipped(why) => format!("not tried: {why}"),
            };
            writeln!(f, "  ${} {}: {what}", slot.id, slot.ty)?;
        }
        let live = self
            .slots
            .iter()
            .filter(|s| matches!(s.outcome, Outcome::Changed { .. } | Outcome::Retimed { .. }))
            .count();
        writeln!(f, "{live} of {} slots change the scene", self.slots.len())?;
        match &self.together {
            None => writeln!(
                f,
                "every slot changed at once: the scene draws and checks as before"
            ),
            Some(why) => writeln!(f, "every slot changed at once: {why}"),
        }
    }
}

/// The key that marks a slot unseen.
const SEEN: &str = nakkash_engine::SEEN;

/// The slot declarations of `raw`: the scene's, then each composition's.
fn declarations(raw: &Json) -> Vec<&serde_json::Map<String, Json>> {
    let comps = raw["comps"]
        .as_object()
        .into_iter()
        .flat_map(|c| c.values());
    std::iter::once(raw)
        .chain(comps)
        .filter_map(|owner| owner["slots"].as_object())
        .collect()
}

/// Marks each slot of the scene at `path` that `verified` found changes
/// nothing `"seen": false`, writing a shorthand declaration out typed, and
/// clears the mark of each it saw change the render; slots it did not try
/// keep theirs. Says how many are marked.
///
/// # Errors
/// An I/O diagnostic when the scene cannot be written, or the format's
/// when the marked scene does not validate, a bug.
fn mark(path: &Path, session: &Session, verified: &Verification) -> Result<usize> {
    let mut raw = session.document().raw().clone();
    let mut marked = 0;
    for slot in &verified.slots {
        let seen = match slot.outcome {
            Outcome::Unchanged => false,
            Outcome::Changed { .. } | Outcome::Retimed { .. } => true,
            Outcome::Broken(_) | Outcome::Skipped(_) => continue,
        };
        let owner = match &slot.comp {
            None => &mut raw,
            Some(comp) => &mut raw["comps"][comp],
        };
        let Some(declared) = owner["slots"].get_mut(&slot.id) else {
            continue;
        };
        if !declared.is_object() {
            *declared = json!({ slot.ty: declared.clone() });
        }
        let Some(fields) = declared.as_object_mut() else {
            continue;
        };
        match seen {
            true => {
                fields.remove(SEEN);
            }
            false => {
                fields.insert(SEEN.into(), Json::Bool(false));
                marked += 1;
            }
        }
    }
    let written = nakkash_engine::write_document(&raw);
    written.parse::<Session>()?;
    fs::write(path, written).map_err(|e| Diagnostic::io("write", path, &e))?;
    Ok(marked)
}

/// How many frames a slot is drawn at before it is found to change none.
const CANDIDATES: usize = 8;

/// How many frames, evenly spread, are looked at for what each shows.
const SAMPLES: u32 = 96;

/// Changes each slot of the scene alone, the scene's and each
/// composition's, and draws the frames where what follows it shows, until
/// one differs; then changes them all and draws and checks the scene once.
fn verify(session: &Session, args: &Args) -> Result<Verification> {
    let frames = session.frames()?;
    let count = frames.end.max(1);
    let samples = spread(count, SAMPLES);
    let graph = session.graph()?;
    let shown: Vec<BTreeSet<String>> = samples
        .iter()
        .map(|&f| {
            session.frame(f64::from(f)).map(|state| {
                let mut ids = BTreeSet::new();
                shown_ids(&state, graph, None, &mut ids);
                ids
            })
        })
        .collect::<Result<_>>()?;
    let mut drawn: BTreeMap<u32, Image> = BTreeMap::new();
    let picture = match &args.picture {
        Some(path) => path.clone(),
        None => test_card()?,
    };
    let mut verified = Verification::default();
    let mut edits = Vec::new();
    for listed in session.slots() {
        let id = listed.slot.id.to_string();
        let ty = listed.slot.ty.name();
        let Some(value) = changed(&listed, &picture, args.clip.as_deref()) else {
            verified.slots.push(Verified {
                comp: listed.comp.clone(),
                id,
                ty,
                edit: String::new(),
                outcome: Outcome::Skipped("no clip to give it (`--clip`)".into()),
            });
            continue;
        };
        // A composition's own slot is set where the scene declares it.
        let edit = match &listed.comp {
            None => format!("${id}={value}"),
            Some(comp) => format!("scene comps.{comp}.slots.{id}={value}"),
        };
        let mut variant = session.variant();
        let outcome = match variant.edit_variant(&edit) {
            Err(error) => Outcome::Broken(error.to_string()),
            Ok(_) if listed.followers.iter().any(|f| f == "dur") => match variant.frames() {
                Ok(after) if after != frames => Outcome::Retimed {
                    frames: [frames.end, after.end],
                },
                Ok(_) => Outcome::Unchanged,
                Err(error) => Outcome::Broken(error.to_string()),
            },
            Ok(_) => {
                let ids: BTreeSet<String> = listed
                    .followers
                    .iter()
                    .filter_map(|f| layer_of(f).map(|(comp, id)| shown_key(comp, id)))
                    .collect();
                let at = where_shown(&samples, &shown, &ids, count);
                first_change(session, &variant, &at, &mut drawn)
            }
        };
        if !matches!(outcome, Outcome::Broken(_)) {
            edits.push(edit.clone());
        }
        verified.slots.push(Verified {
            comp: listed.comp.clone(),
            id,
            ty,
            edit,
            outcome,
        });
    }
    let middle = spread(count, CANDIDATES as u32);
    verified.together = together(session, &edits, &middle);
    Ok(verified)
}

/// `n` frames of `count`, evenly spread, each in the middle of its share.
fn spread(count: u32, n: u32) -> Vec<u32> {
    let count = count.max(1);
    let n = n.min(count).max(1);
    (0..n)
        .map(|i| ((f64::from(i) + 0.5) * f64::from(count) / f64::from(n)) as u32)
        .map(|f| f.min(count - 1))
        .collect()
}

/// How a layer is named among those a frame shows: by its id, after its
/// composition's, `card/title`, inside one.
fn shown_key(comp: Option<&str>, id: &str) -> String {
    match comp {
        Some(comp) => format!("{comp}/{id}"),
        None => id.to_owned(),
    }
}

/// Every layer `state` shows, at any depth, by [`shown_key`]: a
/// composition's layers while the composition shows. `comp` is the
/// composition `state` is of, `None` for the scene.
fn shown_ids(
    state: &FrameState,
    graph: &SceneGraph,
    comp: Option<&str>,
    into: &mut BTreeSet<String>,
) {
    for node in state.nodes.iter().filter(|n| n.shown && !n.hidden) {
        into.insert(shown_key(comp, &node.id));
        if let ContentState::Comp(index, Some(inner)) = &node.content
            && let Some(inner_comp) = graph.comps.get(*index)
        {
            shown_ids(inner, graph, Some(&inner_comp.id), into);
        }
    }
}

/// Up to [`CANDIDATES`] frames to draw a slot at: of the `samples`, those
/// where most of the layers `ids` show, spread over the times they show;
/// evenly spread frames when none shows at a sample.
fn where_shown(
    samples: &[u32],
    shown: &[BTreeSet<String>],
    ids: &BTreeSet<String>,
    count: u32,
) -> Vec<u32> {
    let scored: Vec<(usize, u32)> = samples
        .iter()
        .zip(shown)
        .map(|(&f, s)| (s.intersection(ids).count(), f))
        .filter(|(n, _)| *n > 0)
        .collect();
    if scored.is_empty() {
        return spread(count, CANDIDATES as u32);
    }
    // The frames showing the most first, then the rest of those showing
    // any, each group spread over its times.
    let most = scored.iter().map(|(n, _)| *n).max().unwrap_or(0);
    let mut at: Vec<u32> = Vec::new();
    for best in [true, false] {
        let group: Vec<u32> = scored
            .iter()
            .filter(|(n, _)| (*n == most) == best)
            .map(|(_, f)| *f)
            .collect();
        let step = group.len().div_ceil(CANDIDATES).max(1);
        at.extend(
            group
                .iter()
                .step_by(step)
                .filter(|f| !at.contains(f))
                .copied()
                .collect::<Vec<_>>(),
        );
    }
    at.truncate(CANDIDATES);
    at
}

/// Draws `variant` at `frames`, in order, against `session`
/// drawn there, until a frame differs.
fn first_change(
    session: &Session,
    variant: &Session,
    frames: &[u32],
    drawn: &mut BTreeMap<u32, Image>,
) -> Outcome {
    for &frame in frames {
        if let std::collections::btree_map::Entry::Vacant(entry) = drawn.entry(frame) {
            match small(session, frame) {
                Ok(image) => {
                    entry.insert(image);
                }
                Err(error) => {
                    return Outcome::Broken(format!("the original does not draw: {error}"));
                }
            }
        }
        let after = match small(variant, frame) {
            Ok(image) => image,
            Err(error) => return Outcome::Broken(error.to_string()),
        };
        let before = &drawn[&frame];
        let pixels = differing(before, &after);
        if pixels > 0 {
            return Outcome::Changed { frame, pixels };
        }
    }
    Outcome::Unchanged
}

/// How much smaller than the canvas `--verify` draws frames, on each side.
const VERIFY_SCALE: f64 = 0.5;

/// `frame` of `session` drawn at [`VERIFY_SCALE`], quickly.
fn small(session: &Session, frame: u32) -> Result<Image> {
    let options = RenderOptions {
        quality: Quality::Fast,
        ..RenderOptions::default()
    };
    session.render_small(f64::from(frame), VERIFY_SCALE, options)
}

/// How many pixels of two images of one size differ by more than one step
/// in a channel.
fn differing(a: &Image, b: &Image) -> usize {
    a.pixels()
        .chunks_exact(4)
        .zip(b.pixels().chunks_exact(4))
        .filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 1))
        .count()
}

/// Why the scene with every edit in `edits` made fails: it does not take
/// them, does not draw at the middle candidate, or its check finds more
/// errors than the original's.
fn together(session: &Session, edits: &[String], candidates: &[u32]) -> Option<String> {
    let mut variant = session.variant();
    for edit in edits {
        if let Err(error) = variant.edit_variant(edit) {
            return Some(format!("`{edit}` does not apply: {error}"));
        }
    }
    let middle = candidates.get(candidates.len() / 2).copied().unwrap_or(0);
    let middle = variant
        .frames()
        .ok()
        .map_or(middle, |f| middle.min(f.end.saturating_sub(1)));
    if let Err(error) = variant.render(f64::from(middle)) {
        return Some(format!("it does not draw frame {middle}: {error}"));
    }
    let errors = |s: &Session| {
        s.check().map(|r| {
            r.violations
                .iter()
                .filter(|v| v.severity == Severity::Error)
                .count()
        })
    };
    match (errors(session), errors(&variant)) {
        (Ok(before), Ok(after)) if after > before => Some(format!(
            "its check finds {after} errors, the original {before}"
        )),
        (_, Err(error)) => Some(format!("it does not check: {error}")),
        _ => None,
    }
}

/// The value an edit gives `listed` to change it, written as an edit
/// writes it: longer words, the colour's hue turned half round (a neutral
/// made light or dark), a number moved (a share halved), a test picture, the
/// clip given, a choice's next word.
fn changed(listed: &Listed, picture: &Path, clip: Option<&Path>) -> Option<String> {
    let value = &listed.slot.value;
    let quoted = |text: &str| {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\t', "\\t");
        format!("\"{escaped}\"")
    };
    Some(match listed.slot.ty {
        SlotType::Text => {
            let words = value.as_str().unwrap_or_default();
            match words.trim().is_empty() {
                true => quoted("Text"),
                false => quoted(&format!("{words} {words}")),
            }
        }
        SlotType::Color => {
            let [r, g, b, a] = color(value).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let mut lch = Oklch::from_srgb([r, g, b]);
            if lch.c < 0.05 {
                // A neutral: as far in lightness as it can go, and coloured.
                lch.l = if lch.l > 0.5 { 0.35 } else { 0.75 };
                lch.c = 0.15;
                lch.h = 145.0;
            } else {
                lch.h = (lch.h + 180.0).rem_euclid(360.0);
            }
            let [r, g, b] = lch.to_srgb().map(|c| (c * 255.0).round() as u8);
            let a = (a * 255.0).round() as u8;
            format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
        }
        SlotType::Number => {
            let n = value.as_f64().unwrap_or(0.0);
            let to = match listed.slot.role.as_deref() {
                Some("duration") => (n * 0.6 * 100.0).round() / 100.0,
                _ if n == 0.0 => 1.0,
                // A share, such as an opacity, stays a share.
                _ if n > 0.0 && n <= 1.0 => n / 2.0,
                _ => n * 1.5,
            };
            to.to_string()
        }
        SlotType::Point => {
            let xy: Vec<f64> = value
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Json::as_f64)
                .collect();
            let [x, y] = [
                xy.first().copied().unwrap_or(0.0),
                xy.get(1).copied().unwrap_or(0.0),
            ];
            format!("[{},{}]", x * 1.2 + 10.0, y * 1.2 + 10.0)
        }
        SlotType::Image => quoted(&picture.display().to_string()),
        SlotType::Video => quoted(&clip?.display().to_string()),
        // The next of its words, the first after the last.
        SlotType::Choice => {
            let values = &listed.slot.contract.values;
            let at = values.iter().position(|v| Some(v.as_str()) == value.as_str());
            let next = values.get(at.map_or(0, |i| (i + 1) % values.len()))?;
            quoted(next)
        }
    })
}

/// A test card to give image slots: 400 by 200 pixels, quarters of four
/// strong colours under a white diagonal, written in the system's
/// temporary folder.
fn test_card() -> Result<PathBuf> {
    let (w, h) = (400u32, 200u32);
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let colour = match (x < w / 2, y < h / 2) {
                _ if x.abs_diff(y * 2) < 6 => [255, 255, 255, 255],
                (true, true) => [230, 20, 120, 255],
                (false, true) => [20, 200, 230, 255],
                (true, false) => [250, 210, 10, 255],
                (false, false) => [40, 40, 210, 255],
            };
            pixels.extend(colour);
        }
    }
    let png = Image::new(w, h, pixels)?.to_png()?;
    let path = std::env::temp_dir().join(format!("nakkash-test-card-{}.png", std::process::id()));
    fs::write(&path, png).map_err(|e| Diagnostic::io("write", &path, &e))?;
    Ok(path)
}

#[cfg(test)]
#[path = "../../tests/unit/commands/slots.rs"]
mod tests;
