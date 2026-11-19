use serde_json::json;

use super::*;
use crate::store::tests::scratch;

#[test]
fn keeps_each_frame_and_reads_it_back() {
    let store = scratch("matte-frames");
    let clip = Sha256::of(b"clip");
    let first = Alpha::from_unit(3, 2, &[0.0, 0.5, 1.0, 1.5, -1.0, 0.25]);
    assert_eq!(first.values, [0, 32768, 65535, 65535, 0, 16384]);
    let mut writer = MatteWriter::new(&store, [3, 2]);
    writer.push(&first).unwrap();
    writer.push(&first).unwrap();
    let digest = writer
        .finish(json!({"edges":"hard"}), clip, Made::here(1))
        .unwrap();
    let matte = Manifest::<Matte>::open(&store, &digest, &json!({"edges":"hard"}), &clip).unwrap();
    assert_eq!(matte.frames(), 2);
    assert_eq!(
        matte.content.frames[0], matte.content.frames[1],
        "one file for equal frames"
    );
    assert_eq!(matte.alpha(&store, 1).unwrap(), Some(first));
    assert_eq!(matte.alpha(&store, 2).unwrap(), None);
    std::fs::remove_dir_all(store.root()).unwrap();
}

#[test]
fn writes_the_same_bytes_for_the_same_alpha() {
    let alpha = Alpha::from_unit(2, 2, &[0.1, 0.2, 0.3, 0.4]);
    assert_eq!(alpha.encode().unwrap(), alpha.encode().unwrap());
    assert_eq!(Alpha::decode(&alpha.encode().unwrap()).unwrap(), alpha);
}

#[test]
fn rounds_to_eight_bits_to_the_nearest_level() {
    let alpha = Alpha {
        width: 4,
        height: 1,
        values: vec![0, 128, 129, 65535],
    };
    assert_eq!(
        (0..4).map(|i| alpha.level8(i)).collect::<Vec<_>>(),
        [0, 0, 1, 255]
    );
}

#[test]
fn refuses_an_alpha_of_another_size() {
    let store = scratch("matte-size");
    let mut writer = MatteWriter::new(&store, [3, 2]);
    let error = writer.push(&Alpha::from_unit(2, 2, &[0.0; 4])).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
}

#[test]
fn reports_a_missing_or_foreign_frame() {
    let store = scratch("matte-missing");
    let clip = Sha256::of(b"clip");
    let manifest = Manifest::new(
        json!({}),
        clip,
        [2, 2],
        Made::here(1),
        Matte {
            frames: vec![
                Sha256::of(b"gone"),
                store
                    .put(&Alpha::from_unit(1, 1, &[1.0]).encode().unwrap())
                    .unwrap(),
            ],
        },
    );
    assert_eq!(manifest.alpha(&store, 0).unwrap_err().kind, Kind::Missing);
    assert_eq!(
        manifest.alpha(&store, 1).unwrap_err().kind,
        Kind::Invalid,
        "1x1, not 2x2"
    );
    let junk = store.put(b"not a png").unwrap();
    assert_eq!(Alpha::decode(b"not a png").unwrap_err().kind, Kind::Invalid);
    let _ = junk;
    std::fs::remove_dir_all(store.root()).unwrap();
}
