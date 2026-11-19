use std::io::Cursor;

use proptest::prelude::*;

use super::*;

/// The bytes of one protobuf field.
fn field(number: u64, value: Field) -> Vec<u8> {
    let mut out = Vec::new();
    let (wire, payload) = match value {
        Field::Varint(v) => (0, varint(v)),
        Field::Bytes(b) => {
            let mut p = varint(b.len() as u64);
            p.extend(b);
            (2, p)
        }
    };
    out.extend(varint(number << 3 | wire));
    out.extend(payload);
    out
}

enum Field {
    Varint(u64),
    Bytes(Vec<u8>),
}

fn varint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let b = (v & 0x7f) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return out;
        }
        out.push(b | 0x80);
    }
}

fn message(fields: &[Vec<u8>]) -> Vec<u8> {
    fields.concat()
}

fn opset(domain: &str, version: u64) -> Vec<u8> {
    field(
        8,
        Field::Bytes(message(&[
            field(1, Field::Bytes(domain.into())),
            field(2, Field::Varint(version)),
        ])),
    )
}

fn node(op: &str, domain: Option<&str>, attributes: &[Vec<u8>]) -> Vec<u8> {
    let mut fields = vec![field(4, Field::Bytes(op.into()))];
    if let Some(domain) = domain {
        fields.push(field(7, Field::Bytes(domain.into())));
    }
    fields.extend(attributes.iter().map(|a| field(5, Field::Bytes(a.clone()))));
    field(1, Field::Bytes(message(&fields)))
}

fn initializer(location: u64, weights: usize) -> Vec<u8> {
    field(
        5,
        Field::Bytes(message(&[
            field(8, Field::Bytes("w".into())),
            field(9, Field::Bytes(vec![7; weights])),
            field(14, Field::Varint(location)),
        ])),
    )
}

fn model(opsets: &[Vec<u8>], graph: &[Vec<u8>]) -> Vec<u8> {
    let mut fields = vec![field(1, Field::Varint(8))];
    fields.extend(opsets.iter().cloned());
    fields.push(field(7, Field::Bytes(message(graph))));
    message(&fields)
}

fn checked(bytes: &[u8]) -> Result<Summary> {
    check(Cursor::new(bytes), bytes.len() as u64)
}

#[test]
fn passes_a_plain_graph_and_counts_it() {
    let bytes = model(
        &[opset("", 17)],
        &[
            node("Add", None, &[]),
            node("Relu", Some("ai.onnx"), &[]),
            initializer(0, 100_000),
        ],
    );
    let summary = checked(&bytes).unwrap();
    assert_eq!(
        summary,
        Summary {
            ir_version: 8,
            opset: 17,
            nodes: 2,
            initializers: 1
        }
    );
}

#[test]
fn reads_subgraphs_in_attributes() {
    let branch = field(
        6,
        Field::Bytes(message(&[node("Custom", Some("com.example"), &[])])),
    );
    let bytes = model(&[opset("", 17)], &[node("If", None, &[branch])]);
    let error = checked(&bytes).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert!(error.message.contains("com.example"), "{}", error.message);
}

#[rstest::rstest]
#[case::custom_operator_set(model(&[opset("", 17), opset("com.example", 1)], &[]), "operator set")]
#[case::custom_node(model(&[opset("", 17)], &[node("Op", Some("com.example"), &[])]), "node of domain")]
#[case::external_data(model(&[opset("", 17)], &[initializer(1, 4)]), "outside the file")]
#[case::local_function(message(&[opset("", 17), field(25, Field::Bytes(vec![]))]), "function")]
#[case::no_standard_opset(model(&[], &[]), "no standard operator set")]
fn refuses_what_is_not_plain_onnx(#[case] bytes: Vec<u8>, #[case] why: &str) {
    let error = checked(&bytes).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert!(error.message.contains(why), "{}", error.message);
}

#[test]
fn refuses_external_data_entries() {
    let tensor = field(
        5,
        Field::Bytes(field(
            13,
            Field::Bytes(message(&[field(1, Field::Bytes("location".into()))])),
        )),
    );
    let error = checked(&model(&[opset("", 17)], &[tensor])).unwrap_err();
    assert!(
        error.message.contains("outside the file"),
        "{}",
        error.message
    );
}

#[rstest::rstest]
#[case::truncated(model(&[opset("", 17)], &[initializer(0, 10)])[..20].to_vec())]
#[case::length_past_end(vec![0x3a, 0xff, 0x01])]
#[case::group(vec![0x0b])]
#[case::field_zero(vec![0x00])]
#[case::long_varint(vec![0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01])]
fn refuses_what_is_not_protobuf(#[case] bytes: Vec<u8>) {
    assert_eq!(checked(&bytes).unwrap_err().kind, Kind::Invalid);
}

#[test]
fn refuses_a_file_over_the_limit() {
    let error = check(Cursor::new(Vec::<u8>::new()), MAX_BYTES + 1).unwrap_err();
    assert!(error.message.contains("over protobuf"), "{}", error.message);
}

#[test]
fn refuses_graphs_nested_too_deep() {
    let mut graph = message(&[node("Add", None, &[])]);
    for _ in 0..=MAX_DEPTH {
        graph = message(&[node("If", None, &[field(6, Field::Bytes(graph))])]);
    }
    let error = checked(&model(&[opset("", 17)], &[graph])).unwrap_err();
    assert!(error.message.contains("too deep"), "{}", error.message);
}

#[test]
fn names_the_file_it_refuses() {
    let path = std::env::temp_dir().join(format!("nakkash-onnx-{}.onnx", std::process::id()));
    std::fs::write(&path, model(&[opset("x", 1)], &[])).unwrap();
    let error = check_file(&path).unwrap_err();
    std::fs::remove_file(&path).unwrap();
    assert!(
        error.message.contains(&path.display().to_string()),
        "{}",
        error.message
    );
    assert_eq!(check_file(&path).unwrap_err().kind, Kind::Io);
}

proptest! {
    #[test]
    fn never_panics_on_any_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = checked(&bytes);
    }
}

/// Every `.onnx` file under `NAKKASH_ML_MODELS` passes the check, as the
/// models of the lock must. Run where the models are.
#[test]
#[ignore = "needs the models: set NAKKASH_ML_MODELS"]
fn the_real_graphs_pass() {
    let root =
        std::path::PathBuf::from(std::env::var("NAKKASH_ML_MODELS").expect("NAKKASH_ML_MODELS"));
    let mut found = 0;
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "onnx") {
                let start = std::time::Instant::now();
                let summary = check_file(&path).unwrap();
                println!("{} {summary:?} {:?}", path.display(), start.elapsed());
                found += 1;
            }
        }
    }
    assert!(found > 0);
}
