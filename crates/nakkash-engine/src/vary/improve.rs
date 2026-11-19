//! Improvement ([D78]): the check's craft repairs, each kept only when the
//! report after it is no worse on any rule and layer, and the finding it
//! targets is gone or measured better. Judged by the check's numbers, never
//! by looking at frames ([D20], [D22]).
//!
//! | Rule | Repair |
//! |---|---|
//! | `judder` | `mblur` on the layer |
//! | `easing` | the layer's constant-speed keys get `in-out-cubic` |
//! | `contrast` | the text's fill moves in lightness, hue kept |
//!
//! [D20]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d20-quality-is-measured-in-five-layers
//! [D22]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d22-no-ai-judge
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

use std::collections::BTreeSet;

use nakkash_format::Result;
use nakkash_query::{Code, Report, Violation};
use serde_json::{Map, Value as Json, json};

use super::Made;
use super::colour;
use super::places::{self, Held, Place, Visit};
use crate::session::Session;

/// The most repairs tried one at a time, per rule, when trying them all
/// at once is not kept.
const MOST_ALONE: usize = 6;

/// The most texts whose contrast a variant repairs.
const MOST_REPAIRED: usize = 16;

/// The rules a repair exists for, in the order they are tried.
const REPAIRED: [Code; 3] = [Code::Judder, Code::Easing, Code::Contrast];

/// One repair: what it changes on a layer.
#[derive(Debug, Clone, PartialEq)]
enum Repair {
    /// Motion blur on.
    Blur,
    /// Curves on the constant-speed keys of its moves.
    Ease,
    /// Its fill at this luminance.
    Relight(f64),
    /// Its fill this colour: one of the scene's own, under a palette lock.
    Paint(colour::Rgba),
}

/// Improves `made` in place by the repairs its report asks for, and says
/// what was kept.
pub(crate) fn improve(made: &mut Made, master: &Session) -> Result<String> {
    let held = Held::of(master.scene(), master.document().raw());
    let mut report = made.session.full_check()?;
    let before = report.violations.len();
    let mut kept: Vec<String> = Vec::new();
    let mut tried = 0;
    for code in REPAIRED {
        let found: Vec<Violation> = report
            .violations
            .iter()
            .filter(|v| v.code == code)
            .cloned()
            .collect();
        if found.is_empty() {
            continue;
        }
        let places: Vec<(Place, Violation)> = found
            .into_iter()
            .filter_map(|v| {
                let ids: Vec<String> = v.address.layers.iter().map(ToString::to_string).collect();
                let place = places::resolve(made.session.document().raw(), None, &ids)?;
                (!held.holds(&place)).then_some((place, v))
            })
            .collect();
        let mut unique: Vec<(Place, Violation)> = Vec::new();
        let mut seen = BTreeSet::new();
        for (p, v) in places {
            if seen.insert(p.clone()) {
                unique.push((p, v));
            }
        }
        let options: Vec<Vec<Repair>> = match code {
            Code::Judder => vec![vec![Repair::Blur]],
            Code::Easing => vec![vec![Repair::Ease]],
            // Lighter, then darker: the gate says which reads.
            _ => vec![vec![Repair::Relight(1.0)], vec![Repair::Relight(0.0)]],
        };
        for option in &options {
            // All of the rule's findings at once first.
            let all: Vec<(Place, Repair)> = unique
                .iter()
                .map(|(p, _)| (p.clone(), option[0].clone()))
                .collect();
            tried += 1;
            if let Some((session, r)) = attempt(made, &report, &all, &unique, code)? {
                kept.push(format!("{code} {} layers", unique.len()));
                made.session = session;
                report = r;
                break;
            }
            // Then one by one, the first few.
            let mut any = false;
            for (p, v) in unique.iter().take(MOST_ALONE) {
                tried += 1;
                let one = [(p.clone(), option[0].clone())];
                if let Some((session, r)) = attempt(
                    made,
                    &report,
                    &one,
                    std::slice::from_ref(&(p.clone(), v.clone())),
                    code,
                )? {
                    kept.push(format!("{code} {}", v.address));
                    made.session = session;
                    report = r;
                    any = true;
                }
            }
            if any {
                break;
            }
        }
    }
    Ok(format!(
        "improve: {} repairs kept of {tried} tried, findings {before} to {}{}",
        kept.len(),
        report.violations.len(),
        match kept.is_empty() {
            true => String::new(),
            false => format!(" ({})", kept.join(", ")),
        }
    ))
}

/// Repairs the contrast errors `made` adds to `baseline`, its master's
/// report, given `report`, its own: each text's fill moves to the lightest
/// or darkest of its hue, or of the scene's colours under a `palette` lock,
/// whichever clears it and adds nothing ([D78]). Every faint text tries a
/// choice at once, so a variant is checked a few times, not once a text.
/// Says what it did, or `None` when there was nothing to repair, with the
/// report the variant has now.
///
/// [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error
pub(crate) fn repair_contrast(
    made: &mut Made,
    master: &Session,
    baseline: &Report,
    report: Report,
) -> Result<(Option<String>, Report)> {
    let held = Held::of(master.scene(), master.document().raw());
    let mut current = report;
    let mut gated = super::gate(baseline, current.clone(), made);
    let errors = |g: &super::Gated| {
        g.new
            .violations
            .iter()
            .filter(|v| v.severity == nakkash_query::Severity::Error)
            .count()
    };
    let targets: Vec<(Place, Violation)> = gated
        .new
        .violations
        .iter()
        .filter(|v| v.code == Code::Contrast && v.severity == nakkash_query::Severity::Error)
        .filter_map(|v| {
            let ids: Vec<String> = v.address.layers.iter().map(ToString::to_string).collect();
            let place = places::resolve(made.session.document().raw(), None, &ids)?;
            (!held.holds(&place)).then(|| (place, v.clone()))
        })
        .take(MOST_REPAIRED)
        .collect();
    if targets.is_empty() {
        return Ok((None, current));
    }
    // Under a palette lock, the lightest and darkest of the scene's own
    // colours; else the lightest and darkest of the text's hue.
    let options: Vec<Repair> = match held.palette {
        true => {
            let palette = super::palette::gather(master.document().raw(), &Held::default(), &[]);
            let by = |e: &&super::palette::Entry| colour::luminance(e.colour);
            let light = palette.iter().max_by(|a, b| by(a).total_cmp(&by(b)));
            let dark = palette.iter().min_by(|a, b| by(a).total_cmp(&by(b)));
            [light, dark]
                .into_iter()
                .flatten()
                .map(|e| Repair::Paint(e.colour))
                .collect()
        }
        false => vec![Repair::Relight(1.0), Repair::Relight(0.0)],
    };
    let faint = |g: &super::Gated, v: &Violation| {
        g.new.violations.iter().any(|n| {
            n.code == Code::Contrast
                && n.severity == nakkash_query::Severity::Error
                && n.address == v.address
        })
    };
    let mut open: Vec<usize> = (0..targets.len()).collect();
    let mut fixed = 0;
    for option in &options {
        if open.is_empty() {
            break;
        }
        // Every open text takes this choice, to see which it clears ...
        let all: Vec<(&Place, &Repair)> = open.iter().map(|&i| (&targets[i].0, option)).collect();
        let Some(trial) = relit(made, &all) else {
            continue;
        };
        let after = super::gate(baseline, trial.1.clone(), &trial.0);
        let cleared: Vec<usize> = open
            .iter()
            .copied()
            .filter(|&i| !faint(&after, &targets[i].1))
            .collect();
        if cleared.is_empty() {
            continue;
        }
        // ... and keeps it for those alone, when that adds nothing.
        let (trial, after) = match cleared.len() == open.len() {
            true => (trial, after),
            false => {
                let some: Vec<(&Place, &Repair)> =
                    cleared.iter().map(|&i| (&targets[i].0, option)).collect();
                let Some(trial) = relit(made, &some) else {
                    continue;
                };
                let after = super::gate(baseline, trial.1.clone(), &trial.0);
                (trial, after)
            }
        };
        if errors(&after) < errors(&gated)
            && after.new.violations.len() <= gated.new.violations.len()
        {
            let (variant, report) = trial;
            made.session = variant.session;
            current = report;
            gated = after;
            fixed += cleared.len();
            open.retain(|i| !cleared.contains(i));
        }
    }
    Ok((
        Some(format!(
            "contrast repaired on {fixed} of {} texts the variant made too faint",
            targets.len()
        )),
        current,
    ))
}

/// `made` with each repair made on its text, and its full report; `None`
/// when nothing changed or the scene does not check.
fn relit(made: &Made, repairs: &[(&Place, &Repair)]) -> Option<(Made, Report)> {
    let mut raw = made.session.document().raw().clone();
    let mut changed = false;
    places::each_mut(&mut raw, |visit| {
        if let Visit::Layer { place, layer } = visit {
            for (p, repair) in repairs {
                if *p == place {
                    changed |= repair_layer(layer, repair);
                }
            }
        }
    });
    if !changed {
        return None;
    }
    let session = made.session.with_raw(raw).ok()?;
    let report = session.full_check().ok()?;
    let variant = Made {
        session,
        notes: Vec::new(),
        wrapped: made.wrapped.clone(),
    };
    Some((variant, report))
}

/// The session with `repairs` made, and its report, when that report adds
/// nothing to `report` and clears or betters a finding of `code` among
/// `targets`; `None` otherwise.
fn attempt(
    made: &Made,
    report: &Report,
    repairs: &[(Place, Repair)],
    targets: &[(Place, Violation)],
    _code: Code,
) -> Result<Option<(Session, Report)>> {
    let mut raw = made.session.document().raw().clone();
    let mut changed = false;
    places::each_mut(&mut raw, |visit| {
        if let Visit::Layer { place, layer } = visit {
            for (p, repair) in repairs {
                if p == place {
                    changed |= repair_layer(layer, repair);
                }
            }
        }
    });
    if !changed {
        return Ok(None);
    }
    let Ok(session) = made.session.with_raw(raw) else {
        return Ok(None);
    };
    let Ok(after) = session.full_check() else {
        return Ok(None);
    };
    let mut new = after.clone();
    new.since(report);
    if !new.violations.is_empty() {
        return Ok(None);
    }
    // At least one target is gone, or measured better: the report after
    // has nothing as bad on its layer.
    let better = targets.iter().any(|(_, v)| !after.knows(v));
    Ok(better.then_some((session, after)))
}

/// Makes `repair` on `layer`; says whether anything changed.
fn repair_layer(layer: &mut Map<String, Json>, repair: &Repair) -> bool {
    match repair {
        Repair::Blur => layer.insert("mblur".into(), json!(true)) != Some(json!(true)),
        Repair::Ease => {
            let mut changed = false;
            if let Some(Json::Object(anim)) = layer.get_mut("anim") {
                for (name, track) in anim.iter_mut() {
                    if matches!(name.as_str(), "position" | "x" | "y" | "anchor") {
                        changed |= ease(track);
                    }
                }
            }
            changed
        }
        Repair::Relight(_) | Repair::Paint(_) => {
            let paint = |fill: &mut Json| match repair {
                Repair::Paint(c) => {
                    *fill = json!(colour::hex(*c));
                    true
                }
                Repair::Relight(y) => relight(fill, *y),
                _ => false,
            };
            let mut changed = false;
            // An effect's fill paints over the layer's own: move that one.
            if let Some(Json::Array(fx)) = layer.get_mut("fx") {
                for item in fx.iter_mut().filter_map(Json::as_object_mut) {
                    if let Some(fill) = item.get_mut("fill") {
                        changed |= paint(fill);
                    }
                }
            }
            if !changed && let Some(fill) = layer.get_mut("fill") {
                changed |= paint(fill);
            }
            if !changed && layer.contains_key("text") {
                let mut fill = json!("#808080");
                changed = paint(&mut fill);
                layer.insert("fill".into(), fill);
            }
            changed
        }
    }
}

/// `fill` moved to luminance `y`, hue kept; a slot it follows is left, the
/// fill no longer following it.
fn relight(fill: &mut Json, y: f64) -> bool {
    let colour = match &*fill {
        Json::String(s) if s.starts_with('$') => Some([0.5, 0.5, 0.5, 1.0]),
        other => nakkash_format::color(other),
    };
    let Some(c) = colour else { return false };
    let to = colour::relit(c, y);
    *fill = json!(colour::hex(to));
    true
}

/// Gives every constant-speed key of `track` an ease; says whether any
/// changed.
fn ease(track: &mut Json) -> bool {
    const CURVE: &str = "in-out-cubic";
    match track {
        // `a -> b @0-30f`, with no curve after the range.
        Json::String(s) => {
            let Some((_, after)) = s.rsplit_once('@') else {
                return false;
            };
            let words: Vec<&str> = after.split_whitespace().collect();
            match words.len() {
                1 => {
                    s.push(' ');
                    s.push_str(CURVE);
                    true
                }
                _ if words.get(1) == Some(&"linear") => {
                    *s = s.replacen(" linear", &format!(" {CURVE}"), 1);
                    true
                }
                _ => false,
            }
        }
        Json::Array(keys) => {
            let mut changed = false;
            let last = keys.len().saturating_sub(1);
            for key in keys.iter_mut().take(last) {
                match key {
                    Json::Array(k) => match k.get(2) {
                        None => {
                            k.push(json!(CURVE));
                            changed = true;
                        }
                        Some(Json::String(c)) if c == "linear" => {
                            k[2] = json!(CURVE);
                            changed = true;
                        }
                        _ => {}
                    },
                    Json::Object(k) => match k.get("curve") {
                        None => {
                            k.insert("curve".into(), json!(CURVE));
                            changed = true;
                        }
                        Some(Json::String(c)) if c == "linear" => {
                            k.insert("curve".into(), json!(CURVE));
                            changed = true;
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            changed
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/vary/improve.rs"]
mod tests;
