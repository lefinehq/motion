//! Small PDF files written by hand for the tests: one page, its content
//! stream and resources as text, and any further objects, with a correct
//! cross-reference table so no reader has to repair them.

use motion_raster::kurbo::Rect;
use motion_raster::{Blending, Frame, Image, RenderOptions};

use crate::Drawing;

/// One more object of a sample file, numbered from 5 in order.
pub(crate) enum Object {
    /// A dictionary or any other object, as written.
    Plain(String),
    /// A stream: its dictionary, without `/Length`, and its bytes.
    Stream(String, Vec<u8>),
}

/// A plain object.
pub(crate) fn plain(text: &str) -> Object {
    Object::Plain(text.to_owned())
}

/// A stream object.
pub(crate) fn stream(dict: &str, bytes: impl Into<Vec<u8>>) -> Object {
    Object::Stream(dict.to_owned(), bytes.into())
}

/// A one-page PDF file `width` by `height` points, whose page holds
/// `resources` and `page` (more page keys, such as `/ArtBox`), draws
/// `content`, and is followed by `objects`, numbered 5 on.
pub(crate) fn pdf(
    (width, height): (f64, f64),
    page: &str,
    resources: &str,
    content: &str,
    objects: Vec<Object>,
) -> Vec<u8> {
    let mut all = vec![
        Object::Plain("<< /Type /Catalog /Pages 2 0 R >>".to_owned()),
        Object::Plain("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned()),
        Object::Plain(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] {page} \
             /Resources << {resources} >> /Contents 4 0 R >>"
        )),
        stream("<<>>", content.as_bytes()),
    ];
    all.extend(objects);
    let mut file = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in all.iter().enumerate() {
        offsets.push(file.len());
        file.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        match object {
            Object::Plain(text) => file.extend(text.as_bytes()),
            Object::Stream(dict, bytes) => {
                let dict = dict.trim_end_matches(">>");
                file.extend(format!("{dict} /Length {} >>\nstream\n", bytes.len()).as_bytes());
                file.extend(bytes);
                file.extend(b"\nendstream");
            }
        }
        file.extend(b"\nendobj\n");
    }
    let xref = file.len();
    file.extend(format!("xref\n0 {}\n0000000000 65535 f \n", all.len() + 1).as_bytes());
    for offset in offsets {
        file.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    file.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            all.len() + 1
        )
        .as_bytes(),
    );
    file
}

/// A page 20 by 20 points that draws `content` with `resources`.
pub(crate) fn page(resources: &str, content: &str, objects: Vec<Object>) -> Vec<u8> {
    pdf((20.0, 20.0), "", resources, content, objects)
}

/// `bytes` read as vector footage.
pub(crate) fn read(bytes: &[u8]) -> Drawing {
    crate::read(bytes).expect("a sample PDF reads")
}

/// The whole page of `drawing` drawn as vectors at `scale` pixels a point,
/// with the portable rasteriser.
pub(crate) fn drawn(drawing: &Drawing, scale: f64) -> Image {
    let size = drawing.size();
    let (width, height) = (
        (size.width * scale).round() as u32,
        (size.height * scale).round() as u32,
    );
    let frame = Frame {
        background: None,
        blending: Blending::Gamma,
        nodes: vec![drawing.placed(
            drawing.bounds(),
            Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
        )],
    };
    motion_raster::render_with(&frame, width, height, RenderOptions::GOLDEN).expect("drawn")
}

/// The page of the PDF `bytes` drawn at one pixel a point.
pub(crate) fn render(bytes: &[u8]) -> Image {
    drawn(&read(bytes), 1.0)
}

/// The straight RGBA pixel at `(x, y)`, y down from the page's top.
pub(crate) fn at(image: &Image, x: u32, y: u32) -> [u8; 4] {
    image.pixel(x, y).expect("a pixel inside the image")
}

/// Whether two pixels differ by at most `by` on every channel.
pub(crate) fn near(a: [u8; 4], b: [u8; 4], by: u8) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= by)
}

/// `bytes` in a zlib stream of stored (uncompressed) deflate blocks: what
/// a `/FlateDecode` filter reads, written without a compressor.
pub(crate) fn zlib_stored(bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let chunks: Vec<&[u8]> = bytes.chunks(65535).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        out.push(u8::from(i + 1 == chunks.len()));
        let length = chunk.len() as u16;
        out.extend(length.to_le_bytes());
        out.extend((!length).to_le_bytes());
        out.extend(*chunk);
    }
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    out.extend(((b << 16) | a).to_be_bytes());
    out
}
