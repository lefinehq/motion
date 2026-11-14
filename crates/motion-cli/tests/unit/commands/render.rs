use super::*;
use rstest::rstest;

#[rstest]
#[case::one("7", 7, 7)]
#[case::a_range("0-24", 0, 24)]
#[case::spaces_around_the_ends("3 - 5", 3, 5)]
#[case::a_range_of_one("4-4", 4, 4)]
fn frames_parse(#[case] text: &str, #[case] first: u32, #[case] last: u32) {
    assert_eq!(text.parse::<Frames>(), Ok(Frames { first, last }));
}

#[rstest]
#[case::backwards("5-3", "`5-3` ends before it starts")]
#[case::not_a_number("x", "`x` is not a frame number; expected `N` or `A-B`")]
#[case::negative("-2", "`` is not a frame number; expected `N` or `A-B`")]
#[case::open_end("2-", "`` is not a frame number; expected `N` or `A-B`")]
fn bad_frames_are_rejected(#[case] text: &str, #[case] message: &str) {
    assert_eq!(text.parse::<Frames>(), Err(message.to_owned()));
}

#[test]
fn both_ends_are_drawn() {
    assert_eq!(Frames { first: 2, last: 4 }.range(), 2..5);
    assert_eq!(Frames::one(u32::MAX).range(), u32::MAX..u32::MAX);
}

#[test]
fn files_are_numbered_to_five_digits() {
    assert_eq!(file_name(42), "frame-00042.png");
    assert_eq!(file_name(123_456), "frame-123456.png");
}
