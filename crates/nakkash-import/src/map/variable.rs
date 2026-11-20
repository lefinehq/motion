//! What a designer would vary ([D77]): once the scene is written, every text
//! layer's words, every colour it draws with, every footage file and its
//! length become slots of the scene, each with a role, and the values
//! follow them.
//!
//! - **Text.** One `text` slot per text layer; a copy of a text already met
//!   in the same composition, in the same font, follows the same slot. The
//!   layer gets a `fit` from the size its lines cover.
//! - **Colour.** Every colour place the format types as a colour is a use;
//!   uses within [`TOLERANCE`] of each other in OKLab are one palette
//!   entry, held by the most used, and a use that is not its entry exactly
//!   keeps its difference in its reference.
//! - **Media.** One `image` or `video` slot per footage file, which every
//!   layer drawing it follows, `null` for a file the scene cannot draw.
//! - **Timing.** The scene's `dur` follows a `duration` slot.
//!
//! A composition whose name says it is for editing is the strongest
//! evidence of a placeholder.
//!
//! [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary

use std::collections::HashMap;

use nakkash_format::colors;
use nakkash_format::oklch::{self, Oklch, Shift};
use nakkash_format::shorthand::Font;
use nakkash_format::{Address, Id, Step};
use serde::Serialize;
use serde_json::{Map, Value as Json, json};

use super::StoredText;
use super::ids::Ids;

/// The key that marks a layer drawing footage, by the number of its
/// [`MediaFact`], until the slots are made.
pub(super) const MEDIA: &str = "\u{0}media";

/// The least contrast the import asks of words against the background: the
/// ratio for body text in the web's accessibility guidelines.
const TEXT_CONTRAST: f64 = 4.5;

/// `color` as `#RRGGBBAA`.
fn hex8(color: [f64; 4]) -> String {
    let [r, g, b, a] = color.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
}

/// How far apart in OKLab two colours may be and still be one palette
/// entry: about two differences a viewer just notices.
pub const TOLERANCE: f64 = 0.04;

/// The words in a composition's name that say it is for editing.
const EDIT_WORDS: [&str; 9] = [
    "edit",
    "change",
    "replace",
    "your",
    "place",
    "placeholder",
    "insert",
    "customize",
    "customise",
];

/// The footage a media layer draws, or would draw.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct MediaFact {
    /// The footage item's id.
    pub item: u32,
    /// The footage item's name.
    pub name: String,
    /// The path the project stores, when it stores one.
    pub stored: Option<String>,
    /// The layer key: `image` or `video`.
    pub key: &'static str,
    /// Whether the scene draws the file.
    pub found: bool,
    /// Why it does not, when it does not.
    pub why: Option<String>,
}

/// What the walk knows that the written scene does not say.
pub(super) struct Known<'a> {
    /// The footage of each mark.
    pub media: &'a [MediaFact],
    /// The project's name of each layer, by scope and id.
    pub layer_names: &'a HashMap<(Option<String>, String), String>,
    /// The project's name of each scope.
    pub comp_names: &'a HashMap<Option<String>, String>,
    /// The lines the project laid each text out in.
    pub texts: &'a [StoredText],
    /// The project's item id of each scope's composition, none for the
    /// scene's own.
    pub comp_items: &'a HashMap<Option<String>, u32>,
    /// The project's id of each layer written, by scope and id.
    pub layer_items: &'a HashMap<(Option<String>, String), u32>,
}

/// What the import made a designer can vary ([D77]).
///
/// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Variables {
    /// The text slots.
    pub text: usize,
    /// The palette's entries, a colour control's among them.
    pub colors: usize,
    /// The image and video slots.
    pub media: usize,
    /// The timing slots.
    pub timing: usize,
    /// The slots of placeholder compositions: compositions a designer left
    /// empty for a product screen or a logo ([D91]).
    ///
    /// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
    pub screens: usize,
    /// The compositions whose names say they are for editing.
    pub edit_comps: Vec<String>,
}

/// A layer of the scene, by where it is written.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct LayerAt {
    /// The scene composition, none for the scene's own.
    scope: Option<String>,
    /// Its index in the list.
    index: usize,
}

impl LayerAt {
    /// The steps from the scene to it.
    fn steps(&self) -> Vec<Step> {
        let mut steps = match &self.scope {
            None => Vec::new(),
            Some(comp) => vec![Step::Key("comps".into()), Step::Key(comp.clone())],
        };
        steps.extend([Step::Key("layers".into()), Step::Index(self.index)]);
        steps
    }
}

/// Every layer of `scene`, the scene's own first, then each composition's.
fn layers(scene: &Json) -> Vec<LayerAt> {
    let own = scene["layers"].as_array().map_or(0, Vec::len);
    let mut all: Vec<LayerAt> = (0..own)
        .map(|index| LayerAt { scope: None, index })
        .collect();
    if let Some(comps) = scene["comps"].as_object() {
        for (id, comp) in comps {
            let count = comp["layers"].as_array().map_or(0, Vec::len);
            all.extend((0..count).map(|index| LayerAt {
                scope: Some(id.clone()),
                index,
            }));
        }
    }
    all
}

/// The value at `steps`, indices only.
fn at_mut<'j>(json: &'j mut Json, steps: &[Step]) -> Option<&'j mut Json> {
    steps.iter().try_fold(json, |json, step| match step {
        Step::Key(key) => json.get_mut(key.as_str()),
        Step::Index(index) => json.get_mut(*index),
        Step::Id(_) => None,
    })
}

/// The value at `steps`, indices only.
fn at<'j>(json: &'j Json, steps: &[Step]) -> Option<&'j Json> {
    steps.iter().try_fold(json, |json, step| match step {
        Step::Key(key) => json.get(key.as_str()),
        Step::Index(index) => json.get(*index),
        Step::Id(_) => None,
    })
}

/// Whether a composition's name says it is for editing.
pub(super) fn is_edit_comp(name: &str) -> bool {
    words(name).any(|word| EDIT_WORDS.contains(&word.as_str()))
}

/// The words of `name`, in lower case, digits apart.
fn words(name: &str) -> impl Iterator<Item = String> + '_ {
    name.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.trim_matches(|c: char| c.is_ascii_digit()).to_lowercase())
        .filter(|w| !w.is_empty())
}

/// Makes the slots of `scene` ([D77]) and says what it made.
///
/// [D77]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d77-every-imported-template-names-what-a-designer-would-vary
pub(super) fn make(scene: &mut Json, known: &Known<'_>) -> Variables {
    let mut ids = Ids::default();
    for taken in declared_ids(scene) {
        ids.give(&taken, "slot");
    }
    let all = layers(scene);
    let edit: Vec<Option<String>> = known
        .comp_names
        .iter()
        .filter(|(_, name)| is_edit_comp(name))
        .map(|(scope, _)| scope.clone())
        .collect();
    let mut made = Variables {
        edit_comps: known
            .comp_names
            .iter()
            .filter(|(_, name)| is_edit_comp(name))
            .map(|(_, name)| name.clone())
            .collect(),
        ..Variables::default()
    };
    made.edit_comps.sort();
    made.edit_comps.dedup();
    let mut slots = Map::new();
    made.text = texts(scene, known, &all, &edit, &mut ids, &mut slots);
    made.media = media(scene, known, &all, &edit, &mut ids, &mut slots);
    made.screens = screens(scene, known, &mut ids, &mut slots);
    made.colors = palette(scene, &mut ids, &mut slots);
    made.timing = timing(scene, &mut ids, &mut slots);
    if !slots.is_empty() {
        let declared = scene
            .as_object_mut()
            .expect("a scene is an object")
            .entry("slots")
            .or_insert_with(|| Json::Object(Map::new()));
        if let Some(declared) = declared.as_object_mut() {
            declared.extend(slots);
        }
    }
    made
}

/// Takes the marks of media layers out of `scene`, where no slots are made.
pub(super) fn unmark(scene: &mut Json) {
    for at in layers(scene) {
        if let Some(layer) = at_mut(scene, &at.steps()).and_then(Json::as_object_mut) {
            layer.remove(MEDIA);
        }
    }
}

/// The ids of every slot the scene and its compositions declare.
fn declared_ids(scene: &Json) -> Vec<String> {
    let own = scene["slots"].as_object().into_iter().flat_map(Map::keys);
    let comps = scene["comps"]
        .as_object()
        .into_iter()
        .flat_map(Map::values)
        .flat_map(|comp| comp["slots"].as_object().into_iter().flat_map(Map::keys));
    own.chain(comps).cloned().collect()
}

/// A text layer met.
struct Text {
    at: LayerAt,
    /// Its words, as the format reads them.
    words: String,
    /// Its font, as written.
    font: String,
    /// Its size.
    size: f64,
    /// Whether it is box text.
    boxed: bool,
    /// Its layer's name in the project.
    name: String,
    /// Its composition's name in the project.
    comp: String,
    /// Whether its composition is for editing.
    edit: bool,
    /// The size its stored lines cover, when the project stored them.
    covers: Option<[f64; 2]>,
    /// How many lines the project laid it out in.
    lines: usize,
    /// Its layer's id in the scene.
    layer_id: String,
}

/// The text slots: one per text layer, copies stacked in one composition
/// sharing one; the layer follows it and gets its `fit`.
fn texts(
    scene: &mut Json,
    known: &Known<'_>,
    all: &[LayerAt],
    edit: &[Option<String>],
    ids: &mut Ids,
    slots: &mut Map<String, Json>,
) -> usize {
    let mut met: Vec<Text> = Vec::new();
    for at in all {
        let Some(layer) = at_mut(scene, &at.steps()) else {
            continue;
        };
        // A layer left out draws no words to vary.
        if hidden(layer) {
            continue;
        }
        let Some(Json::String(written)) = layer.get("text") else {
            continue;
        };
        // `$$` is escaped text, which the format reads with one `$` fewer;
        // any other reference is a text a control already drives.
        let words = match written.strip_prefix("$$") {
            Some(rest) => format!("${rest}"),
            None if written.is_empty() || nakkash_format::slots::is_reference(written) => continue,
            None => written.clone(),
        };
        let font = layer["font"].as_str().unwrap_or_default().to_owned();
        let size = font.parse::<Font>().map_or(0.0, |f| f.size);
        let id = layer["id"].as_str().unwrap_or_default().to_owned();
        let address = id
            .parse::<Id>()
            .ok()
            .map(|id| Address::written(at.scope.as_deref(), id));
        let stored = known
            .texts
            .iter()
            .find(|t| Some(&t.layer) == address.as_ref());
        met.push(Text {
            at: at.clone(),
            words,
            font,
            size,
            boxed: layer.get("box").is_some(),
            name: known
                .layer_names
                .get(&(at.scope.clone(), id.clone()))
                .cloned()
                .unwrap_or_default(),
            comp: known.comp_names.get(&at.scope).cloned().unwrap_or_default(),
            edit: edit.contains(&at.scope),
            covers: stored.map(|t| covers(&t.lines, size)),
            lines: stored.map_or(1, |t| t.lines.len().max(1)),
            layer_id: id,
        });
    }
    // Copies stacked in one composition share the first one's slot.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, text) in met.iter().enumerate() {
        let same = groups.iter_mut().find(|g| {
            let first = &met[g[0]];
            first.at.scope == text.at.scope && first.words == text.words && first.font == text.font
        });
        match same {
            Some(group) => group.push(i),
            None => groups.push(vec![i]),
        }
    }
    let roles: Vec<&'static str> = groups
        .iter()
        .map(|g| text_role(&met[g[0]], &met, &groups))
        .collect();
    // Edit compositions first, then the scene's own texts, then by size.
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by(|&a, &b| {
        let (a, b) = (&met[groups[a][0]], &met[groups[b][0]]);
        (!a.edit, a.at.scope.is_some(), &a.at.scope)
            .cmp(&(!b.edit, b.at.scope.is_some(), &b.at.scope))
            .then(b.size.total_cmp(&a.size))
            .then(b.at.index.cmp(&a.at.index))
    });
    for g in order {
        let first = &met[groups[g][0]];
        let id = ids.give(roles[g], "text");
        let name = match first.name.is_empty() {
            true => first.words.chars().take(40).collect(),
            false => first.name.clone(),
        };
        let label = match first.comp.is_empty() {
            true => name,
            false => format!("{name} in {}", first.comp),
        };
        let mut declared = json!({"text": first.words, "label": label, "role": roles[g]});
        if let Some(uid) = text_uid(known, first) {
            declared["uid"] = json!(uid);
        }
        let (overflow, max_lines) = match (first.boxed, first.lines > 1 || first.words.contains('\n')) {
            (true, _) => ("shrink", None),
            (false, true) => ("wrap", Some(first.lines.max(first.words.lines().count()) + 1)),
            (false, false) => ("shrink", Some(1)),
        };
        declared["overflow"] = json!(overflow);
        if first.size > 0.0 {
            declared["min-size"] = json!(super::keys::num((first.size * MIN_SHARE * 100.0).round() / 100.0));
        }
        if let Some(lines) = max_lines {
            declared["max-lines"] = json!(lines);
        }
        slots.insert(id.clone(), declared);
        for &i in &groups[g] {
            let text = &met[i];
            let Some(layer) = at_mut(scene, &text.at.steps()).and_then(Json::as_object_mut) else {
                continue;
            };
            layer.insert("text".into(), json!(format!("${id}")));
            if !layer.contains_key("fit") {
                layer.insert("fit".into(), json!(fit(text)));
            }
        }
    }
    groups.len()
}

/// The share of its size a text may shrink to before it is too small: the
/// floor the import writes as a text slot's `min-size` ([D91]). Our choice,
/// about a step down a type scale; the designer moves it.
///
/// [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point
const MIN_SHARE: f64 = 0.75;

/// The uid of a text's slot: its layer in the project, by the
/// composition's item id and the layer's own id, which a later import of
/// the same project gives again.
fn text_uid(known: &Known<'_>, text: &Text) -> Option<String> {
    let layer_id = known.layer_items.get(&(text.at.scope.clone(), text.layer_id.clone()))?;
    let comp = known.comp_items.get(&text.at.scope).copied().unwrap_or(0);
    Some(format!("text:{comp}/{layer_id}"))
}

/// The size `lines` of text in `size` px cover, as the project laid them
/// out: the widest line, and a line's height per line. The import's
/// caller measures it again in the scene's own fonts.
fn covers(lines: &[crate::model::ComposedLine], size: f64) -> [f64; 2] {
    let width = lines
        .iter()
        .map(|line| line.segments.iter().map(|s| s.width).sum::<f64>())
        .fold(0.0, f64::max);
    let height = size * 1.2 * lines.len().max(1) as f64;
    [width.max(1.0), height.max(1.0)]
}

/// The `fit` of a text: box text shrinks into its box; point text of one
/// line shrinks into the size it covers, of several wraps at its width.
fn fit(text: &Text) -> String {
    if text.boxed {
        return "shrink".into();
    }
    let [w, h] = text
        .covers
        .unwrap_or([text.size.max(1.0) * 10.0, text.size.max(1.0) * 1.2]);
    let mode = match text.lines > 1 || text.words.contains('\n') {
        true => "wrap",
        false => "shrink",
    };
    format!("{mode} {}x{}", size(w), size(h))
}

impl super::Import {
    /// Writes again the size in each text layer's `fit` as `size` measures
    /// it: the size the layer's lines cover as written, set in the scene's
    /// own fonts, where the import wrote the size the project laid out. A
    /// layer `size` gives none for keeps the project's.
    ///
    /// # Errors
    /// An invalid diagnostic, as a bug in the import, when the format
    /// rejects the scene with the sizes written.
    pub fn measure_fits(
        &mut self,
        mut size: impl FnMut(&nakkash_format::Document, &Address) -> Option<[f64; 2]>,
    ) -> nakkash_format::Result<()> {
        let mut lists: Vec<(Option<String>, &mut Json)> = Vec::new();
        let Some(scene) = self.json.as_object_mut() else {
            return Ok(());
        };
        for (name, value) in scene.iter_mut() {
            match name.as_str() {
                "layers" => lists.push((None, value)),
                "comps" => lists.extend(
                    value
                        .as_object_mut()
                        .into_iter()
                        .flatten()
                        .filter_map(|(id, comp)| Some((Some(id.clone()), comp.get_mut("layers")?))),
                ),
                _ => {}
            }
        }
        let mut changed = false;
        for (comp, layers) in lists {
            for layer in layers.as_array_mut().into_iter().flatten() {
                let Some(fields) = layer.as_object_mut() else {
                    continue;
                };
                let Some((mode, _)) = fields
                    .get("fit")
                    .and_then(Json::as_str)
                    .and_then(|f| f.split_once(' '))
                else {
                    continue;
                };
                let mode = mode.to_owned();
                let Some(id) = fields
                    .get("id")
                    .and_then(Json::as_str)
                    .and_then(|id| id.parse::<Id>().ok())
                else {
                    continue;
                };
                let address = Address::written(comp.as_deref(), id);
                if let Some([w, h]) = size(&self.document, &address) {
                    let fit = format!(
                        "{mode} {}x{}",
                        self::size(w.max(1.0)),
                        self::size(h.max(1.0))
                    );
                    changed |= fields.get("fit") != Some(&json!(fit));
                    fields.insert("fit".into(), json!(fit));
                }
            }
        }
        if changed {
            self.document = super::read_back(&self.json)?;
        }
        Ok(())
    }
}

/// Whether `layer` is left out of the frame.
fn hidden(layer: &Json) -> bool {
    layer.get("hidden") == Some(&Json::Bool(true))
}

/// A size as `fit` writes it: rounded up to a hundredth.
pub(super) fn size(value: f64) -> String {
    let up = (value * 100.0).ceil() / 100.0;
    super::keys::num(up).to_string()
}

/// What a text is for: from its name, its words, then its size among the
/// texts of its composition.
fn text_role(text: &Text, met: &[Text], groups: &[Vec<usize>]) -> &'static str {
    if let Some(role) = named_role(&text.name) {
        return role;
    }
    if let Some(role) = worded_role(&text.words) {
        return role;
    }
    // The distinct sizes of the texts in its composition, largest first.
    let mut sizes: Vec<f64> = groups
        .iter()
        .map(|g| &met[g[0]])
        .filter(|t| t.at.scope == text.at.scope)
        .map(|t| t.size)
        .collect();
    sizes.sort_by(|a, b| b.total_cmp(a));
    sizes.dedup();
    let largest = sizes.first().copied().unwrap_or(text.size);
    let rank = sizes.iter().position(|s| *s == text.size).unwrap_or(0);
    let long = text.lines >= 3 || text.words.chars().count() >= 80;
    match rank {
        _ if long => "body",
        0 => "title",
        1 => "subtitle",
        _ if text.size < largest * 0.6 => "label",
        _ => "text",
    }
}

/// The role a layer's name gives, by its words.
fn named_role(name: &str) -> Option<&'static str> {
    const NAMED: [(&str, &[&str]); 12] = [
        ("title", &["title", "headline", "heading", "header", "main"]),
        (
            "subtitle",
            &[
                "subtitle",
                "subhead",
                "subheadline",
                "tagline",
                "slogan",
                "sub",
            ],
        ),
        (
            "body",
            &["body", "description", "paragraph", "copy", "info"],
        ),
        ("cta", &["cta", "button", "btn"]),
        ("url", &["url", "website", "web", "www", "link", "site"]),
        ("email", &["email", "mail"]),
        ("phone", &["phone", "tel", "telephone"]),
        ("date", &["date"]),
        ("price", &["price", "cost"]),
        ("number", &["number", "count", "counter", "digit", "digits"]),
        ("label", &["label", "caption", "tag", "name", "lower"]),
        ("title", &["headline"]),
    ];
    let words: Vec<String> = words(name.trim_start_matches('@')).collect();
    NAMED
        .iter()
        .find(|(_, vocabulary)| words.iter().any(|w| vocabulary.contains(&w.as_str())))
        .map(|(role, _)| *role)
}

/// The role a text's words give: an address, a phone number, a price, a
/// date or a number.
fn worded_role(words: &str) -> Option<&'static str> {
    let text = words.trim();
    let lower = text.to_lowercase();
    let one_word = !text.contains(char::is_whitespace);
    let domain = |s: &str| {
        s.rsplit_once('.').is_some_and(|(name, tld)| {
            !name.is_empty() && (2..=6).contains(&tld.len()) && tld.chars().all(char::is_alphabetic)
        })
    };
    if one_word
        && text.contains('@')
        && text
            .split_once('@')
            .is_some_and(|(a, b)| !a.is_empty() && domain(b))
    {
        return Some("email");
    }
    if one_word
        && (lower.starts_with("www.")
            || lower.starts_with("http")
            || domain(lower.trim_end_matches('/')))
    {
        return Some("url");
    }
    let digits = text.chars().filter(char::is_ascii_digit).count();
    let other = text
        .chars()
        .filter(|c| !c.is_ascii_digit() && !" +-().".contains(*c))
        .count();
    let date_like = text.split(['.', '/', '-']).count() == 3
        && text
            .split(['.', '/', '-'])
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    if date_like {
        return Some("date");
    }
    if digits >= 7 && other == 0 {
        return Some("phone");
    }
    if digits > 0 && text.chars().any(|c| "$€£¥₺₹".contains(c)) {
        return Some("price");
    }
    if digits > 0
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || " .,:%+-x".contains(c))
    {
        return Some("number");
    }
    None
}

/// The image and video slots: one per footage file, which every layer
/// drawing it follows; the layers keep the file's proportions in their box.
fn media(
    scene: &mut Json,
    known: &Known<'_>,
    all: &[LayerAt],
    edit: &[Option<String>],
    ids: &mut Ids,
    slots: &mut Map<String, Json>,
) -> usize {
    // By footage item: its fact, and the layers that draw it.
    let mut files: Vec<(&MediaFact, Json, Vec<LayerAt>, bool)> = Vec::new();
    for at in all {
        let Some(layer) = at_mut(scene, &at.steps()).and_then(Json::as_object_mut) else {
            continue;
        };
        let Some(mark) = layer.remove(MEDIA) else {
            continue;
        };
        // A layer left out, such as a guide picture, draws no file.
        if layer.get("hidden") == Some(&Json::Bool(true)) {
            continue;
        }
        let Some(fact) = mark
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| known.media.get(n))
        else {
            continue;
        };
        let value = layer.get(fact.key).cloned().unwrap_or(Json::Null);
        let in_edit = edit.contains(&at.scope);
        match files
            .iter_mut()
            .find(|(f, ..)| f.item == fact.item && f.key == fact.key)
        {
            Some((_, _, layers, edit)) => {
                layers.push(at.clone());
                *edit |= in_edit;
            }
            None => files.push((fact, value, vec![at.clone()], in_edit)),
        }
    }
    let mut made: Vec<(String, &MediaFact)> = Vec::new();
    // Placeholders first: what edit compositions hold, then logos.
    files.sort_by_key(|(fact, _, layers, edit)| {
        (
            !*edit,
            media_role(fact, known, &layers[0], *edit) != "logo",
            layers[0].clone(),
        )
    });
    for (fact, value, layers, edit) in &files {
        let role = media_role(fact, known, &layers[0], *edit);
        let id = ids.give(role, "media");
        let file = fact
            .stored
            .as_deref()
            .map(super::footage::file_name)
            .filter(|name| !name.is_empty())
            .unwrap_or(&fact.name);
        let comp = known
            .comp_names
            .get(&layers[0].scope)
            .cloned()
            .unwrap_or_default();
        let mut label = match comp.is_empty() {
            true => file.to_owned(),
            false => format!("{file} in {comp}"),
        };
        if let Some(why) = &fact.why {
            label.push_str(&format!(", {why}"));
        }
        let mut declared = Map::new();
        declared.insert(fact.key.to_owned(), value.clone());
        declared.insert("label".into(), json!(label));
        declared.insert("role".into(), json!(role));
        declared.insert("uid".into(), json!(format!("footage:{}", fact.item)));
        declared.insert("kind".into(), json!(role));
        let boxed = at(scene, &layers[0].steps())
            .and_then(|l| l.get("box"))
            .and_then(Json::as_array)
            .and_then(|b| Some((b.first()?.as_f64()?, b.get(1)?.as_f64()?)));
        if let Some((w, h)) = boxed.filter(|(w, h)| *w >= 1.0 && *h >= 1.0) {
            declared.insert("aspect".into(), json!(ratio(w.round() as u64, h.round() as u64)));
            declared.insert("min-px".into(), json!([w.round(), h.round()]));
        }
        slots.insert(id.clone(), Json::Object(declared));
        for at in layers {
            if let Some(layer) = at_mut(scene, &at.steps()).and_then(Json::as_object_mut) {
                layer.insert(fact.key.into(), json!(format!("${id}")));
                layer.entry("fit").or_insert_with(|| json!("contain"));
            }
        }
        made.push((id, fact));
    }
    made.len()
}

/// Words in a composition's name that say it stands for a screen.
const SCREEN_WORDS: [&str; 9] = [
    "screen",
    "screenshot",
    "mockup",
    "ui",
    "app",
    "display",
    "device",
    "website",
    "dashboard",
];

/// The slots of placeholder compositions: a composition that draws nothing
/// (no layers, or only pivots, cameras and hidden layers), that a visible
/// `comp` layer draws, and whose name says it is for editing or is a
/// screen. A designer ships it empty for the buyer's picture, so the import
/// gives it an image layer over its whole canvas, which follows a slot of
/// no file yet and so draws nothing until a variant gives it one.
/// Compositions of one name are one slot, as copies of one placeholder the
/// project keeps at two lengths or rates.
fn screens(
    scene: &mut Json,
    known: &Known<'_>,
    ids: &mut Ids,
    slots: &mut Map<String, Json>,
) -> usize {
    let drawn: std::collections::BTreeSet<String> = layers(scene)
        .iter()
        .filter_map(|place| at(scene, &place.steps()))
        .filter(|layer| !hidden(layer))
        .filter_map(|layer| layer.get("comp").and_then(Json::as_str).map(str::to_owned))
        .collect();
    // By the project's name: the compositions of that name, in scene order.
    let mut placeholders: Vec<(String, Vec<String>)> = Vec::new();
    let Some(comps) = scene["comps"].as_object() else {
        return 0;
    };
    for (key, comp) in comps {
        let name = known
            .comp_names
            .get(&Some(key.clone()))
            .cloned()
            .unwrap_or_default();
        let draws = comp["layers"].as_array().is_some_and(|layers| {
            layers.iter().any(|layer| {
                !hidden(layer)
                    && layer.get("pivot") != Some(&Json::Bool(true))
                    && layer.get("camera").is_none()
            })
        });
        let named = is_edit_comp(&name) || words(&name).any(|w| SCREEN_WORDS.contains(&w.as_str()));
        if draws || !named || !drawn.contains(key) {
            continue;
        }
        match placeholders.iter_mut().find(|(n, _)| *n == name) {
            Some((_, keys)) => keys.push(key.clone()),
            None => placeholders.push((name, vec![key.clone()])),
        }
    }
    for (name, keys) in &placeholders {
        let logo = words(name).any(|w| ["logo", "brand", "emblem"].contains(&w.as_str()));
        let role = if logo { "logo" } else { "screen" };
        let id = ids.give(role, "media");
        let sizes: Vec<String> = keys
            .iter()
            .filter_map(|key| scene["comps"][key]["size"].as_str().map(str::to_owned))
            .collect();
        let mut sizes_seen = sizes.clone();
        sizes_seen.dedup();
        let first = keys.first().and_then(|k| known.comp_items.get(&Some(k.clone()))).copied();
        let mut declared = json!({
            "image": null,
            "label": format!("{name}, {}, empty in the project", sizes_seen.join(" and ")),
            "role": role,
            "kind": role,
            "accepts": ["image", "video"],
        });
        if let Some(item) = first {
            declared["uid"] = json!(format!("comp:{item}"));
        }
        if let Some((w, h)) = sizes.first().and_then(|s| s.split_once('x')).and_then(|(w, h)| Some((w.parse::<u64>().ok()?, h.parse::<u64>().ok()?))) {
            declared["aspect"] = json!(ratio(w, h));
            declared["min-px"] = json!([w, h]);
        }
        slots.insert(id.clone(), declared);
        for key in keys {
            let size = scene["comps"][key]["size"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            let Some((w, h)) = size
                .split_once('x')
                .and_then(|(w, h)| Some((w.parse::<f64>().ok()?, h.parse::<f64>().ok()?)))
            else {
                continue;
            };
            let taken: Vec<&str> = scene["comps"][key]["layers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| l.get("id").and_then(Json::as_str))
                .collect();
            let layer_id = (1..)
                .map(|n| match n {
                    1 => role.to_owned(),
                    n => format!("{role}-{n}"),
                })
                .find(|candidate| !taken.contains(&candidate.as_str()))
                .expect("some number is free");
            let layer = json!({"id": layer_id, "image": format!("${id}"), "box": [w, h], "fit": "cover"});
            if let Some(list) = scene["comps"][key]["layers"].as_array_mut() {
                list.insert(0, layer);
            } else if let Some(comp) = scene["comps"][key].as_object_mut() {
                comp.insert("layers".into(), json!([layer]));
            }
        }
    }
    placeholders.len()
}

/// `w:h` in lowest terms.
fn ratio(w: u64, h: u64) -> String {
    fn gcd(a: u64, b: u64) -> u64 {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let g = gcd(w, h).max(1);
    format!("{}:{}", w / g, h / g)
}

/// What a footage file is for: a logo by its name or its composition's, a
/// texture when it is blended over what is below it, and otherwise the
/// kind it is.
fn media_role(fact: &MediaFact, known: &Known<'_>, at: &LayerAt, edit: bool) -> &'static str {
    const LOGO: [&str; 4] = ["logo", "brand", "logotype", "emblem"];
    const TEXTURE: [&str; 12] = [
        "grain", "noise", "dust", "leak", "leaks", "texture", "overlay", "flare", "bokeh", "smoke",
        "scratch", "film",
    ];
    let file = fact
        .stored
        .as_deref()
        .map(super::footage::file_name)
        .unwrap_or_default();
    let comp = known
        .comp_names
        .get(&at.scope)
        .map(String::as_str)
        .unwrap_or_default();
    let names = [fact.name.as_str(), file, comp];
    let any = |vocabulary: &[&str]| {
        names
            .iter()
            .flat_map(|n| words(n).collect::<Vec<_>>())
            .any(|w| vocabulary.contains(&w.as_str()))
    };
    if any(&LOGO) || (edit && words(comp).any(|w| w == "logo")) {
        return "logo";
    }
    if !edit && any(&TEXTURE) {
        return "texture";
    }
    match fact.key {
        "video" => "video",
        _ => "image",
    }
}

/// One colour the scene draws with.
struct Use {
    /// Where it is.
    steps: Vec<Step>,
    /// Its value, straight RGBA.
    color: [f64; 4],
    /// Whether it is a key of a colour a bake keyed frame by frame.
    baked: bool,
    /// Whether it is a text's colour.
    text: bool,
    /// Whether it paints the background.
    background: bool,
}

/// The palette: every colour the scene draws with follows one entry. A
/// colour that cannot show, or that already follows a slot, is left.
fn palette(scene: &mut Json, ids: &mut Ids, slots: &mut Map<String, Json>) -> usize {
    let uses: Vec<Use> = colors::places(scene)
        .into_iter()
        .filter(|place| place.shown)
        .filter_map(|place| {
            let color = at(scene, &place.steps).and_then(colour_of)?;
            Some(Use {
                steps: place.steps,
                color,
                baked: place.baked,
                text: place.text,
                background: place.background,
            })
        })
        .collect();
    if uses.is_empty() {
        return 0;
    }
    // Entries, heaviest colour first.
    let mut weights: Vec<([u8; 4], [f64; 4], usize)> = Vec::new();
    for u in uses.iter().filter(|u| !u.baked) {
        let key = eight_bit(u.color);
        match weights.iter_mut().find(|(k, ..)| *k == key) {
            Some((.., n)) => *n += 1,
            None => weights.push((key, u.color, 1)),
        }
    }
    weights.sort_by(|a, b| b.2.cmp(&a.2));
    // A colour control the scene already has is the entry of its colour:
    // the designer named it.
    let controls = colour_controls(scene);
    let mut entries: Vec<[f64; 4]> = controls.iter().map(|(_, color)| *color).collect();
    for (_, color, _) in &weights {
        if !entries
            .iter()
            .any(|e| oklch::distance(rgb(*e), rgb(*color)) <= TOLERANCE)
        {
            entries.push(*color);
        }
    }
    if entries.is_empty() {
        // Only baked keys: their first is the entry.
        entries.push(uses[0].color);
    }
    let nearest = |color: [f64; 4]| {
        (0..entries.len())
            .min_by(|&a, &b| {
                oklch::distance(rgb(entries[a]), rgb(color))
                    .total_cmp(&oklch::distance(rgb(entries[b]), rgb(color)))
            })
            .expect("a palette has an entry")
    };
    let owner: Vec<usize> = uses.iter().map(|u| nearest(u.color)).collect();
    let names = entry_names(&entries, &uses, &owner);
    let entry_ids: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(e, (name, _))| match controls.get(e) {
            Some((id, _)) => id.clone(),
            None => ids.give(name, "color"),
        })
        .collect();
    for (e, color) in entries.iter().enumerate() {
        let (_, role) = &names[e];
        match controls.get(e) {
            // A control keeps its declaration, and says what it paints when
            // the scene's colours follow it.
            Some((id, _)) if owner.contains(&e) => {
                if let Some(declared) = scene["slots"].get_mut(id.as_str()) {
                    let value = declared.clone();
                    if !value.is_object() {
                        *declared = json!({"color": value});
                    }
                    declared["role"] = json!(role);
                }
            }
            Some(_) => {}
            None => {
                slots.insert(
                    entry_ids[e].clone(),
                    json!({"color": channels(*color), "label": color_label(*color), "role": role, "uid": format!("color:{}", hex8(*color))}),
                );
            }
        }
    }
    // Words are read against the background: the text entry's pair.
    let new = |role: &str| {
        (0..entries.len())
            .find(|&e| names[e].1 == role && controls.get(e).is_none())
            .map(|e| entry_ids[e].clone())
    };
    if let (Some(text), Some(background)) = (new("text"), new("background"))
        && let Some(declared) = slots.get_mut(&text)
    {
        declared["pair"] = json!(background);
        declared["contrast"] = json!(TEXT_CONTRAST);
    }
    for (u, e) in uses.iter().zip(&owner) {
        let shift = Shift::between(entries[*e], u.color);
        let written = match shift.is_none() {
            true => format!("${}", entry_ids[*e]),
            false => format!("${} {shift}", entry_ids[*e]),
        };
        if let Some(place) = at_mut(scene, &u.steps) {
            *place = Json::String(written);
        }
    }
    (0..entries.len())
        .filter(|e| *e >= controls.len() || owner.contains(e))
        .count()
}

/// The scene's own colour slots, which controls became (D44, D53), with
/// their colours.
fn colour_controls(scene: &Json) -> Vec<(String, [f64; 4])> {
    scene["slots"]
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(id, declared)| {
            let value = match declared {
                Json::Object(fields) => fields.get("color")?,
                Json::String(_) | Json::Array(_) => declared,
                _ => return None,
            };
            Some((id.clone(), nakkash_format::color(value)?))
        })
        .collect()
}

/// `value` as a colour, when it is one the scene wrote and not a reference.
fn colour_of(value: &Json) -> Option<[f64; 4]> {
    match value {
        Json::String(text) if text.starts_with('#') => nakkash_format::color(value),
        Json::Array(items) if (3..=4).contains(&items.len()) => nakkash_format::color(value),
        _ => None,
    }
}

/// The name each entry goes by, and its role: `background` for the one
/// that paints the background, `text` for the one most text uses, then by
/// weight `accent` for colours and `light`, `dark` or `grey` for neutrals.
fn entry_names(entries: &[[f64; 4]], uses: &[Use], owner: &[usize]) -> Vec<(String, &'static str)> {
    let count = |e: usize, f: &dyn Fn(&Use) -> bool| {
        uses.iter()
            .zip(owner)
            .filter(|(u, o)| **o == e && f(u))
            .count()
    };
    let weight: Vec<usize> = (0..entries.len()).map(|e| count(e, &|_| true)).collect();
    let background = (0..entries.len())
        .filter(|&e| count(e, &|u| u.background) > 0)
        .max_by_key(|&e| (count(e, &|u| u.background), weight[e]));
    let text = (0..entries.len())
        .filter(|&e| Some(e) != background && count(e, &|u| u.text) > 0)
        .max_by_key(|&e| (count(e, &|u| u.text), weight[e]));
    entries
        .iter()
        .enumerate()
        .map(|(e, color)| {
            let role: &'static str = if Some(e) == background {
                "background"
            } else if Some(e) == text {
                "text"
            } else {
                let lch = Oklch::from_srgb(rgb(*color));
                match (lch.c > 0.04, lch.l) {
                    (true, _) => "accent",
                    (false, l) if l > 0.75 => "light",
                    (false, l) if l < 0.25 => "dark",
                    (false, _) => "grey",
                }
            };
            (role.to_owned(), role)
        })
        .collect()
}

/// A colour's name for people: its hue and its value, `red #E63946`.
fn color_label(color: [f64; 4]) -> String {
    let lch = Oklch::from_srgb(rgb(color));
    let hue = if lch.c < 0.03 {
        match lch.l {
            l if l > 0.95 => "white",
            l if l < 0.15 => "black",
            _ => "grey",
        }
    } else {
        match lch.h {
            h if !(40.0..350.0).contains(&h) => "red",
            h if h < 75.0 && lch.l < 0.55 => "brown",
            h if h < 75.0 => "orange",
            h if h < 120.0 => "yellow",
            h if h < 180.0 => "green",
            h if h < 230.0 => "cyan",
            h if h < 290.0 => "blue",
            h if h < 330.0 => "purple",
            _ => "pink",
        }
    };
    let [r, g, b, _] = eight_bit(color);
    format!("{hue} #{r:02X}{g:02X}{b:02X}")
}

/// A colour's channels as the scene writes them.
fn channels(color: [f64; 4]) -> Json {
    Json::Array(color.iter().map(|c| super::keys::num(*c)).collect())
}

/// The red, green and blue of `color`.
fn rgb(color: [f64; 4]) -> [f64; 3] {
    [color[0], color[1], color[2]]
}

/// `color` in eight bits a channel.
fn eight_bit(color: [f64; 4]) -> [u8; 4] {
    color.map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// The timing slots: the scene's `dur` follows `duration`, in seconds.
fn timing(scene: &mut Json, ids: &mut Ids, slots: &mut Map<String, Json>) -> usize {
    let Some(seconds) = scene["dur"]
        .as_str()
        .and_then(|d| d.strip_suffix('s'))
        .and_then(|d| d.parse::<f64>().ok())
    else {
        return 0;
    };
    let id = ids.give("duration", "timing");
    slots.insert(
        id.clone(),
        json!({"number": super::keys::num(seconds), "label": "Duration in seconds", "role": "duration", "uid": "duration"}),
    );
    scene["dur"] = json!(format!("${id}"));
    1
}

#[cfg(test)]
#[path = "../../tests/unit/map/variable.rs"]
mod tests;
