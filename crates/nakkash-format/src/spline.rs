//! The natural cubic spline that joins the points of a curve of levels:
//! the `curves` effect's `rgb`, `red`, `green`, `blue` and `alpha` paths.
//! The rasteriser draws a curve through it, and the import uses it to put
//! the keys of an animated curve on one set of inputs, so the keys move
//! into each other point by point.

/// A natural cubic spline through points in rising `x`: second
/// derivatives 0 at both ends, flat beyond them.
///
/// For a fixed set of `x`, the spline is linear in the `y`: the spline
/// through a mix of two sets of `y` is the same mix of their splines. And
/// the spline through its own levels at more points, between the same
/// ends, is itself. [`Spline::on`] relies on both.
#[derive(Debug, Clone, PartialEq)]
pub struct Spline {
    points: Vec<[f64; 2]>,
    second: Vec<f64>,
}

impl Spline {
    /// The spline through `points`, which are in rising `x`. No points is
    /// the straight line through `(0, 0)` and `(1, 1)`.
    pub fn new(points: &[[f64; 2]]) -> Spline {
        let points = match points {
            [] => vec![[0.0, 0.0], [1.0, 1.0]],
            _ => points.to_vec(),
        };
        let n = points.len();
        let mut second = vec![0.0; n];
        if n > 2 {
            // The tridiagonal system for the inner second derivatives,
            // solved forwards then backwards.
            let mut upper = vec![0.0; n];
            let mut rhs = vec![0.0; n];
            for i in 1..n - 1 {
                let (h0, h1) = (
                    points[i][0] - points[i - 1][0],
                    points[i + 1][0] - points[i][0],
                );
                let slope =
                    (points[i + 1][1] - points[i][1]) / h1 - (points[i][1] - points[i - 1][1]) / h0;
                let diagonal = 2.0 * (h0 + h1) - h0 * upper[i - 1];
                upper[i] = h1 / diagonal;
                rhs[i] = (6.0 * slope - h0 * rhs[i - 1]) / diagonal;
            }
            for i in (1..n - 1).rev() {
                second[i] = rhs[i] - upper[i] * second[i + 1];
            }
        }
        Spline { points, second }
    }

    /// The points it passes through.
    pub fn points(&self) -> &[[f64; 2]] {
        &self.points
    }

    /// Its level at `x`.
    pub fn at(&self, x: f64) -> f64 {
        let p = &self.points;
        let (first, last) = (p[0], p[p.len() - 1]);
        if p.len() == 1 || x <= first[0] {
            return first[1];
        }
        if x >= last[0] {
            return last[1];
        }
        let i = p.partition_point(|q| q[0] <= x) - 1;
        let (a, b) = (p[i], p[i + 1]);
        let h = b[0] - a[0];
        let (t, u) = ((b[0] - x) / h, (x - a[0]) / h);
        t * a[1]
            + u * b[1]
            + ((t * t * t - t) * self.second[i] + (u * u * u - u) * self.second[i + 1]) * h * h
                / 6.0
    }

    /// The points of a spline on `inputs`, in rising order, that keeps
    /// this one, and how far it strays from this one at any input from 0
    /// to 1, measured on a grid of 1,021 inputs (four between each of 256
    /// levels).
    ///
    /// An input between this spline's ends takes its level there. When
    /// `inputs` hold all of this spline's own inputs, the new spline is
    /// this one exactly between its ends: a natural spline through its own
    /// levels is itself. An input beyond an end has no level that keeps
    /// the flat part exactly, since the flat part meets the curve at a
    /// corner and a spline has none; its level is the one that keeps the
    /// whole curve closest, by least squares over the grid, and the stray
    /// says what is left.
    pub fn on(&self, inputs: &[f64]) -> (Vec<[f64; 2]>, f64) {
        let (low, high) = (self.points[0][0], self.points[self.points.len() - 1][0]);
        let grid: Vec<f64> = (0..=GRID).map(|i| f64::from(i) / f64::from(GRID)).collect();
        let mut points: Vec<[f64; 2]> = inputs.iter().map(|&x| [x, self.at(x)]).collect();
        let mut free: Vec<usize> = (0..inputs.len())
            .filter(|&i| inputs[i] < low || inputs[i] > high)
            .collect();
        // Levels run from 0 to 1: a fitted level beyond them is held at the
        // bound, and the others are fitted again without it.
        while !free.is_empty() {
            let levels = self.fitted(&points, &free, &grid);
            let beyond: Vec<usize> = free
                .iter()
                .zip(&levels)
                .filter(|(_, level)| !(0.0..=1.0).contains(*level))
                .map(|(&j, _)| j)
                .collect();
            for (&j, level) in free.iter().zip(&levels) {
                points[j][1] = level.clamp(0.0, 1.0);
            }
            free.retain(|j| !beyond.contains(j));
            if beyond.is_empty() {
                break;
            }
        }
        let other = Spline::new(&points);
        let stray = grid
            .iter()
            .map(|&x| (other.at(x) - self.at(x)).abs())
            .fold(0.0, f64::max);
        (points, stray)
    }
}

impl Spline {
    /// The levels at the `free` inputs of `points` that bring the spline
    /// through `points` closest to this one over `grid`, by least squares;
    /// the other levels stay. The spline is linear in its levels, so the
    /// free levels are the weights of one spline per free input, through 1
    /// there and 0 at every other input, added to the spline with them all 0.
    fn fitted(&self, points: &[[f64; 2]], free: &[usize], grid: &[f64]) -> Vec<f64> {
        let with = |level: &dyn Fn(usize) -> f64| {
            let points: Vec<[f64; 2]> = (0..points.len())
                .map(|i| [points[i][0], level(i)])
                .collect();
            Spline::new(&points)
        };
        let fixed = with(&|i| match free.contains(&i) {
            true => 0.0,
            false => points[i][1],
        });
        let left: Vec<f64> = grid.iter().map(|&x| self.at(x) - fixed.at(x)).collect();
        let bases: Vec<Vec<f64>> = free
            .iter()
            .map(|&j| {
                let basis = with(&|i| f64::from(u8::from(i == j)));
                grid.iter().map(|&x| basis.at(x)).collect()
            })
            .collect();
        least_squares(&bases, &left)
    }
}

/// The steps of the grid a stray is measured on: four between each of
/// 256 levels.
const GRID: u16 = 1020;

/// The weights of `bases` whose sum comes closest to `target`, by least
/// squares: the normal equations, solved by elimination with partial
/// pivoting. A basis that adds nothing gets weight 0.
fn least_squares(bases: &[Vec<f64>], target: &[f64]) -> Vec<f64> {
    let n = bases.len();
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let mut rows: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            let mut row: Vec<f64> = (0..n).map(|j| dot(&bases[i], &bases[j])).collect();
            row.push(dot(&bases[i], target));
            row
        })
        .collect();
    for col in 0..n {
        let pivot = (col..n)
            .max_by(|&a, &b| rows[a][col].abs().total_cmp(&rows[b][col].abs()))
            .unwrap_or(col);
        rows.swap(col, pivot);
        if rows[col][col].abs() < 1e-15 {
            continue;
        }
        let pivot = rows[col].clone();
        for (i, row) in rows.iter_mut().enumerate() {
            if i != col {
                let factor = row[col] / pivot[col];
                for (cell, by) in row[col..].iter_mut().zip(&pivot[col..]) {
                    *cell -= factor * by;
                }
            }
        }
    }
    (0..n)
        .map(|i| match rows[i][i].abs() < 1e-15 {
            true => 0.0,
            false => rows[i][n] / rows[i][i],
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/spline.rs"]
mod tests;
