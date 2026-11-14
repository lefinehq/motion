//! Walks the chunk tree into a [`Project`]: the only place that knows which
//! chunk goes where. A list is read as the desktop tool reads it, by name,
//! scanning forward from the last chunk read
//! ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listitem));
//! every chunk the scan passes over is counted in [`Project::skipped`].

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};

use motion_format::{Diagnostic, Kind, Result};

use crate::model::{
    Color, Composition, Footage, Item, ItemKind, Layer, Project, Source, Span, Switches, Version,
};
use crate::record::{
    self, Alias, Cdta, Head, Idta, Ldta, Nhed, PlaceholderOpti, Ratio, Record, SolidOpti, Sspc,
    text,
};
use crate::rifx::{self, Chunk, Name};

mod property;
mod text;

/// The root folder of the project's items.
const FOLD: Name = Name::list(b"Fold");
/// One item.
const ITEM: Name = Name::list(b"Item");
/// A folder's contents.
const SFDR: Name = Name::list(b"Sfdr");
/// One layer of a composition.
const LAYR: Name = Name::list(b"Layr");
/// A footage source.
const PIN: Name = Name::list(b"Pin ");
/// A file path, around its `alas`.
const ALS2: Name = Name::list(b"Als2");
/// A UTF-8 string, such as the name after an `idta`.
const UTF8: Name = Name::chunk(b"Utf8");
/// An importer's record, read as [`SolidOpti`] or [`PlaceholderOpti`].
const OPTI: Name = Name::chunk(b"opti");

/// The path of the root list in diagnostics; the paths below it leave it out.
const ROOT: &str = "RIFX";

/// How deep lists may nest before the file is refused, so a crafted file
/// cannot exhaust the stack.
const MAX_DEPTH: usize = 128;

/// Reads a whole `.aep` file into a [`Project`].
///
/// # Errors
/// A diagnostic, with the chunk path, when the file is not a RIFX project, a
/// chunk runs past the end of its list, or a required record is missing or
/// too short to read.
pub fn decode(file: &[u8]) -> Result<Project> {
    let root = rifx::read(file)?;
    let decoder = Decoder::default();
    let mut scan = Scan::new(
        &decoder,
        root.chunks().map_err(|d| d.at(ROOT))?,
        ROOT.into(),
        0,
    )?;
    let head: Head = scan.record()?;
    decoder.wrapped_text.set(head.major >= Head::WRAPPED_TEXT);
    let depth = scan.optional::<Nhed>()?.map(|nhed| nhed.depth);
    let mut fold = scan.require_list(FOLD)?;
    let mut items = decoder.items(&mut fold, None)?;
    name_layers(&mut items);
    drop((fold, scan));
    Ok(Project {
        version: Version {
            major: head.major,
            minor: head.minor,
        },
        bits_per_channel: depth.and_then(|d| Nhed::BITS.get(usize::from(d)).copied()),
        items,
        skipped: decoder.skipped.into_inner(),
    })
}

/// Gives each layer without a name of its own the name of the item it shows.
fn name_layers(items: &mut [Item]) {
    let names: HashMap<u32, String> = items.iter().map(|i| (i.id, i.name.clone())).collect();
    items
        .iter_mut()
        .filter_map(|item| match &mut item.kind {
            ItemKind::Composition(composition) => Some(&mut composition.layers),
            _ => None,
        })
        .flatten()
        .filter(|layer| layer.name.is_empty())
        .for_each(|layer| {
            layer.name = layer
                .source
                .and_then(|id| names.get(&id))
                .cloned()
                .unwrap_or_default();
        });
}

/// What the whole walk shares.
#[derive(Default)]
struct Decoder {
    /// Whether names are `Utf8` chunks inside their wrapper, as from
    /// [`Head::WRAPPED_TEXT`] on, rather than NUL-terminated bytes.
    wrapped_text: Cell<bool>,
    /// The chunks passed over so far, by name.
    skipped: RefCell<BTreeMap<String, usize>>,
}

impl Decoder {
    /// Counts chunks called `names` as skipped.
    fn skip(&self, names: impl IntoIterator<Item = Name>) {
        let mut skipped = self.skipped.borrow_mut();
        names
            .into_iter()
            .for_each(|name| *skipped.entry(name.to_string()).or_default() += 1);
    }

    /// The items of a folder list, each folder followed by its contents.
    fn items(&self, folder_list: &mut Scan<'_, '_>, folder: Option<u32>) -> Result<Vec<Item>> {
        let nested = folder_list
            .lists(ITEM)
            .map(|item| self.item(item?, folder))
            .collect::<Result<Vec<_>>>()?;
        Ok(nested.concat())
    }

    /// One item (and, for a folder, its contents), or none for an item type
    /// the reader does not know.
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listitem)).
    fn item(&self, mut scan: Scan<'_, '_>, folder: Option<u32>) -> Result<Vec<Item>> {
        let idta: Idta = scan.record()?;
        let name = scan.find(UTF8).map(|c| text(c.body)).unwrap_or_default();
        let item = |name: String, kind| Item {
            id: idta.id,
            name,
            folder,
            kind,
        };
        match idta.kind {
            Idta::FOLDER => {
                let contents = match scan.list(SFDR)? {
                    Some(mut list) => self.items(&mut list, Some(idta.id))?,
                    None => Vec::new(),
                };
                Ok([vec![item(name, ItemKind::Folder)], contents].concat())
            }
            Idta::COMPOSITION => {
                let composition = self.composition(&mut scan)?;
                Ok(vec![item(name, ItemKind::Composition(composition))])
            }
            Idta::FOOTAGE => {
                let (footage, own_name) = self.footage(scan.require_list(PIN)?)?;
                let name = if name.is_empty() { own_name } else { name };
                Ok(vec![item(name, ItemKind::Footage(footage))])
            }
            _ => {
                self.skip([ITEM]);
                Ok(Vec::new())
            }
        }
    }

    /// A composition's settings
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#cdta)).
    fn composition(&self, scan: &mut Scan<'_, '_>) -> Result<Composition> {
        let cdta: Cdta = scan.record()?;
        let duration = cdta.duration.value();
        let [r, g, b] = cdta.background.map(|channel| f64::from(channel) / 255.0);
        Ok(Composition {
            width: cdta.width,
            height: cdta.height,
            pixel_aspect: cdta.pixel_aspect.value(),
            frame_rate: cdta.frame_rate,
            duration,
            background: Color { r, g, b, a: 1.0 },
            work_area: Span {
                start: cdta.work_start.value(),
                end: match cdta.work_end.num {
                    Cdta::TO_END => duration,
                    _ => cdta.work_end.value(),
                },
            },
            layers: scan
                .lists(LAYR)
                .map(|layer| self.layer(layer?))
                .collect::<Result<_>>()?,
        })
    }

    /// One layer: its record and its name
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#ldta)).
    fn layer(&self, mut scan: Scan<'_, '_>) -> Result<Layer> {
        let ldta: Ldta = scan.record()?;
        let name = scan.find(UTF8).map(|c| text(c.body)).unwrap_or_default();
        let stretch = Ratio {
            num: ldta.stretch_num,
            den: ldta.stretch_den,
        }
        .value();
        let start_time = ldta.start.value();
        let timing = property::Timing {
            start: start_time,
            stretch,
        };
        let id = |id: u32| (id != 0 && id != u32::MAX).then_some(id);
        Ok(Layer {
            id: ldta.id,
            name,
            kind: ldta.kind.into(),
            source: id(ldta.source),
            parent: id(ldta.parent),
            in_point: start_time + ldta.in_point.value() * stretch,
            out_point: start_time + ldta.out_point.value() * stretch,
            start_time,
            stretch,
            switches: Switches {
                enabled: ldta.is_on(Ldta::ENABLED),
                three_d: ldta.is_on(Ldta::THREE_D),
                solo: ldta.is_on(Ldta::SOLO),
                shy: ldta.is_on(Ldta::SHY),
                guide: ldta.is_on(Ldta::GUIDE),
                motion_blur: ldta.is_on(Ldta::MOTION_BLUR),
                null: ldta.is_on(Ldta::NULL),
                adjustment: ldta.is_on(Ldta::ADJUSTMENT),
            },
            blend_mode: ldta.blend_mode.into(),
            track_matte: ldta.track_matte.into(),
            matte_layer: ldta.matte_layer.and_then(id),
            properties: self.group(scan.require_list(property::TDGP)?, String::new(), timing)?,
        })
    }

    /// A footage item's main source, and the name it gives the item
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#source-kinds)).
    fn footage(&self, mut pin: Scan<'_, '_>) -> Result<(Footage, String)> {
        let sspc: Sspc = pin.record()?;
        let path = pin
            .list(ALS2)?
            .and_then(|mut als2| als2.find(Alias::NAME))
            .map(|alas| {
                serde_json::from_slice::<Alias>(alas.body)
                    .ok()
                    .map(|a| a.fullpath)
            });
        let opti = pin.find(OPTI);
        let (source, name) = match (path, opti) {
            (Some(path), _) => {
                let file_name = path
                    .as_deref()
                    .and_then(|p| p.rsplit(['/', '\\']).next())
                    .unwrap_or_default()
                    .to_owned();
                (Source::File { path }, file_name)
            }
            (None, Some(opti)) if sspc.code == Sspc::SOLID => {
                let solid: SolidOpti = pin.parse(opti, ())?;
                let [a, r, g, b] = solid.argb.map(f64::from);
                (
                    Source::Solid {
                        color: Color { r, g, b, a },
                    },
                    solid.name,
                )
            }
            (None, opti) => {
                let name = opti
                    .map(|opti| pin.parse::<PlaceholderOpti>(opti, ()))
                    .transpose()?
                    .map(|record| record.name);
                (Source::Placeholder, name.unwrap_or_default())
            }
        };
        let frame_rate = match sspc.conform_rate {
            0.0 => sspc.native_rate,
            conform => conform,
        };
        Ok((
            Footage {
                width: sspc.width,
                height: sspc.height,
                pixel_aspect: sspc.pixel_aspect.value(),
                frame_rate,
                duration: sspc.duration.value(),
                source,
            },
            name,
        ))
    }
}

/// The children of one list, read forward by name. Chunks the scan passes
/// over, and those it never reaches, are counted as skipped.
struct Scan<'a, 'd> {
    /// The walk this list belongs to.
    decoder: &'d Decoder,
    /// The list's children.
    chunks: Vec<Chunk<'a>>,
    /// The first child not read or passed over yet.
    next: usize,
    /// Where the list is, as `LIST:Fold/LIST:Item[3]`, for diagnostics.
    path: String,
    /// How many lists enclose this one.
    depth: usize,
}

impl<'a, 'd> Scan<'a, 'd> {
    /// A scan over `chunks`, the children of the list at `path`.
    fn new(
        decoder: &'d Decoder,
        chunks: Vec<Chunk<'a>>,
        path: String,
        depth: usize,
    ) -> Result<Self> {
        match depth {
            MAX_DEPTH.. => Err(Diagnostic::new(
                Kind::Invalid,
                format!("lists nest more than {MAX_DEPTH} deep"),
            )
            .at(path)
            .expected("a shallower project")),
            _ => Ok(Self {
                decoder,
                chunks,
                next: 0,
                path,
                depth,
            }),
        }
    }

    /// The path of a child called `name`.
    fn child_path(&self, name: impl std::fmt::Display) -> String {
        match (self.depth, self.path.as_str()) {
            (0, _) => name.to_string(),
            (_, path) => format!("{path}/{name}"),
        }
    }

    /// The next chunk called `name`, counting the chunks before it as
    /// skipped; `None`, and nothing passed over, when there is none.
    fn find(&mut self, name: Name) -> Option<Chunk<'a>> {
        let rest = self.chunks.get(self.next..).unwrap_or_default();
        let offset = rest.iter().position(|chunk| chunk.name == name)?;
        let found = rest.get(offset).copied();
        self.decoder.skip(rest.iter().take(offset).map(|c| c.name));
        self.next += offset + 1;
        found
    }

    /// The next chunk called `name`.
    ///
    /// # Errors
    /// A [`Kind::Missing`] diagnostic when there is none.
    fn require(&mut self, name: Name) -> Result<Chunk<'a>> {
        self.find(name).ok_or_else(|| {
            Diagnostic::new(Kind::Missing, format!("no `{name}` where one is required"))
                .at(self.path.clone())
                .expected(format!("a `{name}` chunk"))
        })
    }

    /// Reads `chunk` as a `T`, naming its path in an error.
    fn parse<T: binrw::BinRead>(&self, chunk: Chunk<'a>, args: T::Args<'_>) -> Result<T> {
        record::parse(chunk.body, chunk.name, args).map_err(|d| d.at(self.child_path(chunk.name)))
    }

    /// The next `R` record.
    fn record<R: Record>(&mut self) -> Result<R> {
        let chunk = self.require(R::NAME)?;
        self.parse(chunk, ())
    }

    /// The next `R` record, if there is one.
    fn optional<R: Record>(&mut self) -> Result<Option<R>> {
        self.find(R::NAME)
            .map(|chunk| self.parse(chunk, ()))
            .transpose()
    }

    /// A scan over the next list called `name`, if there is one.
    fn list(&mut self, name: Name) -> Result<Option<Scan<'a, 'd>>> {
        self.find(name)
            .map(|chunk| self.open(chunk, self.child_path(name)))
            .transpose()
    }

    /// A scan over the next list called `name`.
    fn require_list(&mut self, name: Name) -> Result<Scan<'a, 'd>> {
        let chunk = self.require(name)?;
        self.open(chunk, self.child_path(name))
    }

    /// Scans over every remaining list called `name`, each with its index in
    /// its path (`LIST:Item[3]`).
    fn lists(&mut self, name: Name) -> impl Iterator<Item = Result<Scan<'a, 'd>>> {
        (0..).map_while(move |index| {
            let chunk = self.find(name)?;
            Some(self.open(chunk, self.child_path(format_args!("{name}[{index}]"))))
        })
    }

    /// The remaining children, split into runs that each start with a chunk
    /// called `separator`, such as a group's `tdmn` and the body after it.
    /// Chunks before the first separator are skipped.
    fn children(&mut self, separator: Name) -> Vec<(Chunk<'a>, Vec<Chunk<'a>>)> {
        let rest = self.chunks.get(self.next..).unwrap_or_default();
        let runs = rest
            .chunk_by(|_, next| next.name != separator)
            .filter_map(|run| match run.split_first() {
                Some((first, body)) if first.name == separator => Some((*first, body.to_vec())),
                _ => {
                    self.decoder.skip(run.iter().map(|chunk| chunk.name));
                    None
                }
            })
            .collect();
        self.next = self.chunks.len();
        runs
    }

    /// A scan over the children of `chunk`, found at `path`.
    fn open(&self, chunk: Chunk<'a>, path: String) -> Result<Scan<'a, 'd>> {
        let chunks = chunk.chunks().map_err(|d| d.at(path.clone()))?;
        Scan::new(self.decoder, chunks, path, self.depth + 1)
    }
}

impl Drop for Scan<'_, '_> {
    fn drop(&mut self) {
        let rest = self.chunks.get(self.next..).unwrap_or_default();
        self.decoder.skip(rest.iter().map(|c| c.name));
    }
}

#[cfg(test)]
#[path = "../tests/unit/decode.rs"]
mod tests;
