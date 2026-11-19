use super::*;
use crate::segment::tests::Greys;

#[test]
fn shows_frames_spread_over_the_clip_with_the_prompts() {
    assert_eq!(chosen(48, 8, &[]), [0, 6, 13, 20, 26, 33, 40, 47]);
    assert_eq!(chosen(48, 4, &[5, 47]), [0, 5, 15, 31, 47]);
    assert_eq!(chosen(3, 8, &[]), [0, 1, 2]);
    assert_eq!(chosen(1, 1, &[]), [0]);
}

#[test]
fn draws_a_sheet_with_the_mask_over_each_frame() {
    // The mask covers the left half of every frame.
    let mut mask = Plane::filled(8, 6, -1.0);
    for y in 0..6 {
        for x in 0..4 {
            mask.data[y * 8 + x] = 1.0;
        }
    }
    let masks = vec![mask; 4];
    let prompts = [Prompt {
        frame: 1,
        area: Some([1.0, 1.0, 5.0, 4.0]),
        include: vec![[2.0, 2.0]],
        exclude: vec![[6.0, 5.0]],
    }];
    let points = vec![vec![[3.0, 3.0]]; 4];
    let (png, shown) = sheet(&mut Greys(4), &masks, &prompts, &points, 4, 16).unwrap();
    assert_eq!(shown, [0, 1, 2, 3]);
    let decoded = png::Decoder::new(Cursor::new(png)).read_info().unwrap();
    let info = decoded.info();
    // Two by two tiles of 16 by 12 with 4 px gaps.
    assert_eq!((info.width, info.height), (2 * 20 + 4, 2 * 16 + 4));
}
