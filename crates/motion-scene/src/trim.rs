//! Trim: cutting outlines down to a stretch of their length.
//!
//! A stretch runs from `start` to `end`, swapped if `start` is after `end`,
//! and the offset (360° is once around) moves it along. Lengths are arc
//! lengths in the space each outline sits in. A stretch that runs past the
//! end goes on from the beginning: as one line around a single closed
//! outline, or as a second piece otherwise. An empty stretch leaves nothing,
//! so a capped stroke draws no dot, and a whole one leaves the outlines as
//! they are, closed.

use kurbo::{BezPath, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point};
use motion_format::item::props;

use crate::state::ItemState;
use crate::track::ARC_ACCURACY;

/// Cuts the outlines at `at` in `outlines` by the trim `item`: each on its
/// own, or as one line in order when its mode is `joined`.
pub(crate) fn apply(item: &ItemState, at: &[usize], outlines: &mut [BezPath]) {
    let stretch = Stretch::of(item);
    if stretch == Stretch::Whole {
        return;
    }
    match item.setting("mode") {
        "joined" => {
            let cut = stretch.cut(&at.iter().map(|&i| &outlines[i]).collect::<Vec<_>>());
            for (&i, path) in at.iter().zip(cut) {
                outlines[i] = path;
            }
        }
        _ => {
            for &i in at {
                let [path] = <[BezPath; 1]>::try_from(stretch.cut(&[&outlines[i]]))
                    .expect("one outline in, one out");
                outlines[i] = path;
            }
        }
    }
}

/// The part of a line a trim keeps, as shares of its length.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stretch {
    /// All of it.
    Whole,
    /// None of it.
    Empty,
    /// From `from` in 0..1 for `width` in (0, 1), which may run past the end.
    Part {
        /// Where it starts.
        from: f64,
        /// How much it keeps.
        width: f64,
    },
}

impl Stretch {
    /// The stretch the trim `item` keeps.
    fn of(item: &ItemState) -> Stretch {
        let (a, b) = (item.number(props::TRIM_START), item.number(props::TRIM_END));
        let (start, end) = (a.min(b), a.max(b));
        let width = end - start;
        if width >= 1.0 {
            return Stretch::Whole;
        }
        if width <= 0.0 {
            return Stretch::Empty;
        }
        let from = start + item.number(props::TRIM_OFFSET) / 360.0;
        Stretch::Part {
            from: from - from.floor(),
            width,
        }
    }

    /// `paths` cut as one line, in order: one path out for each path in.
    fn cut(self, paths: &[&BezPath]) -> Vec<BezPath> {
        let mut out = vec![BezPath::new(); paths.len()];
        let Stretch::Part { from, width } = self else {
            if self == Stretch::Whole {
                out = paths.iter().map(|&p| p.clone()).collect();
            }
            return out;
        };
        let line = Line::of(paths);
        if line.length <= 0.0 {
            return paths.iter().map(|&p| p.clone()).collect();
        }
        let (a, b) = (from * line.length, (from + width) * line.length);
        if b <= line.length {
            line.emit(a, b, None, &mut out);
        } else {
            let last = line.emit(a, line.length, None, &mut out);
            // Around a single closed outline the stretch goes on as one line.
            let on = line.round.then_some(last).flatten();
            line.emit(0.0, b - line.length, on, &mut out);
        }
        out
    }
}

/// One segment of a line.
struct Seg {
    /// The segment.
    seg: PathSeg,
    /// Which path it belongs to, by position in the line.
    path: usize,
    /// Which subpath of the line it belongs to.
    sub: usize,
    /// How far along the line it starts.
    start: f64,
    /// Its arc length.
    length: f64,
}

/// Paths laid end to end as one line.
struct Line {
    /// Every segment, in order.
    segs: Vec<Seg>,
    /// The total arc length.
    length: f64,
    /// Whether the line is a single closed outline, so it goes on round.
    round: bool,
}

impl Line {
    fn of(paths: &[&BezPath]) -> Line {
        let mut segs = Vec::new();
        let (mut sub, mut length, mut closed) = (0, 0.0, Vec::new());
        for (path, outline) in paths.iter().enumerate() {
            let (mut first, mut last) = (Point::ZERO, Point::ZERO);
            let mut push = |seg: PathSeg, sub: usize, length: &mut f64| {
                let arc = seg.arclen(ARC_ACCURACY);
                segs.push(Seg {
                    seg,
                    path,
                    sub,
                    start: *length,
                    length: arc,
                });
                *length += arc;
            };
            for el in outline.elements() {
                match *el {
                    PathEl::MoveTo(p) => {
                        sub = closed.len();
                        closed.push(false);
                        (first, last) = (p, p);
                    }
                    PathEl::LineTo(p) => {
                        push(PathSeg::Line(kurbo::Line::new(last, p)), sub, &mut length);
                        last = p;
                    }
                    PathEl::QuadTo(c, p) => {
                        push(
                            PathSeg::Quad(kurbo::QuadBez::new(last, c, p)),
                            sub,
                            &mut length,
                        );
                        last = p;
                    }
                    PathEl::CurveTo(c1, c2, p) => {
                        let cubic = kurbo::CubicBez::new(last, c1, c2, p);
                        push(PathSeg::Cubic(cubic), sub, &mut length);
                        last = p;
                    }
                    PathEl::ClosePath => {
                        if last != first {
                            push(
                                PathSeg::Line(kurbo::Line::new(last, first)),
                                sub,
                                &mut length,
                            );
                        }
                        closed[sub] = true;
                        last = first;
                    }
                }
            }
        }
        Line {
            segs,
            length,
            round: closed == [true],
        }
    }

    /// Appends the stretch from `a` to `b` along the line to the paths in
    /// `out`, going on from subpath `on` if it ends where `a` lies; returns
    /// the subpath it ended on.
    fn emit(&self, a: f64, b: f64, on: Option<usize>, out: &mut [BezPath]) -> Option<usize> {
        let mut pen = on;
        for s in &self.segs {
            let end = s.start + s.length;
            if s.length <= 0.0 || end <= a || s.start >= b {
                continue;
            }
            let t0 = match a > s.start {
                true => s.seg.inv_arclen(a - s.start, ARC_ACCURACY),
                false => 0.0,
            };
            let t1 = match b < end {
                true => s.seg.inv_arclen(b - s.start, ARC_ACCURACY),
                false => 1.0,
            };
            let piece = s.seg.subsegment(t0..t1);
            let path = &mut out[s.path];
            if pen != Some(s.sub) {
                path.move_to(piece.start());
            }
            match piece {
                PathSeg::Line(l) => path.line_to(l.p1),
                PathSeg::Quad(q) => path.quad_to(q.p1, q.p2),
                PathSeg::Cubic(c) => path.curve_to(c.p1, c.p2, c.p3),
            }
            pen = Some(s.sub);
        }
        pen
    }
}

#[cfg(test)]
#[path = "../tests/unit/trim.rs"]
mod tests;
