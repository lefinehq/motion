//! Keyframe tracks: one property's value over time.
//!
//! Part of step 2 (Compile). Compiling gives every property of every node a
//! [`Track`]; evaluating (step 3) samples them. Times are frames.

use motion_format::Curve;

use crate::curve;

/// One keyframe: a value at a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    /// The frame the key sits on.
    pub frame: f64,
    /// The property's value at that frame.
    pub value: f64,
    /// How the value eases from this key to the next.
    pub curve: Curve,
}

/// The keys of one property: never empty, sorted by frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// Keys in non-decreasing frame order; same-frame keys keep their order.
    keys: Vec<Key>,
}

impl Track {
    /// Builds a track from `keys`, sorting them stably by frame.
    ///
    /// Panics if `keys` is empty: a track always has a value, and callers
    /// build keys from validated input that is never empty.
    pub fn new(mut keys: Vec<Key>) -> Track {
        assert!(!keys.is_empty(), "a track needs at least one key");
        // Stable, so two keys on one frame keep the order they were given.
        keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        Track { keys }
    }

    /// A track that holds `value` at every frame.
    pub fn constant(value: f64) -> Track {
        Track::new(vec![Key {
            frame: 0.0,
            value,
            curve: Curve::Linear,
        }])
    }

    /// The keys, sorted by frame.
    pub fn keys(&self) -> &[Key] {
        &self.keys
    }

    /// Whether the value is the same at every frame.
    pub fn is_constant(&self) -> bool {
        self.keys.iter().all(|k| k.value == self.keys[0].value)
    }

    /// The value at `frame`.
    ///
    /// Before the first key it is the first value, after the last key the last
    /// value. Between two keys it eases with the left key's curve. When
    /// several keys share a frame, the last of them applies from that frame on.
    pub fn sample(&self, frame: f64) -> f64 {
        // Number of keys at or before `frame`; this lands after every key of
        // a shared frame, so the last of them is the one we pick.
        let after = self.keys.partition_point(|k| k.frame <= frame);
        let Some(left) = after.checked_sub(1).map(|i| &self.keys[i]) else {
            return self.keys[0].value;
        };
        let Some(right) = self.keys.get(after) else {
            return left.value;
        };
        // right.frame > frame >= left.frame, so the span is positive.
        let p = (frame - left.frame) / (right.frame - left.frame);
        left.value + (right.value - left.value) * curve::sample(left.curve, p)
    }
}

#[cfg(test)]
#[path = "../tests/unit/track.rs"]
mod tests;
