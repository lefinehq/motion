//! A band-limited kernel for reading a sound between its samples: a
//! Kaiser-windowed sinc, tabulated finely and read with linear
//! interpolation (Smith's bandlimited interpolation). It resamples a sound
//! to the mix rate and plays it slower or faster, and gives the same
//! samples on every run, with no state between reads.

/// How many zero crossings the kernel reaches on each side.
const ZEROS: usize = 16;

/// Table entries per zero crossing.
const STEPS: usize = 512;

/// The Kaiser window's shape: about 90 dB of stop-band rejection.
const BETA: f64 = 9.0;

/// How far below the Nyquist frequency the pass band ends, as a fraction
/// of it, leaving the transition band room to fall before aliasing.
const ROLLOFF: f64 = 0.96;

/// A tabulated windowed sinc.
#[derive(Debug, Clone)]
pub struct Kernel {
    /// The kernel at `i / STEPS` zero crossings from its middle, for `i`
    /// from 0 to `ZEROS * STEPS`, then a 0 past the end.
    table: Vec<f32>,
}

impl Kernel {
    /// The kernel every sound uses.
    pub fn standard() -> Kernel {
        let length = ZEROS * STEPS;
        let i0_beta = bessel_i0(BETA);
        let mut table: Vec<f32> = (0..=length)
            .map(|i| {
                let x = i as f64 / STEPS as f64;
                let ratio = x / ZEROS as f64;
                let window = bessel_i0(BETA * (1.0 - ratio * ratio).max(0.0).sqrt()) / i0_beta;
                (sinc(x) * window) as f32
            })
            .collect();
        table.push(0.0);
        Kernel { table }
    }

    /// The kernel's value `x` zero crossings from its middle, 0 beyond its
    /// reach.
    fn at(&self, x: f64) -> f64 {
        let place = x.abs() * STEPS as f64;
        let index = place.floor();
        if index >= (ZEROS * STEPS) as f64 {
            return 0.0;
        }
        let index = index as usize;
        let fraction = place - index as f64;
        let (a, b) = (
            f64::from(self.table[index]),
            f64::from(self.table[index + 1]),
        );
        a + (b - a) * fraction
    }

    /// `samples` read at `position`, in samples from the first, when each
    /// read moves on by `step` samples: above 1 the read speeds the sound
    /// up, so the kernel narrows its band to keep what would fold back out.
    /// Samples before the first and after the last are silence.
    pub fn read(&self, samples: &[f32], position: f64, step: f64) -> f32 {
        let cutoff = ROLLOFF * (1.0 / step.abs().max(f64::MIN_POSITIVE)).min(1.0);
        let reach = ZEROS as f64 / cutoff;
        let first = (position - reach).ceil().max(0.0);
        let last = (position + reach).floor().min(samples.len() as f64 - 1.0);
        if last < first {
            return 0.0;
        }
        let mut sum = 0.0;
        let (first, last) = (first as usize, last as usize);
        for (i, sample) in samples[first..=last].iter().enumerate() {
            let distance = position - (first + i) as f64;
            sum += f64::from(*sample) * self.at(distance * cutoff);
        }
        (sum * cutoff) as f32
    }
}

/// The most phases a rate change keeps weights for; a ratio needing more
/// reads through the table instead.
const MOST_PHASES: u64 = 4096;

/// The weights of one phase of a rate change: the first sample they start
/// at, relative to the sample at or before the read, and one weight a
/// sample from there.
struct Phase {
    first: i64,
    weights: Vec<f64>,
}

impl Kernel {
    /// `samples` at `from` samples a second, resampled to `to`: the same
    /// samples as [`Kernel::read`] gives at each output's position. When
    /// the ratio of the rates reduces to few enough phases, as 44.1 to 48
    /// kHz does to 160, each phase's weights are worked out once.
    pub fn resample(&self, samples: &[f32], from: u32, to: u32) -> Vec<f32> {
        let step = f64::from(from) / f64::from(to);
        let frames = (samples.len() as f64 / step).ceil() as usize;
        let divisor = gcd(u64::from(from), u64::from(to));
        let (up, down) = (u64::from(to) / divisor, u64::from(from) / divisor);
        if up > MOST_PHASES {
            return (0..frames)
                .map(|n| self.read(samples, n as f64 * step, step))
                .collect();
        }
        let cutoff = ROLLOFF * (1.0 / step).min(1.0);
        let reach = ZEROS as f64 / cutoff;
        let phases: Vec<Phase> = (0..up)
            .map(|phase| {
                let fraction = phase as f64 / up as f64;
                let first = (fraction - reach).ceil() as i64;
                let last = (fraction + reach).floor() as i64;
                let weights = (first..=last)
                    .map(|k| cutoff * self.at((fraction - k as f64) * cutoff))
                    .collect();
                Phase { first, weights }
            })
            .collect();
        let length = samples.len() as i64;
        (0..frames as u64)
            .map(|n| {
                let numerator = n * down;
                let base = (numerator / up) as i64;
                let phase = &phases[(numerator % up) as usize];
                let start = base + phase.first;
                let mut sum = 0.0;
                for (j, weight) in phase.weights.iter().enumerate() {
                    let at = start + j as i64;
                    if (0..length).contains(&at) {
                        sum += f64::from(samples[at as usize]) * weight;
                    }
                }
                sum as f32
            })
            .collect()
    }
}

/// The greatest common divisor of `a` and `b`.
fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// `sin(πx) / πx`, 1 at 0.
fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    let px = std::f64::consts::PI * x;
    px.sin() / px
}

/// The modified Bessel function of the first kind, order 0, by its series,
/// summed until a term no longer changes the sum.
fn bessel_i0(x: f64) -> f64 {
    let half = x / 2.0;
    let mut sum = 1.0;
    let mut term = 1.0;
    let mut k = 1.0;
    loop {
        term *= (half / k) * (half / k);
        let next = sum + term;
        if next == sum {
            return sum;
        }
        sum = next;
        k += 1.0;
    }
}

#[cfg(test)]
#[path = "../tests/unit/kernel.rs"]
mod tests;
