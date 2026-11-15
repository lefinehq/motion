//! Helpers shared by the unit tests: the fixture font, copies of it whose
//! `name` table says what a test needs, and font collections.

use skrifa::raw::tables::name::Name;
use skrifa::raw::{FontRef, TopLevelTable};

/// The raster crate's fixture font, an OFL subset of Lato Black whose
/// PostScript name is `FixtureSans-Black`.
pub(crate) const FIXTURE: &[u8] =
    include_bytes!("../../../motion-raster/tests/fixtures/FixtureSans-Black.ttf");

/// A font's tables, each as its tag and its bytes.
type Tables = Vec<([u8; 4], Vec<u8>)>;

/// The fixture with a `name` table of only `names`, each `(name ID, text)`,
/// as Windows English (US) Unicode names: the fixture's outlines under the
/// names a test gives them.
pub(crate) fn with_names(names: &[(u16, &str)]) -> Vec<u8> {
    let (version, mut tables) = tables_of(FIXTURE);
    let name = tables
        .iter_mut()
        .find(|(tag, _)| *tag == Name::TAG.to_be_bytes())
        .expect("the fixture has names");
    name.1 = name_table(names);
    sfnt(0, version, &tables)
}

/// A font collection of `fonts`, each a lone font, in order.
pub(crate) fn collection(fonts: &[&[u8]]) -> Vec<u8> {
    let count = u32::try_from(fonts.len()).expect("few fonts");
    let mut out = Vec::new();
    out.extend(b"ttcf");
    out.extend(0x0001_0000u32.to_be_bytes());
    out.extend(count.to_be_bytes());
    let mut bodies = Vec::new();
    let mut base = 12 + 4 * fonts.len();
    for font in fonts {
        out.extend(
            u32::try_from(base)
                .expect("a small collection")
                .to_be_bytes(),
        );
        let (version, tables) = tables_of(font);
        let body = sfnt(base, version, &tables);
        base += body.len();
        bodies.push(body);
    }
    out.extend(bodies.concat());
    out
}

/// The version and tables of the lone font `bytes`.
fn tables_of(bytes: &[u8]) -> (u32, Tables) {
    let font = FontRef::new(bytes).expect("a font");
    let tables = font
        .table_directory
        .table_records()
        .iter()
        .map(|record| {
            let data = font.table_data(record.tag()).expect("a listed table");
            (record.tag().to_be_bytes(), data.as_bytes().to_vec())
        })
        .collect();
    (font.table_directory.sfnt_version(), tables)
}

/// A `name` table, format 0, of `names`.
fn name_table(names: &[(u16, &str)]) -> Vec<u8> {
    let mut sorted = names.to_vec();
    sorted.sort_by_key(|(id, _)| *id);
    let count = u16::try_from(sorted.len()).expect("few names");
    let mut head = Vec::new();
    let mut strings = Vec::new();
    head.extend(0u16.to_be_bytes());
    head.extend(count.to_be_bytes());
    head.extend((6 + 12 * count).to_be_bytes());
    for (id, text) in sorted {
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        // Windows, Unicode BMP, English (US).
        for field in [3, 1, 0x409, id] {
            head.extend(u16::to_be_bytes(field));
        }
        head.extend(
            u16::try_from(utf16.len())
                .expect("a short name")
                .to_be_bytes(),
        );
        head.extend(
            u16::try_from(strings.len())
                .expect("short names")
                .to_be_bytes(),
        );
        strings.extend(utf16);
    }
    head.extend(strings);
    head
}

/// A font of `tables`, each 4-byte aligned, to be placed at `base` in its
/// file (a collection's offsets count from the file's start). Checksums are
/// left zero: nothing the tests read checks them.
fn sfnt(base: usize, version: u32, tables: &Tables) -> Vec<u8> {
    let count = u16::try_from(tables.len()).expect("few tables");
    let power = 1u16 << (15 - count.leading_zeros());
    let mut out = Vec::new();
    out.extend(version.to_be_bytes());
    for field in [
        count,
        power * 16,
        power.trailing_zeros() as u16,
        (count - power) * 16,
    ] {
        out.extend(field.to_be_bytes());
    }
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, data) in tables {
        out.extend(tag);
        out.extend(0u32.to_be_bytes());
        out.extend(
            u32::try_from(base + offset)
                .expect("a small font")
                .to_be_bytes(),
        );
        out.extend(
            u32::try_from(data.len())
                .expect("a small table")
                .to_be_bytes(),
        );
        body.extend(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    out.extend(body);
    out
}
