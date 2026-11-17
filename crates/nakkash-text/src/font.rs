//! One usable font face, read from font bytes with `skrifa`: its names,
//! weight and style, and for a variable font the axis settings of the named
//! instance it stands for. A face also answers to the other names its own
//! `name` table gives it ([`Face::answers_to`]), so a project that stored an
//! earlier build's name for it still finds it.

use std::sync::Arc;

use nakkash_format::{Diagnostic, Kind, Result};
use parley::FontData;
use parley::fontique::Blob;
use skrifa::raw::FileRef;
use skrifa::string::StringId;
use skrifa::{FontRef, MetadataProvider, Tag};

/// Font bytes shared by every face read from them, and by the renderer.
pub(crate) type Bytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// The slant of a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slant {
    /// Upright.
    Normal,
    /// A designed italic.
    Italic,
    /// A slanted upright.
    Oblique,
}

/// One face a [`FontSet`](crate::FontSet) can select.
#[derive(Debug, Clone)]
pub struct Face {
    /// The PostScript name, such as `Lato-Black` or `InterVariable-Bold`: what project
    /// files store, and the exact way to ask for this face.
    pub postscript_name: String,
    /// The family, such as `Lato`: the typographic family when the font
    /// names one, else its family name.
    pub family: String,
    /// The style within the family, such as `Bold Italic`: the typographic
    /// subfamily when the font names one, else its subfamily; for a named
    /// instance of a variable font, the instance's subfamily.
    pub style: String,
    /// The other names the font gives this face, as it writes them: its full
    /// name (name ID 4), and its family with its style, both the
    /// typographic pair (16, 17) and the legacy one (1, 2), where a
    /// `Regular` style is left out, as a regular face's name leaves it out
    /// (`IBM Plex Sans`). Once each, without the PostScript name.
    pub names: Vec<String>,
    /// The version its file names, such as `Version 3.100`; empty when it
    /// names none.
    pub version: String,
    /// The weight, 1 to 1000 (400 regular, 700 bold).
    pub weight: f32,
    /// The slant.
    pub slant: Slant,
    /// For a named instance of a variable font, its axis settings in user
    /// units, such as `wght` 700; empty for a static face.
    pub variations: Vec<(Tag, f32)>,
    /// The font bytes and the face's index in them, shared with the renderer.
    pub data: FontData,
}

/// Reads every face in `bytes`, a font or a collection: each static face
/// once, and a variable face once per named instance.
///
/// # Errors
/// An invalid [`Diagnostic`] naming `source` when the bytes are not a font.
pub(crate) fn faces(bytes: Bytes, source: &str) -> Result<Vec<Face>> {
    let blob: Blob<u8> = Blob::new(bytes.clone());
    let not_a_font = |e: skrifa::raw::ReadError| {
        Diagnostic::new(Kind::Invalid, format!("`{source}` is not a font file: {e}"))
            .expected("an OpenType or TrueType font (`.ttf`, `.otf`, `.ttc`, `.otc`)")
    };
    let file = FileRef::new((*bytes).as_ref()).map_err(not_a_font)?;
    let mut found = Vec::new();
    for (index, font) in file.fonts().enumerate() {
        let font = font.map_err(not_a_font)?;
        let data = FontData::new(blob.clone(), index as u32);
        found.extend(faces_of(&font, &data, source)?);
    }
    Ok(found)
}

/// The faces one font offers.
fn faces_of(font: &FontRef, data: &FontData, source: &str) -> Result<Vec<Face>> {
    let legacy_family = name(font, StringId::FAMILY_NAME);
    let family = name(font, StringId::TYPOGRAPHIC_FAMILY_NAME)
        .or_else(|| legacy_family.clone())
        .ok_or_else(|| missing_name(source, "family"))?;
    let legacy_style = name(font, StringId::SUBFAMILY_NAME);
    let style = name(font, StringId::TYPOGRAPHIC_SUBFAMILY_NAME)
        .or_else(|| legacy_style.clone())
        .unwrap_or_default();
    let version = name(font, StringId::VERSION_STRING).unwrap_or_default();
    let attributes = font.attributes();
    let slant = slant(attributes.style);
    let instances = font.named_instances();
    if instances.is_empty() {
        let postscript_name = name(font, StringId::POSTSCRIPT_NAME)
            .ok_or_else(|| missing_name(source, "PostScript"))?;
        let names = other_names(
            &postscript_name,
            [
                name(font, StringId::FULL_NAME),
                Some(with_style(&family, &style)),
                legacy_family.map(|f| with_style(&f, legacy_style.as_deref().unwrap_or_default())),
            ],
        );
        return Ok(vec![Face {
            postscript_name,
            family,
            style,
            names,
            version,
            weight: attributes.weight.value(),
            slant,
            variations: Vec::new(),
            data: data.clone(),
        }]);
    }
    let axes: Vec<Tag> = font.axes().iter().map(|axis| axis.tag()).collect();
    // Named instances without a PostScript name of their own take the
    // variations prefix (or the family without spaces) and the subfamily.
    let prefix = name(font, StringId::VARIATIONS_POSTSCRIPT_NAME_PREFIX)
        .unwrap_or_else(|| family.replace(' ', ""));
    instances
        .iter()
        .map(|instance| {
            let style = name(font, instance.subfamily_name_id()).unwrap_or_default();
            let postscript_name = instance
                .postscript_name_id()
                .and_then(|id| name(font, id))
                .or_else(|| {
                    (!style.is_empty()).then(|| format!("{prefix}-{}", style.replace(' ', "")))
                })
                .ok_or_else(|| missing_name(source, "instance"))?;
            // An instance's full name is its family and subfamily, as for
            // the font's own default instance.
            let names = other_names(&postscript_name, [Some(with_style(&family, &style))]);
            let variations: Vec<(Tag, f32)> =
                axes.iter().copied().zip(instance.user_coords()).collect();
            let weight = variations
                .iter()
                .find(|(tag, _)| *tag == WEIGHT_AXIS)
                .map_or(attributes.weight.value(), |&(_, w)| w);
            Ok(Face {
                postscript_name,
                family: family.clone(),
                style,
                names,
                version: version.clone(),
                weight,
                slant,
                variations,
                data: data.clone(),
            })
        })
        .collect()
}

impl Face {
    /// Whether this face is the one `name` names: its PostScript name or one
    /// of its [`names`](Face::names), compared in lower case, without spaces
    /// or punctuation. `OpenSans-Extrabold` is the face named
    /// `OpenSans-ExtraBold`, and `IBMPlexSans` the one whose family is
    /// `IBM Plex Sans` and whose style is `Regular`. A name of another style
    /// never is: `BebasNeueBold` is not `Bebas Neue Regular`.
    pub fn answers_to(&self, name: &str) -> bool {
        let wanted = name_key(name);
        !wanted.is_empty()
            && std::iter::once(&self.postscript_name)
                .chain(&self.names)
                .any(|own| name_key(own) == wanted)
    }
}

/// `name` as names are compared: in lower case, without spaces or
/// punctuation, so `Open Sans ExtraBold`, `OpenSans-Extrabold` and
/// `opensans_extrabold` are one name.
pub(crate) fn name_key(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// The style a regular face's names leave out.
const REGULAR: &str = "Regular";

/// `family` and `style` as one name, `family` alone when `style` is empty
/// or [`REGULAR`].
fn with_style(family: &str, style: &str) -> String {
    match style.is_empty() || style.eq_ignore_ascii_case(REGULAR) {
        true => family.to_owned(),
        false => format!("{family} {style}"),
    }
}

/// `names`, once each by [`name_key`], without any that is `postscript`.
fn other_names<const N: usize>(postscript: &str, names: [Option<String>; N]) -> Vec<String> {
    let mut seen = vec![name_key(postscript)];
    let mut out = Vec::new();
    for name in names.into_iter().flatten() {
        let key = name_key(&name);
        if !key.is_empty() && !seen.contains(&key) {
            seen.push(key);
            out.push(name);
        }
    }
    out
}

/// The weight axis of a variable font.
const WEIGHT_AXIS: Tag = Tag::new(b"wght");

/// The English name `id`, else the first one, if the font has it.
fn name(font: &FontRef, id: StringId) -> Option<String> {
    font.localized_strings(id)
        .english_or_first()
        .map(|s| s.chars().collect::<String>())
        .filter(|s| !s.is_empty())
}

fn missing_name(source: &str, which: &str) -> Diagnostic {
    Diagnostic::new(
        Kind::Invalid,
        format!("a face in `{source}` has no {which} name"),
    )
    .expected("a font whose `name` table names its family and PostScript name")
}

/// The slant `skrifa` reads from the font's style.
fn slant(style: skrifa::attribute::Style) -> Slant {
    match style {
        skrifa::attribute::Style::Normal => Slant::Normal,
        skrifa::attribute::Style::Italic => Slant::Italic,
        skrifa::attribute::Style::Oblique(_) => Slant::Oblique,
    }
}

#[cfg(test)]
#[path = "../tests/unit/font.rs"]
mod tests;
