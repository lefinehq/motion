use super::*;

#[test]
fn a_colour_moved_to_another_hue_keeps_its_luminance() {
    let accent = [0.9, 0.22, 0.27, 1.0];
    let y = luminance(accent);
    for hue in [0.0, 90.0, 180.0, 270.0] {
        let moved = with_luminance(hue, to_lch(accent).c, y, 1.0);
        assert!((luminance(moved) - y).abs() < 1e-4, "{hue}: {moved:?}");
    }
}

#[test]
fn oklch_reads_back() {
    let c = [0.2, 0.5, 0.8, 1.0];
    let back = from_lch(to_lch(c), 1.0);
    for (a, b) in c.iter().zip(back) {
        assert!((a - b).abs() < 1e-6, "{c:?} {back:?}");
    }
}

#[test]
fn white_and_black_are_the_ends_of_luminance() {
    assert!((luminance([1.0, 1.0, 1.0, 1.0]) - 1.0).abs() < 1e-9);
    assert!(luminance([0.0, 0.0, 0.0, 1.0]).abs() < 1e-9);
}
