//! Path construction, clipping, painting, colour and images.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::rc::Rc;
use std::sync::Arc;

use kurbo::{Affine, Arc as KArc, BezPath, PathEl, Point, Rect, Shape, Stroke, StrokeOpts, Vec2};

use crate::colour::{self, Space};
use crate::graphics::{Cap, Join};
use crate::interp::{Fault, Frame, Halt, Interp, PathElement, PsError, R};
use crate::object::{Key, Object, Value};
use crate::ops::state::{inverse, matrix_at, matrix_of};
use crate::page::{Clip, FillRule, Item, Raster, Rgb};

/// The tolerance curves are approximated to, in page units (points).
const TOLERANCE: f64 = 0.01;

// ----- path construction ---------------------------------------------------

pub(crate) fn newpath(i: &mut Interp) -> R {
    i.g.path.clear();
    Ok(())
}

/// The current point in page space, or `nocurrentpoint`.
fn current(i: &Interp) -> R<Point> {
    i.g.path
        .current
        .ok_or_else(|| PsError::NoCurrentPoint.into())
}

pub(crate) fn currentpoint(i: &mut Interp) -> R {
    let p = inverse(i.g.ctm)? * current(i)?;
    i.push(Object::real(p.x))?;
    i.push(Object::real(p.y))
}

/// The two numbers on top, as a point.
fn point(i: &Interp, depth: usize) -> R<Point> {
    Ok(Point::new(i.num(depth + 1)?, i.num(depth)?))
}

/// Counts one more path element against the output limit.
fn grow(i: &mut Interp) -> R {
    if i.g.path.bez.elements().len() >= i.limits.path_elements {
        return Err(Fault::Halt(Halt::Output));
    }
    Ok(())
}

pub(crate) fn moveto(i: &mut Interp) -> R {
    let p = i.g.ctm * point(i, 0)?;
    grow(i)?;
    i.drop_n(2);
    i.g.path.move_to(p);
    Ok(())
}

/// A distance in user space, in page space.
fn delta(i: &Interp, d: Vec2) -> Vec2 {
    let c = i.g.ctm.as_coeffs();
    Vec2::new(c[0] * d.x + c[2] * d.y, c[1] * d.x + c[3] * d.y)
}

pub(crate) fn rmoveto(i: &mut Interp) -> R {
    let from = current(i)?;
    let p = from + delta(i, point(i, 0)?.to_vec2());
    grow(i)?;
    i.drop_n(2);
    i.g.path.move_to(p);
    Ok(())
}

pub(crate) fn lineto(i: &mut Interp) -> R {
    current(i)?;
    let p = i.g.ctm * point(i, 0)?;
    grow(i)?;
    i.drop_n(2);
    i.g.path.line_to(p);
    Ok(())
}

pub(crate) fn rlineto(i: &mut Interp) -> R {
    let from = current(i)?;
    let p = from + delta(i, point(i, 0)?.to_vec2());
    grow(i)?;
    i.drop_n(2);
    i.g.path.line_to(p);
    Ok(())
}

pub(crate) fn curveto(i: &mut Interp) -> R {
    current(i)?;
    let m = i.g.ctm;
    let (a, b, c) = (m * point(i, 4)?, m * point(i, 2)?, m * point(i, 0)?);
    grow(i)?;
    i.drop_n(6);
    i.g.path.curve_to(a, b, c);
    Ok(())
}

pub(crate) fn rcurveto(i: &mut Interp) -> R {
    let from = current(i)?;
    let (a, b, c) = (point(i, 4)?, point(i, 2)?, point(i, 0)?);
    let (a, b, c) = (
        from + delta(i, a.to_vec2()),
        from + delta(i, b.to_vec2()),
        from + delta(i, c.to_vec2()),
    );
    grow(i)?;
    i.drop_n(6);
    i.g.path.curve_to(a, b, c);
    Ok(())
}

/// Appends an arc in user space: a line to its start (or a move when there
/// is no current point), then its curves.
fn append_arc(i: &mut Interp, centre: Point, radius: f64, start: f64, sweep: f64) -> R {
    let m = i.g.ctm;
    let begin = centre + Vec2::from_angle(start) * radius;
    if i.g.path.current.is_some() {
        i.g.path.line_to(m * begin);
    } else {
        i.g.path.move_to(m * begin);
    }
    let arc = KArc::new(centre, (radius, radius), start, sweep, 0.0);
    for element in arc.append_iter(TOLERANCE.min(radius.abs() * 1e-3).max(1e-6)) {
        grow(i)?;
        if let PathEl::CurveTo(a, b, c) = element {
            i.g.path.curve_to(m * a, m * b, m * c);
        }
    }
    Ok(())
}

/// `arc` and `arcn`.
fn arc_op(i: &mut Interp, clockwise: bool) -> R {
    let (centre, radius) = (point(i, 3)?, i.num(2)?);
    let (a1, mut a2) = (i.num(1)?, i.num(0)?);
    i.drop_n(5);
    if clockwise {
        while a2 > a1 {
            a2 -= 360.0;
        }
    } else {
        while a2 < a1 {
            a2 += 360.0;
        }
    }
    append_arc(i, centre, radius, a1.to_radians(), (a2 - a1).to_radians())
}

pub(crate) fn arc(i: &mut Interp) -> R {
    arc_op(i, false)
}

pub(crate) fn arcn(i: &mut Interp) -> R {
    arc_op(i, true)
}

/// `arct` and `arcto`: an arc tangent to the lines from the current point
/// to (x1, y1) and from there to (x2, y2). Returns the tangent points.
fn tangent_arc(i: &mut Interp) -> R<(Point, Point)> {
    let p0 = inverse(i.g.ctm)? * current(i)?;
    let (p1, p2, r) = (point(i, 3)?, point(i, 1)?, i.num(0)?);
    i.drop_n(5);
    let (u, v) = (p0 - p1, p2 - p1);
    let (lu, lv) = (u.hypot(), v.hypot());
    let cross = u.cross(v);
    if lu == 0.0 || lv == 0.0 || cross.abs() < 1e-12 || r == 0.0 {
        let m = i.g.ctm;
        i.g.path.line_to(m * p1);
        return Ok((p1, p1));
    }
    let (u, v) = (u / lu, v / lv);
    let angle = u.dot(v).clamp(-1.0, 1.0).acos();
    let d = r / (angle / 2.0).tan();
    let (t1, t2) = (p1 + u * d, p1 + v * d);
    let bisector = (u + v).normalize();
    let centre = p1 + bisector * (r / (angle / 2.0).sin());
    let start = (t1 - centre).atan2();
    let end = (t2 - centre).atan2();
    let mut sweep = end - start;
    // The arc turns the short way round, the way the corner turns.
    if cross > 0.0 {
        while sweep > 0.0 {
            sweep -= 2.0 * PI;
        }
    } else {
        while sweep < 0.0 {
            sweep += 2.0 * PI;
        }
    }
    append_arc(i, centre, r, start, sweep)?;
    Ok((t1, t2))
}

pub(crate) fn arct(i: &mut Interp) -> R {
    tangent_arc(i).map(drop)
}

pub(crate) fn arcto(i: &mut Interp) -> R {
    let (t1, t2) = tangent_arc(i)?;
    for v in [t1.x, t1.y, t2.x, t2.y] {
        i.push(Object::real(v))?;
    }
    Ok(())
}

pub(crate) fn closepath(i: &mut Interp) -> R {
    i.g.path.close();
    Ok(())
}

pub(crate) fn flattenpath(i: &mut Interp) -> R {
    let bez = i.g.path.bez.clone();
    let mut flat = BezPath::new();
    kurbo::flatten(&bez, i.g.flat.max(0.05) * 0.1, |element| flat.push(element));
    i.tick(u64::try_from(flat.elements().len()).unwrap_or(0))?;
    i.g.path.bez = flat;
    Ok(())
}

pub(crate) fn reversepath(i: &mut Interp) -> R {
    let reversed = i.g.path.bez.reverse_subpaths();
    i.g.path.bez = reversed;
    if let Some(PathEl::MoveTo(p)) = i.g.path.bez.elements().last().copied() {
        i.g.path.current = Some(p);
    }
    Ok(())
}

/// The stroke style of the current state, in user space.
fn stroke_style(i: &Interp) -> Stroke {
    let cap = match i.g.cap {
        Cap::Butt => kurbo::Cap::Butt,
        Cap::Round => kurbo::Cap::Round,
        Cap::Square => kurbo::Cap::Square,
    };
    let join = match i.g.join {
        Join::Miter => kurbo::Join::Miter,
        Join::Round => kurbo::Join::Round,
        Join::Bevel => kurbo::Join::Bevel,
    };
    let mut style = Stroke::new(i.g.line_width)
        .with_caps(cap)
        .with_join(join)
        .with_miter_limit(i.g.miter);
    if !i.g.dash.is_empty() {
        // An odd-length pattern repeats to an even one, as the language
        // reads it.
        let mut pattern = i.g.dash.clone();
        if pattern.len() % 2 == 1 {
            pattern.extend_from_within(..);
        }
        style = style.with_dashes(i.g.dash_offset, pattern);
    }
    style
}

pub(crate) fn strokepath(i: &mut Interp) -> R {
    let m = i.g.ctm;
    let Ok(inv) = inverse(m) else {
        i.g.path.clear();
        return Ok(());
    };
    let user = inv * i.g.path.painted();
    let outline = kurbo::stroke(&user, &stroke_style(i), &StrokeOpts::default(), TOLERANCE);
    i.tick(u64::try_from(outline.elements().len()).unwrap_or(0))?;
    i.g.path.clear();
    i.g.path.bez = m * outline;
    Ok(())
}

/// The page's box in page space.
fn page_box(i: &Interp) -> Rect {
    let b = i.out.bounding_box;
    Rect::new(0.0, 0.0, b.width(), b.height())
}

pub(crate) fn clippath(i: &mut Interp) -> R {
    let path = match &i.g.clip_path {
        Some(path) => (**path).clone(),
        None => page_box(i).to_path(TOLERANCE),
    };
    i.g.path.clear();
    i.g.path.bez = path;
    Ok(())
}

pub(crate) fn pathbbox(i: &mut Interp) -> R {
    let bez = i.g.path.painted();
    if bez.elements().is_empty() && i.g.path.current.is_none() {
        return Err(PsError::NoCurrentPoint.into());
    }
    let inv = inverse(i.g.ctm)?;
    let bounds = if bez.elements().is_empty() {
        let p = inv * current(i)?;
        Rect::from_points(p, p)
    } else {
        (inv * bez).bounding_box()
    };
    for v in [bounds.x0, bounds.y0, bounds.x1, bounds.y1] {
        i.push(Object::real(v))?;
    }
    Ok(())
}

pub(crate) fn pathforall(i: &mut Interp) -> R {
    let procs = [i.proc(3)?, i.proc(2)?, i.proc(1)?, i.proc(0)?];
    let inv = inverse(i.g.ctm)?;
    let mut elements = Vec::new();
    for element in i.g.path.bez.elements() {
        elements.push(match *element {
            PathEl::MoveTo(p) => {
                let p = inv * p;
                PathElement::Move(p.x, p.y)
            }
            PathEl::LineTo(p) => {
                let p = inv * p;
                PathElement::Line(p.x, p.y)
            }
            PathEl::QuadTo(a, b) => {
                let (a, b) = (inv * a, inv * b);
                PathElement::Curve([a.x, a.y, b.x, b.y, b.x, b.y])
            }
            PathEl::CurveTo(a, b, c) => {
                let (a, b, c) = (inv * a, inv * b, inv * c);
                PathElement::Curve([a.x, a.y, b.x, b.y, c.x, c.y])
            }
            PathEl::ClosePath => PathElement::Close,
        });
    }
    i.drop_n(4);
    i.push_frame(Frame::PathForall {
        elements: elements.into(),
        index: 0,
        procs,
    })
}

// ----- clipping ------------------------------------------------------------

pub(crate) fn initclip(i: &mut Interp) -> R {
    i.g.clip = None;
    i.g.clip_path = None;
    Ok(())
}

/// Narrows the clip by `path` in page space.
fn narrow(i: &mut Interp, path: BezPath, rule: FillRule) -> R {
    if !i.out.room(path.elements().len()) {
        return Err(Fault::Halt(Halt::Output));
    }
    i.out.clips.push(Clip {
        path: path.clone(),
        rule,
        parent: i.g.clip,
    });
    i.g.clip = Some(i.out.clips.len() - 1);
    i.g.clip_path = Some(Rc::new(path));
    Ok(())
}

pub(crate) fn clip(i: &mut Interp) -> R {
    let path = i.g.path.painted();
    narrow(i, path, FillRule::NonZero)
}

pub(crate) fn eoclip(i: &mut Interp) -> R {
    let path = i.g.path.painted();
    narrow(i, path, FillRule::EvenOdd)
}

/// The rectangles of `rectfill`, `rectstroke` and `rectclip`: four numbers,
/// or an array of numbers in fours. Returns them and the operands used.
fn rects(i: &Interp, depth: usize) -> R<(Vec<Rect>, usize)> {
    let top = i.peek(depth)?;
    let numbers = match top.value {
        Value::Array(arr) | Value::Packed(arr) => (i.numbers(arr)?, 1),
        Value::String(_) => {
            return Err(PsError::TypeCheck.into());
        }
        _ => {
            let n = [
                i.num(depth + 3)?,
                i.num(depth + 2)?,
                i.num(depth + 1)?,
                i.num(depth)?,
            ];
            (n.to_vec(), 4)
        }
    };
    let rects = numbers
        .0
        .chunks_exact(4)
        .map(|r| Rect::new(r[0], r[1], r[0] + r[2], r[1] + r[3]))
        .collect();
    Ok((rects, numbers.1))
}

/// Rectangles in user space as a path in page space, each wound the way
/// its sides run.
fn rect_path(m: Affine, rects: &[Rect]) -> BezPath {
    let mut path = BezPath::new();
    for r in rects {
        path.move_to(m * Point::new(r.x0, r.y0));
        path.line_to(m * Point::new(r.x1, r.y0));
        path.line_to(m * Point::new(r.x1, r.y1));
        path.line_to(m * Point::new(r.x0, r.y1));
        path.close_path();
    }
    path
}

pub(crate) fn rectclip(i: &mut Interp) -> R {
    let (rects, used) = rects(i, 0)?;
    i.drop_n(used);
    let path = rect_path(i.g.ctm, &rects);
    i.g.path.clear();
    narrow(i, path, FillRule::NonZero)
}

// ----- painting ------------------------------------------------------------

/// The colour paint operators use, reporting a pattern that is not drawn.
fn paint_colour(i: &mut Interp) -> Option<Rgb> {
    if i.g.paint.is_none() && matches!(*i.g.space, Space::Pattern { .. }) {
        i.unsupported("pattern fills (`setpattern`), drawn as nothing");
    }
    i.g.paint
}

/// Adds an item, within the output limit.
fn emit(i: &mut Interp, item: Item, elements: usize) -> R {
    if i.shown {
        i.unsupported("drawing after the first page");
        return Ok(());
    }
    if !i.out.room(elements) || i.out.items.len() >= i.limits.items {
        return Err(Fault::Halt(Halt::Output));
    }
    i.out.items.push(item);
    Ok(())
}

fn fill_with(i: &mut Interp, rule: FillRule) -> R {
    let path = i.g.path.painted();
    i.g.path.clear();
    if let Some(colour) = paint_colour(i)
        && !path.elements().is_empty()
    {
        let n = path.elements().len();
        let clip = i.g.clip;
        emit(
            i,
            Item::Fill {
                path,
                rule,
                colour,
                clip,
            },
            n,
        )?;
    }
    Ok(())
}

pub(crate) fn fill(i: &mut Interp) -> R {
    fill_with(i, FillRule::NonZero)
}

pub(crate) fn eofill(i: &mut Interp) -> R {
    fill_with(i, FillRule::EvenOdd)
}

/// Strokes `path`, given in page space, with the current style.
fn stroke_path(i: &mut Interp, path: &BezPath, extra: Affine) -> R {
    let m = i.g.ctm * extra;
    let Some(colour) = paint_colour(i) else {
        return Ok(());
    };
    let Ok(inv) = inverse(m) else {
        // A matrix without an inverse squeezes the pen flat.
        return Ok(());
    };
    if path.elements().is_empty() {
        return Ok(());
    }
    let user = inv * path.clone();
    let n = user.elements().len();
    let style = stroke_style(i);
    let clip = i.g.clip;
    emit(
        i,
        Item::Stroke {
            path: user,
            style,
            transform: m,
            colour,
            clip,
        },
        n,
    )
}

pub(crate) fn stroke(i: &mut Interp) -> R {
    let path = i.g.path.painted();
    i.g.path.clear();
    stroke_path(i, &path, Affine::IDENTITY)
}

pub(crate) fn rectfill(i: &mut Interp) -> R {
    let (rects, used) = rects(i, 0)?;
    i.drop_n(used);
    let path = rect_path(i.g.ctm, &rects);
    if let Some(colour) = paint_colour(i) {
        let n = path.elements().len();
        let clip = i.g.clip;
        emit(
            i,
            Item::Fill {
                path,
                rule: FillRule::NonZero,
                colour,
                clip,
            },
            n,
        )?;
    }
    Ok(())
}

pub(crate) fn rectstroke(i: &mut Interp) -> R {
    // A matrix after the rectangles changes the pen, not where they lie.
    // Rectangle arrays hold fours, so a six-number array over a number or
    // an array is a matrix.
    let top = i.peek(0)?;
    let below = i.peek(1).ok();
    let with_matrix = top.as_arr().is_some_and(|a| a.len == 6)
        && below.is_some_and(|o| o.as_f64().is_some() || o.as_arr().is_some());
    let (extra, skip) = if with_matrix {
        (matrix_at(i, 0)?, 1)
    } else {
        (Affine::IDENTITY, 0)
    };
    let (rects, used) = rects(i, skip)?;
    i.drop_n(used + skip);
    let path = rect_path(i.g.ctm, &rects);
    stroke_path(i, &path, extra)
}

pub(crate) fn erasepage(i: &mut Interp) -> R {
    i.out.items.clear();
    Ok(())
}

/// `showpage` ends the page: an EPS file draws one, and what a program
/// paints after it is not part of the footage.
pub(crate) fn showpage(i: &mut Interp) -> R {
    i.shown = true;
    crate::ops::state::initgraphics(i)
}

pub(crate) fn shfill(i: &mut Interp) -> R {
    i.dict(0)?;
    i.drop_n(1);
    i.unsupported("smooth shading (`shfill`), drawn as nothing");
    Ok(())
}

pub(crate) fn upath_unsupported(i: &mut Interp) -> R {
    i.pop()?;
    i.unsupported("user paths (`ufill`, `ustroke`, `uappend`), drawn as nothing");
    Ok(())
}

pub(crate) fn upath(i: &mut Interp) -> R {
    i.boolean(0)?;
    i.drop_n(1);
    i.unsupported("user paths (`upath`)");
    let arr = i.vm.new_array(Vec::new())?;
    i.push(Object::exe(Value::Array(arr)))
}

/// `execform`: paints a form by running its procedure inside its box and
/// matrix, once per use (forms are not cached).
pub(crate) fn execform(i: &mut Interp) -> R {
    let form = i.dict(0)?;
    let get = |i: &mut Interp, name: &str| {
        let key = Key::Name(i.vm.name(name.as_bytes()));
        i.vm.dict_get(form, &key)
    };
    let matrix = match get(i, "Matrix").and_then(Object::as_arr) {
        Some(arr) => matrix_of(i, arr)?,
        None => Affine::IDENTITY,
    };
    let bbox = match get(i, "BBox").and_then(Object::as_arr) {
        Some(arr) => i.numbers(arr)?,
        None => Vec::new(),
    };
    let paint = get(i, "PaintProc").ok_or(PsError::Undefined)?;
    crate::ops::state::gsave(i)?;
    i.g.ctm *= matrix;
    if bbox.len() == 4 {
        let rect = Rect::new(bbox[0], bbox[1], bbox[2], bbox[3]);
        let path = rect_path(i.g.ctm, &[rect]);
        narrow(i, path, FillRule::NonZero)?;
    }
    i.g.path.clear();
    // The form stays on the stack for its procedure to take.
    let result = i.call(paint);
    crate::ops::state::grestore(i)?;
    result
}

// ----- colour --------------------------------------------------------------

pub(crate) fn setgray(i: &mut Interp) -> R {
    let g = i.num(0)?;
    i.drop_n(1);
    i.set_space(Space::Gray, None)?;
    i.set_colour(vec![g.clamp(0.0, 1.0)])
}

/// The current colour as RGB numbers.
fn current_rgb(i: &Interp) -> [f64; 3] {
    let c = i.g.paint.unwrap_or(Rgb::BLACK);
    [f64::from(c.r), f64::from(c.g), f64::from(c.b)]
}

pub(crate) fn currentgray(i: &mut Interp) -> R {
    let value = if matches!(*i.g.space, Space::Gray) {
        i.g.colour.first().copied().unwrap_or(0.0)
    } else {
        let [r, g, b] = current_rgb(i);
        0.3 * r + 0.59 * g + 0.11 * b
    };
    i.push(Object::real(value))
}

pub(crate) fn setrgbcolor(i: &mut Interp) -> R {
    let (r, g, b) = (i.num(2)?, i.num(1)?, i.num(0)?);
    i.drop_n(3);
    i.set_space(Space::Rgb, None)?;
    i.set_colour(vec![r, g, b])
}

pub(crate) fn currentrgbcolor(i: &mut Interp) -> R {
    let rgb = if matches!(*i.g.space, Space::Rgb) {
        [i.g.colour[0], i.g.colour[1], i.g.colour[2]]
    } else {
        current_rgb(i)
    };
    for v in rgb {
        i.push(Object::real(v))?;
    }
    Ok(())
}

pub(crate) fn sethsbcolor(i: &mut Interp) -> R {
    let (h, s, b) = (i.num(2)?, i.num(1)?, i.num(0)?);
    i.drop_n(3);
    let [r, g, b] = colour::hsb_to_rgb(h, s.clamp(0.0, 1.0), b.clamp(0.0, 1.0));
    i.set_space(Space::Rgb, None)?;
    i.set_colour(vec![r, g, b])
}

pub(crate) fn currenthsbcolor(i: &mut Interp) -> R {
    let [r, g, b] = current_rgb(i);
    for v in colour::rgb_to_hsb(r, g, b) {
        i.push(Object::real(v))?;
    }
    Ok(())
}

pub(crate) fn setcmykcolor(i: &mut Interp) -> R {
    let (c, m, y, k) = (i.num(3)?, i.num(2)?, i.num(1)?, i.num(0)?);
    i.drop_n(4);
    i.set_space(Space::Cmyk, None)?;
    i.set_colour(vec![c, m, y, k])
}

pub(crate) fn currentcmykcolor(i: &mut Interp) -> R {
    let cmyk = if matches!(*i.g.space, Space::Cmyk) {
        [i.g.colour[0], i.g.colour[1], i.g.colour[2], i.g.colour[3]]
    } else {
        let [r, g, b] = current_rgb(i);
        let (c, m, y) = (1.0 - r, 1.0 - g, 1.0 - b);
        let k = c.min(m).min(y);
        [c - k, m - k, y - k, k]
    };
    for v in cmyk {
        i.push(Object::real(v))?;
    }
    Ok(())
}

pub(crate) fn setcolorspace(i: &mut Interp) -> R {
    let object = i.peek(0)?;
    let space = i.colour_space(object)?;
    i.drop_n(1);
    i.set_space(space, Some(object))
}

pub(crate) fn currentcolorspace(i: &mut Interp) -> R {
    let object = match i.g.space_object {
        Some(object) if object.as_arr().is_some() => object,
        other => {
            let name = match (other.map(|o| o.value), &*i.g.space) {
                (Some(Value::Name(name)), _) => i.vm.name_text(name),
                (_, Space::Rgb) => "DeviceRGB".into(),
                (_, Space::Cmyk) => "DeviceCMYK".into(),
                (_, Space::Pattern { .. }) => "Pattern".into(),
                _ => "DeviceGray".into(),
            };
            let name = i.name_obj(&name);
            let arr = i.vm.new_array(vec![name])?;
            Object::lit(Value::Array(arr))
        }
    };
    i.push(object)
}

pub(crate) fn setcolor(i: &mut Interp) -> R {
    let space = Rc::clone(&i.g.space);
    if let Space::Pattern { under } = &*space {
        // A pattern dictionary on top, its underlying colour below.
        let n = under.as_ref().map_or(0, |u| u.components());
        i.need(n + 1)?;
        i.drop_n(n + 1);
        i.g.paint = None;
        return Ok(());
    }
    let n = space.components();
    let mut colour = Vec::with_capacity(n);
    for depth in (0..n).rev() {
        colour.push(i.num(depth)?);
    }
    i.drop_n(n);
    i.set_colour(colour)
}

pub(crate) fn currentcolor(i: &mut Interp) -> R {
    let colour = i.g.colour.clone();
    for v in colour {
        i.push(Object::real(v))?;
    }
    Ok(())
}

pub(crate) fn setpattern(i: &mut Interp) -> R {
    i.pop()?;
    i.set_space(Space::Pattern { under: None }, None)?;
    i.g.paint = None;
    Ok(())
}

pub(crate) fn makepattern(i: &mut Interp) -> R {
    let (dict, _) = (i.dict(1)?, matrix_at(i, 0)?);
    let copy = i.vm.dicts[dict as usize].clone();
    let id = i.vm.new_dict(copy.max)?;
    i.vm.dicts[id as usize] = copy;
    i.drop_n(2);
    i.push(Object::lit(Value::Dict(id)))
}

// ----- images --------------------------------------------------------------

/// What an image operator was given.
struct ImageSpec {
    width: usize,
    height: usize,
    bits: u32,
    matrix: Affine,
    sources: Vec<Object>,
    /// Several sources, one per component.
    multiple: bool,
    space: Rc<Space>,
    decode: Vec<f64>,
    interpolate: bool,
    /// For a mask: whether sample value 1 paints.
    mask_paints_one: Option<bool>,
}

/// The entry `name` of dictionary `dict`.
fn entry(i: &mut Interp, dict: u32, name: &str) -> Option<Object> {
    let key = Key::Name(i.vm.name(name.as_bytes()));
    i.vm.dict_get(dict, &key)
}

/// A whole-number dictionary entry.
fn whole(i: &mut Interp, dict: u32, name: &str) -> R<usize> {
    let value = entry(i, dict, name)
        .and_then(Object::as_f64)
        .ok_or(PsError::TypeCheck)?;
    if !(0.0..=1.0e6).contains(&value) {
        return Err(PsError::RangeCheck.into());
    }
    // Checked to be a small non-negative number just above.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as usize)
}

/// An image dictionary, the form the language's second level added.
fn image_dict(i: &mut Interp, dict: u32, space: Rc<Space>, mask: bool) -> R<ImageSpec> {
    let kind = entry(i, dict, "ImageType")
        .and_then(Object::as_f64)
        .unwrap_or(1.0);
    if kind != 1.0 {
        i.unsupported("masked images (image types 3 and 4), drawn as nothing");
        return Err(PsError::RangeCheck.into());
    }
    let width = whole(i, dict, "Width")?;
    let height = whole(i, dict, "Height")?;
    let bits = if mask {
        1
    } else {
        whole(i, dict, "BitsPerComponent")?
    };
    let matrix = match entry(i, dict, "ImageMatrix").and_then(Object::as_arr) {
        Some(arr) => matrix_of(i, arr)?,
        None => return Err(PsError::TypeCheck.into()),
    };
    let multiple = entry(i, dict, "MultipleDataSources") == Some(Object::bool(true));
    let source = entry(i, dict, "DataSource").ok_or(PsError::TypeCheck)?;
    let sources = match (multiple, source.as_arr()) {
        (true, Some(arr)) if !source.exec => i.vm.arr(arr).to_vec(),
        _ => vec![source],
    };
    let decode = match entry(i, dict, "Decode").and_then(Object::as_arr) {
        Some(arr) => i.numbers(arr)?,
        None => space.default_decode(u32::try_from(bits).unwrap_or(8)),
    };
    let interpolate = entry(i, dict, "Interpolate") == Some(Object::bool(true));
    let mask_paints_one = mask.then(|| decode.first().copied().unwrap_or(0.0) > 0.5);
    Ok(ImageSpec {
        width,
        height,
        bits: u32::try_from(bits).unwrap_or(8),
        matrix,
        sources,
        multiple,
        space,
        decode,
        interpolate,
        mask_paints_one,
    })
}

/// A whole number operand for an image size.
fn size_at(i: &Interp, depth: usize) -> R<usize> {
    let value = i.num(depth)?;
    if !(0.0..=1.0e6).contains(&value) {
        return Err(PsError::RangeCheck.into());
    }
    // Checked to be a small non-negative number just above.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(value as usize)
}

pub(crate) fn image(i: &mut Interp) -> R {
    let spec = if let Value::Dict(dict) = i.peek(0)?.value {
        let space = Rc::clone(&i.g.space);
        let spec = image_dict(i, dict, space, false)?;
        i.drop_n(1);
        spec
    } else {
        let (width, height, bits) = (size_at(i, 4)?, size_at(i, 3)?, size_at(i, 2)?);
        let matrix = matrix_at(i, 1)?;
        let source = i.peek(0)?;
        i.drop_n(5);
        let bits = u32::try_from(bits).unwrap_or(8);
        ImageSpec {
            width,
            height,
            bits,
            matrix,
            sources: vec![source],
            multiple: false,
            space: Rc::new(Space::Gray),
            decode: vec![0.0, 1.0],
            interpolate: false,
            mask_paints_one: None,
        }
    };
    draw_image(i, spec)
}

pub(crate) fn colorimage(i: &mut Interp) -> R {
    let (n, multiple) = (size_at(i, 0)?, i.boolean(1)?);
    let space = match n {
        1 => Space::Gray,
        3 => Space::Rgb,
        4 => Space::Cmyk,
        _ => return Err(PsError::RangeCheck.into()),
    };
    let count = if multiple { n } else { 1 };
    let base = 2 + count;
    let (width, height, bits) = (
        size_at(i, base + 3)?,
        size_at(i, base + 2)?,
        size_at(i, base + 1)?,
    );
    let matrix = matrix_at(i, base)?;
    let mut sources = Vec::with_capacity(count);
    for depth in (2..2 + count).rev() {
        sources.push(i.peek(depth)?);
    }
    i.drop_n(base + 4);
    let decode = [0.0, 1.0].repeat(n);
    draw_image(
        i,
        ImageSpec {
            width,
            height,
            bits: u32::try_from(bits).unwrap_or(8),
            matrix,
            sources,
            multiple,
            space: Rc::new(space),
            decode,
            interpolate: false,
            mask_paints_one: None,
        },
    )
}

pub(crate) fn imagemask(i: &mut Interp) -> R {
    let spec = if let Value::Dict(dict) = i.peek(0)?.value {
        let spec = image_dict(i, dict, Rc::new(Space::Gray), true)?;
        i.drop_n(1);
        spec
    } else {
        let (width, height, polarity) = (size_at(i, 4)?, size_at(i, 3)?, i.boolean(2)?);
        let matrix = matrix_at(i, 1)?;
        let source = i.peek(0)?;
        i.drop_n(5);
        ImageSpec {
            width,
            height,
            bits: 1,
            matrix,
            sources: vec![source],
            multiple: false,
            space: Rc::new(Space::Gray),
            decode: vec![0.0, 1.0],
            interpolate: false,
            mask_paints_one: Some(polarity),
        }
    };
    draw_image(i, spec)
}

/// Reads `need` bytes from an image data source: a string (used again and
/// again), a procedure (called until it has given enough), or a file.
fn read_source(i: &mut Interp, source: Object, need: usize, out: &mut Vec<u8>) -> R {
    match source.value {
        Value::File(file) => {
            while out.len() < need {
                match i.read_byte(file)? {
                    Some(byte) => out.push(byte),
                    None => break,
                }
                if out.len().is_multiple_of(1024) {
                    i.tick(16)?;
                }
            }
            i.settle(file)?;
        }
        Value::String(s) if !source.exec => {
            let bytes = i.vm.str_bytes(s).to_vec();
            if bytes.is_empty() {
                return Ok(());
            }
            while out.len() < need {
                let take = (need - out.len()).min(bytes.len());
                out.extend_from_slice(&bytes[..take]);
                i.tick(1)?;
            }
        }
        _ => {
            while out.len() < need {
                i.call(source)?;
                let result = i.pop()?;
                let Value::String(s) = result.value else {
                    return Err(PsError::TypeCheck.into());
                };
                if s.len == 0 {
                    break;
                }
                out.extend_from_slice(i.vm.str_bytes(s));
                i.tick(1 + u64::from(s.len) / 1024)?;
            }
        }
    }
    Ok(())
}

/// The sample at `index` (counting samples, not bytes) of `data`.
fn sample(data: &[u8], bits: u32, index: usize) -> u32 {
    match bits {
        8 => u32::from(data.get(index).copied().unwrap_or(0)),
        16 => {
            let hi = u32::from(data.get(index * 2).copied().unwrap_or(0));
            let lo = u32::from(data.get(index * 2 + 1).copied().unwrap_or(0));
            (hi << 8) | lo
        }
        1 | 2 | 4 => {
            let bit = index * bits as usize;
            let byte = u32::from(data.get(bit / 8).copied().unwrap_or(0));
            let shift = 8 - bits as usize - bit % 8;
            (byte >> shift) & ((1 << bits) - 1)
        }
        _ => 0,
    }
}

fn draw_image(i: &mut Interp, spec: ImageSpec) -> R {
    let ImageSpec {
        width,
        height,
        bits,
        matrix,
        sources,
        multiple,
        space,
        decode,
        interpolate,
        mask_paints_one,
    } = spec;
    if !matches!(bits, 1 | 2 | 4 | 8 | 16) {
        return Err(PsError::RangeCheck.into());
    }
    let pixels = width.saturating_mul(height);
    if pixels > i.limits.image_pixels {
        return Err(Fault::Halt(Halt::Output));
    }
    let n = if mask_paints_one.is_some() {
        1
    } else {
        space.components()
    };
    if n == 0 {
        return Err(PsError::RangeCheck.into());
    }
    let bits_usize = bits as usize;
    // The samples are held whole before they are turned into pixels.
    i.vm.charge((width * n * bits_usize).div_ceil(8).saturating_mul(height))?;
    // Read every row, from one source or one per component.
    let mut planes: Vec<Vec<u8>> = Vec::new();
    let per_row = |components: usize| (width * components * bits_usize).div_ceil(8);
    if multiple && sources.len() == n {
        for source in &sources {
            let mut data = Vec::new();
            read_source(i, *source, per_row(1) * height, &mut data)?;
            planes.push(data);
        }
    } else {
        let mut data = Vec::new();
        let source = *sources.first().ok_or(PsError::TypeCheck)?;
        read_source(i, source, per_row(n) * height, &mut data)?;
        planes.push(data);
    }
    i.vm.charge(pixels * 4)?;
    i.tick(u64::try_from(pixels / 64).unwrap_or(0))?;
    let max = f64::from((1u32 << bits) - 1);
    let paint = i.g.paint;
    let mut rgba = Vec::with_capacity(pixels * 4);
    let mut cache: HashMap<Vec<u32>, Option<Rgb>> = HashMap::new();
    let device = matches!(*space, Space::Gray | Space::Rgb | Space::Cmyk);
    let mut values = vec![0.0; n];
    let mut key = vec![0u32; n];
    for row in 0..height {
        for col in 0..width {
            for c in 0..n {
                key[c] = if planes.len() == 1 {
                    let row_start = row * per_row(n) * 8 / bits_usize;
                    sample(&planes[0], bits, row_start + col * n + c)
                } else {
                    let row_start = row * per_row(1) * 8 / bits_usize;
                    sample(&planes[c], bits, row_start + col)
                };
            }
            if let Some(one_paints) = mask_paints_one {
                let paints = (key[0] == 1) == one_paints;
                match (paints, paint) {
                    (true, Some(colour)) => {
                        let [r, g, b] = colour.to_u8();
                        rgba.extend_from_slice(&[r, g, b, 255]);
                    }
                    _ => rgba.extend_from_slice(&[0, 0, 0, 0]),
                }
                continue;
            }
            for c in 0..n {
                let (d0, d1) = (
                    decode.get(2 * c).copied().unwrap_or(0.0),
                    decode.get(2 * c + 1).copied().unwrap_or(1.0),
                );
                values[c] = d0 + f64::from(key[c]) * (d1 - d0) / max;
            }
            let colour = if device {
                i.resolve(&space, &values)?
            } else if let Some(colour) = cache.get(&key) {
                *colour
            } else {
                let colour = i.resolve(&space, &values)?;
                cache.insert(key.clone(), colour);
                colour
            };
            match colour {
                Some(colour) => {
                    let [r, g, b] = colour.to_u8();
                    rgba.extend_from_slice(&[r, g, b, 255]);
                }
                None => rgba.extend_from_slice(&[0, 0, 0, 0]),
            }
        }
    }
    if width == 0 || height == 0 {
        return Ok(());
    }
    // The image matrix takes user space to image space; the item needs
    // image space to the page.
    let Ok(to_user) = inverse(matrix) else {
        return Ok(());
    };
    let transform = i.g.ctm * to_user;
    let raster = Raster {
        width: u32::try_from(width).map_err(|_| PsError::LimitCheck)?,
        height: u32::try_from(height).map_err(|_| PsError::LimitCheck)?,
        pixels: rgba,
    };
    let clip = i.g.clip;
    emit(
        i,
        Item::Image {
            raster: Arc::new(raster),
            transform,
            interpolate,
            clip,
        },
        1,
    )
}
