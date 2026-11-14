//! Bézier paths as property values: vertices, each with the handle arriving
//! at it and the handle leaving it, and whether the last joins the first.
//!
//! A path is kept as a flat list of numbers, six per vertex, so a track
//! interpolates two paths of the same shape the way it interpolates points.
//! The string form is a subset of SVG path data, absolute commands only:
//! `M x,y`, `L x,y`, `C x1,y1 x2,y2 x,y` and `Z`.

use std::fmt;
use std::str::FromStr;

use crate::grammar::{Parsed, blank0, blank1, number, parse_all};
use crate::{Diagnostic, Result, fmt_num};
use winnow::combinator::{alt, cut_err, eof, opt, preceded, repeat, separated_pair, terminated};
use winnow::prelude::*;

/// The accepted form of a path.
pub const PATH_FORM: &str =
    "SVG path data with absolute commands, e.g. `M0,0 C10,0 20,10 20,20 L0,20 Z`: `M` once, \
     then `L x,y` or `C x1,y1 x2,y2 x,y`, and `Z` to close";

/// Numbers per vertex: point, arriving handle, leaving handle.
const STRIDE: usize = 6;

/// A bézier path.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    closed: bool,
    /// Six numbers per vertex: `x, y`, the arriving handle's offset, the leaving handle's offset.
    numbers: Vec<f64>,
}

/// One vertex of a [`Path`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vertex {
    /// Where it is.
    pub point: [f64; 2],
    /// The handle arriving at it, as an offset from `point`.
    pub arriving: [f64; 2],
    /// The handle leaving it, as an offset from `point`.
    pub leaving: [f64; 2],
}

impl Path {
    /// A path through `vertices`, closed or open.
    pub fn new(closed: bool, vertices: impl IntoIterator<Item = Vertex>) -> Path {
        let numbers = vertices
            .into_iter()
            .flat_map(|v| {
                [
                    v.point[0],
                    v.point[1],
                    v.arriving[0],
                    v.arriving[1],
                    v.leaving[0],
                    v.leaving[1],
                ]
            })
            .collect();
        Path { closed, numbers }
    }

    /// Whether the last vertex joins the first.
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// How many vertices it has.
    pub fn len(&self) -> usize {
        self.numbers.len() / STRIDE
    }

    /// Whether it has no vertex.
    pub fn is_empty(&self) -> bool {
        self.numbers.is_empty()
    }

    /// Its vertices, in order.
    pub fn vertices(&self) -> impl Iterator<Item = Vertex> + '_ {
        self.numbers.chunks_exact(STRIDE).map(|n| Vertex {
            point: [n[0], n[1]],
            arriving: [n[2], n[3]],
            leaving: [n[4], n[5]],
        })
    }

    /// Its numbers, six per vertex.
    pub fn components(&self) -> &[f64] {
        &self.numbers
    }

    /// The same shape of path holding `numbers`.
    ///
    /// # Panics
    /// If `numbers` does not hold six per vertex of this path.
    pub fn with_components(&self, numbers: &[f64]) -> Path {
        assert_eq!(numbers.len(), self.numbers.len(), "six numbers per vertex");
        Path {
            closed: self.closed,
            numbers: numbers.to_vec(),
        }
    }

    /// Whether a track can move between this path and `other`: the same
    /// number of vertices, both open or both closed.
    pub fn same_shape(&self, other: &Path) -> bool {
        self.closed == other.closed && self.numbers.len() == other.numbers.len()
    }
}

impl FromStr for Path {
    type Err = Diagnostic;

    fn from_str(text: &str) -> Result<Path> {
        parse_all(text, path, |problem| {
            let message = match problem {
                Some(Problem::NoMove) => format!("`{text}` must start with `M x,y`"),
                Some(Problem::Command) => format!("`{text}` has a command other than `L`, `C` or `Z`"),
                Some(Problem::Points) => format!("`{text}` has a command without its points"),
                None => format!("`{text}` is not a valid path"),
            };
            Diagnostic::syntax(message).expected(PATH_FORM)
        })
    }
}

/// Why a path failed.
#[derive(Debug, Clone, Copy)]
enum Problem {
    NoMove,
    Command,
    Points,
}

/// A segment as written: a straight line or a cubic to its end point.
enum Segment {
    Line([f64; 2]),
    Cubic([f64; 2], [f64; 2], [f64; 2]),
}

fn path(input: &mut &str) -> Parsed<Path, Problem> {
    let start = preceded((blank0, 'M', blank0), cut_err(point))
        .context(Problem::NoMove)
        .parse_next(input)?;
    let segments: Vec<Segment> = repeat(0.., preceded(blank1, segment)).parse_next(input)?;
    let closed = opt(preceded(blank0, 'Z')).parse_next(input)?.is_some();
    terminated(blank0, eof)
        .context(Problem::Command)
        .parse_next(input)?;
    Ok(build(start, &segments, closed))
}

fn segment(input: &mut &str) -> Parsed<Segment, Problem> {
    alt((
        preceded(('L', blank0), cut_err(point.context(Problem::Points))).map(Segment::Line),
        preceded(
            ('C', blank0),
            cut_err(
                (point, preceded(blank1, point), preceded(blank1, point)).context(Problem::Points),
            ),
        )
        .map(|(c1, c2, end)| Segment::Cubic(c1, c2, end)),
    ))
    .parse_next(input)
}

/// `x,y` or `x y`.
fn point(input: &mut &str) -> Parsed<[f64; 2], Problem> {
    separated_pair(number, alt(((blank0, ',', blank0).void(), blank1.void())), number)
        .map(|(x, y)| [x, y])
        .parse_next(input)
}

/// The vertices the commands draw. A closed path whose last segment ends
/// where it started joins that end to the first vertex, so writing a path
/// and reading it back gives the same vertices.
fn build(start: [f64; 2], segments: &[Segment], closed: bool) -> Path {
    let mut vertices = vec![Vertex {
        point: start,
        arriving: [0.0; 2],
        leaving: [0.0; 2],
    }];
    for segment in segments {
        let previous = vertices.last_mut().expect("a path starts with a vertex");
        let (end, arriving) = match *segment {
            Segment::Line(end) => (end, [0.0; 2]),
            Segment::Cubic(c1, c2, end) => {
                previous.leaving = minus(c1, previous.point);
                (end, minus(c2, end))
            }
        };
        vertices.push(Vertex {
            point: end,
            arriving,
            leaving: [0.0; 2],
        });
    }
    if closed && vertices.len() > 1 && vertices.last().map(|v| v.point) == Some(start) {
        let last = vertices.pop().expect("more than one vertex");
        vertices[0].arriving = last.arriving;
    }
    Path::new(closed, vertices)
}

fn minus(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn plus(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

impl fmt::Display for Path {
    /// `M` to the first vertex, a `C` to each next one, and on a closed path a
    /// `C` back to the first and `Z`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = |v: [f64; 2]| format!("{},{}", fmt_num(v[0]), fmt_num(v[1]));
        let vertices: Vec<Vertex> = self.vertices().collect();
        let Some(first) = vertices.first() else {
            return Ok(());
        };
        write!(f, "M{}", p(first.point))?;
        let closing = self.closed.then_some(first);
        for (from, to) in vertices.iter().zip(vertices.iter().skip(1).chain(closing)) {
            write!(
                f,
                " C{} {} {}",
                p(plus(from.point, from.leaving)),
                p(plus(to.point, to.arriving)),
                p(to.point)
            )?;
        }
        if self.closed {
            f.write_str(" Z")?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/unit/path.rs"]
mod tests;
