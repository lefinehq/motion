//! The graphics state: the matrix, the current path, the clip, the colour
//! and the line style, as `gsave` copies and `grestore` brings back.
//!
//! The current path is kept in page space, as the language keeps it in
//! device space: a point is placed by the matrix current when it is added,
//! and later changes of the matrix do not move it.

use std::rc::Rc;

use kurbo::{Affine, BezPath, PathEl, Point};

use crate::colour::Space;
use crate::object::Object;
use crate::page::Rgb;

/// Line caps, as `setlinecap` numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cap {
    Butt,
    Round,
    Square,
}

/// Line joins, as `setlinejoin` numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Join {
    Miter,
    Round,
    Bevel,
}

/// The current path, in page space.
#[derive(Debug, Clone, Default)]
pub(crate) struct Path {
    pub bez: BezPath,
    /// The current point.
    pub current: Option<Point>,
    /// Where the current subpath started.
    pub start: Option<Point>,
    /// Whether the last subpath was closed, so the next segment starts a
    /// new subpath at its start.
    pub closed: bool,
}

impl Path {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn move_to(&mut self, p: Point) {
        if let Some(PathEl::MoveTo(last)) = self.bez.elements_mut().last_mut() {
            *last = p;
        } else {
            self.bez.move_to(p);
        }
        self.current = Some(p);
        self.start = Some(p);
        self.closed = false;
    }

    /// Starts a new subpath where a closed one began, before a segment.
    fn reopen(&mut self) {
        if self.closed {
            if let Some(start) = self.start {
                self.bez.move_to(start);
            }
            self.closed = false;
        }
    }

    pub fn line_to(&mut self, p: Point) {
        self.reopen();
        self.bez.line_to(p);
        self.current = Some(p);
    }

    pub fn curve_to(&mut self, a: Point, b: Point, p: Point) {
        self.reopen();
        self.bez.curve_to(a, b, p);
        self.current = Some(p);
    }

    pub fn close(&mut self) {
        if self.current.is_some() && !self.closed {
            if !matches!(self.bez.elements().last(), Some(PathEl::MoveTo(_)) | None) {
                self.bez.close_path();
            }
            self.closed = true;
            self.current = self.start;
        }
    }

    /// The path without a trailing lone move, which paints nothing.
    pub fn painted(&self) -> BezPath {
        let mut bez = self.bez.clone();
        if matches!(bez.elements().last(), Some(PathEl::MoveTo(_))) {
            bez.pop();
        }
        bez
    }
}

/// One graphics state.
#[derive(Debug, Clone)]
pub(crate) struct GState {
    pub ctm: Affine,
    pub path: Path,
    /// The current clip, by index into the page's clips.
    pub clip: Option<usize>,
    /// The innermost clip path, for `clippath`.
    pub clip_path: Option<Rc<BezPath>>,
    pub space: Rc<Space>,
    /// The colour space as the program gave it, for `currentcolorspace`.
    pub space_object: Option<Object>,
    pub colour: Vec<f64>,
    /// The colour fills and strokes paint, or `None` when they paint
    /// nothing (a separation of no colorant, an unsupported pattern).
    pub paint: Option<Rgb>,
    pub line_width: f64,
    pub cap: Cap,
    pub join: Join,
    pub miter: f64,
    pub dash: Vec<f64>,
    pub dash_offset: f64,
    pub flat: f64,
    pub smoothness: f64,
    pub stroke_adjust: bool,
    pub overprint: bool,
    pub font: Object,
}

impl GState {
    pub fn new(ctm: Affine) -> Self {
        Self {
            ctm,
            path: Path::default(),
            clip: None,
            clip_path: None,
            space: Rc::new(Space::Gray),
            space_object: None,
            colour: vec![0.0],
            paint: Some(Rgb::BLACK),
            line_width: 1.0,
            cap: Cap::Butt,
            join: Join::Miter,
            miter: 10.0,
            dash: Vec::new(),
            dash_offset: 0.0,
            flat: 1.0,
            smoothness: 0.02,
            stroke_adjust: false,
            overprint: false,
            font: Object::NULL,
        }
    }
}
