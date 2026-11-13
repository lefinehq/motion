//! Unit tests for `src/at.rs`.

use crate::at;
#[test]
fn at_prints_resolved_values() {
    let s = motion_format::parse_document(include_str!("../../../../examples/launch.json"))
        .unwrap()
        .scene;
    let g = motion_scene::compile(&s, &motion_scene::ApproxTextMeasure);
    assert_eq!(
        at(&g, 200.0, &["title".into()]).unwrap(),
        "title x=960 y=260 w=580.8 h=115.2 opacity=1"
    );
    assert_eq!(at(&g, 0.0, &[]).unwrap().lines().count(), 2);
    assert_eq!(
        at(&g, 0.0, &["nope".into()]).unwrap_err().to_string(),
        "no layer with id `nope`"
    );
    assert_eq!(
        at(&g, -1.0, &[]).unwrap_err().to_string(),
        "time must be 0 or later"
    );
    let empty = motion_scene::compile(
        &motion_format::parse_document(r#"{"motion":1,"dur":"1s","layers":[]}"#)
            .unwrap()
            .scene,
        &motion_scene::ApproxTextMeasure,
    );
    assert_eq!(at(&empty, 0.0, &[]).unwrap(), "");
}
