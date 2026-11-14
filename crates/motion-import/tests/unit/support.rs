//! Builders shared by the unit tests: chunks, lists, fixed records and whole
//! files, laid out as the spec says, so each test states only the bytes it is
//! about; and a small decoded project for the tests of what reads the model.

use crate::model::{
    BaselineOption, Caps, CharacterStyle, Child, Color, Composition, Ease, Expression, Footage,
    Group, Interpolation, Item, ItemKind, Justification, Kerning, Keyframe, Layer, LayerKind,
    LeadingType, LineJoin, ParagraphStyle, Project, Property, Reason, Run, Source, Span, Stream,
    Switches, TextDocument, TextFrame, Value, Version,
};
use crate::{BlendMode, TrackMatte};

/// A project with a folder holding a composition, and a solid: a text layer
/// with a transform, a text document and an effect, and a null.
pub(crate) fn sample_project() -> Project {
    let property = |match_name: &str, value| Property {
        match_name: match_name.into(),
        name: None,
        enabled: true,
        value,
        expression: None,
        min: None,
        max: None,
    };
    let group = |match_name: &str, children| Group {
        match_name: match_name.into(),
        name: None,
        enabled: true,
        children,
    };
    let key = |time, value| Keyframe {
        time,
        value: Value::Number(value),
        in_interpolation: Interpolation::Linear,
        out_interpolation: Interpolation::Bezier,
        in_ease: vec![Ease {
            speed: 0.0,
            influence: 1.0 / 6.0,
        }],
        out_ease: vec![Ease {
            speed: 0.0,
            influence: 1.0 / 6.0,
        }],
        spatial: None,
        continuous: false,
        auto_bezier: false,
        roving: false,
    };
    let opacity = Property {
        expression: Some(Expression {
            text: "wiggle(1, 0.1)\r\n// shake".into(),
            enabled: false,
        }),
        min: Some(0.0),
        max: Some(100.0),
        ..property(
            "ADBE Opacity",
            Stream::Animated(vec![key(0.5, 0.0), key(1.25, 1.0)]),
        )
    };
    let transform = group(
        "ADBE Transform Group",
        vec![
            Child::Property(property(
                "ADBE Position",
                Stream::Static(Value::Vector3([960.0, 540.5, 0.0])),
            )),
            Child::Property(opacity),
        ],
    );
    let text = group(
        "ADBE Text Properties",
        vec![
            Child::Property(property(
                "ADBE Text Document",
                Stream::Static(Value::Text(Box::new(sample_document()))),
            )),
            Child::Property(property(
                "ADBE Marker",
                Stream::Static(Value::Opaque {
                    reason: Reason::Markers,
                }),
            )),
        ],
    );
    let slider = Group {
        name: Some("Speed".into()),
        enabled: false,
        ..group(
            "ADBE Slider Control",
            vec![Child::Property(property(
                "ADBE Slider Control-0001",
                Stream::Static(Value::Number(2.0)),
            ))],
        )
    };
    let effects = group("ADBE Effect Parade", vec![Child::Group(slider)]);
    let styles = group(
        "ADBE Layer Styles",
        vec![Child::Group(group("ADBE Blend Options Group", vec![]))],
    );
    let layer = |id, name: &str, kind, switches, children| Layer {
        id,
        name: name.into(),
        kind,
        source: None,
        parent: None,
        in_point: 0.0,
        out_point: 4.0,
        start_time: 0.0,
        stretch: 1.0,
        switches,
        blend_mode: BlendMode::Normal,
        track_matte: TrackMatte::None,
        matte_layer: None,
        properties: group("", children),
    };
    let on = Switches {
        enabled: true,
        ..Switches::default()
    };
    let title = Layer {
        parent: Some(11),
        ..layer(
            10,
            "Title",
            LayerKind::Text,
            Switches {
                three_d: true,
                ..on
            },
            vec![
                Child::Group(text),
                Child::Group(effects),
                Child::Group(transform),
                Child::Group(styles),
            ],
        )
    };
    let null = Layer {
        source: Some(3),
        ..layer(
            11,
            "Ctrl",
            LayerKind::Av,
            Switches { null: true, ..on },
            vec![],
        )
    };
    let comp = Composition {
        width: 1920,
        height: 1080,
        pixel_aspect: 1.0,
        frame_rate: 29.97,
        duration: 4.0,
        background: Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        },
        work_area: Span {
            start: 0.0,
            end: 4.0,
        },
        layers: vec![title, null],
    };
    let solid = Footage {
        width: 100,
        height: 100,
        pixel_aspect: 1.0,
        frame_rate: 0.0,
        duration: 0.0,
        source: Source::Solid {
            color: Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
        },
    };
    let item = |id, name: &str, folder, kind| Item {
        id,
        name: name.into(),
        folder,
        kind,
    };
    Project {
        version: Version {
            major: 94,
            minor: 9,
        },
        bits_per_channel: Some(8),
        items: vec![
            item(1, "Scenes", None, ItemKind::Folder),
            item(2, "Main", Some(1), ItemKind::Composition(comp)),
            item(3, "Red Solid", None, ItemKind::Footage(solid)),
        ],
        skipped: [("fvdv".to_owned(), 2), ("LIST:Ewst".to_owned(), 1)].into(),
    }
}

/// A two-line, centred box text in white Inter-Bold 44 with a red stroke.
pub(crate) fn sample_document() -> TextDocument {
    let white = Color {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    let paragraph = ParagraphStyle {
        justification: Justification::Center,
        first_line_indent: 0.0,
        start_indent: 0.0,
        end_indent: 0.0,
        space_before: 0.0,
        space_after: 0.0,
        leading_type: LeadingType::Roman,
        auto_hyphenate: true,
        every_line_composer: false,
    };
    let character = CharacterStyle {
        font: "Inter-Bold".into(),
        size: 44.0,
        faux_bold: false,
        faux_italic: false,
        auto_leading: true,
        leading: 52.8,
        horizontal_scale: 1.0,
        vertical_scale: 1.0,
        tracking: 0.0,
        baseline_shift: 0.0,
        kerning: Kerning::Metrics,
        caps: Caps::All,
        baseline_option: BaselineOption::Normal,
        ligatures: true,
        tsume: 0.0,
        no_break: false,
        fill: Some(white),
        stroke: Some(Color {
            g: 0.0,
            b: 0.0,
            ..white
        }),
        stroke_over_fill: false,
        stroke_width: 2.0,
        line_join: LineJoin::Round,
    };
    TextDocument {
        text: "Hello\nworld".into(),
        paragraphs: vec![
            Run {
                range: 0..6,
                style: paragraph,
            },
            Run {
                range: 6..11,
                style: paragraph,
            },
        ],
        characters: vec![Run {
            range: 0..11,
            style: character,
        }],
        frame: TextFrame::Box {
            left: -110.0,
            top: -70.0,
            width: 220.0,
            height: 140.0,
        },
        lines: vec![],
    }
}

/// The paragraph style every [`text_body`] document inherits: centred.
pub(crate) const PARAGRAPH_DEFAULT: &str =
    "<< /0 2 /1 0.0 /2 0.0 /3 0.0 /4 0.0 /5 0.0 /8 0 /9 true /29 false >>";

/// The character style every [`text_body`] document inherits: font 0 at
/// 44, white fill, red stroke not applied.
pub(crate) const CHARACTER_DEFAULT: &str = "<< /0 0 /1 44.0 /2 false /3 false /4 true \
     /5 .01 /6 1.0 /7 1.0 /8 0 /9 0.0 /11 1 /12 0 /13 0 /18 true /36 0.0 /52 false \
     /53 << /99 /SimplePaint /0 << /0 1 /1 [ 1.0 1.0 1.0 1.0 ] >> >> \
     /54 << /99 /SimplePaint /0 << /0 1 /1 [ 1.0 1.0 0.0 0.0 ] >> >> \
     /56 true /57 false /58 false /62 0 /63 1.0 >>";

/// A `LIST:btdk` body as the format writes it, with `fonts` in its table,
/// `frames` (each `<< ... >>`) and `documents` (each built by
/// [`text_document`]), inheriting [`CHARACTER_DEFAULT`] and
/// [`PARAGRAPH_DEFAULT`] through one sheet each.
pub(crate) fn text_body(fonts: &[&str], frames: &[&str], documents: &[String]) -> String {
    let fonts: Vec<String> = fonts
        .iter()
        .map(|font| format!("<< /0 << /99 /CoolTypeFont /0 << /0 ({font}) /2 1 >> >> >>"))
        .collect();
    format!(
        " /98 << /0 9 >> /0 << /1 << /0 [ {} ] >> \
         /5 << /0 [ << /0 << /0 (Normal RGB) /6 << >> >> >> ] >> \
         /6 << /0 [ << /0 << /0 (Normal RGB) /5 << >> >> >> ] /1 [ << /0 0 >> ] >> \
         /8 << /0 [ {} ] >> >> \
         /1 << /0 << >> /1 [ {} ] /2 {CHARACTER_DEFAULT} /3 {PARAGRAPH_DEFAULT} >>",
        fonts.join(" "),
        frames.join(" "),
        documents.join(" ")
    )
}

/// One document of a [`text_body`]: `text` as stored (paragraphs ending in
/// CR), `paragraphs` and `characters` as `(count, style keys)` runs on
/// sheet 0, and the composed-line cache `composed`.
pub(crate) fn text_document(
    text: &str,
    paragraphs: &[(usize, &str)],
    characters: &[(usize, &str)],
    composed: &str,
) -> String {
    let paragraphs: Vec<String> = paragraphs
        .iter()
        .map(|(count, keys)| {
            format!("<< /0 << /0 << /0 () /5 << {keys} >> /6 0 >> >> /1 {count} >>")
        })
        .collect();
    let characters: Vec<String> = characters
        .iter()
        .map(|(count, keys)| {
            format!("<< /0 << /0 << /0 () /5 0 /6 << {keys} >> >> >> /1 {count} >>")
        })
        .collect();
    format!(
        "<< /0 << /0 ({text}) /5 << /0 [ {} ] >> /6 << /0 [ {} ] >> >> \
         /1 << /0 [ << /0 0 >> ] /2 [ {composed} ] >> >>",
        paragraphs.join(" "),
        characters.join(" ")
    )
}

/// A chunk with `tag` and `body`, padded to an even length.
pub(crate) fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = u32::try_from(body.len()).expect("a small test body");
    let pad: &[u8] = if body.len() % 2 == 1 { &[0] } else { &[] };
    [tag.as_slice(), &size.to_be_bytes(), body, pad].concat()
}

/// A `LIST` of type `kind` holding `children`, each already built.
pub(crate) fn list(kind: &[u8; 4], children: &[Vec<u8>]) -> Vec<u8> {
    chunk(b"LIST", &[kind.as_slice(), &children.concat()].concat())
}

/// A whole file: the `RIFX` header, the `Egg!` form and `children`.
pub(crate) fn file(children: &[Vec<u8>]) -> Vec<u8> {
    chunk(b"RIFX", &[b"Egg!".as_slice(), &children.concat()].concat())
}

/// A record of `size` zero bytes with `fields` written at their offsets, as
/// the spec's tables give them.
pub(crate) fn record(size: usize, fields: &[(usize, &[u8])]) -> Vec<u8> {
    fields
        .iter()
        .fold(vec![0; size], |mut bytes, (offset, field)| {
            bytes[*offset..offset + field.len()].copy_from_slice(field);
            bytes
        })
}

/// A file of version `major` with `items` in its root folder.
pub(crate) fn project(major: u16, items: &[Vec<u8>]) -> Vec<u8> {
    file(&[
        chunk(b"head", &record(20, &[(0, &major.to_be_bytes())])),
        chunk(b"nhed", &record(32, &[(0x0F, &[1])])),
        list(b"Fold", &[&[chunk(b"fdta", &[0; 14])], items].concat()),
    ])
}

/// A `LIST:Item` of `kind` with `id`, `name` and the chunks after the name.
pub(crate) fn item(kind: u16, id: u32, name: &str, rest: &[Vec<u8>]) -> Vec<u8> {
    let idta = record(84, &[(0, &kind.to_be_bytes()), (0x10, &id.to_be_bytes())]);
    let head = [chunk(b"idta", &idta), chunk(b"Utf8", name.as_bytes())];
    list(b"Item", &[&head, rest].concat())
}

/// A 204-byte `cdta`: `width` x `height` at `fps`, 24576 timebase units a
/// second, lasting `seconds`.
pub(crate) fn cdta(width: u16, height: u16, fps: u16, seconds: u32) -> Vec<u8> {
    let tb = 24576u32.to_be_bytes();
    chunk(
        b"cdta",
        &record(
            204,
            &[
                (0x20, &tb),
                (0x24, &u32::MAX.to_be_bytes()),
                (0x28, &600u32.to_be_bytes()),
                (0x2C, &(seconds * 24576).to_be_bytes()),
                (0x30, &tb),
                (0x34, &[255, 0, 51]),
                (0x8C, &width.to_be_bytes()),
                (0x8E, &height.to_be_bytes()),
                (0x90, &[0, 0, 0, 1, 0, 0, 0, 1]),
                (0x9C, &(u32::from(fps) << 16).to_be_bytes()),
            ],
        ),
    )
}
