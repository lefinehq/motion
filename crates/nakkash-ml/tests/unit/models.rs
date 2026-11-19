use std::process;

use super::*;
use crate::lock::tests::{DOWNLOAD, small_lock};

/// The smallest plain ONNX model: IR 8, the standard operators at 17, an
/// empty graph.
pub(crate) const ONNX: [u8; 10] = [0x08, 0x08, 0x42, 0x04, 0x0a, 0x00, 0x10, 0x11, 0x3a, 0x00];

/// An `.npy` file of the floats 1 and 2.
pub(crate) fn npy() -> Vec<u8> {
    let mut header = "{'descr': '<f4', 'fortran_order': False, 'shape': (2,), }".to_owned();
    while (10 + header.len() + 1) % 64 != 0 {
        header.push(' ');
    }
    header.push('\n');
    let mut bytes = vec![0x93, b'N', b'U', b'M', b'P', b'Y', 1, 0];
    bytes.extend((header.len() as u16).to_le_bytes());
    bytes.extend(header.as_bytes());
    bytes.extend(1f32.to_le_bytes());
    bytes.extend(2f32.to_le_bytes());
    bytes
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("nakkash-ml-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

fn lock(obtain: &str) -> ModelLock {
    ModelLock::read(&small_lock(
        &Sha256::of(&ONNX),
        &Sha256::of(&npy()),
        "ok",
        obtain,
    ))
    .unwrap()
}

#[test]
fn imports_what_the_lock_pins_and_opens_it() {
    let root = scratch("import");
    let folder = root.join("export");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("g.onnx"), ONNX).unwrap();
    fs::write(folder.join("c.npy"), npy()).unwrap();
    fs::write(folder.join("other.bin"), b"not pinned").unwrap();
    let store = ModelStore::new(&root.join("cache"));
    let lock = lock(DOWNLOAD);
    let imported = store.import(&lock, &folder).unwrap();
    assert_eq!(
        imported,
        [
            Imported::Kept {
                path: folder.join("c.npy"),
                name: "test.model c".into()
            },
            Imported::Kept {
                path: folder.join("g.onnx"),
                name: "test.model g".into()
            },
            Imported::Unknown {
                path: folder.join("other.bin")
            },
        ]
    );
    let graph = store.graph(&lock, "test.model", "g", 2).unwrap();
    assert_eq!(graph.path, store.files().path(&Sha256::of(&ONNX)));
    assert_eq!((graph.threads, graph.placement), (2, crate::Placement::Cpu));
    assert_eq!(graph.run().backend, "cpu");
    let constant = store.constant(&lock, "test.model", "c").unwrap();
    assert_eq!(constant, Tensor::f32([2], vec![1.0, 2.0]));
    let states = store.verify(&lock).unwrap();
    assert_eq!(states.iter().filter(|s| s.present).count(), 2, "{states:?}");
    assert!(!states[0].present, "no runtime library: {states:?}");
    assert_eq!(
        store.runtime_library(&lock).unwrap_err().kind,
        Kind::Missing
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn refuses_to_import_a_graph_that_fails_the_check() {
    let root = scratch("import-refused");
    let custom = [0x08, 0x08, 0x42, 0x05, 0x0a, 0x01, b'x', 0x10, 0x01];
    fs::write(root.join("g.onnx"), custom).unwrap();
    let lock = ModelLock::read(&small_lock(
        &Sha256::of(&custom),
        &Sha256::of(b"c"),
        "ok",
        DOWNLOAD,
    ))
    .unwrap();
    let store = ModelStore::new(&root.join("cache"));
    let error = store.import(&lock, &root).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert!(!store.files().has(&Sha256::of(&custom)));
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn names_how_to_get_a_missing_file() {
    let root = scratch("missing");
    let store = ModelStore::new(&root);
    let error = store
        .graph(&lock(DOWNLOAD), "test.model", "g", 1)
        .unwrap_err();
    assert_eq!(error.kind, Kind::Missing);
    assert!(error.message.contains(FETCH_COMMAND), "{}", error.message);
    let export = r#"{"export":{"weights":"w","weights_sha256":"0000000000000000000000000000000000000000000000000000000000000000","code":"c","exporter":"e","toolchain":"t","parity_max_abs":0.0}}"#;
    let error = store
        .constant(&lock(export), "test.model", "c")
        .unwrap_err();
    assert!(error.message.contains(IMPORT_COMMAND), "{}", error.message);
    assert_eq!(
        store
            .graph(&lock(DOWNLOAD), "test.model", "h", 1)
            .unwrap_err()
            .kind,
        Kind::UnknownName
    );
    assert!(
        store
            .compiled(&Sha256::of(b"g"), "mlprogram-all", "1.31.0")
            .ends_with(format!("{}-mlprogram-all-1.31.0", Sha256::of(b"g")))
    );
    fs::remove_dir_all(&root).unwrap();
}

#[rstest::rstest]
#[case::not_npy(b"nope".to_vec())]
#[case::other_shape(npy().iter().map(|&b| if b == b'2' { b'3' } else { b }).collect())]
#[case::short(npy()[..npy().len() - 1].to_vec())]
fn refuses_a_constant_that_is_not_the_locked_array(#[case] bytes: Vec<u8>) {
    assert_eq!(npy_f32(&bytes, &[2]).unwrap_err().kind, Kind::Invalid);
}

#[test]
fn reads_a_constant_of_several_dimensions() {
    let header = "{'descr': '<f4', 'fortran_order': False, 'shape': (1, 2), }";
    let mut bytes = vec![0x93, b'N', b'U', b'M', b'P', b'Y', 1, 0];
    bytes.extend((header.len() as u16).to_le_bytes());
    bytes.extend(header.as_bytes());
    bytes.extend([0u8; 8]);
    assert_eq!(npy_f32(&bytes, &[1, 2]).unwrap(), [0.0, 0.0]);
}
