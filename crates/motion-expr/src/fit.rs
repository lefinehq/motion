//! Samples back to keys: one channel, or several that share their key times,
//! as the components of one value do.
//!
//! - **The curve.** Between two keys the curve is a cubic Hermite in time:
//!   the keys' values and two slopes. With handles reaching a third of the
//!   way, a cubic Bézier in time is exactly this, so the keys map onto the
//!   scene's curves without loss. With both values pinned to the samples,
//!   the best slopes are a closed-form least-squares solve, per channel.
//! - **The keys.** Two searches run and the one with fewer keys wins.
//!   - Top-down: a run is split at its worst sample until every sample is
//!     within the tolerance.
//!   - Bottom-up: every sample starts as a key, as curve_fit_nd refits.
//!
//!   Both then remove keys, the one whose removal adds the least error
//!   first, while every sample stays within the tolerance. Top-down does
//!   better on oscillations, bottom-up on steep eases.
//! - **Several channels** share their keys. A segment's error is the worst
//!   of its channels', each measured against its own tolerance, so one
//!   search serves values whose components have different units.
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
    fit_together(times, &[values], &[tolerance])
        .pop()
        .expect("one channel in, one out")
}

/// Fits `channels`, each sampled at `times` (increasing), each within its
/// own tolerance, with keys at the same times in every channel.
///
/// # Panics
/// When a channel's length differs from `times`', `times` is empty, or
/// there is not one tolerance per channel.
pub fn fit_together(times: &[f64], channels: &[&[f64]], tolerances: &[f64]) -> Vec<Fitted> {
    assert!(!times.is_empty(), "at least one sample");
    assert_eq!(channels.len(), tolerances.len(), "a tolerance per channel");
    for channel in channels {
        assert_eq!(times.len(), channel.len(), "a time for every value");
    }
    let n = times.len();
    // A channel within twice its tolerance everywhere is a constant: its
    // middle value, which no key position changes.
    let flat: Vec<Option<f64>> = channels
        .iter()
        .zip(tolerances)
        .map(|(values, &tolerance)| {
            let (lo, hi) = values
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
                    (lo.min(v), hi.max(v))
                });
            (hi - lo <= 2.0 * tolerance).then_some((lo + hi) / 2.0)
        })
        .collect();
    let moving: Vec<usize> = (0..channels.len()).filter(|&c| flat[c].is_none()).collect();
    let samples = Samples {
        times,
        channels: moving.iter().map(|&c| channels[c]).collect(),
        tolerances: moving.iter().map(|&c| tolerances[c]).collect(),
    };
    let (cuts, jumps) = match moving.is_empty() {
        true => ((if n == 1 { vec![0] } else { vec![0, n - 1] }), Vec::new()),
        false => samples.cuts(),
    };
    let mut fitted = samples.build(&cuts, &jumps).into_iter();
    channels
        .iter()
        .zip(&flat)
        .map(|(values, flat)| match flat {
            Some(value) => Fitted {
                keys: cuts
                    .iter()
                    .map(|&i| Key {
                        time: times[i],
                        value: *value,
                        slope_in: 0.0,
                        slope_out: 0.0,
                        hold: false,
                    })
                    .collect(),
                max_error: values.iter().map(|v| (v - value).abs()).fold(0.0, f64::max),
            },
            None => fitted.next().expect("a fit per moving channel"),
        })
        .collect()
}

/// The channels being fitted, those that move.
struct Samples<'a> {
    times: &'a [f64],
    channels: Vec<&'a [f64]>,
    tolerances: Vec<f64>,
}

/// The best fit of samples `a..=b`: each channel's two slopes and largest
/// error, the worst error as a fraction of its tolerance, and where.
struct Segment {
    slopes: Vec<(f64, f64)>,
    errors: Vec<f64>,
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

impl Samples<'_> {
    /// The keys, as sample indices, and the jumps among them.
    fn cuts(&self) -> (Vec<usize>, Vec<usize>) {
        let n = self.times.len();
        let mut jumps: Vec<usize> = self
            .channels
            .iter()
            .zip(&self.tolerances)
            .flat_map(|(values, &tolerance)| jumps(values, tolerance))
            .collect();
        jumps.sort_unstable();
        jumps.dedup();
        // Keys every fit keeps: the ends, and both sides of each jump.
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
                self.split(a, b, &mut top_down);
            }
        }
        top_down.sort_unstable();
        top_down.dedup();
        let mut bottom_up: Vec<usize> = (0..n).collect();
        for cuts in [&mut top_down, &mut bottom_up] {
            self.thin(cuts, &fixed, &jumps);
        }
        let cuts = if bottom_up.len() < top_down.len() {
            bottom_up
        } else {
            top_down
        };
        (cuts, jumps)
    }

    fn segment(&self, a: usize, b: usize) -> Segment {
        let (t0, t1) = (self.times[a], self.times[b]);
        let h = t1 - t0;
        // The normal equations depend on the times only; each channel has
        // its own right-hand side.
        let (mut saa, mut sab, mut sbb) = (0.0, 0.0, 0.0);
        for i in a + 1..b {
            let w = basis((self.times[i] - t0) / h);
            let (x, y) = (w[1] * h, w[3] * h);
            saa += x * x;
            sab += x * y;
            sbb += y * y;
        }
        let det = saa * sbb - sab * sab;
        let solvable = det.abs() > 1e-12 * (saa * sbb).max(f64::MIN_POSITIVE);
        let mut segment = Segment {
            slopes: Vec::with_capacity(self.channels.len()),
            errors: Vec::with_capacity(self.channels.len()),
            error: 0.0,
            worst: a,
        };
        for (values, &tolerance) in self.channels.iter().zip(&self.tolerances) {
            let (p0, p1) = (values[a], values[b]);
            let secant = (p1 - p0) / h;
            let (mut sar, mut sbr) = (0.0, 0.0);
            for i in a + 1..b {
                let w = basis((self.times[i] - t0) / h);
                let r = values[i] - w[0] * p0 - w[2] * p1;
                sar += w[1] * h * r;
                sbr += w[3] * h * r;
            }
            let (m0, m1) = if b - a < 2 {
                (secant, secant)
            } else if solvable {
                ((sbb * sar - sab * sbr) / det, (saa * sbr - sab * sar) / det)
            } else {
                // Too few samples for two slopes: one slope for both ends.
                let (sss, ssr) = (saa + 2.0 * sab + sbb, sar + sbr);
                let m = if sss > 0.0 { ssr / sss } else { secant };
                (m, m)
            };
            let mut error: f64 = 0.0;
            for i in a..=b {
                let w = basis((self.times[i] - t0) / h);
                let fitted = w[0] * p0 + w[1] * h * m0 + w[2] * p1 + w[3] * h * m1;
                let e = (values[i] - fitted).abs();
                if e > error {
                    error = e;
                    if e / tolerance > segment.error {
                        segment.error = e / tolerance;
                        segment.worst = i;
                    }
                }
            }
            segment.slopes.push((m0, m1));
            segment.errors.push(error);
        }
        segment
    }

    /// Adds cuts inside `a..=b` until every segment is within the tolerance.
    fn split(&self, a: usize, b: usize, cuts: &mut Vec<usize>) {
        if b <= a + 1 {
            return;
        }
        let fitted = self.segment(a, b);
        if fitted.error <= 1.0 {
            return;
        }
        let at = if fitted.worst > a && fitted.worst < b {
            fitted.worst
        } else {
            (a + b) / 2
        };
        cuts.push(at);
        self.split(a, at, cuts);
        self.split(at, b, cuts);
    }

    /// Removes cuts that are not `fixed`, least added error first, while the
    /// merged segment stays within the tolerance. Each removal changes only
    /// its neighbours' costs, so only those are measured again.
    fn thin(&self, cuts: &mut Vec<usize>, fixed: &[usize], jumps: &[usize]) {
        let removable = |cuts: &[usize], k: usize| {
            k > 0
                && k + 1 < cuts.len()
                && !fixed.contains(&cuts[k])
                && !jumps.contains(&cuts[k + 1])
        };
        let cost = |cuts: &[usize], k: usize| match removable(cuts, k) {
            true => self.segment(cuts[k - 1], cuts[k + 1]).error,
            false => f64::INFINITY,
        };
        let mut costs: Vec<f64> = (0..cuts.len()).map(|k| cost(cuts, k)).collect();
        loop {
            let best = costs
                .iter()
                .enumerate()
                .min_by(|x, y| x.1.total_cmp(y.1))
                .filter(|&(_, &e)| e <= 1.0)
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

    /// Each channel's keys at `cuts`.
    fn build(&self, cuts: &[usize], jumps: &[usize]) -> Vec<Fitted> {
        let mut fitted: Vec<Fitted> = self
            .channels
            .iter()
            .map(|values| Fitted {
                keys: cuts
                    .iter()
                    .map(|&i| Key {
                        time: self.times[i],
                        value: values[i],
                        slope_in: 0.0,
                        slope_out: 0.0,
                        hold: false,
                    })
                    .collect(),
                max_error: 0.0,
            })
            .collect();
        for k in 0..cuts.len().saturating_sub(1) {
            let (a, b) = (cuts[k], cuts[k + 1]);
            if b == a + 1 && jumps.contains(&b) {
                fitted.iter_mut().for_each(|f| f.keys[k].hold = true);
                continue;
            }
            let segment = self.segment(a, b);
            for (f, (&(m0, m1), &error)) in fitted
                .iter_mut()
                .zip(segment.slopes.iter().zip(&segment.errors))
            {
                f.keys[k].slope_out = m0;
                f.keys[k + 1].slope_in = m1;
                f.max_error = f.max_error.max(error);
            }
        }
        fitted
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
