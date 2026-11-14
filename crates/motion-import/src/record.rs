//! The fixed binary records of the format, as `binrw` structs. Each reads the
//! fields motion uses, at the offsets of the spec's table, and skips the rest.
//! [`parse`] is the one place a `binrw` error becomes a [`Diagnostic`].

use std::fmt;
use std::io::{Cursor, SeekFrom};

use binrw::BinRead;
use binrw::helpers::until_eof;
use motion_format::{Diagnostic, Kind, Result};

use crate::rifx::{Name, Tag};

/// A record that is the whole body of one kind of chunk.
pub(crate) trait Record: for<'a> BinRead<Args<'a> = ()> {
    /// The chunk that holds it.
    const NAME: Name;
}

/// Reads a `T` from the start of `bytes`, big-endian unless `T` says otherwise.
/// `what` names the record in the error.
///
/// # Errors
/// A diagnostic when `bytes` is too short for the record, or a field does
/// not hold one of its allowed values.
pub(crate) fn parse<'a, T: BinRead>(
    bytes: &[u8],
    what: impl fmt::Display,
    args: T::Args<'a>,
) -> Result<T> {
    T::read_be_args(&mut Cursor::new(bytes), args).map_err(|error| {
        if error.is_eof() {
            Diagnostic::new(
                Kind::Invalid,
                format!("`{what}` holds {} bytes, too few to read", bytes.len()),
            )
            .expected(format!("a whole `{what}` record"))
        } else {
            Diagnostic::new(
                Kind::Invalid,
                format!("cannot read `{what}`: {}", error.root_cause()),
            )
        }
    })
}

/// Text up to its first NUL byte, as UTF-8 with bad bytes replaced: the
/// `Utf8` strings and the fixed, NUL-padded names inside records.
pub(crate) fn text(bytes: &[u8]) -> String {
    let until_nul = bytes.split(|&b| b == 0).next().unwrap_or_default();
    String::from_utf8_lossy(until_nul).into_owned()
}

/// The offset of a field, as the spec's tables give it.
const fn at(offset: u64) -> SeekFrom {
    SeekFrom::Start(offset)
}

/// A `fixed 16.16` number: a whole part and a fraction in 1/65536 units.
fn fixed(raw: u32) -> f64 {
    f64::from(raw) / 65536.0
}

/// A signed `fixed 16.16` number, as the effect parameter records store them.
fn signed_fixed(raw: i32) -> f64 {
    f64::from(raw) / 65536.0
}

/// A time or a ratio stored as a dividend and a divisor.
#[derive(BinRead, Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ratio<N>
where
    N: for<'a> BinRead<Args<'a> = ()> + 'static,
{
    /// The dividend.
    pub num: N,
    /// The divisor.
    pub den: u32,
}

impl<N: for<'a> BinRead<Args<'a> = ()> + Into<f64> + Copy> Ratio<N> {
    /// The ratio's value; 0 when the divisor is 0, which the file never means.
    pub fn value(self) -> f64 {
        match self.den {
            0 => 0.0,
            den => self.num.into() / f64::from(den),
        }
    }
}

/// The 8-byte header in front of every chunk body
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#chunks)).
#[derive(BinRead, Debug)]
pub(crate) struct Header {
    /// The chunk tag.
    pub tag: Tag,
    /// The body length, without the header and the pad byte.
    pub size: u32,
}

impl Header {
    /// The header's length in bytes.
    pub const SIZE: usize = 8;
}

/// `head`: the file version
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#head)).
#[derive(BinRead, Debug)]
pub(crate) struct Head {
    /// The major file version.
    pub major: u16,
    /// The minor file version.
    pub minor: u16,
}

impl Record for Head {
    const NAME: Name = Name::chunk(b"head");
}

impl Head {
    /// The first major version whose names are wrapped in a `Utf8` chunk and
    /// whose expressions are `Utf8`, not `expr`
    /// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#expr)).
    pub const WRAPPED_TEXT: u16 = 92;
}

/// `nhed`: project settings; motion reads the colour depth
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#nhed)).
/// Every file version has `nhed`; older ones lack its copy, `nnhd`.
#[derive(BinRead, Debug)]
pub(crate) struct Nhed {
    /// 0, 1 or 2 for 8, 16 or 32 bits per channel.
    #[br(seek_before = at(0x0F))]
    pub depth: u8,
}

impl Record for Nhed {
    const NAME: Name = Name::chunk(b"nhed");
}

impl Nhed {
    /// Bits per channel, by depth code.
    pub const BITS: [u8; 3] = [8, 16, 32];
}

/// `idta`: an item's type and id
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#idta)).
#[derive(BinRead, Debug)]
pub(crate) struct Idta {
    /// [`Idta::FOLDER`], [`Idta::COMPOSITION`] or [`Idta::FOOTAGE`].
    pub kind: u16,
    /// The item id.
    #[br(seek_before = at(0x10))]
    pub id: u32,
}

impl Record for Idta {
    const NAME: Name = Name::chunk(b"idta");
}

impl Idta {
    /// The type of a folder.
    pub const FOLDER: u16 = 1;
    /// The type of a composition.
    pub const COMPOSITION: u16 = 4;
    /// The type of a footage item.
    pub const FOOTAGE: u16 = 7;
}

/// `cdta`: a composition's settings
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#cdta)).
#[derive(BinRead, Debug)]
pub(crate) struct Cdta {
    /// Work area start, from the composition's start.
    #[br(seek_before = at(0x1C))]
    pub work_start: Ratio<i32>,
    /// Work area end; a dividend of [`Cdta::TO_END`] means the composition's end.
    pub work_end: Ratio<u32>,
    /// Duration.
    pub duration: Ratio<u32>,
    /// Background red, green and blue, 0 to 255.
    pub background: [u8; 3],
    /// Width in pixels.
    #[br(seek_before = at(0x8C))]
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// Pixel aspect ratio.
    pub pixel_aspect: Ratio<u32>,
    /// Frames per second.
    #[br(seek_before = at(0x9C), map = fixed)]
    pub frame_rate: f64,
}

impl Record for Cdta {
    const NAME: Name = Name::chunk(b"cdta");
}

impl Cdta {
    /// The work area end that means "the end of the composition".
    pub const TO_END: u32 = u32::MAX;
}

/// `ldta`: a layer's id, timing, switches, blending and links
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#ldta)).
/// The record is 160 bytes before file version 94 and 164 from it on; only
/// the longer form stores a track matte layer.
#[derive(BinRead, Debug)]
pub(crate) struct Ldta {
    /// The layer id.
    pub id: u32,
    /// Stretch dividend; the divisor is [`Ldta::stretch_den`].
    #[br(seek_before = at(0x08))]
    pub stretch_num: i32,
    /// Where the layer's time 0 falls, in composition time.
    pub start: Ratio<i32>,
    /// In point, in layer time.
    pub in_point: Ratio<i32>,
    /// Out point, in layer time.
    pub out_point: Ratio<i32>,
    /// Layer flags A, B and C; the masks are [`Ldta::GUIDE`] and the others.
    #[br(seek_before = at(0x25))]
    pub flags: [u8; 3],
    /// The source item's id; 0 or `u32::MAX` for none.
    pub source: u32,
    /// The blending mode's code.
    #[br(seek_before = at(0x63))]
    pub blend_mode: u8,
    /// The track matte type's code.
    #[br(seek_before = at(0x6B))]
    pub track_matte: u8,
    /// Stretch divisor.
    pub stretch_den: u32,
    /// The layer type's code.
    #[br(seek_before = at(0x83))]
    pub kind: u8,
    /// The parent layer's id; 0 for none.
    pub parent: u32,
    /// The track matte layer's id, in the 164-byte form; 0 for none.
    #[br(seek_before = at(0xA0), try)]
    pub matte_layer: Option<u32>,
}

impl Record for Ldta {
    const NAME: Name = Name::chunk(b"ldta");
}

/// The switches of [`Ldta::flags`], each a flag byte's index and mask.
impl Ldta {
    /// Guide layer, in flags A.
    pub const GUIDE: (usize, u8) = (0, 0x02);
    /// Adjustment layer, in flags B.
    pub const ADJUSTMENT: (usize, u8) = (1, 0x02);
    /// 3D layer, in flags B.
    pub const THREE_D: (usize, u8) = (1, 0x04);
    /// Solo, in flags B.
    pub const SOLO: (usize, u8) = (1, 0x08);
    /// Null layer, in flags B.
    pub const NULL: (usize, u8) = (1, 0x80);
    /// Video enabled, in flags C.
    pub const ENABLED: (usize, u8) = (2, 0x01);
    /// Motion blur, in flags C.
    pub const MOTION_BLUR: (usize, u8) = (2, 0x08);
    /// Shy, in flags C.
    pub const SHY: (usize, u8) = (2, 0x40);

    /// Whether the switch at `(byte, mask)` is on.
    pub fn is_on(&self, (byte, mask): (usize, u8)) -> bool {
        self.flags.get(byte).is_some_and(|flags| flags & mask != 0)
    }
}

/// `tdsb`: the flags of a property group or stream
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#tdsb)).
#[derive(BinRead, Debug)]
pub(crate) struct Tdsb {
    /// The flag word.
    pub flags: u32,
}

impl Record for Tdsb {
    const NAME: Name = Name::chunk(b"tdsb");
}

impl Tdsb {
    /// Whether the group or stream applies.
    pub fn enabled(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Whether the property is hidden in the timeline because it does not
    /// apply to the layer as it is: a 3D-only property on a 2D layer, the
    /// unused half of a separable position.
    pub fn hidden(&self) -> bool {
        self.flags & 0x02 != 0
    }

    /// Whether a separable property, `ADBE Position`, has its dimensions
    /// separated into streams of their own.
    pub fn separated(&self) -> bool {
        self.flags & 0x800 != 0
    }
}

/// `tdb4`: a stream's dimensions, kind and time base
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#tdb4)).
#[derive(BinRead, Debug)]
pub(crate) struct Tdb4 {
    /// Values per keyframe: 1 to 3, or 4 for a colour.
    #[br(seek_before = at(0x02))]
    pub dims: u16,
    /// Stream flags: [`Tdb4::SINGLE_EASE`] and [`Tdb4::SPATIAL`].
    pub flags: u16,
    /// Keyframe time units per second of the layer's time.
    #[br(seek_before = at(0x0C))]
    pub time_base: u32,
    /// Bit 0: the value lives in a container next to the stream.
    #[br(seek_before = at(0x38))]
    pub value_flags: u16,
    /// Bit 0: the value is a colour.
    pub type_flags: u16,
    /// Bit 0: the expression is switched off.
    #[br(seek_before = at(0x77))]
    pub expression_flags: u8,
}

impl Record for Tdb4 {
    const NAME: Name = Name::chunk(b"tdb4");
}

impl Tdb4 {
    /// Stream flag: one ease for the whole value, and tangents per dimension.
    pub const SINGLE_EASE: u16 = 0x02;
    /// Stream flag: the values form a motion path.
    pub const SPATIAL: u16 = 0x08;

    /// How the stream's values and keyframes are laid out.
    pub fn layout(&self) -> Layout {
        match (
            self.value_flags & 0x01 != 0,
            self.flags & Self::SINGLE_EASE != 0,
        ) {
            (false, false) => Layout::PerDimension,
            (false, true) => Layout::SingleEase,
            (true, true) => Layout::SingleEaseValueless,
            (true, false) => Layout::Valueless,
        }
    }

    /// Whether the value is a colour.
    pub fn is_color(&self) -> bool {
        self.type_flags & 0x01 != 0
    }

    /// Whether the values form a motion path, with spatial tangents.
    pub fn is_spatial(&self) -> bool {
        self.flags & Self::SPATIAL != 0
    }

    /// Whether the stream's expression, if any, is switched on.
    pub fn expression_enabled(&self) -> bool {
        self.expression_flags & 0x01 == 0
    }
}

/// `lhd3`: the header of a `LIST:list`
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#lhd3)).
#[derive(BinRead, Debug)]
pub(crate) struct Lhd3 {
    /// How many items the `ldat` holds.
    #[br(seek_before = at(0x08))]
    pub count: u32,
    /// The size of one item in bytes.
    #[br(seek_before = at(0x10))]
    pub item_size: u32,
}

impl Record for Lhd3 {
    const NAME: Name = Name::chunk(b"lhd3");
}

/// One keyframe item of an `ldat`: its time, interpolation and flags, then a
/// body laid out by the stream's [`Layout`]
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#keyframe-items)).
#[derive(BinRead, Debug)]
#[br(import(dims: usize, layout: Layout))]
pub(crate) struct KeyItem {
    /// The time in the stream's time base, from the layer's start.
    pub time: i32,
    /// The in interpolation's code.
    pub in_interpolation: u8,
    /// The out interpolation's code.
    pub out_interpolation: u8,
    /// Key flags: [`KeyItem::CONTINUOUS`] and the others.
    pub flags: u16,
    /// The ease and the value.
    #[br(args(dims, layout))]
    pub body: KeyBody,
}

impl KeyItem {
    /// Key flag: the out ease follows the in ease.
    pub const CONTINUOUS: u16 = 0x08;
    /// Key flag: the ease is computed from the neighbours.
    pub const AUTO_BEZIER: u16 = 0x10;
    /// Key flag: the time is derived from the motion path.
    pub const ROVING: u16 = 0x20;
    /// Spatial flag: the path runs smoothly through the keyframe.
    pub const SPATIAL_CONTINUOUS: u32 = 0x01;
    /// Spatial flag: the tangents are computed from the neighbours.
    pub const SPATIAL_AUTO_BEZIER: u32 = 0x02;
}

/// The part of a keyframe item after its first 8 bytes, one variant per
/// [`Layout`].
#[derive(BinRead, Debug)]
#[br(import(dims: usize, layout: Layout))]
pub(crate) enum KeyBody {
    /// The value, then each ease part for every dimension in turn.
    #[br(pre_assert(layout == Layout::PerDimension))]
    PerDimension {
        /// The value.
        #[br(count = dims)]
        value: Vec<f64>,
        /// In speed per dimension.
        #[br(count = dims)]
        in_speed: Vec<f64>,
        /// In influence per dimension.
        #[br(count = dims)]
        in_influence: Vec<f64>,
        /// Out speed per dimension.
        #[br(count = dims)]
        out_speed: Vec<f64>,
        /// Out influence per dimension.
        #[br(count = dims)]
        out_influence: Vec<f64>,
    },
    /// One ease, then the value and its spatial tangents.
    #[br(pre_assert(layout == Layout::SingleEase))]
    SingleEase {
        /// Spatial flags: [`KeyItem::SPATIAL_CONTINUOUS`] and the other.
        spatial_flags: u32,
        /// The ease, after a reserved word and the segment's length.
        #[br(pad_before = 12)]
        ease: OneEase,
        /// The value.
        #[br(count = dims)]
        value: Vec<f64>,
        /// The tangent into the keyframe.
        #[br(count = dims)]
        in_tangent: Vec<f64>,
        /// The tangent out of the keyframe.
        #[br(count = dims)]
        out_tangent: Vec<f64>,
    },
    /// One ease; the value lives in a container next to the stream.
    #[br(pre_assert(layout == Layout::SingleEaseValueless))]
    SingleEaseValueless {
        /// The ease, after the spatial flags, a reserved word and the length.
        #[br(pad_before = 16)]
        ease: OneEase,
    },
    /// Timing only; the value lives in a container next to the stream.
    #[br(pre_assert(layout == Layout::Valueless))]
    Valueless,
}

/// The one temporal ease of a single-ease keyframe item.
#[derive(BinRead, Debug)]
pub(crate) struct OneEase {
    /// In speed.
    pub in_speed: f64,
    /// In influence, from 0 to 1.
    pub in_influence: f64,
    /// Out speed.
    pub out_speed: f64,
    /// Out influence, from 0 to 1.
    pub out_influence: f64,
}

/// How a stream lays out its values, from [`Tdb4::layout`]
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#stream-kinds)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Layout {
    /// Numbers: the value, then an ease per dimension.
    PerDimension,
    /// Spatial values and colours: one ease, then the value and its tangents.
    SingleEase,
    /// Paths, gradients and orientation: one ease; the value lives elsewhere.
    SingleEaseValueless,
    /// Text documents and markers: timing only; the value lives elsewhere.
    Valueless,
}

/// `cdat`: a stream's static value, as big-endian doubles
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#cdat)).
/// The value is the first `dims` of them.
#[derive(BinRead, Debug)]
pub(crate) struct Cdat {
    /// Every double in the chunk.
    #[br(parse_with = until_eof)]
    pub numbers: Vec<f64>,
}

impl Record for Cdat {
    const NAME: Name = Name::chunk(b"cdat");
}

/// `tdum` and `tduM`: a stream's minimum and maximum, one double each
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#tdum)).
#[derive(BinRead, Debug)]
pub(crate) struct Bound {
    /// The bound.
    pub value: f64,
}

impl Bound {
    /// The chunk of a minimum.
    pub const MIN: Name = Name::chunk(b"tdum");
    /// The chunk of a maximum.
    pub const MAX: Name = Name::chunk(b"tduM");
}

/// `shph`: a path's flags and the box its points are normalized to
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#shph)).
#[derive(BinRead, Debug)]
pub(crate) struct Shph {
    /// Bit 3: the path is open.
    #[br(seek_before = at(0x03))]
    pub flags: u8,
    /// The box's left, top, right and bottom edges.
    pub bounds: [f32; 4],
}

impl Record for Shph {
    const NAME: Name = Name::chunk(b"shph");
}

impl Shph {
    /// Whether the path's last vertex joins its first.
    pub fn closed(&self) -> bool {
        self.flags & 0x08 == 0
    }

    /// The point at `[x, y]`, normalized to the box, in the path's space.
    pub fn point(&self, [x, y]: [f32; 2]) -> [f64; 2] {
        let [left, top, right, bottom] = self.bounds.map(f64::from);
        [
            left + f64::from(x) * (right - left),
            top + f64::from(y) * (bottom - top),
        ]
    }
}

/// The `ldat` of a path's `LIST:list`: points, three per vertex (the vertex,
/// its outgoing control point, the next vertex's incoming one), each an x
/// and a y normalized to the [`Shph`] box
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#vertex-items)).
#[derive(BinRead, Debug)]
pub(crate) struct Points {
    /// Every point in the chunk.
    #[br(parse_with = until_eof)]
    pub points: Vec<[f32; 2]>,
}

impl Record for Points {
    const NAME: Name = Name::chunk(b"ldat");
}

/// `otda`: one orientation value, X, Y and Z angles in degrees, big-endian
/// unlike their little-endian copies in the stream
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#otda)).
#[derive(BinRead, Debug)]
pub(crate) struct Otda {
    /// The angles.
    pub angles: [f64; 3],
}

impl Record for Otda {
    const NAME: Name = Name::chunk(b"otda");
}

/// `sspc`: a footage source's cached media settings
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#sspc)).
/// The fields motion reads sit at the same offsets in the 222-byte form and in
/// the 286-byte form of older files.
#[derive(BinRead, Debug)]
pub(crate) struct Sspc {
    /// The importer's format code: [`Sspc::SOLID`] for a solid.
    #[br(seek_before = at(0x16))]
    pub code: Tag,
    /// Width in pixels.
    #[br(seek_before = at(0x1E))]
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Duration.
    pub duration: Ratio<u32>,
    /// The media's own frame rate.
    #[br(seek_before = at(0x3A), map = fixed)]
    pub native_rate: f64,
    /// Pixel aspect ratio.
    #[br(seek_before = at(0x88))]
    pub pixel_aspect: Ratio<u32>,
    /// The frame rate the footage is interpreted at; 0 for the native rate.
    #[br(seek_before = at(0x94), map = fixed)]
    pub conform_rate: f64,
}

impl Record for Sspc {
    const NAME: Name = Name::chunk(b"sspc");
}

impl Sspc {
    /// The format code of a solid.
    pub const SOLID: Tag = Tag(*b"Soli");
}

/// The `opti` record of a solid: its colour and name
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#solid-record-soli)).
/// The name field is 256 bytes, or 64 in the older 90-byte record.
#[derive(BinRead, Debug)]
pub(crate) struct SolidOpti {
    /// Alpha, red, green and blue, 0 to 1.
    #[br(seek_before = at(0x0A))]
    pub argb: [f32; 4],
    /// The solid's name, NUL-padded.
    #[br(parse_with = until_eof, map = |bytes: Vec<u8>| text(&bytes))]
    pub name: String,
}

impl Record for SolidOpti {
    const NAME: Name = Name::chunk(b"opti");
}

/// The `opti` record of a placeholder: its name
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#placeholder-record)).
#[derive(BinRead, Debug)]
pub(crate) struct PlaceholderOpti {
    /// The placeholder's name, NUL-padded.
    #[br(seek_before = at(0x0A), parse_with = until_eof, map = |bytes: Vec<u8>| text(&bytes))]
    pub name: String,
}

impl Record for PlaceholderOpti {
    const NAME: Name = Name::chunk(b"opti");
}

/// The JSON path record of an `alas` chunk; motion reads the full path
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#alas)).
#[derive(serde::Deserialize, Debug)]
pub(crate) struct Alias {
    /// The absolute path of the file, or of an image sequence's folder.
    pub fullpath: String,
}

impl Alias {
    /// The chunk that holds it.
    pub const NAME: Name = Name::chunk(b"alas");
}

#[cfg(test)]
#[path = "../tests/unit/record.rs"]
mod tests;

/// `EfDC`: how many effect definitions a `LIST:EfdG` holds
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#efdc)).
#[derive(BinRead, Debug)]
pub(crate) struct Efdc {
    /// The number of `LIST:EfDf` definitions; little-endian, unlike the
    /// records around it.
    #[br(little)]
    pub count: u32,
}

impl Record for Efdc {
    const NAME: Name = Name::chunk(b"EfDC");
}

/// `parn`: how many parameters a `LIST:parT` defines
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#parn)).
#[derive(BinRead, Debug)]
pub(crate) struct Parn {
    /// The number of `tdmn` and `pard` pairs.
    pub count: u32,
}

impl Record for Parn {
    const NAME: Name = Name::chunk(b"parn");
}

/// `pard`: one effect parameter's definition, the SDK's parameter record: a
/// common header, then a body laid out by the control type
/// ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#pard)).
#[derive(BinRead, Debug)]
pub(crate) struct Pard {
    /// UI flags: [`Pard::INVISIBLE`] and others.
    #[br(seek_before = at(0x04))]
    pub ui_flags: u32,
    /// The control type's code.
    #[br(seek_before = at(0x0C))]
    pub control: u32,
    /// The parameter's name.
    #[br(map = |bytes: [u8; 32]| text(&bytes))]
    pub name: String,
    /// Parameter flags: [`Pard::CANNOT_TIME_VARY`] and others.
    pub flags: u32,
    /// The body of the control type.
    #[br(seek_before = at(0x38), args(control))]
    pub body: PardBody,
}

impl Record for Pard {
    const NAME: Name = Name::chunk(b"pard");
}

impl Pard {
    /// UI flag: the parameter is hidden from the panels and the timeline.
    pub const INVISIBLE: u32 = 0x200;
    /// Parameter flag: the parameter cannot be animated.
    pub const CANNOT_TIME_VARY: u32 = 0x02;
}

/// The body of a [`Pard`], one variant per control type the reader knows.
/// Only the defaults and bounds are read: the cached value is not the
/// instance's.
#[derive(BinRead, Debug)]
#[br(import(control: u32))]
pub(crate) enum PardBody {
    /// A layer.
    #[br(pre_assert(control == 0))]
    Layer,
    /// An integer slider.
    #[br(pre_assert(control == 1))]
    IntegerSlider {
        /// The smallest valid value.
        #[br(seek_before = at(0x7C))]
        min: i32,
        /// The largest valid value.
        max: i32,
        /// The default.
        #[br(seek_before = at(0x8C))]
        default: i32,
    },
    /// A fixed-point slider.
    #[br(pre_assert(control == 2))]
    FixedSlider {
        /// The smallest valid value.
        #[br(seek_before = at(0x7C), map = signed_fixed)]
        min: f64,
        /// The largest valid value.
        #[br(map = signed_fixed)]
        max: f64,
        /// The default.
        #[br(seek_before = at(0x8C), map = signed_fixed)]
        default: f64,
    },
    /// An angle, in degrees.
    #[br(pre_assert(control == 3))]
    Angle {
        /// The default.
        #[br(seek_before = at(0x3C), map = signed_fixed)]
        default: f64,
    },
    /// A checkbox.
    #[br(pre_assert(control == 4))]
    Checkbox {
        /// The default: 0 or 1.
        #[br(seek_before = at(0x3C))]
        default: u8,
    },
    /// A colour.
    #[br(pre_assert(control == 5))]
    Color {
        /// The default: alpha, red, green, blue, 0 to 255.
        #[br(seek_before = at(0x3C))]
        default: [u8; 4],
    },
    /// A 2D point.
    #[br(pre_assert(control == 6))]
    Point {
        /// The default, in percent of the layer's width and height.
        #[br(seek_before = at(0x44), map = |raw: [i32; 2]| raw.map(signed_fixed))]
        default: [f64; 2],
    },
    /// A popup menu.
    #[br(pre_assert(control == 7))]
    Popup {
        /// How many choices it has.
        #[br(seek_before = at(0x3C))]
        count: i16,
        /// The default, a 1-based choice.
        default: i16,
    },
    /// A floating-point slider.
    #[br(pre_assert(control == 10))]
    FloatSlider {
        /// The smallest valid value; not finite when there is no bound.
        #[br(seek_before = at(0x68))]
        min: f32,
        /// The largest valid value; not finite when there is no bound.
        max: f32,
        /// The default.
        #[br(seek_before = at(0x78))]
        default: f32,
        /// Display flags: bit 0 shows the value in percent.
        #[br(seek_before = at(0x7E))]
        display: u16,
    },
    /// A 3D point.
    #[br(pre_assert(control == 18))]
    Point3d {
        /// The default, in percent of the layer's width, height and height.
        #[br(seek_before = at(0x50))]
        default: [f64; 3],
    },
    /// A control type whose body the reader does not need: no data, a group's
    /// start or end, a button, a path, arbitrary data, or one it does not know.
    Other,
}
