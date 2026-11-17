//! Unit tests for `src/cos.rs`: the object-syntax grammar and the tree.

use proptest::prelude::*;

use super::*;
use crate::test_support::{text_body, text_document};

/// A dictionary of `entries`.
fn dict(entries: &[(&str, Cos)]) -> Cos {
    Cos::Dict(
        entries
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect(),
    )
}

/// A plain-byte string.
fn bytes(text: &str) -> Cos {
    Cos::String(CosString {
        bytes: text.as_bytes().to_vec(),
        encoding: Encoding::Bytes,
    })
}

#[test]
fn reads_every_kind_of_value_in_order() {
    let body = b" /98 << /0 9 >> /1 [ 1 -2 2.0 .5 -.03571 true false /nil (a\\)b) << >> [ ] ]";
    assert_eq!(
        parse(body).unwrap(),
        dict(&[
            ("98", dict(&[("0", Cos::Integer(9))])),
            (
                "1",
                Cos::Array(vec![
                    Cos::Integer(1),
                    Cos::Integer(-2),
                    Cos::Real(2.0),
                    Cos::Real(0.5),
                    Cos::Real(-0.03571),
                    Cos::Bool(true),
                    Cos::Bool(false),
                    Cos::Name("nil".into()),
                    bytes("a)b"),
                    Cos::Dict(vec![]),
                    Cos::Array(vec![]),
                ])
            ),
        ])
    );
}

#[test]
fn keeps_the_order_keys_are_written_in() {
    let Cos::Dict(entries) = parse(b" /4 1 /0 2 /5 3").unwrap() else {
        panic!("a dictionary")
    };
    let keys: Vec<&str> = entries.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["4", "0", "5"]);
}

#[test]
fn integers_and_reals_stay_apart() {
    let cos = parse(b" /0 9 /1 9.0 /2 0.0").unwrap();
    assert_eq!(
        cos,
        dict(&[
            ("0", Cos::Integer(9)),
            ("1", Cos::Real(9.0)),
            ("2", Cos::Real(0.0))
        ])
    );
}

#[test]
fn strings_keep_their_encoding_and_raw_bytes() {
    // U+0D0A has a CR and a LF byte, written raw; `\(` escapes a paren byte.
    let body = [
        b" /0 (\xFE\xFF\x00H\x0D\x0A\x00\\(".as_slice(),
        b") /1 (00110)",
    ]
    .concat();
    let cos = parse(&body).unwrap();
    let Some(Cos::String(utf16)) = cos.get("0") else {
        panic!("a string")
    };
    assert_eq!(utf16.encoding, Encoding::Utf16);
    assert_eq!(utf16.bytes, [0x00, b'H', 0x0D, 0x0A, 0x00, b'(']);
    assert_eq!(utf16.text(), "H\u{0D0A}(");
    let Some(Cos::String(plain)) = cos.get("1") else {
        panic!("a string")
    };
    assert_eq!(
        (plain.encoding, plain.text()),
        (Encoding::Bytes, "00110".into())
    );
}

#[test]
fn text_replaces_broken_utf16_and_odd_bytes() {
    let lone = CosString {
        bytes: vec![0xD8, 0x00, 0x00],
        encoding: Encoding::Utf16,
    };
    assert_eq!(lone.text(), "\u{FFFD}");
}

#[test]
fn white_space_between_tokens_may_vary() {
    let cos = parse(b"\n/0\t<</1\r\n[1 2]>>  ").unwrap();
    assert_eq!(
        cos.get("0/1"),
        Some(&Cos::Array(vec![Cos::Integer(1), Cos::Integer(2)]))
    );
}

#[test]
fn an_empty_body_is_an_empty_dictionary() {
    assert_eq!(parse(b"").unwrap(), Cos::Dict(vec![]));
    assert_eq!(parse(b"   ").unwrap(), Cos::Dict(vec![]));
}

#[test]
fn names_what_failed_and_where() {
    let cases: [(&[u8], &str); 8] = [
        (
            b" /0 << /1 2",
            "a dictionary is not closed with `>>` at byte 11",
        ),
        (b" /0 [ 1 2", "an array is not closed with `]` at byte 9"),
        (b" /0 (abc", "a string is not closed with `)` at byte 8"),
        (b" /0 >>", "expected a value at byte 4"),
        (
            b" /0 1 junk",
            "unexpected bytes after the last entry at byte 6",
        ),
        (
            b" /0 99999999999999999999",
            "a number is out of range at byte 4",
        ),
        (
            b" /0 << 1 >>",
            "a dictionary is not closed with `>>` at byte 7",
        ),
        (b" /0 -", "expected a value at byte 4"),
    ];
    for (body, message) in cases {
        let error = parse(body).unwrap_err();
        assert_eq!(error.kind, Kind::Syntax, "{message}");
        assert_eq!(error.message, format!("text document: {message}"));
        assert!(error.expected.is_some());
    }
}

#[test]
fn refuses_values_nested_too_deep() {
    let deep = [
        b" /0 ".as_slice(),
        &[b'['; MAX_DEPTH + 1],
        &[b']'; MAX_DEPTH + 1],
    ]
    .concat();
    let error = parse(&deep).unwrap_err();
    assert!(error.message.contains("values nest too deep"), "{error}");
    let fits = [b" /0 ".as_slice(), &[b'['; MAX_DEPTH], &[b']'; MAX_DEPTH]].concat();
    assert!(parse(&fits).is_ok());
}

#[test]
fn get_follows_keys_and_indices() {
    let mut cos = parse(b" /1 << /1 [ << /0 (x) >> << /0 7 >> ] >>").unwrap();
    assert_eq!(cos.get("1/1/1/0"), Some(&Cos::Integer(7)));
    assert_eq!(cos.get("/1/1/1/0"), Some(&Cos::Integer(7)));
    assert_eq!(cos.get("1/1/2"), None);
    assert_eq!(cos.get("1/1/x"), None);
    assert_eq!(cos.get("1/1/0/0/9"), None);
    assert_eq!(cos.get(""), Some(&cos.clone()));
    *cos.get_mut("1/1/1/0").unwrap() = Cos::Bool(true);
    assert_eq!(cos.get("1/1/1/0"), Some(&Cos::Bool(true)));
    assert!(cos.get_mut("1/1/1/1").is_none());
    assert!(cos.get_mut("1/1/9").is_none());
    assert!(cos.get_mut("1/1/1/0/0").is_none());
}

#[test]
fn items_mut_reaches_array_items_only() {
    let mut array = Cos::Array(vec![Cos::Integer(1)]);
    array.items_mut()[0] = Cos::Integer(2);
    assert_eq!(array, Cos::Array(vec![Cos::Integer(2)]));
    assert!(Cos::Integer(1).items_mut().is_empty());
}

#[test]
fn overlay_keeps_the_base_order_and_the_top_values() {
    let base = dict(&[("0", Cos::Integer(1)), ("1", Cos::Integer(2))]);
    let top = dict(&[("2", Cos::Integer(9)), ("1", Cos::Integer(8))]);
    assert_eq!(
        base.overlay(&top),
        dict(&[
            ("0", Cos::Integer(1)),
            ("1", Cos::Integer(8)),
            ("2", Cos::Integer(9))
        ])
    );
    assert_eq!(base.overlay(&Cos::Integer(3)), Cos::Integer(3));
    assert_eq!(Cos::Integer(3).overlay(&top), top);
}

/// A body like the ones projects store, for the never-panic tests.
fn realistic_body() -> Vec<u8> {
    let document = text_document(
        "Hi\r",
        &[(3, "")],
        &[(3, "/1 12.0")],
        "<< /99 /PC /6 [ << /99 /L /6 [ << /99 /S /15 << /0 3 >> >> ] >> ] >>",
    );
    text_body(&["Inter-Bold"], &[], &[document]).into_bytes()
}

proptest! {
    #[test]
    fn never_panics_on_any_bytes(body in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..512)) {
        let _ = parse(&body);
    }

    #[test]
    fn never_panics_on_a_truncated_body(cut in 0usize..2048) {
        let body = realistic_body();
        let _ = parse(&body[..cut.min(body.len())]);
    }

    #[test]
    fn never_panics_on_a_corrupted_body(at in 0usize..2048, byte in proptest::prelude::any::<u8>()) {
        let mut body = realistic_body();
        let at = at % body.len();
        body[at] = byte;
        let _ = parse(&body);
    }
}
