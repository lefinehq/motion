use std::sync::{Arc, Mutex};

use super::*;
use crate::lock::platform;
use crate::segment::tests::{Fake, Greys, Seen, fakes};
use crate::{Graph, OpenGraph, Outputs, Tensor};

/// A plain ONNX file told apart by its IR version.
fn onnx(version: u8) -> Vec<u8> {
    vec![
        0x08, version, 0x42, 0x04, 0x0a, 0x00, 0x10, 0x11, 0x3a, 0x00,
    ]
}

/// An `.npy` file of 256 zeros.
fn zeros() -> Vec<u8> {
    let mut header = "{'descr': '<f4', 'fortran_order': False, 'shape': (256,), }".to_owned();
    while !(10 + header.len() + 1).is_multiple_of(64) {
        header.push(' ');
    }
    header.push('\n');
    let mut bytes = vec![0x93, b'N', b'U', b'M', b'P', b'Y', 1, 0];
    bytes.extend((header.len() as u16).to_le_bytes());
    bytes.extend(header.as_bytes());
    bytes.extend([0u8; 1024]);
    bytes
}

const GRAPHS: [&str; 5] = [
    "image-encoder",
    "decoder-multi",
    "decoder-single",
    "memory-encoder",
    "memory-attention",
];

/// A lock of the segmenter and the edge model on stand-in files, kept in a
/// store under `root`.
fn setup(root: &std::path::Path) -> (ModelLock, ModelStore) {
    let folder = root.join("export");
    fs::create_dir_all(&folder).unwrap();
    let graph = |name: &str, bytes: &[u8]| {
        fs::write(folder.join(format!("{name}.onnx")), bytes).unwrap();
        format!(
            r#""{name}":{{"sha256":"{}","size":{},"placement":"cpu","obtain":{{"download":{{"url":"u"}}}}}}"#,
            Sha256::of(bytes),
            bytes.len()
        )
    };
    let segment: Vec<String> = GRAPHS
        .iter()
        .enumerate()
        .map(|(i, g)| graph(g, &onnx(10 + i as u8)))
        .collect();
    let edge = graph("matte", &onnx(20));
    fs::write(folder.join("c.npy"), zeros()).unwrap();
    let licence = r#"{"spdx":"MIT","url":"u","class":"ok"}"#;
    let text = format!(
        r#"{{"lock":1,"runtime":{{"version":"1","licence":{licence},"package":{{"url":"p","sha256":"{zero}","sha512":"x","size":1}},
          "libraries":{{"{here}":{{"path":"l","sha256":"{zero}","size":1}}}}}},
          "models":{{"segment.video":{{"does":"d","licence":{licence},"graphs":{{{segment}}},
            "constants":{{"no-memory-embedding":{{"sha256":"{c}","size":{n},"shape":[256],"obtain":{{"download":{{"url":"u"}}}}}}}}}},
            "matte.edges":{{"does":"d","licence":{licence},"graphs":{{{edge}}}}}}},"refused":{{}}}}"#,
        zero = Sha256::of(b""),
        here = platform(),
        segment = segment.join(","),
        c = Sha256::of(&zeros()),
        n = zeros().len(),
    );
    let lock = ModelLock::read(&text).unwrap();
    let store = ModelStore::new(&root.join("cache"));
    store.import(&lock, &folder).unwrap();
    (lock, store)
}

/// A backend handing out the segmenter's fakes by graph name, and an edge
/// model that sees everything as the object.
struct Fakes {
    graphs: Mutex<Vec<Option<Box<dyn Graph>>>>,
    opened: Mutex<Vec<String>>,
}

impl Backend for Fakes {
    fn open(&self, graph: &OpenGraph) -> Result<Box<dyn Graph>> {
        self.opened.lock().unwrap().push(graph.graph.clone());
        if graph.graph == "matte" {
            return Ok(Box::new(Fake {
                inputs: &["input_image"],
                f: Box::new(|_| {
                    Outputs(vec![(
                        "output_image".into(),
                        Tensor::f32([1, 1, 1024, 1024], vec![10.0; 1 << 20]),
                    )])
                }),
            }));
        }
        let i = GRAPHS.iter().position(|g| *g == graph.graph).unwrap();
        Ok(self.graphs.lock().unwrap()[i]
            .take()
            .expect("each graph opened once per segmenter"))
    }
    fn runtime(&self) -> Option<nakkash_baked::Runtime> {
        None
    }
}

fn backend() -> Fakes {
    Fakes {
        graphs: Mutex::new(
            fakes(Arc::new(Mutex::new(Seen::default())))
                .into_iter()
                .map(Some)
                .collect(),
        ),
        opened: Mutex::new(Vec::new()),
    }
}

#[test]
fn bakes_a_matte_and_keeps_its_segmentation_for_the_next_bake() {
    let root = std::env::temp_dir().join(format!("nakkash-bake-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (lock, store) = setup(&root);
    let baked = Store::new(root.join("baked"));
    let cutout = Cutout {
        prompts: vec![Prompt {
            frame: 1,
            area: Some([1.0, 1.0, 5.0, 4.0]),
            include: vec![],
            exclude: vec![],
        }],
        edges: Edges::Refine,
        band: 12.0,
        agree: 0.9,
        smooth: None,
        request: json!({"edges": "refine"}),
    };
    let clip = Sha256::of(b"clip");
    let first = backend();
    let done = Baker::new(&first, &store, &lock, &baked, 2, |_| {})
        .cutout(&mut Greys(4), &cutout, &clip)
        .unwrap();
    assert_eq!((done.frames, done.segmentation_kept), (4, false));
    let matte = nakkash_baked::Manifest::<nakkash_baked::Matte>::open(
        &baked,
        &done.digest,
        &cutout.request,
        &clip,
    )
    .unwrap();
    assert_eq!((matte.frames(), matte.size), (4, [8, 6]));
    assert_eq!(matte.made.threads, 2);
    assert!(
        matte.made.graphs.iter().any(|g| g.model == "matte.edges"),
        "{:?}",
        matte.made.graphs
    );
    assert_eq!(matte.made.graphs.len(), 6);
    // Again, with the hard mask only: the segmentation is kept, no segmenter
    // graph opens, and the matte is another asset.
    let second = backend();
    let hard = Cutout {
        edges: Edges::Hard,
        request: json!({"edges": "hard"}),
        ..cutout
    };
    let again = Baker::new(&second, &store, &lock, &baked, 2, |_| {})
        .cutout(&mut Greys(4), &hard, &clip)
        .unwrap();
    assert!(again.segmentation_kept);
    assert!(second.opened.lock().unwrap().is_empty());
    assert_ne!(again.digest, done.digest);
    // Another thread count is another key: segmented again.
    let third = backend();
    let other = Baker::new(&third, &store, &lock, &baked, 3, |_| {})
        .cutout(&mut Greys(4), &hard, &clip)
        .unwrap();
    assert!(!other.segmentation_kept);
    fs::remove_dir_all(&root).unwrap();
}
