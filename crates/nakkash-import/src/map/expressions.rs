//! Baked expressions in the scene ([D38]): the samples an expression gave
//! are fitted into keys, which replace the keys under it, and the report
//! says what each became.
//!
//! - A value's components share their key times, as a scene key holds the
//!   whole value; a position that moves along a path is written one
//!   component at a time instead, as each then fits on its own.
//! - A source text expression whose text never changes sets the text.
//!
//! [D38]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d38-expressions-are-baked-at-import

use nakkash_expr::Dialect;
use nakkash_expr::fit::{Fitted, fit, fit_together};
use nakkash_format::ValueType;
use nakkash_format::property::Target;

use super::Walk;
use super::items::{Place, plural, range_text};
use super::keys::{Convert, Written, every_frame, fitted, on_layer_clock};
use super::report::{ExpressionReport, Outcome};
use crate::bake::{Bake, Sampled, Samples};
use crate::model::{Property, Stream, TextDocument, Value};

/// One track a baked property becomes: its target's name, the component
/// target it sets when it sets one, and its keys.
pub(super) struct BakedTrack {
    pub name: String,
    pub component: Option<Target>,
    pub written: Written,
}

impl<'p> Walk<'p> {
    /// The samples of `property`'s expression when it has one that is on
    /// and was evaluated; one that was not is reported.
    pub(super) fn evaluated(&mut self, property: &Property, place: &Place<'_>) -> Option<&'p Bake> {
        if !property.expression.as_ref().is_some_and(|e| e.enabled) {
            return None;
        }
        let bakes = self.bakes;
        let Some(bake) = bakes.get(property) else {
            let why = bakes
                .stopped
                .clone()
                .unwrap_or_else(|| "it was not evaluated".into());
            self.not_baked(place, property, None, &why);
            return None;
        };
        if let Err(why) = &bake.outcome {
            self.not_baked(place, property, Some(bake), why);
            return None;
        }
        Some(bake)
    }

    /// Reports an expression that is not baked; the keys under it stay.
    pub(super) fn not_baked(
        &mut self,
        place: &Place<'_>,
        property: &Property,
        bake: Option<&Bake>,
        why: &str,
    ) {
        self.mapping.unsupported(
            &place.at,
            format!("an expression it could not bake: {why} (the keys under it are kept)"),
        );
        self.mapping.expressions.push(ExpressionReport {
            at: place.at.clone(),
            source: property
                .expression
                .as_ref()
                .map(|e| e.text.clone())
                .unwrap_or_default(),
            engine: bake.map_or("", |b| engine_name(b.dialect)).to_owned(),
            frames: 0,
            outcome: Outcome::NotBaked {
                why: why.to_owned(),
            },
        });
    }

    /// The tracks `bake` becomes on a property written to `target`
    /// (`found` in the registry) through `convert`, and its third number to
    /// `depth` where the row splits a 3D vector, with its report, or `None`,
    /// reported, when its values are not of the kind the row takes. An
    /// orientation, whose keys turn the shortest way rather than angle by
    /// angle, is keyed at every frame it was evaluated.
    pub(super) fn baked_tracks(
        &mut self,
        bake: &Bake,
        property: &Property,
        (target, found): (&str, Option<Target>),
        depth: Option<(&str, Target)>,
        convert: Convert,
        place: &Place<'_>,
    ) -> Option<Vec<BakedTrack>> {
        let Ok(samples) = &bake.outcome else {
            return None;
        };
        let Sampled::Numbers {
            channels,
            tolerances,
        } = &samples.values
        else {
            self.not_baked(
                place,
                property,
                Some(bake),
                "it gives text for a value that is not text",
            );
            return None;
        };
        let range = found.and_then(|t| t.prop.spec().range);
        let (channels, clamped) = clamp_samples(channels, range, convert);
        if clamped > 0
            && let Some(range) = range
        {
            self.mapping.clamped(
                &place.at,
                format!(
                    "{} of {} an expression gives outside {}",
                    plural(clamped, "frame"),
                    samples.times.len(),
                    range_text(range)
                ),
            );
        }
        let template = property.value.value_at(samples.times[0]);
        // Keys are fitted in the layer's own time, which they are written in.
        let (times, channels) = on_layer_clock(&samples.times, &channels, place.clock);
        let spatial = found.is_some_and(Target::moves_in_space);
        let mut tracks = Vec::new();
        let mut errors = Vec::new();
        let mut keys = 0;
        // The depth of a split vector fits on its own, as a number.
        let (channels, tolerances) = match depth {
            Some((name, target)) if channels.len() > 2 && tolerances.len() > 2 => {
                let one = fit(&times, &channels[2], tolerances[2]);
                errors.push(one.max_error);
                let fits = std::slice::from_ref(&one);
                let written = match still(fits) {
                    true => {
                        Written::Value(Convert::Depth.value(&Value::Number(one.keys[0].value))?)
                    }
                    false => {
                        keys += one.keys.len();
                        fitted(fits, &Value::Number(0.0), Convert::Depth)?
                    }
                };
                tracks.push(BakedTrack {
                    name: name.to_owned(),
                    component: target.component.is_some().then_some(target),
                    written,
                });
                (&channels[..2], &tolerances[..2])
            }
            _ => (&channels[..], &tolerances[..]),
        };
        if found.is_some_and(|t| t.value_type() == ValueType::Orientation) {
            keys = samples.times.len();
            tracks.push(BakedTrack {
                name: target.to_owned(),
                component: None,
                written: every_frame(&times, channels, &template, convert)?,
            });
        } else if spatial && let Some(found) = found {
            let names = found.prop.spec().components;
            for (i, name) in names.iter().enumerate() {
                let (Some(values), Some(&tolerance)) = (channels.get(i), tolerances.get(i)) else {
                    continue;
                };
                let one = fit(&times, values, tolerance);
                errors.push(one.max_error);
                let component = Target {
                    component: Some(i),
                    ..found
                };
                let fits = std::slice::from_ref(&one);
                let template = Value::Number(0.0);
                let written = match still(fits) {
                    true => Written::Value(
                        convert
                            .component(i)
                            .value(&template.with_numbers(&[one.keys[0].value]))?,
                    ),
                    false => {
                        keys += one.keys.len();
                        fitted(fits, &template, convert.component(i))?
                    }
                };
                tracks.push(BakedTrack {
                    name: (*name).to_owned(),
                    component: Some(component),
                    written,
                });
            }
        } else {
            let views: Vec<&[f64]> = channels.iter().map(Vec::as_slice).collect();
            let fits: Vec<Fitted> = fit_together(&times, &views, tolerances);
            errors = fits.iter().map(|f| f.max_error).collect();
            let written = match still(&fits) {
                true => {
                    let numbers: Vec<f64> = fits.iter().map(|f| f.keys[0].value).collect();
                    convert
                        .value(&template.with_numbers(&numbers))
                        .map(Written::Value)
                }
                false => {
                    keys = fits.first().map_or(0, |f| f.keys.len());
                    fitted(&fits, &template, convert)
                }
            };
            let Some(written) = written else {
                self.not_baked(
                    place,
                    property,
                    Some(bake),
                    "its values are not of the kind the scene takes here",
                );
                return None;
            };
            tracks.push(BakedTrack {
                name: target.to_owned(),
                component: found.filter(|t| t.component.is_some()),
                written,
            });
        }
        self.mapping.expressions.push(ExpressionReport {
            at: place.at.clone(),
            source: bake.source.clone(),
            engine: engine_name(bake.dialect).to_owned(),
            frames: samples.times.len(),
            outcome: Outcome::Baked {
                approximate: samples.approximate,
                keys_before: key_count(&property.value),
                keys,
                max_error: errors,
            },
        });
        Some(tracks)
    }

    /// Marks the one still value of `tracks` as read from a control when
    /// `source` only reads one ([`super::slots`]), to become a reference
    /// once every read is known.
    pub(super) fn follow_slot(
        &mut self,
        source: &str,
        convert: Convert,
        tracks: &mut [BakedTrack],
    ) {
        let [
            BakedTrack {
                component: None,
                written: Written::Value(value),
                ..
            },
        ] = tracks
        else {
            return;
        };
        let report = self.mapping.expressions.len().saturating_sub(1);
        if let Some(marked) = self.read_control(source, (value, convert), report) {
            *value = marked;
        }
    }

    /// The document a source text expression sets, when its text never
    /// changes; one that changes keeps its first frame's text, reported.
    pub(super) fn baked_document(
        &mut self,
        bake: &Bake,
        document: &TextDocument,
        property: &Property,
        place: &Place<'_>,
    ) -> Option<TextDocument> {
        let Ok(Samples {
            values: Sampled::Text(texts),
            times,
            approximate,
            ..
        }) = &bake.outcome
        else {
            self.not_baked(
                place,
                property,
                Some(bake),
                "it gives a number for source text",
            );
            return None;
        };
        let first = texts.first()?.replace('\r', "\n");
        let changes = texts.iter().any(|t| t.replace('\r', "\n") != first);
        if changes {
            self.mapping.unsupported(
                &place.at,
                "source text that an expression changes over time (its first frame's text is kept)",
            );
        }
        self.mapping.expressions.push(ExpressionReport {
            at: place.at.clone(),
            source: bake.source.clone(),
            engine: engine_name(bake.dialect).to_owned(),
            frames: times.len(),
            outcome: Outcome::Baked {
                approximate: *approximate,
                keys_before: key_count(&property.value),
                keys: usize::from(changes) + 1,
                max_error: Vec::new(),
            },
        });
        Some(with_text(document, &first))
    }
}

/// `document` with `text` in place of its own: unchanged when it is the
/// same, else in the style of its first paragraph and first characters,
/// with no stored layout, which no longer applies.
fn with_text(document: &TextDocument, text: &str) -> TextDocument {
    if document.text == text {
        return document.clone();
    }
    let whole = 0..text.len();
    let mut changed = document.clone();
    changed.text = text.to_owned();
    changed.paragraphs.truncate(1);
    changed.characters.truncate(1);
    for run in &mut changed.paragraphs {
        run.range = whole.clone();
    }
    for run in &mut changed.characters {
        run.range = whole.clone();
    }
    changed.lines.clear();
    changed
}

/// `channels` with every sample clamped into `range`, given in the scene's
/// units, which are the project's times `factor`: the original clamps what
/// an expression gives at each frame. Also how many frames were clamped.
fn clamp_samples(
    channels: &[Vec<f64>],
    range: Option<(f64, f64)>,
    convert: Convert,
) -> (std::borrow::Cow<'_, [Vec<f64>]>, usize) {
    let Some((lo, hi)) = range.filter(|_| (0..channels.len()).all(|i| convert.factor(i) > 0.0))
    else {
        return (std::borrow::Cow::Borrowed(channels), 0);
    };
    let bounds = |i: usize| (lo / convert.factor(i), hi / convert.factor(i));
    let frames = channels.first().map_or(0, Vec::len);
    let outside = |f: usize| {
        channels.iter().enumerate().any(|(i, c)| {
            let (lo, hi) = bounds(i);
            c.get(f).is_some_and(|&v| v < lo || v > hi)
        })
    };
    let clamped = (0..frames).filter(|&f| outside(f)).count();
    if clamped == 0 {
        return (std::borrow::Cow::Borrowed(channels), 0);
    }
    let owned = channels
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let (lo, hi) = bounds(i);
            c.iter().map(|v| v.clamp(lo, hi)).collect()
        })
        .collect();
    (std::borrow::Cow::Owned(owned), clamped)
}

/// Whether fitted channels hold one value all the time: each a constant.
fn still(fits: &[Fitted]) -> bool {
    fits.iter().all(|f| {
        f.keys.iter().all(|k| {
            k.value == f.keys[0].value && k.slope_in == 0.0 && k.slope_out == 0.0 && !k.hold
        })
    })
}

/// How many keys a stream has.
pub(super) fn key_count(stream: &Stream) -> usize {
    match stream {
        Stream::Static(_) => 0,
        Stream::Animated(keys) => keys.len(),
    }
}

/// The report's name for a dialect.
pub(super) fn engine_name(dialect: Dialect) -> &'static str {
    match dialect {
        Dialect::Modern => "modern",
        Dialect::Legacy => "legacy",
    }
}

#[cfg(test)]
#[path = "../../tests/unit/map/expressions.rs"]
mod tests;
