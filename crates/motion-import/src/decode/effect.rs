//! Effect definitions: the `LIST:parT` of an effect's first instance, and the
//! project's table of them, `LIST:EfdG`
//! ([spec](https://github.com/forticheprod/aep_parser/blob/main/docs/file_format.md#listpart)).
//! Both hold the same definitions; the first one read for a match name wins.

use motion_format::{Diagnostic, Kind, Result};

use super::{Decoder, Scan};
use crate::model::{Color, Control, EffectDefinition, Parameter};
use crate::record::{Efdc, Pard, PardBody, Parn, Record, text};
use crate::rifx::Name;

/// The project's effect definitions.
pub(super) const EFDG: Name = Name::list(b"EfdG");
/// One effect definition: its match name and an effect body.
const EFDF: Name = Name::list(b"EfDf");
/// An effect body: its name, parameter definitions and parameter values.
const SSPC: Name = Name::list(b"sspc");
/// The parameter definitions of an effect.
const PART: Name = Name::list(b"parT");
/// An effect's display name, as a wrapper.
const FNAM: Name = Name::chunk(b"fnam");
/// A popup's choices, or a checkbox's or button's label, as a wrapper.
const PDNM: Name = Name::chunk(b"pdnm");
/// The match name of the definition or parameter that follows.
const TDMN: Name = Name::chunk(b"tdmn");

impl Decoder {
    /// Records the definition in an effect body, the `LIST:sspc` that `scan`
    /// reads, of the effect called `match_name`. Only the first instance of
    /// each effect type holds one; the others have an empty `LIST:parT`.
    ///
    /// # Errors
    /// A diagnostic when a parameter definition cannot be read.
    pub(super) fn define_effect(&self, scan: &mut Scan<'_, '_>, match_name: &str) -> Result<()> {
        let name = match scan.find(FNAM) {
            Some(fnam) => self.wrapped_text(scan, fnam)?,
            None => String::new(),
        };
        let Some(part) = scan.list(PART)? else {
            return Ok(());
        };
        let parameters = self.parameters(part)?;
        if !parameters.is_empty() {
            self.effects
                .borrow_mut()
                .entry(match_name.to_owned())
                .or_insert(EffectDefinition { name, parameters });
        }
        Ok(())
    }

    /// Records every definition in the project's `LIST:EfdG`.
    ///
    /// # Errors
    /// A diagnostic when a definition has no match name or no body, or a
    /// parameter definition cannot be read.
    pub(super) fn effect_table(&self, mut efdg: Scan<'_, '_>) -> Result<()> {
        let count = efdg.record::<Efdc>()?.count;
        let path = efdg.child_path(EFDF);
        let mut read = 0;
        efdg.lists(EFDF).try_for_each(|efdf| {
            let mut efdf = efdf?;
            let match_name = text(efdf.require(TDMN)?.body);
            let mut sspc = efdf.require_list(SSPC)?;
            read += 1;
            self.define_effect(&mut sspc, &match_name)
        })?;
        counted(count, read, "`EfDC` counts {count} definitions", &path)
    }

    /// The parameters a `LIST:parT` defines, each a `tdmn` followed by its
    /// `pard` and, for a popup, its choices.
    fn parameters(&self, mut part: Scan<'_, '_>) -> Result<Vec<Parameter>> {
        let count = part.optional::<Parn>()?.map_or(0, |parn| parn.count);
        let path = part.child_path(PART);
        let parameters = part
            .children(TDMN)
            .into_iter()
            .map(|(tdmn, body)| {
                let match_name = text(tdmn.body);
                let pard = body.iter().find(|chunk| chunk.name == Pard::NAME);
                let pard: Pard = match pard {
                    Some(chunk) => part.parse(*chunk, ())?,
                    None => {
                        return Err(Diagnostic::new(
                            Kind::Missing,
                            format!("effect parameter `{match_name}` has no `pard`"),
                        )
                        .at(path.clone()));
                    }
                };
                let label = body
                    .iter()
                    .find(|chunk| chunk.name == PDNM)
                    .map(|pdnm| self.wrapped_text(&part, *pdnm))
                    .transpose()?
                    .unwrap_or_default();
                self.skip(
                    body.iter()
                        .map(|chunk| chunk.name)
                        .filter(|name| *name != Pard::NAME && *name != PDNM),
                );
                Ok(parameter(match_name, &pard, &label))
            })
            .collect::<Result<Vec<_>>>()?;
        counted(
            count,
            parameters.len(),
            "`parn` counts {count} parameters",
            &path,
        )?;
        Ok(parameters)
    }
}

/// Checks that a list holds the `count` entries its count record says it
/// does; `what` says what was counted, with `{count}` for the number.
///
/// # Errors
/// A diagnostic at `path` when the list holds `read` entries instead.
fn counted(count: u32, read: usize, what: &str, path: &str) -> Result<()> {
    match usize::try_from(count).is_ok_and(|count| count == read) {
        true => Ok(()),
        false => Err(Diagnostic::new(
            Kind::Invalid,
            format!(
                "{}, the list holds {read}",
                what.replace("{count}", &count.to_string())
            ),
        )
        .at(path)),
    }
}

/// The parameter `match_name` defines with `pard`; `label` is its `pdnm`,
/// which holds a popup's choices separated by `|`.
fn parameter(match_name: String, pard: &Pard, label: &str) -> Parameter {
    let bound = |value: f32| value.is_finite().then_some(f64::from(value));
    let control = match pard.body {
        PardBody::Layer => Control::Layer,
        PardBody::IntegerSlider { min, max, default } => Control::Slider {
            default: f64::from(default),
            min: Some(f64::from(min)),
            max: Some(f64::from(max)),
            percent: false,
        },
        PardBody::FixedSlider { min, max, default } => Control::Slider {
            default,
            min: Some(min),
            max: Some(max),
            percent: false,
        },
        PardBody::FloatSlider {
            min,
            max,
            default,
            display,
        } => Control::Slider {
            default: f64::from(default),
            min: bound(min),
            max: bound(max),
            percent: display & 0x01 != 0,
        },
        PardBody::Angle { default } => Control::Angle { default },
        PardBody::Checkbox { default } => Control::Checkbox {
            default: default != 0,
        },
        PardBody::Color {
            default: [a, r, g, b],
        } => {
            let channel = |c: u8| f64::from(c) / 255.0;
            Control::Color {
                default: Color {
                    r: channel(r),
                    g: channel(g),
                    b: channel(b),
                    a: channel(a),
                },
            }
        }
        PardBody::Point { default } => Control::Point {
            default: default.iter().map(|percent| percent / 100.0).collect(),
        },
        PardBody::Point3d { default } => Control::Point {
            default: default.iter().map(|percent| percent / 100.0).collect(),
        },
        PardBody::Popup { count, default } => {
            let choices: Vec<String> = label.split('|').map(str::to_owned).collect();
            let count = usize::try_from(count).unwrap_or_default();
            Control::Popup {
                default: u16::try_from(default).unwrap_or(1).max(1),
                choices: choices.into_iter().take(count).collect(),
            }
        }
        PardBody::Other => Control::Other { code: pard.control },
    };
    Parameter {
        match_name,
        name: pard.name.clone(),
        invisible: pard.ui_flags & Pard::INVISIBLE != 0,
        animatable: pard.flags & Pard::CANNOT_TIME_VARY == 0,
        control,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/decode/effect.rs"]
mod tests;
