//! Our own randomness and noise, for the language's `random`, `noise` and
//! `wiggle`. The original generator is unpublished, so values from these are
//! approximations, and the bake marks them so. They are deterministic: the
//! same seed gives the same bits on every platform.

/// Mixes `parts` into one seed (SplitMix64's finaliser over each part).
pub(crate) fn mix(parts: &[u64]) -> u64 {
    parts.iter().fold(0x9E37_79B9_7F4A_7C15, |acc, &part| {
        finalise(acc ^ finalise(part))
    })
}

fn finalise(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The next state of a SplitMix64 generator, and a number in `[0, 1)`.
pub(crate) fn next(state: u64) -> (u64, f64) {
    let state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let bits = finalise(
        state
            .wrapping_sub(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0x9E37_79B9_7F4A_7C15),
    ) >> 11;
    (state, bits as f64 / (1u64 << 53) as f64)
}

/// One-dimensional gradient noise at `x`, in `[-1, 1]` and zero at whole
/// numbers, with a smooth quintic fade between them.
pub(crate) fn gradient(seed: u64, x: f64) -> f64 {
    let cell = x.floor();
    let f = x - cell;
    let slope = |c: f64| {
        let (_, u) = next(mix(&[seed, c.to_bits()]));
        u * 2.0 - 1.0
    };
    let (g0, g1) = (slope(cell), slope(cell + 1.0));
    let fade = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    // Each gradient gives a ramp through zero at its corner; the fade
    // blends them. The largest value of the blend is 0.5 per unit slope.
    2.0 * (g0 * f * (1.0 - fade) + g1 * (f - 1.0) * fade)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_numbers() {
        let (a, x) = next(mix(&[1, 2, 3]));
        let (b, y) = next(mix(&[1, 2, 3]));
        assert_eq!((a, x.to_bits()), (b, y.to_bits()));
        assert_ne!(next(mix(&[1, 2, 4])).1, x);
        assert!((0.0..1.0).contains(&x));
    }

    #[test]
    fn noise_is_bounded_smooth_and_zero_at_whole_numbers() {
        for i in 0..2000 {
            let x = f64::from(i) * 0.013 - 5.0;
            let n = gradient(7, x);
            assert!((-1.0..=1.0).contains(&n), "{x}: {n}");
            assert!((gradient(7, x + 1e-6) - n).abs() < 1e-4, "jump at {x}");
        }
        assert_eq!(gradient(7, 3.0), 0.0);
    }
}
