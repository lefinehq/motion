//! Unit tests for `src/drawing.rs`: laying a drawing over a box, and
//! drawing it into pictures.

use crate::sample::{at, page, pdf, read};

use super::*;

#[test]
fn placed_stretches_the_region_over_the_frame_and_cuts_the_rest() {
    let drawing = read(&page("", "1 0 0 rg 0 0 20 20 re f", vec![]));
    let region = Rect::new(5.0, 5.0, 15.0, 10.0);
    let Node::Layer(layer) = drawing.placed(region, Rect::new(0.0, 0.0, 100.0, 50.0)) else {
        panic!("a layer");
    };
    assert_eq!(layer.clip, Some(Clip::Rect(region)));
    assert_eq!(
        layer.transform * nakkash_raster::kurbo::Point::new(5.0, 5.0),
        (0.0, 0.0).into()
    );
    assert_eq!(
        layer.transform * nakkash_raster::kurbo::Point::new(15.0, 10.0),
        (100.0, 50.0).into()
    );
}

#[test]
fn a_picture_is_drawn_once_per_region_and_size() {
    let drawing = read(&pdf(
        (40.0, 20.0),
        "",
        "",
        "1 0 0 rg 0 0 20 20 re f",
        vec![],
    ));
    let first = drawing.picture(drawing.bounds(), 80, 40).unwrap();
    assert_eq!((first.width(), first.height()), (80, 40));
    assert_eq!(drawing.picture(drawing.bounds(), 80, 40).unwrap(), first);
    let half = drawing
        .picture(Rect::new(0.0, 0.0, 20.0, 20.0), 10, 10)
        .unwrap();
    assert_eq!((half.width(), half.height()), (10, 10));
    assert_eq!(drawing.pictures.lock().unwrap().len(), 2);
}

#[test]
fn a_picture_shows_the_region_alone() {
    let drawing = read(&page("", "1 0 0 rg 0 10 10 10 re f", vec![]));
    // The top-left quarter, red, over the whole picture.
    let picture = drawing
        .picture(Rect::new(0.0, 0.0, 10.0, 10.0), 4, 4)
        .unwrap();
    let frame = Frame {
        background: None,
        blending: Blending::Gamma,
        nodes: vec![Node::Image(nakkash_raster::ImageDraw {
            picture,
            rect: Rect::new(0.0, 0.0, 4.0, 4.0),
        })],
    };
    let image = nakkash_raster::render_with(&frame, 4, 4, RenderOptions::GOLDEN).unwrap();
    assert_eq!(at(&image, 0, 0), [255, 0, 0, 255]);
    assert_eq!(at(&image, 3, 3), [255, 0, 0, 255]);
}

#[test]
fn sizes_and_regions_without_pixels_are_errors() {
    let drawing = read(&page("", "", vec![]));
    assert_eq!(
        drawing.picture(drawing.bounds(), 0, 4).unwrap_err().kind,
        Kind::OutOfRange
    );
    let flat = Rect::new(0.0, 0.0, 0.0, 4.0);
    assert_eq!(
        drawing.picture(flat, 4, 4).unwrap_err().kind,
        Kind::OutOfRange
    );
}

#[test]
fn debug_shows_the_page_not_the_nodes() {
    let drawing = read(&page("", "0 0 1 1 re f", vec![]));
    let text = format!("{drawing:?}");
    assert!(text.contains("PDF") && text.contains("nodes: 1"), "{text}");
}
