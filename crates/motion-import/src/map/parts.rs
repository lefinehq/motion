//! An expression selector becomes a `parts` selector ([D64]): one `part`
//! item per part of its layer's text, each keyed with the amount the
//! selector gave that part. With an expression, the amount is the bake's
//! for each part; without one, every part takes the amount's own value.
//!
//! [D64]: https://github.com/lefinehq/motion/blob/main/docs/design/decisions.md#d64-a-selector-may-choose-each-part-by-its-own-amount

use motion_expr::fit::{fit, fit_together};
use motion_format::item::PART;
use serde_json::{Map, Value as Json, json};

use super::Walk;
use super::expressions::{engine_name, key_count};
use super::items::{Place, plural};
use super::keys::{Convert, Written, fitted};
use super::report::{ExpressionReport, Outcome};
use crate::bake::{Sampled, Samples, address, dialect};
use crate::model::{Group, Property, Stream, Value};
use crate::parts::{self, Counted};

/// How far, in percent, a part's fitted amount may stray from a sample.
const TOLERANCE: f64 = 0.05;

/// A part's samples: their times, and its amounts across and down there.
type Series = (Vec<f64>, [Vec<f64>; 2]);

impl Walk<'_> {
    /// Counts the parts of every expression selector in `text`, a layer's
    /// `ADBE Text Properties`, for [`Walk::part_items`].
    pub(super) fn count_parts(&mut self, text: &Group) {
        for amount in text.properties().filter(|p| p.match_name == parts::AMOUNT) {
            let counted = parts::count(text, amount);
            self.part_counts.insert(address(amount), counted);
        }
    }

    /// The `part` items of the expression selector whose amount is
    /// `amount`, and the count they follow; none, reported, when its parts
    /// or their amounts cannot be known.
    pub(super) fn part_items(
        &mut self,
        amount: &Property,
        place: &Place<'_>,
    ) -> Option<(&'static str, Vec<Json>)> {
        let counted = self
            .part_counts
            .get(&address(amount))
            .cloned()
            .unwrap_or_else(|| Err("its text could not be found".to_owned()));
        let Counted { by, total } = match counted {
            Ok(counted) => counted,
            Err(why) => return self.no_parts(place, &why),
        };
        let series = match amount.expression.as_ref().is_some_and(|e| e.enabled) {
            true => self.baked_parts(amount, total, place)?,
            false => vec![held(amount, place.frame_rate); total],
        };
        let mut clamped = 0;
        let mut keys = 0;
        let mut items = Vec::with_capacity(series.len());
        for (times, axes) in series {
            let axes = axes.map(|values| {
                values
                    .iter()
                    .map(|&v| {
                        let kept = v.clamp(-100.0, 100.0);
                        clamped += usize::from(kept != v);
                        kept
                    })
                    .collect::<Vec<f64>>()
            });
            let (item, written) = part_item(&times, &axes, place)?;
            keys += written;
            items.push(item);
        }
        if clamped > 0 {
            self.mapping.clamped(
                &place.at,
                format!(
                    "{} of a part's amount outside -100 % to 100 %",
                    plural(clamped, "value")
                ),
            );
        }
        if let Some(report) = self
            .mapping
            .expressions
            .last_mut()
            .filter(|r| r.at == place.at)
            && let Outcome::Baked { keys: written, .. } = &mut report.outcome
        {
            *written = keys;
        }
        Some((by, items))
    }

    /// Each part's samples of a baked amount: its times and its amounts
    /// across and down, in percent.
    fn baked_parts(
        &mut self,
        amount: &Property,
        total: usize,
        place: &Place<'_>,
    ) -> Option<Vec<Series>> {
        let baked = match self.bakes.parts(amount) {
            Some(baked) => baked.clone(),
            None => Err(self
                .bakes
                .stopped
                .clone()
                .unwrap_or_else(|| "it was not evaluated".into())),
        };
        let samples = match baked {
            Ok(samples) if samples.len() == total => samples,
            Ok(samples) => {
                let why = format!(
                    "it was evaluated for {} parts where the text has {total}",
                    samples.len()
                );
                return self.no_parts(place, &why);
            }
            Err(why) => return self.no_parts(place, &why),
        };
        let mut series = Vec::with_capacity(total);
        for Samples { times, values, .. } in &samples {
            let Sampled::Numbers { channels, .. } = values else {
                return self.no_parts(place, "it gives text");
            };
            let axis = |i: usize| channels.get(i).cloned().unwrap_or_default();
            series.push((times.clone(), [axis(0), axis(1)]));
        }
        let source = amount
            .expression
            .as_ref()
            .map(|e| e.text.clone())
            .unwrap_or_default();
        self.mapping.expressions.push(ExpressionReport {
            at: place.at.clone(),
            source,
            engine: engine_name(dialect(&self.project.expression_engine)).to_owned(),
            frames: samples.iter().map(|s| s.times.len()).sum(),
            outcome: Outcome::Baked {
                approximate: samples.iter().any(|s| s.approximate),
                keys_before: key_count(&amount.value),
                keys: 0,
                max_error: Vec::new(),
            },
        });
        Some(series)
    }

    /// Reports an expression selector whose parts cannot be written.
    fn no_parts<T>(&mut self, place: &Place<'_>, why: &str) -> Option<T> {
        self.mapping.unsupported(
            &place.at,
            format!("an expression selector it could not bake: {why} (it chooses nothing)"),
        );
        None
    }
}

/// The samples of an amount without an expression: its own values across
/// and down at every frame from its first key to its last, in percent, the
/// same for each part.
fn held(amount: &Property, frame_rate: f64) -> Series {
    let at = |t: f64, i: usize| {
        amount
            .value
            .value_at(t)
            .numbers()
            .and_then(|n| n.get(i).copied())
            .unwrap_or(100.0)
    };
    let times = match &amount.value {
        Stream::Animated(keys) if keys.len() > 1 && frame_rate > 0.0 => {
            let (from, to) = (keys[0].time, keys[keys.len() - 1].time);
            let step = 1.0 / frame_rate;
            let mut times: Vec<f64> = std::iter::successors(Some(from), |t| Some(t + step))
                .take_while(|&t| t < to)
                .collect();
            times.push(to);
            times
        }
        _ => vec![0.0],
    };
    let axes = [0, 1].map(|i| times.iter().map(|&t| at(t, i)).collect());
    (times, axes)
}

/// One `part` item from its amounts across and down, in percent, and how
/// many keys it holds: one number when both axes agree, else a point.
fn part_item(times: &[f64], [x, y]: &[Vec<f64>; 2], place: &Place<'_>) -> Option<(Json, usize)> {
    let together = x.iter().zip(y).all(|(a, b)| (a - b).abs() <= TOLERANCE);
    let (fits, template, convert) = match together {
        true => (
            vec![fit(times, x, TOLERANCE)],
            Value::Number(0.0),
            Convert::Percent,
        ),
        false => (
            fit_together(times, &[x, y], &[TOLERANCE; 2]),
            Value::Vector3([0.0; 3]),
            Convert::PointPercent,
        ),
    };
    let still = fits.iter().all(|f| {
        f.keys.iter().all(|k| {
            k.value == f.keys[0].value && k.slope_in == 0.0 && k.slope_out == 0.0 && !k.hold
        })
    });
    let count = fits.first().map_or(0, |f| f.keys.len());
    let written = match still {
        true => {
            let numbers: Vec<f64> = fits.iter().map(|f| f.keys[0].value).collect();
            convert
                .value(&template.with_numbers(&numbers))
                .map(Written::Value)?
        }
        false => fitted(&fits, &template, convert, place.clock)?,
    };
    let name = PART.rows[0].name;
    let mut keys = Map::new();
    match written {
        Written::Value(value) => {
            keys.insert("part".into(), value);
        }
        Written::Track { keys: track, first } => {
            keys.insert("part".into(), first);
            keys.insert("anim".into(), json!({ name: track }));
        }
    }
    Some((Json::Object(keys), if still { 1 } else { count }))
}
