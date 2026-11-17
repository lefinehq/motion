//! Fonts and text, as far as a program that does not show text needs:
//! fonts can be found, defined, scaled and selected, so procedure sets
//! that prepare fonts run, but no glyph is drawn. Every operator that
//! would draw or measure text is reported.

use kurbo::Affine;

use crate::interp::{Interp, R};
use crate::object::{Key, Object, Value};
use crate::ops::state::{matrix_at, new_matrix};

/// The glyph names of the printable ASCII codes 32 to 126, as the standard
/// encodings name them.
const ASCII_GLYPHS: [&str; 95] = [
    "space",
    "exclam",
    "quotedbl",
    "numbersign",
    "dollar",
    "percent",
    "ampersand",
    "quoteright",
    "parenleft",
    "parenright",
    "asterisk",
    "plus",
    "comma",
    "hyphen",
    "period",
    "slash",
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "colon",
    "semicolon",
    "less",
    "equal",
    "greater",
    "question",
    "at",
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "bracketleft",
    "backslash",
    "bracketright",
    "asciicircum",
    "underscore",
    "quoteleft",
    "a",
    "b",
    "c",
    "d",
    "e",
    "f",
    "g",
    "h",
    "i",
    "j",
    "k",
    "l",
    "m",
    "n",
    "o",
    "p",
    "q",
    "r",
    "s",
    "t",
    "u",
    "v",
    "w",
    "x",
    "y",
    "z",
    "braceleft",
    "bar",
    "braceright",
    "asciitilde",
];

/// Defines the standard encodings: the printable ASCII names, and
/// `.notdef` elsewhere, since no text is drawn.
pub(crate) fn install(i: &mut Interp) -> R {
    let notdef = i.name_obj(".notdef");
    let mut names = vec![notdef; 256];
    for (offset, glyph) in ASCII_GLYPHS.iter().enumerate() {
        names[32 + offset] = i.name_obj(glyph);
    }
    for encoding in ["StandardEncoding", "ISOLatin1Encoding"] {
        let arr = i.vm.new_array(names.clone())?;
        i.vm.dict_put_name(i.systemdict, encoding, Object::lit(Value::Array(arr)))?;
    }
    Ok(())
}

/// A stand-in font dictionary for a font that is not loaded.
fn stand_in(i: &mut Interp, name: Object) -> R<Object> {
    let dict = i.vm.new_dict(12)?;
    let matrix = new_matrix(i, Affine::scale(0.001))?;
    let bbox = i.vm.new_array(vec![
        Object::int(0),
        Object::int(0),
        Object::int(1000),
        Object::int(1000),
    ])?;
    let encoding = i.lookup_str("StandardEncoding").unwrap_or(Object::NULL);
    let glyphs = i.vm.new_dict(1)?;
    let id = i.next_font;
    i.next_font += 1;
    let entries = [
        ("FontName", name),
        ("FontType", Object::int(1)),
        ("FontMatrix", matrix),
        ("FontBBox", Object::lit(Value::Array(bbox))),
        ("Encoding", encoding),
        ("PaintType", Object::int(0)),
        ("CharStrings", Object::lit(Value::Dict(glyphs))),
        ("FID", Object::lit(Value::FontId(id))),
    ];
    for (key, value) in entries {
        i.vm.dict_put_name(dict, key, value)?;
    }
    Ok(Object::lit(Value::Dict(dict)))
}

pub(crate) fn findfont(i: &mut Interp) -> R {
    let name = i.peek(0)?;
    let key = i.vm.key(name);
    let font = match i.vm.dict_get(i.fontdirectory, &key) {
        Some(font) => font,
        None => {
            let font = stand_in(i, name)?;
            i.vm.dict_put(i.fontdirectory, key, font)?;
            font
        }
    };
    i.drop_n(1);
    i.push(font)
}

pub(crate) fn definefont(i: &mut Interp) -> R {
    let (name, font) = (i.peek(1)?, i.peek(0)?);
    let dict = i.dict(0)?;
    let fid = Key::Name(i.vm.name(b"FID"));
    if i.vm.dict_get(dict, &fid).is_none() {
        let id = i.next_font;
        i.next_font += 1;
        i.vm.dict_put(dict, fid, Object::lit(Value::FontId(id)))?;
    }
    let key = i.vm.key(name);
    i.vm.dict_put(i.fontdirectory, key, font)?;
    i.drop_n(2);
    i.push(font)
}

/// A copy of font `dict` whose matrix is followed by `m`.
fn transformed(i: &mut Interp, dict: u32, m: Affine) -> R<Object> {
    let copy = i.vm.dicts[dict as usize].clone();
    let id = i.vm.new_dict(copy.max)?;
    i.vm.dicts[id as usize] = copy;
    let key = Key::Name(i.vm.name(b"FontMatrix"));
    let current = match i.vm.dict_get(id, &key).and_then(Object::as_arr) {
        Some(arr) => crate::ops::state::matrix_of(i, arr)?,
        None => Affine::scale(0.001),
    };
    let matrix = new_matrix(i, m * current)?;
    i.vm.dict_put(id, key, matrix)?;
    Ok(Object::lit(Value::Dict(id)))
}

pub(crate) fn scalefont(i: &mut Interp) -> R {
    let (dict, scale) = (i.dict(1)?, i.num(0)?);
    let font = transformed(i, dict, Affine::scale(scale))?;
    i.drop_n(2);
    i.push(font)
}

pub(crate) fn makefont(i: &mut Interp) -> R {
    let (dict, m) = (i.dict(1)?, matrix_at(i, 0)?);
    let font = transformed(i, dict, m)?;
    i.drop_n(2);
    i.push(font)
}

pub(crate) fn setfont(i: &mut Interp) -> R {
    let font = i.peek(0)?;
    i.dict(0)?;
    i.drop_n(1);
    i.g.font = font;
    Ok(())
}

pub(crate) fn currentfont(i: &mut Interp) -> R {
    let font = i.g.font;
    if font == Object::NULL {
        let name = i.name_obj("Courier");
        let font = stand_in(i, name)?;
        return i.push(font);
    }
    i.push(font)
}

pub(crate) fn selectfont(i: &mut Interp) -> R {
    let (name, scale) = (i.peek(1)?, i.peek(0)?);
    i.drop_n(2);
    i.push(name)?;
    findfont(i)?;
    i.push(scale)?;
    if scale.as_f64().is_some() {
        scalefont(i)?;
    } else {
        makefont(i)?;
    }
    setfont(i)
}

/// Reports text that is not drawn and drops the operator's `n` operands.
fn text_unsupported(i: &mut Interp, n: usize, what: &str) -> R {
    i.need(n)?;
    i.drop_n(n);
    i.unsupported(format!("text (`{what}`), drawn as nothing"));
    Ok(())
}

pub(crate) fn show(i: &mut Interp) -> R {
    i.string(0)?;
    text_unsupported(i, 1, "show")
}

pub(crate) fn ashow(i: &mut Interp) -> R {
    text_unsupported(i, 3, "ashow")
}

pub(crate) fn widthshow(i: &mut Interp) -> R {
    text_unsupported(i, 4, "widthshow")
}

pub(crate) fn awidthshow(i: &mut Interp) -> R {
    text_unsupported(i, 6, "awidthshow")
}

pub(crate) fn kshow(i: &mut Interp) -> R {
    text_unsupported(i, 2, "kshow")
}

pub(crate) fn xshow(i: &mut Interp) -> R {
    text_unsupported(i, 2, "xshow")
}

pub(crate) fn glyphshow(i: &mut Interp) -> R {
    text_unsupported(i, 1, "glyphshow")
}

pub(crate) fn charpath(i: &mut Interp) -> R {
    i.boolean(0)?;
    text_unsupported(i, 2, "charpath")
}

pub(crate) fn stringwidth(i: &mut Interp) -> R {
    i.string(0)?;
    i.drop_n(1);
    i.unsupported("text measurement (`stringwidth`), measured as zero");
    i.push(Object::int(0))?;
    i.push(Object::int(0))
}

pub(crate) fn setcachedevice(i: &mut Interp) -> R {
    i.need(6)?;
    i.drop_n(6);
    Ok(())
}

pub(crate) fn setcachedevice2(i: &mut Interp) -> R {
    i.need(10)?;
    i.drop_n(10);
    Ok(())
}
