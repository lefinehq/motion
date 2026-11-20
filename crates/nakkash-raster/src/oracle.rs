//! What holds a faster kernel to the one it replaces ([D90], rule 7).
//!
//! Exact means the same operations on every pixel, in the same order, so a
//! kernel made faster gives the same bits as its reference: the code it
//! replaces, kept beside it. These are the tools its tests compare the two
//! with:
//!
//! - inputs that find where arithmetic differs ([`edge_floats`],
//!   [`Noise`]): signed zeros, subnormals, NaNs with payloads, infinities,
//!   the neighbours of every half level an 8-bit value rounds at, and sums
//!   an `f64` cannot hold exactly;
//! - the run lengths that reach every SIMD tail ([`WIDTHS`]);
//! - output grids with junk around them and in them ([`Padded`]), so a
//!   kernel that writes past its end or reads before writing shows;
//! - a comparison by bits that names the first difference ([`same_bits`]);
//! - every instruction set the processor has, each forced in turn
//!   ([`levels`]), every thread count ([`on_threads`]) and every split of
//!   the work (`par::split_in_tests`).
//!
//! The tests of this module check the checks: a change of one unit in the
//! last place, of a NaN's payload or of a zero's sign is found, and a write
//! past the end is.
//!
//! [D90]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d90-a-frame-is-drawn-from-a-plan-that-knows-what-each-drawing-is-under-one-memory-budget

use std::fmt::Write as _;

use vello_cpu::Level;

use crate::effect::{EFFECTS, Effect, ParamValue};
use crate::pixels::{Pixel, Pixels};

/// Run lengths that reach the tail of every vector width: 4, 8 and 16
/// lanes, one short and one past.
pub(crate) const WIDTHS: &[usize] = &[1, 2, 3, 4, 5, 7, 8, 9, 15, 16, 17, 31, 32, 33];

/// What fills the guards and the inside of a [`Padded`] grid: a quiet NaN
/// with a payload no arithmetic makes, as in the memory pool's clobber
/// mode.
pub(crate) const JUNK: f32 = f32::from_bits(0x7FC0_A5A5);

/// Floats at which arithmetic that is not the reference's differs from it.
pub(crate) fn edge_floats() -> Vec<f32> {
    let mut edges = vec![
        0.0,
        -0.0,
        // Subnormals: the least, half the least normal, the largest.
        f32::from_bits(1),
        -f32::from_bits(1),
        f32::MIN_POSITIVE / 2.0,
        f32::from_bits(0x007F_FFFF),
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        // NaNs: the usual one, with payloads, negative, signalling.
        f32::NAN,
        f32::from_bits(0x7FC0_0001),
        f32::from_bits(0xFFC0_0000),
        f32::from_bits(0x7F80_0001),
        JUNK,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        f32::MIN,
        f32::EPSILON,
        // Past the unit range, as light beyond white is.
        2.0,
        4.0,
        100.0,
        65504.0,
        -1.0,
        // Sums an `f64` holds only rounded, beside small values.
        16_777_217.0,
        1.0e8,
        1.0e-8,
        3.0e38,
        1.0e-30,
    ];
    // Each 8-bit level, the half levels between them and their neighbours:
    // where a conversion to bytes rounds one way or the other.
    for k in 0..=255u8 {
        let level = f32::from(k) / 255.0;
        let half = (f32::from(k) + 0.5) / 255.0;
        edges.extend([level, half, half.next_down(), half.next_up()]);
    }
    for x in [0.5f32, 1.0] {
        edges.extend([x.next_down(), x.next_up()]);
    }
    edges
}

/// A repeatable stream of test values, a quarter of them from
/// [`edge_floats`].
#[derive(Debug, Clone)]
pub(crate) struct Noise {
    state: u64,
    edges: Vec<f32>,
}

impl Noise {
    /// The stream for `seed`.
    pub(crate) fn new(seed: u64) -> Noise {
        Noise {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            edges: edge_floats(),
        }
    }

    /// The next 64 bits (xorshift64*).
    pub(crate) fn bits(&mut self) -> u64 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `[0, 1)`.
    pub(crate) fn unit(&mut self) -> f32 {
        (self.bits() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// The next value: an edge, one in the unit range, one in the range
    /// light beyond white reaches, or any bits at all.
    pub(crate) fn float(&mut self) -> f32 {
        let pick = self.bits();
        match pick % 4 {
            0 => self.edges[(pick >> 8) as usize % self.edges.len()],
            1 => self.unit(),
            2 => self.unit() * 6.0 - 2.0,
            _ => f32::from_bits((self.bits() >> 32) as u32),
        }
    }

    /// The next premultiplied pixel: alpha often exactly 0 or 1, colours
    /// at most alpha but for a few beyond it, as unclamped light is.
    pub(crate) fn pixel(&mut self) -> Pixel {
        let alpha = match self.bits() % 4 {
            0 => 0.0,
            1 => 1.0,
            _ => self.unit(),
        };
        let mut colour = || match self.bits() % 8 {
            0 => self.float(),
            _ => self.unit() * alpha,
        };
        [colour(), colour(), colour(), alpha]
    }

    /// `len` values from [`Noise::float`].
    pub(crate) fn floats(&mut self, len: usize) -> Vec<f32> {
        (0..len).map(|_| self.float()).collect()
    }

    /// `len` pixels from [`Noise::pixel`].
    pub(crate) fn pixels(&mut self, len: usize) -> Vec<Pixel> {
        (0..len).map(|_| self.pixel()).collect()
    }
}

/// A value compared by its bits.
pub(crate) trait Bits: Copy {
    /// Its bits, every lane.
    fn bits(self) -> Vec<u64>;
}

impl Bits for f32 {
    fn bits(self) -> Vec<u64> {
        vec![u64::from(self.to_bits())]
    }
}

impl Bits for f64 {
    fn bits(self) -> Vec<u64> {
        vec![self.to_bits()]
    }
}

impl Bits for u8 {
    fn bits(self) -> Vec<u64> {
        vec![u64::from(self)]
    }
}

impl<T: Bits, const N: usize> Bits for [T; N] {
    fn bits(self) -> Vec<u64> {
        self.iter().flat_map(|lane| lane.bits()).collect()
    }
}

/// Whether `candidate` has the bits of `reference`, item for item; if not,
/// what differs: the first item that does, both values and their bits, and
/// how many items differ.
pub(crate) fn same_bits<T: Bits + std::fmt::Debug>(
    reference: &[T],
    candidate: &[T],
) -> Result<(), String> {
    if reference.len() != candidate.len() {
        return Err(format!(
            "{} items where the reference has {}",
            candidate.len(),
            reference.len()
        ));
    }
    let mut differ = reference
        .iter()
        .zip(candidate)
        .enumerate()
        .filter(|(_, (a, b))| a.bits() != b.bits());
    let Some((first, (a, b))) = differ.next() else {
        return Ok(());
    };
    let mut told = format!(
        "{} of {} items differ; the first, item {first}, is {b:?} where the reference has {a:?}",
        1 + differ.count(),
        reference.len()
    );
    let hex = |bits: Vec<u64>| {
        bits.iter().fold(String::new(), |mut s, b| {
            let _ = write!(s, " {b:#010x}");
            s
        })
    };
    let _ = write!(told, " (bits{} against{})", hex(b.bits()), hex(a.bits()));
    Err(told)
}

/// Asserts [`same_bits`], naming `what` was compared.
#[track_caller]
pub(crate) fn assert_same_bits<T: Bits + std::fmt::Debug>(
    what: &str,
    reference: &[T],
    candidate: &[T],
) {
    if let Err(told) = same_bits(reference, candidate) {
        panic!("{what}: {told}");
    }
}

/// A grid a kernel writes, with guards of [`JUNK`] before and after it and
/// junk in it until written, so a write past either end or a read of what
/// was not written shows.
#[derive(Debug, Clone)]
pub(crate) struct Padded {
    all: Vec<f32>,
    len: usize,
}

impl Padded {
    /// How many values guard each end: more than any kernel's vector.
    pub(crate) const GUARD: usize = 64;

    /// A grid of `len` values, all junk.
    pub(crate) fn new(len: usize) -> Padded {
        Padded {
            all: vec![JUNK; len + 2 * Padded::GUARD],
            len,
        }
    }

    /// The grid, to be written.
    pub(crate) fn inside_mut(&mut self) -> &mut [f32] {
        &mut self.all[Padded::GUARD..Padded::GUARD + self.len]
    }

    /// The grid as written.
    pub(crate) fn inside(&self) -> &[f32] {
        &self.all[Padded::GUARD..Padded::GUARD + self.len]
    }

    /// The guards and the grid together, as a kernel that strays would
    /// write them.
    pub(crate) fn all_mut(&mut self) -> &mut [f32] {
        &mut self.all
    }

    /// Whether both guards still hold nothing but junk; if not, the first
    /// value written past an end.
    pub(crate) fn guards_whole(&self) -> Result<(), String> {
        let after = Padded::GUARD + self.len;
        let guards = (0..Padded::GUARD).chain(after..self.all.len());
        match guards
            .into_iter()
            .find(|&i| self.all[i].to_bits() != JUNK.to_bits())
        {
            None => Ok(()),
            Some(i) if i < Padded::GUARD => Err(format!(
                "{} written {} before the start",
                self.all[i],
                Padded::GUARD - i
            )),
            Some(i) => Err(format!(
                "{} written {} past the end",
                self.all[i],
                i - after + 1
            )),
        }
    }
}

/// Every instruction set the processor has, each forced: the portable
/// fallback first, then from the narrowest to the widest.
pub(crate) fn levels() -> Vec<(&'static str, Level)> {
    let mut levels = vec![("fallback", Level::fallback())];
    let Some(best) = Level::try_detect() else {
        return levels;
    };
    #[cfg(target_arch = "aarch64")]
    if let Some(neon) = best.as_neon() {
        levels.push(("neon", Level::Neon(neon)));
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if let Some(sse2) = best.as_sse2() {
            levels.push(("sse2", Level::Sse2(sse2)));
        }
        if let Some(sse4) = best.as_sse4_2() {
            levels.push(("sse4.2", Level::Sse4_2(sse4)));
        }
        if let Some(avx2) = best.as_avx2() {
            levels.push(("avx2", Level::Avx2(avx2)));
        }
        if let Some(avx512) = best.as_avx512() {
            levels.push(("avx512", Level::Avx512(avx512)));
        }
    }
    let _ = best;
    levels
}

/// What `work` gives on a pool of its own of `threads` threads.
pub(crate) fn on_threads<R: Send>(threads: usize, work: impl FnOnce() -> R + Send) -> R {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .expect("a thread pool")
        .install(work)
}

/// A 300 by 200 grid of varied, partly transparent colours: several
/// tasks' worth of rows.
pub(crate) fn textured() -> Pixels {
    let mut pixels = Pixels::transparent(300, 200);
    for (i, p) in pixels.data_mut().iter_mut().enumerate() {
        let (x, y) = ((i % 300) as f32, (i / 300) as f32);
        let a = (0.5 + 0.5 * (x * 0.05).sin() * (y * 0.07).cos()).clamp(0.0, 1.0);
        let c = |k: f32| (0.5 + 0.5 * (x * k + y * (0.1 - k)).sin()) * a;
        *p = [c(0.03), c(0.011), c(0.07), a];
    }
    pixels
}

/// Every effect, with its defaults and with each number moved into its
/// range, as `(what, effect)`.
pub(crate) fn every_effect_set() -> Vec<(String, Effect)> {
    let mut all = Vec::new();
    for spec in EFFECTS {
        all.push((spec.name.to_owned(), Effect::new(spec.name, &[]).unwrap()));
        let moved: Vec<(&str, ParamValue)> = spec
            .params
            .iter()
            .filter_map(|p| match (p.default, p.range) {
                (ParamValue::Number(_), Some((lo, hi))) => {
                    let (lo, hi) = (lo.max(-50.0), hi.min(50.0));
                    Some((p.name, ParamValue::Number(lo + (hi - lo) * 0.37)))
                }
                _ => None,
            })
            .collect();
        all.push((
            format!("{} moved", spec.name),
            Effect::new(spec.name, &moved).unwrap(),
        ));
    }
    all
}

#[cfg(test)]
#[path = "../tests/unit/oracle.rs"]
mod tests;
