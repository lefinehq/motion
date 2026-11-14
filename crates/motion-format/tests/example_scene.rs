//! Integration test: the shipped example scene parses and is already in
//! canonical form.

#[test]
fn example_scene_parses_and_round_trips_byte_for_byte() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/launch.json"
    ))
    .unwrap();
    let doc = motion_format::parse_document(&src).unwrap();
    assert_eq!(doc.scene().layers.len(), 2);
    assert_eq!(motion_format::write_document(doc.raw()), src);
}
