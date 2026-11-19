//! The seeded generator every axis draws from ([D78]): SplitMix64, whose
//! output depends only on its seed, on every platform.
//!
//! [D78]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d78-generated-variants-move-along-measured-axes-and-pass-when-they-add-no-error

/// A stream of numbers drawn from one seed.
#[derive(Debug, Clone)]
pub(crate) struct Rng(u64);

impl Rng {
    /// The stream for `axis` of variant `index` of a run seeded `seed`: each
    /// axis draws from its own, so adding an axis does not move the others.
    pub(crate) fn new(seed: u64, index: usize, axis: &str) -> Rng {
        // FNV-1a over the axis's name, mixed with the seed and the index.
        let name = axis.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
        let mut rng = Rng(seed ^ name.rotate_left(17) ^ (index as u64).wrapping_mul(0x9E37_79B9));
        rng.next();
        rng
    }

    /// The next 64 bits.
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..1`.
    pub(crate) fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }

    /// A number in `lo..hi`.
    pub(crate) fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// An index below `n`, which must not be 0.
    pub(crate) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// One of `items`, or `None` when there are none.
    pub(crate) fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        match items.len() {
            0 => None,
            n => items.get(self.below(n)),
        }
    }
}
