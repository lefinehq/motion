//! How timing and format change a master without rewriting it ([D78]):
//! its root becomes a composition, and the variant's root draws it through
//! `comp` layers. A `remap` track on each is the time map; each shows one
//! group of what matters, placed by its own transform, with the other
//! groups' layers hidden through `overrides` ([D43]).
//!
//! Every key, curve and baked expression of the master stays as it was, so
//! nothing the import cannot derive again is touched.
//!
//! [D43]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d43-one-layer-address-and-edits-per-instance
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

use std::path::PathBuf;

use nakkash_format::Address;
use nakkash_format::address::Id;
use serde_json::{Map, Value as Json, json};

/// The id of the composition the master's root becomes.
const MASTER: &str = "vary-master";

/// The id prefix of the layers that draw it.
const SHOW: &str = "vary-";

/// The id of the layer that plays a music track.
const MUSIC: &str = "vary-music";

/// The time map: which moment of the master shows at each frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TimeMap {
    /// The variant's length, in frames.
    pub frames: f64,
    /// Keys `(frame of the variant, second of the master)`, linear between,
    /// at every change of rate.
    pub keys: Vec<(f64, f64)>,
}

/// One `comp` layer that draws the master on the variant's canvas.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shown {
    /// The master's layers it hides, by their address in the master.
    pub hide: Vec<String>,
    /// The point of the master's canvas that lands at `position`.
    pub anchor: [f64; 2],
    /// Where `anchor` lands on the variant's canvas.
    pub position: [f64; 2],
    /// How much larger it is drawn than in the master.
    pub scale: f64,
}

/// The master placed on a canvas of another size.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Layout {
    /// The new canvas.
    pub width: u32,
    /// The new canvas.
    pub height: u32,
    /// The layers that draw the master, bottom first.
    pub shown: Vec<Shown>,
}

/// The whole wrapping of one variant.
#[derive(Debug, Clone)]
pub(crate) struct Wrap {
    /// The canvas, in px.
    size: (f64, f64),
    /// The master's canvas, in px.
    master: (f64, f64),
    /// The time map, if the timing axis made one.
    map: Option<TimeMap>,
    /// The layers that draw the master.
    shown: Vec<Shown>,
    /// A music track to play at the root.
    music: Option<PathBuf>,
}

impl Wrap {
    /// The wrapping of a master whose canvas is `master` px, by a time map,
    /// a layout, or both, and a music track.
    pub(crate) fn new(
        master: (f64, f64),
        map: Option<TimeMap>,
        layout: Option<Layout>,
        music: Option<PathBuf>,
    ) -> Wrap {
        let (size, shown) = match layout {
            Some(layout) => (
                (f64::from(layout.width), f64::from(layout.height)),
                layout.shown,
            ),
            None => (
                master,
                vec![Shown {
                    hide: Vec::new(),
                    anchor: [master.0 / 2.0, master.1 / 2.0],
                    position: [master.0 / 2.0, master.1 / 2.0],
                    scale: 1.0,
                }],
            ),
        };
        Wrap {
            size,
            master,
            map,
            shown,
            music,
        }
    }

    /// The variant: `raw` with its root moved into a composition, drawn by
    /// this wrapping's layers.
    pub(crate) fn apply(&self, raw: &Json) -> Json {
        let mut scene = raw.as_object().cloned().unwrap_or_default();
        let fps = scene.get("fps").and_then(Json::as_f64).unwrap_or(30.0);
        let mut comp = Map::new();
        comp.insert(
            "size".into(),
            json!(format!("{}x{}", self.master.0, self.master.1)),
        );
        comp.insert("fps".into(), json!(fps));
        for key in ["dur", "shutter", "labels"] {
            if let Some(value) = scene.get(key) {
                comp.insert(key.into(), value.clone());
            }
        }
        comp.insert(
            "layers".into(),
            scene.remove("layers").unwrap_or_else(|| json!([])),
        );
        scene.remove("labels");
        let comps = scene
            .entry("comps")
            .or_insert_with(|| Json::Object(Map::new()));
        if let Some(comps) = comps.as_object_mut() {
            comps.insert(MASTER.into(), Json::Object(comp));
        }
        scene.insert(
            "size".into(),
            json!(format!("{}x{}", self.size.0, self.size.1)),
        );
        if let Some(map) = &self.map {
            scene.insert("dur".into(), json!(format!("{}f", map.frames.round())));
        }
        let mut layers = Vec::new();
        for (i, shown) in self.shown.iter().enumerate() {
            let mut layer = Map::new();
            layer.insert("id".into(), json!(format!("{SHOW}{}", i + 1)));
            layer.insert("comp".into(), json!(MASTER));
            layer.insert("anchor".into(), json!(shown.anchor.map(round)));
            layer.insert("position".into(), json!(shown.position.map(round)));
            if (shown.scale - 1.0).abs() > 1e-9 {
                layer.insert("scale".into(), json!(round(shown.scale)));
            }
            if !shown.hide.is_empty() {
                let overrides: Map<String, Json> = shown
                    .hide
                    .iter()
                    .map(|a| (a.clone(), json!({"hidden": true})))
                    .collect();
                layer.insert("overrides".into(), Json::Object(overrides));
            }
            if let Some(map) = &self.map {
                let keys: Vec<Json> = map
                    .keys
                    .iter()
                    .map(|&(frame, second)| json!([round(frame), round(second)]))
                    .collect();
                layer.insert("anim".into(), json!({"remap": keys}));
            }
            // One sound: the first layer's, unless a track replaces it or
            // the time map would bend it.
            if i > 0 || self.music.is_some() || self.map.is_some() {
                layer.insert("volume".into(), json!(0));
            }
            layers.push(Json::Object(layer));
        }
        if let Some(music) = &self.music {
            layers.push(json!({"id": MUSIC, "audio": music.to_string_lossy()}));
        }
        scene.insert("layers".into(), Json::Array(layers));
        self.readdress(&mut scene);
        Json::Object(scene)
    }

    /// Roles and locks that name a layer name it where the variant shows
    /// it: inside the first layer that does not hide it.
    fn readdress(&self, scene: &mut Map<String, Json>) {
        let at = |address: &str| -> String {
            let i = self
                .shown
                .iter()
                .position(|s| {
                    !s.hide
                        .iter()
                        .any(|h| address == h || address.starts_with(&format!("{h}/")))
                })
                .unwrap_or(0);
            format!("{SHOW}{}/{address}", i + 1)
        };
        if let Some(roles) = scene.get_mut("roles").and_then(Json::as_object_mut) {
            for target in roles.values_mut() {
                if let Some(t) = target.as_str().filter(|t| !t.starts_with('$')) {
                    *target = json!(at(t));
                }
            }
        }
        if let Some(locks) = scene.get_mut("locks").and_then(Json::as_array_mut) {
            for lock in locks.iter_mut() {
                if let Some(l) = lock
                    .as_str()
                    .filter(|l| !l.starts_with(['$', '@']) && !["palette", "typefaces"].contains(l))
                {
                    *lock = json!(at(l));
                }
            }
        }
    }

    /// The master's address of the variant's layer at `address`, and how
    /// much larger it is drawn; `None` for a layer the wrapping adds.
    pub(crate) fn read_back(&self, address: &Address) -> Option<(Address, f64)> {
        let (first, rest) = address.layers.split_first()?;
        let index: usize = first.as_ref().strip_prefix(SHOW)?.parse().ok()?;
        let shown = self.shown.get(index.checked_sub(1)?)?;
        if rest.is_empty() {
            return None;
        }
        let ids: Vec<Id> = rest.to_vec();
        Some((Address::layer(ids), shown.scale))
    }
}

/// `v` to six decimals, which the writer keeps.
fn round(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}
