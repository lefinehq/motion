//! An object's mask on one frame, from the segmenter's logits, and what it
//! measures: centroid, area, principal angle, extreme corners.

use nakkash_baked::MaskStats;

use crate::image::{Filter, Plane};

/// A hard mask: which pixels of a frame are the object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mask {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// One flag a pixel, row by row from the top.
    pub inside: Vec<bool>,
}

impl Mask {
    /// The mask the segmenter's `logits` (at any size, usually 256²) give
    /// at `width` by `height`: upsampled bilinearly, inside where the logit
    /// is above 0, as the segmenter's own video predictor does.
    pub fn from_logits(logits: &Plane, width: usize, height: usize) -> Mask {
        let up = logits.resized(width, height, Filter::Triangle);
        Mask {
            width,
            height,
            inside: up.data.iter().map(|&v| v > 0.0).collect(),
        }
    }

    /// The mask at another size: as 0 and 1, resampled with the triangle
    /// filter, inside wherever any of it reaches.
    pub fn resized(&self, width: usize, height: usize) -> Mask {
        let plane = Plane {
            width: self.width,
            height: self.height,
            data: self
                .inside
                .iter()
                .map(|&i| f32::from(u8::from(i)))
                .collect(),
        };
        let out = plane.resized(width, height, Filter::Triangle);
        Mask {
            width,
            height,
            inside: out.data.iter().map(|&v| v > 1e-6).collect(),
        }
    }

    /// The pixels inside, by their centres' coordinates in pixels.
    fn pixels(&self) -> impl Iterator<Item = [f64; 2]> + '_ {
        self.inside
            .iter()
            .enumerate()
            .filter(|(_, i)| **i)
            .map(|(k, _)| [(k % self.width) as f64, (k / self.width) as f64])
    }

    /// Its centroid, area and principal angle, with `reference_area` the
    /// prompted frame's area for the scale; `None` when it is empty.
    pub fn stats(&self, reference_area: f64) -> Option<MaskStats> {
        let (mut n, mut sx, mut sy) = (0.0, 0.0, 0.0);
        for [x, y] in self.pixels() {
            n += 1.0;
            sx += x;
            sy += y;
        }
        if n == 0.0 {
            return None;
        }
        let (mx, my) = (sx / n, sy / n);
        let (mut cxx, mut cyy, mut cxy) = (0.0, 0.0, 0.0);
        for [x, y] in self.pixels() {
            cxx += (x - mx) * (x - mx);
            cyy += (y - my) * (y - my);
            cxy += (x - mx) * (y - my);
        }
        // Half the angle of the covariance's doubled-angle vector: the
        // direction of its largest eigenvector, in y-down coordinates, so
        // clockwise on screen.
        let angle = 0.5 * (2.0 * cxy).atan2(cxx - cyy);
        Some(MaskStats {
            centroid: [mx, my],
            area: n,
            scale: match reference_area > 0.0 {
                true => (n / reference_area).sqrt(),
                false => 1.0,
            },
            angle: angle.to_degrees(),
        })
    }

    /// Its extreme points along the diagonals: top left, top right, bottom
    /// right, bottom left, the corners of a screen or a label seen
    /// roughly square-on; `None` when it is empty.
    pub fn corners(&self) -> Option<[[f64; 2]; 4]> {
        let mut best: Option<[([f64; 2], f64); 4]> = None;
        for p in self.pixels() {
            let scores = [-(p[0] + p[1]), p[0] - p[1], p[0] + p[1], -(p[0] - p[1])];
            let b = best.get_or_insert([
                (p, scores[0]),
                (p, scores[1]),
                (p, scores[2]),
                (p, scores[3]),
            ]);
            for (slot, score) in b.iter_mut().zip(scores) {
                if score > slot.1 {
                    *slot = (p, score);
                }
            }
        }
        best.map(|b| b.map(|(p, _)| p))
    }

    /// It cut to the inside of the quadrilateral `quad`.
    pub fn within(&self, quad: &[[f64; 2]; 4]) -> Mask {
        let mut out = self.clone();
        for (k, inside) in out.inside.iter_mut().enumerate() {
            let p = [(k % self.width) as f64, (k / self.width) as f64];
            *inside = *inside && in_polygon(p, quad);
        }
        out
    }
}

/// Whether `p` lies inside `polygon`, by the crossings of a ray to the
/// right (even-odd).
pub fn in_polygon(p: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    let mut inside = false;
    let n = polygon.len();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if p[0] < x {
                inside = !inside;
            }
        }
    }
    inside
}

#[cfg(test)]
#[path = "../../tests/unit/track/mask.rs"]
mod tests;
