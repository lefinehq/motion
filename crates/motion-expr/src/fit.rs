//! Samples back to keys, one channel at a time.
//!
//! - **The curve.** Between two keys the curve is a cubic Hermite in time:
//!   the keys' values and two slopes. With handles reaching a third of the
//!   way, a cubic Bézier in time is exactly this, so the keys map onto the
//!   scene's curves without loss. With both values pinned to the samples,
//!   the best slopes are a closed-form least-squares solve.
//! - **The keys.** Two searches run and the one with fewer keys wins.
//!   - Top-down: a run is split at its worst sample until every sample is
//!     within the tolerance.
//!   - Bottom-up: every sample starts as a key, as curve_fit_nd refits.
//!
//!   Both then remove keys, the one whose removal adds the least error
//!   first, while every sample stays within the tolerance. Top-down does
//!   better on oscillations, bottom-up on steep eases.
//! - **Jumps** (a value that steps, such as a posterized one) become a held
//!   segment, and no curve is fitted across one.

/// One key of a fitted channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// Its time.
    pub time: f64,
    /// Its value.
    pub value: f64,
    /// The slope arriving at it, in units a second.
    pub slope_in: f64,
    /// The slope leaving it.
    pub slope_out: f64,
    /// Whether the value holds from this key to the next, then jumps.
    pub hold: bool,
}

/// A fitted channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Fitted {
    /// Its keys, in time order.
    pub keys: Vec<Key>,
    /// The largest distance from a sample to the curve.
    pub max_error: f64,
}

/// Fits `values`, sampled at `times` (increasing), within `tolerance`.
///
/// # Panics
/// When `times` and `values` differ in length or are empty.
pub fn fit(times: &[f64], values: &[f64], tolerance: f64) -> Fitted {
    assert_eq!(times.len(), values.len(), "a time for every value");
    assert!(!times.is_empty(), "at least one sample");
    let samples = Samples { times, values };
    let n = times.len();
    let (lo, hi) = values
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        });
    if hi - lo <= 2.0 * tolerance {
        let value = (lo + hi) / 2.0;
        let keys = [0, n - 1]
            .into_iter()
            .take(if n == 1 { 1 } else { 2 })
            .map(|i| Key {
                time: times[i],
                value,
                slope_in: 0.0,
                slope_out: 0.0,
                hold: false,
            })
            .collect();
        let max_error = values.iter().map(|v| (v - value).abs()).fold(0.0, f64::max);
        return Fitted { keys, max_error };
    }
    // Keys every fit keeps: the ends, and both sides of each jump.
    let jumps = jumps(values, tolerance);
    let mut fixed = vec![0];
    for &j in &jumps {
        fixed.extend([j - 1, j]);
    }
    fixed.push(n - 1);
    fixed.dedup();
    let mut top_down = fixed.clone();
    for pair in fixed.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if !(b == a + 1 && jumps.contains(&b)) {
            split(&samples, a, b, tolerance, &mut top_down);
        }
    }
    top_down.sort_unstable();
    top_down.dedup();
    let mut bottom_up: Vec<usize> = (0..n).collect();
    for cuts in [&mut top_down, &mut bottom_up] {
        thin(&samples, cuts, &fixed, &jumps, tolerance);
    }
    let cuts = if bottom_up.len() < top_down.len() {
        bottom_up
    } else {
        top_down
    };
    build(&samples, &cuts, &jumps)
}

#[derive(Clone, Copy)]
struct Samples<'a> {
    times: &'a [f64],
    values: &'a [f64],
}

/// The best fit of samples `a..=b`: its two slopes, its largest error and
/// where.
struct Segment {
    slope_out: f64,
    slope_in: f64,
    error: f64,
    worst: usize,
}

/// Hermite basis at `u`: the weights of the start value, start slope, end
/// value and end slope (slopes per unit of `u`).
fn basis(u: f64) -> [f64; 4] {
    let (u2, u3) = (u * u, u * u * u);
    [
        2.0 * u3 - 3.0 * u2 + 1.0,
        u3 - 2.0 * u2 + u,
        -2.0 * u3 + 3.0 * u2,
        u3 - u2,
    ]
}

fn segment(s: &Samples<'_>, a: usize, b: usize) -> Segment {
    let (t0, t1) = (s.times[a], s.times[b]);
    let (p0, p1) = (s.values[a], s.values[b]);
    let h = t1 - t0;
    let secant = (p1 - p0) / h;
    let (mut saa, mut sab, mut sbb, mut sar, mut sbr) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for i in a + 1..b {
        let w = basis((s.times[i] - t0) / h);
        let r = s.values[i] - w[0] * p0 - w[2] * p1;
        let (x, y) = (w[1] * h, w[3] * h);
        saa += x * x;
        sab += x * y;
        sbb += y * y;
        sar += x * r;
        sbr += y * r;
    }
    let det = saa * sbb - sab * sab;
    let (m0, m1) = if b - a < 2 {
        (secant, secant)
    } else if det.abs() > 1e-12 * (saa * sbb).max(f64::MIN_POSITIVE) {
        ((sbb * sar - sab * sbr) / det, (saa * sbr - sab * sar) / det)
    } else {
        // Too few samples for two slopes: one slope for both ends.
        let (sss, ssr) = (saa + 2.0 * sab + sbb, sar + sbr);
        let m = if sss > 0.0 { ssr / sss } else { secant };
        (m, m)
    };
    let mut error = 0.0;
    let mut worst = a;
    for i in a..=b {
        let w = basis((s.times[i] - t0) / h);
        let fitted = w[0] * p0 + w[1] * h * m0 + w[2] * p1 + w[3] * h * m1;
        let e = (s.values[i] - fitted).abs();
        if e > error {
            error = e;
            worst = i;
        }
    }
    Segment {
        slope_out: m0,
        slope_in: m1,
        error,
        worst,
    }
}

/// Indices `j` where the value jumps from sample `j - 1` to `j`: a step
/// well past the tolerance and far larger than the steps around it.
fn jumps(values: &[f64], tolerance: f64) -> Vec<usize> {
    let step = |j: usize| -> f64 {
        match j {
            0 => 0.0,
            j if j < values.len() => (values[j] - values[j - 1]).abs(),
            _ => 0.0,
        }
    };
    (1..values.len())
        .filter(|&j| {
            let d = step(j);
            d > 4.0 * tolerance && d > 6.0 * step(j - 1).max(step(j + 1))
        })
        .collect()
}

/// Adds cuts inside `a..=b` until every segment is within the tolerance.
fn split(s: &Samples<'_>, a: usize, b: usize, tolerance: f64, cuts: &mut Vec<usize>) {
    if b <= a + 1 {
        return;
    }
    let fitted = segment(s, a, b);
    if fitted.error <= tolerance {
        return;
    }
    let at = if fitted.worst > a && fitted.worst < b {
        fitted.worst
    } else {
        (a + b) / 2
    };
    cuts.push(at);
    split(s, a, at, tolerance, cuts);
    split(s, at, b, tolerance, cuts);
}

/// Removes cuts that are not `fixed`, least added error first, while the
/// merged segment stays within the tolerance. Each removal changes only its
/// neighbours' costs, so only those are measured again.
fn thin(s: &Samples<'_>, cuts: &mut Vec<usize>, fixed: &[usize], jumps: &[usize], tolerance: f64) {
    let removable = |cuts: &[usize], k: usize| {
        k > 0 && k + 1 < cuts.len() && !fixed.contains(&cuts[k]) && !jumps.contains(&cuts[k + 1])
    };
    let cost = |cuts: &[usize], k: usize| match removable(cuts, k) {
        true => segment(s, cuts[k - 1], cuts[k + 1]).error,
        false => f64::INFINITY,
    };
    let mut costs: Vec<f64> = (0..cuts.len()).map(|k| cost(cuts, k)).collect();
    loop {
        let best = costs
            .iter()
            .enumerate()
            .min_by(|x, y| x.1.total_cmp(y.1))
            .filter(|&(_, &e)| e <= tolerance)
            .map(|(k, _)| k);
        let Some(k) = best else { return };
        cuts.remove(k);
        costs.remove(k);
        for j in [k.wrapping_sub(1), k] {
            if j < cuts.len() {
                costs[j] = cost(cuts, j);
            }
        }
    }
}

fn build(s: &Samples<'_>, cuts: &[usize], jumps: &[usize]) -> Fitted {
    let mut keys: Vec<Key> = cuts
        .iter()
        .map(|&i| Key {
            time: s.times[i],
            value: s.values[i],
            slope_in: 0.0,
            slope_out: 0.0,
            hold: false,
        })
        .collect();
    let mut max_error: f64 = 0.0;
    for k in 0..cuts.len().saturating_sub(1) {
        let (a, b) = (cuts[k], cuts[k + 1]);
        if b == a + 1 && jumps.contains(&b) {
            keys[k].hold = true;
            continue;
        }
        let fitted = segment(s, a, b);
        keys[k].slope_out = fitted.slope_out;
        keys[k + 1].slope_in = fitted.slope_in;
        max_error = max_error.max(fitted.error);
    }
    Fitted { keys, max_error }
}

/// The fitted curve's value at `time`.
pub fn value_at(keys: &[Key], time: f64) -> f64 {
    let Some(first) = keys.first() else {
        return 0.0;
    };
    if time <= first.time {
        return first.value;
    }
    for pair in keys.windows(2) {
        let (k0, k1) = (pair[0], pair[1]);
        if time < k1.time {
            if k0.hold {
                return k0.value;
            }
            let h = k1.time - k0.time;
            let w = basis((time - k0.time) / h);
            return w[0] * k0.value
                + w[1] * h * k0.slope_out
                + w[2] * k1.value
                + w[3] * h * k1.slope_in;
        }
    }
    keys.last().map_or(0.0, |k| k.value)
}

#[cfg(test)]
#[path = "../tests/unit/fit.rs"]
mod tests;
