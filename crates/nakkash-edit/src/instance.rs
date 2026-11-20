//! Instances ([D91]): a template filled by a small file of slot values,
//! pinned to the template by its path and hash, which every command merges
//! into the template at one point. This module is that point, for the
//! stages that need no fonts or files: *parse* (the instance's closed
//! schema), *resolve* (the pin, the ids, uids and aliases, the look and the
//! sections) and *type* (each value against its slot's type, bounds and
//! locks, and `raw`). Each stage returns every problem it finds at once, and
//! a stage with an error stops the stages after it; the engine adds the
//! fourth, *measure*.
//!
//! [D91]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d91-an-agent-fills-a-template-through-a-pinned-instance-a-brief-and-one-merge-point

use std::collections::BTreeMap;
use std::fmt;

use nakkash_format::key::{COMP, COMPS, HIDDEN, LAYERS, LOOKS, SLOTS};
use nakkash_format::{Address, Document, Lock, RoleTarget, Slot, SlotType, Step};
use serde_json::{Map, Value as Json, json};

/// The schema label of the instances this module reads and writes.
pub const SCHEMA: &str = "nakkash.Instance.1";

/// The keys an instance may have.
const KEYS: [&str; 7] = [
    "schema", "template", "look", "slots", "sections", "raw", "stale",
];
/// The keys of an instance's `template`.
const PIN_KEYS: [&str; 3] = ["path", "hash", "uids"];
/// The keys of a `raw` entry.
const RAW_KEYS: [&str; 3] = ["at", "expect", "value"];
/// What a section's story is: the words `sections` may give a section's
/// `role`.
pub const STORY_ROLES: [&str; 6] = ["hook", "problem", "feature", "proof", "cta", "unknown"];
/// The first keys `raw` may never set: motion, timing, effects and drawn
/// paths, which only the template's designer changes.
pub const RAW_REFUSED: [&str; 16] = [
    "anim",
    "keys",
    "in",
    "out",
    "loop",
    "span",
    "offset",
    "dur",
    "remap",
    "speed",
    "fx",
    "shapes",
    "mask",
    "animate",
    "behaviour",
    "presets",
];
/// The sentinel that returns a slot to the template's value.
pub const UNSET: &str = "$unset";
/// The sentinel that hides every layer drawing what follows a slot.
pub const HIDE: &str = "$hide";

/// An instance as read.
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    /// The template it fills.
    pub template: Pin,
    /// The look of the template it starts from, if any.
    pub look: Option<String>,
    /// The slot values, by slot id (`title`, or `scene-01/tint` for a
    /// composition's), in the order written.
    pub slots: Map<String, Json>,
    /// What each section is for, by the id of its `comp` layer.
    pub sections: BTreeMap<String, String>,
    /// Static values of layers no slot holds.
    pub raw: Vec<RawSet>,
    /// Values whose slots a newer template no longer has, kept by `migrate`.
    pub stale: Map<String, Json>,
}

/// What an instance fills: the template's file, relative to the instance's
/// folder, the hash of its canonical form, and the uid of every slot the
/// instance writes, as the template declared it when the pin was made.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pin {
    /// The template's file, relative to the instance's folder.
    pub path: String,
    /// `sha256:…` of the template's canonical form.
    pub hash: String,
    /// The uids of the slots written, by id.
    pub uids: BTreeMap<String, String>,
}

/// One `raw` entry: an address, the type it expects and its value.
#[derive(Debug, Clone, PartialEq)]
pub struct RawSet {
    /// The value's address, `scene-01/cc.opacity`.
    pub at: String,
    /// What the address holds: `number`, `string`, `bool`, `color`,
    /// `point` or `json`.
    pub expect: String,
    /// The new value.
    pub value: Json,
}

/// How bad a problem is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Renders, and is listed.
    Warning,
    /// Stops a render.
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Level::Warning => "warning",
            Level::Error => "error",
        })
    }
}

/// One problem of an instance, at one place, with what was measured and
/// the limit in the slot's own units.
#[derive(Debug, Clone, PartialEq)]
pub struct Problem {
    /// Whether it stops a render.
    pub level: Level,
    /// What it is, `text.overflow`; stable, for tools.
    pub code: &'static str,
    /// The stage that found it: 1 parse, 2 resolve, 3 type, 4 measure.
    pub stage: u8,
    /// The slot it is about, if any.
    pub slot: Option<String>,
    /// Where in the instance, `slots.title`.
    pub path: String,
    /// What it says, for people.
    pub message: String,
    /// The declared rule it breaks, `max-lines`, if any.
    pub rule: Option<String>,
    /// What was measured.
    pub measured: Option<Json>,
    /// What was allowed.
    pub limit: Option<Json>,
    /// What would repair it.
    pub hint: Option<String>,
}

impl Problem {
    /// A problem with no slot, rule, measure or hint yet.
    pub fn new(
        level: Level,
        code: &'static str,
        stage: u8,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Problem {
        Problem {
            level,
            code,
            stage,
            slot: None,
            path: path.into(),
            message: message.into(),
            rule: None,
            measured: None,
            limit: None,
            hint: None,
        }
    }

    /// The same, about slot `slot`.
    #[must_use]
    pub fn slot(mut self, slot: impl Into<String>) -> Problem {
        self.slot = Some(slot.into());
        self
    }

    /// The same, with a hint.
    #[must_use]
    pub fn hint(mut self, hint: impl Into<String>) -> Problem {
        self.hint = Some(hint.into());
        self
    }

    /// The same, breaking `rule`, measured `measured` against `limit`.
    #[must_use]
    pub fn measured(mut self, rule: &str, measured: Json, limit: Json) -> Problem {
        self.rule = Some(rule.to_owned());
        self.measured = Some(measured);
        self.limit = Some(limit);
        self
    }

    /// The problem as one JSON object, as `check --json` writes it.
    pub fn to_json(&self) -> Json {
        let mut out = Map::new();
        out.insert("level".into(), json!(self.level.to_string()));
        out.insert("code".into(), json!(self.code));
        out.insert("stage".into(), json!(self.stage));
        if let Some(slot) = &self.slot {
            out.insert("slot".into(), json!(slot));
        }
        out.insert("path".into(), json!(self.path));
        out.insert("message".into(), json!(self.message));
        for (key, value) in [
            ("rule", self.rule.as_ref().map(|r| json!(r))),
            ("measured", self.measured.clone()),
            ("limit", self.limit.clone()),
            ("hint", self.hint.as_ref().map(|h| json!(h))),
        ] {
            if let Some(value) = value {
                out.insert(key.into(), value);
            }
        }
        Json::Object(out)
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} at {}: {}",
            self.level, self.code, self.path, self.message
        )?;
        if let (Some(measured), Some(limit)) = (&self.measured, &self.limit) {
            write!(f, " (measured {measured}, limit {limit})")?;
        }
        if let Some(hint) = &self.hint {
            write!(f, "; {hint}")?;
        }
        Ok(())
    }
}

/// Whether any of `problems` is an error.
pub fn has_errors(problems: &[Problem]) -> bool {
    problems.iter().any(|p| p.level == Level::Error)
}

/// Whether `json` is an instance: an object whose `schema` names one.
pub fn is_instance(json: &Json) -> bool {
    json.get("schema")
        .and_then(Json::as_str)
        .is_some_and(|s| s.starts_with("nakkash.Instance."))
}

impl Instance {
    /// An instance of no values, pinned to `template`, read from the file
    /// `path` relative to the instance's folder, with the hash `hash`.
    pub fn pinned(path: &str, hash: &str) -> Instance {
        Instance {
            template: Pin {
                path: path.to_owned(),
                hash: hash.to_owned(),
                uids: BTreeMap::new(),
            },
            look: None,
            slots: Map::new(),
            sections: BTreeMap::new(),
            raw: Vec::new(),
            stale: Map::new(),
        }
    }

    /// Reads `json` (stage 1, *parse*): the closed schema, every problem at
    /// once.
    ///
    /// # Errors
    /// Every problem of the schema: an unknown or missing key, a schema
    /// label this reader does not know, a value of the wrong shape.
    pub fn read(json: &Json) -> Result<Instance, Vec<Problem>> {
        let mut problems = Vec::new();
        let error =
            |code, path: &str, message: String| Problem::new(Level::Error, code, 1, path, message);
        let Some(fields) = json.as_object() else {
            return Err(vec![error(
                "schema.not_object",
                "",
                "an instance is a JSON object".into(),
            )]);
        };
        for key in fields.keys().filter(|k| !KEYS.contains(&k.as_str())) {
            problems.push(
                error(
                    "schema.unknown_field",
                    key,
                    format!("unknown field `{key}`"),
                )
                .hint(format!("the fields are {}", KEYS.join(", "))),
            );
        }
        match fields.get("schema").and_then(Json::as_str) {
            Some(SCHEMA) => {}
            Some(other) => problems.push(
                error(
                    "schema.version_unsupported",
                    "schema",
                    format!("`{other}` is not a schema this nakkash reads"),
                )
                .hint(format!(
                    "`{SCHEMA}`; `nakkash migrate` upgrades older instances"
                )),
            ),
            None => problems.push(
                error("schema.missing", "schema", "no `schema`".into())
                    .hint(format!("\"schema\":\"{SCHEMA}\"")),
            ),
        }
        let mut pin = Pin::default();
        match fields.get("template").and_then(Json::as_object) {
            None => problems.push(error("schema.missing", "template", "no `template` pin".into()).hint(r#""template":{"path":"promo.json","hash":"sha256:…"}; `nakkash slots <template> --as-instance` writes one"#)),
            Some(t) => {
                for key in t.keys().filter(|k| !PIN_KEYS.contains(&k.as_str())) {
                    problems.push(error("schema.unknown_field", &format!("template.{key}"), format!("unknown field `{key}`")));
                }
                match (t.get("path").and_then(Json::as_str), t.get("hash").and_then(Json::as_str)) {
                    (Some(path), Some(hash)) if !path.is_empty() && hash.starts_with("sha256:") => {
                        pin.path = path.to_owned();
                        pin.hash = hash.to_owned();
                    }
                    _ => problems.push(error("schema.invalid", "template", "the pin needs a `path` and a `hash` of `sha256:…`".into())),
                }
                match t.get("uids") {
                    None => {}
                    Some(Json::Object(uids)) if uids.values().all(Json::is_string) => {
                        pin.uids = uids.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned())).collect();
                    }
                    Some(_) => problems.push(error("schema.invalid", "template.uids", "uids are an object of strings by slot id".into())),
                }
            }
        }
        let look = match fields.get("look") {
            None => None,
            Some(Json::String(look)) => Some(look.clone()),
            Some(_) => {
                problems.push(error(
                    "schema.invalid",
                    "look",
                    "a look is the name of one of the template's `looks`".into(),
                ));
                None
            }
        };
        let object = |key: &str, problems: &mut Vec<Problem>| match fields.get(key) {
            None => Map::new(),
            Some(Json::Object(map)) => map.clone(),
            Some(_) => {
                problems.push(error(
                    "schema.invalid",
                    key,
                    format!("`{key}` is an object"),
                ));
                Map::new()
            }
        };
        let slots = object("slots", &mut problems);
        let stale = object("stale", &mut problems);
        let mut sections = BTreeMap::new();
        for (id, section) in object("sections", &mut problems) {
            let path = format!("sections.{id}");
            match section.get("role").and_then(Json::as_str) {
                Some(role) if section.as_object().is_some_and(|s| s.len() == 1) => {
                    sections.insert(id, role.to_owned());
                }
                _ => problems.push(error(
                    "schema.invalid",
                    &path,
                    r#"a section is {"role":"…"}"#.into(),
                )),
            }
        }
        let mut raw = Vec::new();
        match fields.get("raw") {
            None => {}
            Some(Json::Array(entries)) => {
                for (i, entry) in entries.iter().enumerate() {
                    let path = format!("raw[{i}]");
                    let Some(e) = entry.as_object() else {
                        problems.push(error(
                            "schema.invalid",
                            &path,
                            "a raw entry is an object".into(),
                        ));
                        continue;
                    };
                    for key in e.keys().filter(|k| !RAW_KEYS.contains(&k.as_str())) {
                        problems.push(error(
                            "schema.unknown_field",
                            &format!("{path}.{key}"),
                            format!("unknown field `{key}`"),
                        ));
                    }
                    match (
                        e.get("at").and_then(Json::as_str),
                        e.get("expect").and_then(Json::as_str),
                        e.get("value"),
                    ) {
                        (Some(at), Some(expect), Some(value)) => raw.push(RawSet {
                            at: at.to_owned(),
                            expect: expect.to_owned(),
                            value: value.clone(),
                        }),
                        _ => problems.push(error(
                            "schema.invalid",
                            &path,
                            r#"a raw entry is {"at":"layer.key","expect":"number","value":…}"#
                                .into(),
                        )),
                    }
                }
            }
            Some(_) => problems.push(error("schema.invalid", "raw", "`raw` is a list".into())),
        }
        match problems.is_empty() {
            true => Ok(Instance {
                template: pin,
                look,
                slots,
                sections,
                raw,
                stale,
            }),
            false => Err(problems),
        }
    }

    /// The instance as JSON, in the order this module writes it.
    pub fn to_json(&self) -> Json {
        let mut out = Map::new();
        out.insert("schema".into(), json!(SCHEMA));
        let mut pin = Map::new();
        pin.insert("path".into(), json!(self.template.path));
        pin.insert("hash".into(), json!(self.template.hash));
        if !self.template.uids.is_empty() {
            pin.insert("uids".into(), json!(self.template.uids));
        }
        out.insert("template".into(), Json::Object(pin));
        if let Some(look) = &self.look {
            out.insert("look".into(), json!(look));
        }
        out.insert("slots".into(), Json::Object(self.slots.clone()));
        if !self.sections.is_empty() {
            let sections: Map<String, Json> = self
                .sections
                .iter()
                .map(|(id, role)| (id.clone(), json!({ "role": role })))
                .collect();
            out.insert("sections".into(), Json::Object(sections));
        }
        if !self.raw.is_empty() {
            let raw: Vec<Json> = self
                .raw
                .iter()
                .map(|r| json!({"at": r.at, "expect": r.expect, "value": r.value}))
                .collect();
            out.insert("raw".into(), Json::Array(raw));
        }
        if !self.stale.is_empty() {
            out.insert("stale".into(), Json::Object(self.stale.clone()));
        }
        Json::Object(out)
    }
}

/// A template's slot as an instance names it.
#[derive(Debug, Clone)]
struct Named {
    /// `id`, or `comp/id` for a composition's slot.
    key: String,
    /// The composition that declares it; none for the scene.
    comp: Option<String>,
    slot: Slot,
}

/// Every slot of `template`, as an instance names it.
fn named(template: &Document) -> Vec<Named> {
    template
        .slots()
        .into_iter()
        .map(|listed| Named {
            key: match &listed.comp {
                None => listed.slot.id.to_string(),
                Some(comp) => format!("{comp}/{}", listed.slot.id),
            },
            comp: listed.comp,
            slot: listed.slot,
        })
        .collect()
}

/// The result of merging an instance into its template.
#[derive(Debug, Clone)]
pub struct Merged {
    /// The template's JSON with the instance's values in it, when no stage
    /// found an error; what a session reads.
    pub raw: Option<Json>,
    /// Every problem found, by stage.
    pub problems: Vec<Problem>,
    /// The slots the instance gives a value, by the key it names them with
    /// after relinking, with the value it gives: what the measure stage
    /// reads.
    pub given: Vec<(String, Json)>,
    /// The instance with relinks written and each written slot's uid
    /// pinned, as `migrate` saves it.
    pub relinked: Instance,
}

/// Merges `instance` into `template`, whose canonical hash is `hash`
/// (stages 2 and 3). `file` turns a file an instance gives, relative to
/// the instance's folder, into one relative to the template's.
pub fn merge(
    template: &Document,
    hash: &str,
    instance: &Instance,
    file: &dyn Fn(&str) -> String,
) -> Merged {
    let slots = named(template);
    let mut problems = Vec::new();
    let mut relinked = instance.clone();
    let err = |code, stage, path: String, message: String| {
        Problem::new(Level::Error, code, stage, path, message)
    };
    let warn = |code, stage, path: String, message: String| {
        Problem::new(Level::Warning, code, stage, path, message)
    };

    // Stage 2: the pin, the ids, the look and the sections.
    let changed = instance.template.hash != hash;
    let mut given: Vec<(String, Json)> = Vec::new();
    relinked.slots = Map::new();
    for (key, value) in &instance.slots {
        let path = format!("slots.{key}");
        let pinned_uid = instance.template.uids.get(key);
        let by_key = slots.iter().find(|n| n.key == *key);
        let found = match (by_key, pinned_uid) {
            (Some(n), Some(uid)) if n.slot.uid.as_deref() != Some(uid.as_str()) => None,
            (Some(n), _) => Some(n),
            (None, _) => None,
        };
        let found = match found {
            Some(n) => Some(n),
            None => {
                // By the uid it was pinned with, then by an alias.
                let by_uid = pinned_uid.and_then(|uid| {
                    slots
                        .iter()
                        .find(|n| n.slot.uid.as_deref() == Some(uid.as_str()))
                });
                let by_alias = || {
                    slots
                        .iter()
                        .find(|n| n.slot.contract.aliases.iter().any(|a| a == key))
                };
                match by_uid.or_else(by_alias) {
                    Some(n) => {
                        problems.push(
                            warn(
                                "target.relinked",
                                2,
                                path.clone(),
                                format!("`{key}` is now `{}`", n.key),
                            )
                            .slot(n.key.clone()),
                        );
                        Some(n)
                    }
                    None if pinned_uid.is_some() => {
                        problems.push(err("target.stale", 2, path.clone(), format!("the template no longer has the slot `{key}` was pinned to")).slot(key.clone()).hint("`nakkash migrate` keeps its value under `stale` and pins the template again"));
                        None
                    }
                    None => {
                        problems.push(
                            err(
                                "slot.unknown",
                                2,
                                path.clone(),
                                format!("the template has no slot `{key}`"),
                            )
                            .slot(key.clone())
                            .hint(nearest(key, slots.iter().map(|n| n.key.as_str()))),
                        );
                        None
                    }
                }
            }
        };
        if let Some(n) = found {
            if let Some(uid) = &n.slot.uid {
                relinked.template.uids.insert(n.key.clone(), uid.clone());
            }
            if n.key != *key {
                relinked.template.uids.remove(key);
            }
            relinked.slots.insert(n.key.clone(), value.clone());
            given.push((n.key.clone(), value.clone()));
        }
    }
    if changed && !has_errors(&problems) {
        problems.push(warn("template.changed", 2, "template.hash".into(), format!("the template changed since the instance was pinned ({} then, {hash} now); every slot it writes is still there", instance.template.hash)).hint("`nakkash migrate` pins the template again"));
    }
    relinked.template.hash = hash.to_owned();
    let looks = template.raw().get(LOOKS).and_then(Json::as_object);
    let look = match &instance.look {
        None => None,
        Some(name) => match looks.and_then(|l| l.get(name)).and_then(Json::as_object) {
            Some(look) => Some(look),
            None => {
                problems.push(
                    err(
                        "look.unknown",
                        2,
                        "look".into(),
                        format!("the template has no look `{name}`"),
                    )
                    .hint(nearest(
                        name,
                        looks.into_iter().flat_map(|l| l.keys().map(String::as_str)),
                    )),
                );
                None
            }
        },
    };
    let sections: Vec<String> = sections_of(template);
    for (id, role) in &instance.sections {
        let path = format!("sections.{id}");
        if !sections.contains(id) {
            problems.push(
                err(
                    "section.unknown",
                    2,
                    path.clone(),
                    format!("the template has no section `{id}`"),
                )
                .hint(nearest(id, sections.iter().map(String::as_str))),
            );
        }
        if !STORY_ROLES.contains(&role.as_str()) {
            problems.push(
                err(
                    "choice.unknown",
                    2,
                    format!("{path}.role"),
                    format!("`{role}` is not a story role"),
                )
                .hint(format!("one of {}", STORY_ROLES.join(", "))),
            );
        }
    }
    if has_errors(&problems) {
        return Merged {
            raw: None,
            problems,
            given,
            relinked,
        };
    }

    // Stage 3: each value against its slot, then the locks, then raw.
    let mut raw = template.raw().clone();
    let held = template.scene().held_locks();
    let mut values: Vec<(&Named, &Json, &'static str)> = Vec::new();
    for (key, value) in look.into_iter().flat_map(|l| l.iter()) {
        if let Some(n) = slots.iter().find(|n| n.key == *key) {
            values.push((n, value, "look"));
        }
    }
    for (key, value) in &given {
        if let Some(n) = slots.iter().find(|n| n.key == *key) {
            values.push((n, value, "slots"));
        }
    }
    for (n, value, from) in values {
        let path = match from {
            "look" => format!(
                "looks.{}.{}",
                instance.look.as_deref().unwrap_or_default(),
                n.key
            ),
            _ => format!("slots.{}", n.key),
        };
        if let Some(problem) = locked(template, &held, n, value) {
            problems.push(err("slot.locked", 3, path, problem).slot(n.key.clone()).hint("a slot the designer locked keeps the template's value; ask for the lock to be lifted, or leave it out"));
            continue;
        }
        match sentinel(value) {
            Some(UNSET) => continue,
            Some(HIDE) => {
                for (comp, index) in followers(template.raw(), n) {
                    if let Some(layer) = layer_mut(&mut raw, comp.as_deref(), index) {
                        layer.insert(HIDDEN.into(), json!(true));
                    }
                }
                continue;
            }
            _ => {}
        }
        match typed(n, value, file) {
            Ok((ty, value)) => set_slot(&mut raw, n, ty, value),
            Err(problem) => problems.push(Problem {
                path,
                slot: Some(n.key.clone()),
                ..problem
            }),
        }
    }
    for (i, entry) in instance.raw.iter().enumerate() {
        let path = format!("raw[{i}]");
        match raw_line(entry) {
            Err(message) => problems.push(err("raw.refused", 3, path, message).hint("`raw` sets a static value no slot holds; motion, timing and effects stay the template's")),
            Ok(line) => {
                let doc = match Document::from_value(raw.clone()) {
                    Ok(doc) => doc,
                    Err(_) => continue,
                };
                match crate::edit_line(&doc, &line) {
                    Ok(edited) => {
                        raw = edited.into_raw();
                        problems.push(warn("raw.applied", 3, path, format!("`{}` set outside the template's slots", entry.at)));
                    }
                    Err(d) => problems.push(err("raw.refused", 3, path, d.to_string())),
                }
            }
        }
    }
    if has_errors(&problems) {
        return Merged {
            raw: None,
            problems,
            given,
            relinked,
        };
    }
    match Document::from_value(raw) {
        Ok(doc) => Merged {
            raw: Some(doc.into_raw()),
            problems,
            given,
            relinked,
        },
        Err(d) => {
            problems.push(err(
                "scene.invalid",
                3,
                d.path_text().unwrap_or_default(),
                d.to_string(),
            ));
            Merged {
                raw: None,
                problems,
                given,
                relinked,
            }
        }
    }
}

/// The ids of the template's sections: the `comp` layers of its root.
pub fn sections_of(template: &Document) -> Vec<String> {
    template
        .raw()
        .get(LAYERS)
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter(|l| l.get(COMP).is_some())
        .filter_map(|l| l.get("id").and_then(Json::as_str).map(str::to_owned))
        .collect()
}

/// `$unset` or `$hide` when `value` is `{"$unset":true}` or
/// `{"$hide":true}`.
fn sentinel(value: &Json) -> Option<&'static str> {
    let fields = value.as_object()?;
    if fields.len() != 1 {
        return None;
    }
    [UNSET, HIDE]
        .into_iter()
        .find(|s| fields.get(*s) == Some(&Json::Bool(true)))
}

/// The value an instance gives slot `n`, checked against its type and
/// bounds: a bare value of its type, or for a file slot an object of
/// `image` or `video` (when the slot accepts it); with the type the
/// declaration takes.
fn typed(
    n: &Named,
    value: &Json,
    file: &dyn Fn(&str) -> String,
) -> Result<(SlotType, Json), Problem> {
    let mismatch = |message: String| Problem::new(Level::Error, "type.mismatch", 3, "", message);
    let slot = &n.slot;
    let (ty, value) = match (slot.ty.is_file(), value) {
        (true, Json::String(path)) => (slot.ty, json!(file(path))),
        (true, Json::Object(fields)) => {
            let mut chosen = None;
            for (key, given) in fields {
                let ty = match SlotType::named(key) {
                    Some(ty)
                        if ty.is_file()
                            && (ty == slot.ty || slot.contract.accepts.contains(&ty)) =>
                    {
                        ty
                    }
                    Some(ty) if ty.is_file() => {
                        return Err(mismatch(format!("the slot `{}` takes no {ty}", n.key))
                            .hint(format!("a {}", slot.ty)));
                    }
                    _ => {
                        return Err(mismatch(format!("unknown field `{key}` for a file slot"))
                            .hint("`image` or `video`, as the slot accepts"));
                    }
                };
                let Some(path) = given.as_str().filter(|p| !p.is_empty()) else {
                    return Err(mismatch(format!("`{given}` is not a file")));
                };
                if chosen.replace((ty, json!(file(path)))).is_some() {
                    return Err(mismatch("one file, `image` or `video`".into()));
                }
            }
            chosen.ok_or_else(|| mismatch("no file".into()))?
        }
        (false, Json::Object(fields))
            if fields.len() == 1 && fields.contains_key(slot.ty.name()) =>
        {
            (slot.ty, fields[slot.ty.name()].clone())
        }
        (_, value) => (slot.ty, value.clone()),
    };
    let checked = match ty == slot.ty {
        true => slot.fits(&value),
        false => Ok(()),
    };
    if let Err(d) = checked {
        let code = match d.kind {
            nakkash_format::Kind::OutOfRange => "value.out_of_range",
            nakkash_format::Kind::UnknownName => "choice.unknown",
            _ => "type.mismatch",
        };
        let mut problem = Problem::new(Level::Error, code, 3, "", d.message.clone());
        problem.hint = Some(d.expected.as_deref().map_or_else(
            || format!("a {}, like `{}`", slot.ty, slot.value),
            str::to_owned,
        ));
        if code == "value.out_of_range" {
            problem.rule = Some(if value.as_f64() < slot.contract.min {
                "min".into()
            } else {
                "max".into()
            });
            problem.measured = Some(value.clone());
            problem.limit = Some(json!({"min": slot.contract.min, "max": slot.contract.max}));
        }
        return Err(problem);
    }
    Ok((ty, value))
}

/// Why slot `n` may not take `value`, if a lock holds it: the slot named,
/// a role naming it, a locked layer drawing what follows it, or the
/// palette for a colour the scene does not have.
fn locked(template: &Document, held: &[(Lock, &Lock)], n: &Named, value: &Json) -> Option<String> {
    let scene = template.scene();
    for (lock, written) in held {
        match lock {
            Lock::Slot(id) if n.comp.is_none() && **id == *n.slot.id => {
                return Some(format!("slot `${id}` is locked by `{written}`"));
            }
            Lock::Role(role) => {
                if let Some(r) = scene.role(role)
                    && matches!(&r.target, RoleTarget::Slot(id) if n.comp.is_none() && **id == *n.slot.id)
                {
                    return Some(format!(
                        "slot `{}` is the role `@{role}`, locked by `{written}`",
                        n.key
                    ));
                }
            }
            Lock::Layer(address) => {
                let ids = layer_ids(template.raw(), n);
                if ids.iter().any(|path| path.starts_with(&address.layers)) {
                    return Some(format!(
                        "what follows `{}` is drawn inside `{address}`, locked by `{written}`",
                        n.key
                    ));
                }
            }
            Lock::Palette if n.slot.ty == SlotType::Color && sentinel(value).is_none() => {
                let new = nakkash_format::color(value)?;
                let known = palette(template.raw());
                if !known.iter().any(|c| same_colour(c, &new)) {
                    return Some(format!(
                        "`{value}` is not one of the template's colours, locked by `{written}`"
                    ));
                }
            }
            _ => {}
        }
    }
    None
}

/// Every colour written in `json`, by value.
fn palette(json: &Json) -> Vec<[f64; 4]> {
    fn visit(v: &Json, out: &mut Vec<[f64; 4]>) {
        match v {
            Json::Array(items) => match nakkash_format::color(v) {
                Some(c) if items.len() >= 3 => out.push(c),
                _ => items.iter().for_each(|i| visit(i, out)),
            },
            Json::Object(fields) => fields.values().for_each(|i| visit(i, out)),
            Json::String(_) => out.extend(nakkash_format::color(v)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    visit(json, &mut out);
    out
}

/// Whether two colours are the same to 8 bits.
fn same_colour(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a.iter()
        .zip(b)
        .all(|(a, b)| (a * 255.0).round() == (b * 255.0).round())
}

/// The layers a slot's followers sit on: the composition (none for the
/// scene's own list) and the layer's index in it. A composition's slot is
/// followed only in it; a scene's slot wherever no composition declares
/// one of the same id.
pub fn followers(raw: &Json, n_key: &impl SlotKey) -> Vec<(Option<String>, usize)> {
    let (comp, id) = n_key.scope_and_id();
    let reference = |text: &str| {
        text.strip_prefix('$')
            .filter(|rest| !rest.starts_with('$'))
            .is_some_and(|rest| {
                rest == id || rest.strip_prefix(id).is_some_and(|r| r.starts_with(' '))
            })
    };
    let mut out = Vec::new();
    let mut scan = |list: Option<&Json>, owner: Option<&str>| {
        for (index, layer) in list
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if refers(layer, &reference) {
                out.push((owner.map(str::to_owned), index));
            }
        }
    };
    match comp {
        Some(comp) => scan(
            raw.get(COMPS)
                .and_then(|c| c.get(comp))
                .and_then(|c| c.get(LAYERS)),
            Some(comp),
        ),
        None => {
            scan(raw.get(LAYERS), None);
            for (key, c) in raw
                .get(COMPS)
                .and_then(Json::as_object)
                .into_iter()
                .flatten()
            {
                let shadowed = c.get(SLOTS).and_then(|s| s.get(id)).is_some();
                if !shadowed {
                    scan(c.get(LAYERS), Some(key));
                }
            }
        }
    }
    out
}

/// What names a slot's scope and id, for [`followers`].
pub trait SlotKey {
    /// The composition that declares it, none for the scene, and its id.
    fn scope_and_id(&self) -> (Option<&str>, &str);
}

impl SlotKey for Named {
    fn scope_and_id(&self) -> (Option<&str>, &str) {
        (self.comp.as_deref(), &self.slot.id)
    }
}

impl SlotKey for (Option<&str>, &str) {
    fn scope_and_id(&self) -> (Option<&str>, &str) {
        (self.0, self.1)
    }
}

/// Whether any string of `layer` (its id aside) is a reference `is`
/// accepts.
fn refers(layer: &Json, is: &dyn Fn(&str) -> bool) -> bool {
    match layer {
        Json::String(text) => is(text),
        Json::Array(items) => items.iter().any(|i| refers(i, is)),
        Json::Object(fields) => fields
            .iter()
            .filter(|(k, _)| *k != "id")
            .any(|(_, v)| refers(v, is)),
        _ => false,
    }
}

/// The ids from the root down to every layer drawing what follows `n`,
/// through the `comp` layers that show each composition.
fn layer_ids(raw: &Json, n: &Named) -> Vec<Vec<nakkash_format::Id>> {
    let mut out = Vec::new();
    for (comp, index) in followers(raw, n) {
        let list = match &comp {
            None => raw.get(LAYERS),
            Some(c) => raw
                .get(COMPS)
                .and_then(|cs| cs.get(c))
                .and_then(|c| c.get(LAYERS)),
        };
        let id = list
            .and_then(|l| l.get(index))
            .and_then(|l| l.get("id"))
            .and_then(Json::as_str)
            .and_then(|id| id.parse::<nakkash_format::Id>().ok());
        let Some(id) = id else {
            continue;
        };
        for mut path in paths_to(raw, comp.as_deref(), 0) {
            path.push(id.clone());
            out.push(path);
        }
    }
    out
}

/// The paths of `comp` layers from the root that show composition `comp`
/// (the empty path for the root itself), at most a few levels deep.
fn paths_to(raw: &Json, comp: Option<&str>, depth: usize) -> Vec<Vec<nakkash_format::Id>> {
    let Some(comp) = comp else {
        return vec![Vec::new()];
    };
    if depth > 16 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut lists: Vec<(Option<&str>, &Json)> =
        raw.get(LAYERS).map(|l| (None, l)).into_iter().collect();
    for (key, c) in raw
        .get(COMPS)
        .and_then(Json::as_object)
        .into_iter()
        .flatten()
    {
        if let Some(l) = c.get(LAYERS) {
            lists.push((Some(key.as_str()), l));
        }
    }
    for (owner, list) in lists {
        for layer in list.as_array().into_iter().flatten() {
            if layer.get(COMP).and_then(Json::as_str) != Some(comp) {
                continue;
            }
            let Some(id) = layer
                .get("id")
                .and_then(Json::as_str)
                .and_then(|id| id.parse::<nakkash_format::Id>().ok())
            else {
                continue;
            };
            for mut path in paths_to(raw, owner, depth + 1) {
                path.push(id.clone());
                out.push(path);
            }
        }
    }
    out
}

/// The layer at `index` of composition `comp`'s list, or of the scene's.
fn layer_mut<'j>(
    raw: &'j mut Json,
    comp: Option<&str>,
    index: usize,
) -> Option<&'j mut Map<String, Json>> {
    let list = match comp {
        None => raw.get_mut(LAYERS)?,
        Some(c) => raw.get_mut(COMPS)?.get_mut(c)?.get_mut(LAYERS)?,
    };
    list.get_mut(index)?.as_object_mut()
}

/// Writes `value`, of type `ty`, into slot `n`'s declaration in `raw`,
/// keeping the form it is written in. A file slot given another file type
/// it accepts turns its followers' key into that type's.
fn set_slot(raw: &mut Json, n: &Named, ty: SlotType, value: Json) {
    if ty != n.slot.ty {
        let (from, to) = (n.slot.ty.name(), ty.name());
        for (comp, index) in followers(raw, n) {
            if let Some(layer) = layer_mut(raw, comp.as_deref(), index)
                && let Some(file) = layer.remove(from)
            {
                layer.insert(to.into(), file);
            }
        }
    }
    let owner = match &n.comp {
        None => Some(&mut *raw),
        Some(c) => raw.get_mut(COMPS).and_then(|cs| cs.get_mut(c.as_str())),
    };
    let Some(declared) = owner
        .and_then(|o| o.get_mut(SLOTS))
        .and_then(|s| s.get_mut(&*n.slot.id))
    else {
        return;
    };
    match declared {
        Json::Object(fields) => {
            let old = SlotType::NAMES
                .iter()
                .map(|(name, _)| *name)
                .find(|name| fields.contains_key(*name));
            match old {
                Some(old) if old != ty.name() => {
                    // A new type takes the old one's place, the other keys kept.
                    let rebuilt: Map<String, Json> = std::mem::take(fields)
                        .into_iter()
                        .map(|(k, v)| match k == old {
                            true => (ty.name().to_owned(), value.clone()),
                            false => (k, v),
                        })
                        .collect();
                    *fields = rebuilt;
                }
                _ => {
                    fields.insert(ty.name().to_owned(), value);
                }
            }
        }
        bare => *bare = value,
    }
}

/// The edit line a `raw` entry makes, or why it is refused.
fn raw_line(entry: &RawSet) -> Result<String, String> {
    let address: Address = entry.at.parse().map_err(|d: nakkash_format::Diagnostic| {
        format!("`{}` is not an address: {}", entry.at, d.message)
    })?;
    if address.layers.is_empty() {
        return Err(format!(
            "`{}` names the scene, which `raw` never sets",
            entry.at
        ));
    }
    let Some(Step::Key(first)) = address.keys.first() else {
        return Err(format!("`{}` names a layer, not a value of it", entry.at));
    };
    if RAW_REFUSED.contains(&first.as_str()) || first.starts_with('$') || first == "overrides" {
        return Err(format!(
            "`{first}` is motion, timing, an effect or a drawn path, which only the template's designer changes"
        ));
    }
    if address.keys.iter().any(|s| matches!(s, Step::Index(_))) {
        return Err(format!(
            "`{}` chooses by position; name list items by their id",
            entry.at
        ));
    }
    let fits = match entry.expect.as_str() {
        "number" => entry.value.is_number(),
        "string" => entry.value.is_string(),
        "bool" => entry.value.is_boolean(),
        "color" => nakkash_format::is_color(&entry.value),
        "point" => entry
            .value
            .as_array()
            .is_some_and(|xy| xy.len() == 2 && xy.iter().all(Json::is_number)),
        "json" => true,
        other => {
            return Err(format!(
                "`{other}` is not a type `raw` expects: number, string, bool, color, point or json"
            ));
        }
    };
    if !fits {
        return Err(format!("`{}` is not a {}", entry.value, entry.expect));
    }
    let layers: Vec<String> = address.layers.iter().map(ToString::to_string).collect();
    let keys = Address {
        layers: Vec::new(),
        keys: address.keys.clone(),
    }
    .to_string();
    let keys = keys.trim_start_matches("scene").trim_start_matches('.');
    Ok(format!(
        "{} {keys}={}",
        layers.join("/"),
        serde_json::to_string(&entry.value).unwrap_or_default()
    ))
}

/// The nearest of `known` to `name`, as a hint.
fn nearest<'k>(name: &str, known: impl Iterator<Item = &'k str>) -> String {
    let mut scored: Vec<(usize, &str)> = known.map(|k| (distance(name, k), k)).collect();
    scored.sort();
    let best: Vec<&str> = scored.iter().take(3).map(|(_, k)| *k).collect();
    match best.is_empty() {
        true => "the template declares none".to_owned(),
        false => format!(
            "did you mean {}?",
            best.iter()
                .map(|k| format!("`{k}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The edit distance between `a` and `b`, in characters.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(prev + usize::from(ca != *cb));
            prev = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()]
}

/// The values of `instance` that differ from `template`'s: what a tool
/// writes, leaving out a value equal to the slot's own.
pub fn elide(instance: &mut Instance, template: &Document) {
    let slots = named(template);
    instance.slots.retain(|key, value| {
        let Some(n) = slots.iter().find(|n| n.key == *key) else {
            return true;
        };
        let same = *value == n.slot.value
            || matches!((nakkash_format::color(value), nakkash_format::color(&n.slot.value)), (Some(a), Some(b)) if n.slot.ty == SlotType::Color && same_colour(&a, &b))
            || value.as_object().is_some_and(|o| o.len() == 1 && o.get(n.slot.ty.name()) == Some(&n.slot.value));
        !same && sentinel(value) != Some(UNSET)
    });
    instance
        .template
        .uids
        .retain(|key, _| instance.slots.contains_key(key));
}

#[cfg(test)]
#[path = "../tests/unit/instance.rs"]
mod tests;
