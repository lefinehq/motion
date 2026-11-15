//! Unit tests for `src/when.rs`.

use super::*;
use crate::units::Time::{Frames, Seconds};

fn moment(text: &str) -> Moment {
    text.parse().unwrap()
}

fn based(base: Base, shift: Time) -> Moment {
    Moment { base, shift }
}

fn label(name: &str) -> Base {
    Base::Label(name.to_owned())
}

#[test]
fn moments_read_times_and_bases_with_moves() {
    assert_eq!(moment("2s"), Moment::at(Seconds(2.0)));
    assert_eq!(moment("-6f"), Moment::at(Frames(-6.0)));
    assert_eq!(moment("cta"), based(label("cta"), Frames(0.0)));
    assert_eq!(moment("cta+12f"), based(label("cta"), Frames(12.0)));
    assert_eq!(moment("cta-0.5s"), based(label("cta"), Seconds(-0.5)));
    assert_eq!(moment("<"), based(Base::Before, Frames(0.0)));
    assert_eq!(moment("<+6f"), based(Base::Before, Frames(6.0)));
    assert_eq!(moment("<<-3f"), based(Base::BeforeStart, Frames(-3.0)));
    assert_eq!(moment("part_2+1s"), based(label("part_2"), Seconds(1.0)));
    for bad in ["", "cta+", "cta+x", "Cta", "<<<", "cta 2s", "2", "part-2"] {
        let e = bad.parse::<Moment>().unwrap_err();
        assert_eq!(e.expected.as_deref(), Some(MOMENT_EXPECTED), "{bad}");
    }
}

#[test]
fn moments_print_as_written() {
    for text in [
        "2s", "-6f", "cta", "cta+12f", "cta-0.5s", "<", "<+6f", "<<-3f",
    ] {
        assert_eq!(moment(text).to_string(), text);
    }
}

fn span(text: &str) -> (Moment, Moment) {
    let Moments { start, end } = text.parse().unwrap();
    (start, end)
}

#[test]
fn a_dash_after_a_base_moves_it_unless_the_span_then_has_no_end() {
    assert_eq!(
        span("intro-cta"),
        (
            based(label("intro"), Frames(0.0)),
            based(label("cta"), Frames(0.0))
        )
    );
    assert_eq!(
        span("intro-6f-cta"),
        (
            based(label("intro"), Frames(-6.0)),
            based(label("cta"), Frames(0.0))
        )
    );
    assert_eq!(
        span("intro-2s"),
        (based(label("intro"), Frames(0.0)), Moment::at(Seconds(2.0)))
    );
    assert_eq!(
        span("<<+1s-<+30f"),
        (
            based(Base::BeforeStart, Seconds(1.0)),
            based(Base::Before, Frames(30.0))
        )
    );
    assert_eq!(
        span("0-cta"),
        (Moment::at(Frames(0.0)), based(label("cta"), Frames(0.0)))
    );
    assert_eq!(
        span("1s-cta+2s"),
        (Moment::at(Seconds(1.0)), based(label("cta"), Seconds(2.0)))
    );
}

#[test]
fn a_plain_span_reads_and_fails_as_before() {
    assert_eq!(
        span("0-30f"),
        (Moment::at(Frames(0.0)), Moment::at(Frames(30.0)))
    );
    assert_eq!(
        span("0.5s-1s"),
        (Moment::at(Seconds(0.5)), Moment::at(Seconds(1.0)))
    );
    let plain = "30f".parse::<Moments>().unwrap_err();
    assert_eq!(plain, "30f".parse::<TimeSpan>().unwrap_err());
    let relative = "intro".parse::<Moments>().unwrap_err();
    assert_eq!(relative.expected.as_deref(), Some(MOMENTS_EXPECTED));
}

#[test]
fn a_base_and_its_move_add_in_their_unit_or_in_frames() {
    let at = |m: &str, base: Time| moment(m).resolve(|_| Ok(base), 30.0).unwrap();
    assert_eq!(at("cta+12f", Frames(10.0)), Frames(22.0));
    assert_eq!(at("cta-0.5s", Seconds(2.0)), Seconds(1.5));
    assert_eq!(at("cta+1s", Frames(10.0)), Frames(40.0));
    assert_eq!(at("3s", Frames(10.0)), Seconds(3.0));
}

#[test]
fn label_names_have_no_dash() {
    assert!(is_label("cta") && is_label("part_2") && is_label("a1"));
    assert!(!is_label("part-2") && !is_label("2a") && !is_label("") && !is_label("Cta"));
}
