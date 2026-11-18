//! A parameter's running total over the layer's own time: the integral of
//! its value from time 0. An effect that counts reads it, as particles read
//! how many their `rate` has made by a moment and when each was made. The
//! caller works it out from the parameter's keys; here it is only read, so
//! the rasteriser still knows nothing of animation.

/// One stretch of a [`Total`]: from `start` seconds until the next piece
/// starts (the last for ever), the value starts at `rate` and changes by
/// `slope` each second, so `u` seconds in the total is
/// `total + rate·u + slope·u²/2`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TotalPiece {
    /// When it starts, in seconds of the layer's own time.
    pub start: f32,
    /// The total at its start.
    pub total: f32,
    /// The value at its start, per second.
    pub rate: f32,
    /// How much the value changes each second.
    pub slope: f32,
}

impl TotalPiece {
    /// The total at `time`, which may lie outside the piece.
    fn at(self, time: f32) -> f32 {
        let u = time - self.start;
        self.total + u * (self.rate + 0.5 * self.slope * u)
    }

    /// How long after its start the piece adds `more` to its total, if it
    /// ever does.
    fn reaching(self, more: f32) -> Option<f32> {
        if self.slope == 0.0 {
            return (self.rate > 0.0).then(|| more / self.rate);
        }
        // The root of `rate·u + slope·u²/2 = more` that does not cancel.
        let root = self.rate
            + (self.rate * self.rate + 2.0 * self.slope * more)
                .max(0.0)
                .sqrt();
        match root > 0.0 {
            true => Some(2.0 * more / root),
            false => (self.slope > 0.0).then_some(0.0),
        }
    }
}

/// A running total that never falls, as pieces in time order. The first
/// piece is carried back before its start.
#[derive(Debug, Clone, PartialEq)]
pub struct Total {
    pieces: Vec<TotalPiece>,
}

impl Total {
    /// The total made of `pieces`.
    ///
    /// # Panics
    /// If `pieces` is empty, or their starts or totals fall.
    pub fn new(pieces: Vec<TotalPiece>) -> Total {
        assert!(!pieces.is_empty(), "a total needs a piece");
        assert!(
            pieces
                .windows(2)
                .all(|w| w[0].start <= w[1].start && w[0].total <= w[1].total),
            "a total's pieces rise"
        );
        Total { pieces }
    }

    /// The total of `rate` held from time 0: `rate·t`.
    pub fn constant(rate: f32) -> Total {
        Total::new(vec![TotalPiece {
            start: 0.0,
            total: 0.0,
            rate,
            slope: 0.0,
        }])
    }

    /// Its pieces, in time order.
    pub fn pieces(&self) -> &[TotalPiece] {
        &self.pieces
    }

    /// The total at `time` seconds.
    pub fn at(&self, time: f32) -> f32 {
        let k = self.pieces.partition_point(|p| p.start <= time);
        self.pieces[k.saturating_sub(1)].at(time)
    }

    /// The moment the total first rises past `total`, if it ever does: when
    /// particle `total` is born. A total that stops on a whole number does
    /// not pass it until it rises again.
    pub fn passes(&self, total: f32) -> Option<f32> {
        let k = self.pieces.partition_point(|p| p.total <= total);
        let Some(k) = k.checked_sub(1) else {
            return Some(self.pieces[0].start);
        };
        let piece = self.pieces[k];
        let end = self.pieces.get(k + 1).map(|next| next.start);
        match (piece.reaching(total - piece.total), end) {
            (Some(u), Some(end)) => Some((piece.start + u).min(end)),
            (Some(u), None) => Some(piece.start + u),
            // Only by rounding: the next piece starts above `total`.
            (None, end) => end,
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/total.rs"]
mod tests;
