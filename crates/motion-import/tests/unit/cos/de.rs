//! Unit tests for `src/cos/de.rs`: typed records read out of a tree.

use serde::Deserialize;

use super::*;
use crate::cos::parse;

/// A record naming its keys, with every kind of field.
#[derive(Debug, Deserialize, PartialEq)]
struct Record {
    #[serde(rename = "0")]
    count: usize,
    #[serde(rename = "1")]
    size: f64,
    #[serde(rename = "2")]
    on: bool,
    #[serde(rename = "3")]
    name: String,
    #[serde(rename = "4")]
    color: [f64; 4],
    #[serde(rename = "5")]
    nothing: Option<i64>,
    #[serde(rename = "6")]
    something: Option<i64>,
    #[serde(rename = "7", default)]
    absent: Option<i64>,
}

/// Nodes told apart by their type name at `/99`.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(tag = "99")]
enum Node {
    #[serde(rename = "A")]
    A {
        #[serde(rename = "0")]
        value: f64,
    },
    #[serde(other)]
    Other,
}

#[test]
fn reads_a_record_by_its_keys() {
    let cos =
        parse(b" /9 /ignored /0 3 /1 12 /2 true /3 (Inter-Bold) /4 [ 1.0 .5 0.0 1 ] /5 /nil /6 -2")
            .unwrap();
    assert_eq!(
        read::<Record>(&cos).unwrap(),
        Record {
            count: 3,
            size: 12.0,
            on: true,
            name: "Inter-Bold".into(),
            color: [1.0, 0.5, 0.0, 1.0],
            nothing: None,
            something: Some(-2),
            absent: None,
        }
    );
}

#[test]
fn reads_names_as_strings_and_nodes_by_their_type() {
    let cos = parse(b" /0 [ << /99 /A /0 2.5 >> << /99 /B /0 1 >> ] /1 /Name").unwrap();
    #[derive(Debug, Deserialize)]
    struct Nodes {
        #[serde(rename = "0")]
        nodes: Vec<Node>,
        #[serde(rename = "1")]
        name: String,
    }
    let nodes: Nodes = read(&cos).unwrap();
    assert_eq!(nodes.nodes, [Node::A { value: 2.5 }, Node::Other]);
    assert_eq!(nodes.name, "Name");
}

#[test]
fn errors_name_the_key_path() {
    #[derive(Debug, Deserialize)]
    struct Outer {
        #[serde(rename = "1")]
        _items: Vec<Inner>,
    }
    #[derive(Debug, Deserialize)]
    struct Inner {
        #[serde(rename = "0")]
        _size: f64,
    }
    let cos = parse(b" /1 [ << /0 1.0 >> << /0 (x) >> ]").unwrap();
    let error = read::<Outer>(&cos).unwrap_err();
    assert_eq!(error.kind, Kind::Invalid);
    assert_eq!(error.path.as_deref(), Some("/1/1/0"));
    assert!(
        error.message.starts_with("text document: invalid type"),
        "{error}"
    );
    let cos = parse(b" /1 [ << >> ]").unwrap();
    let error = read::<Outer>(&cos).unwrap_err();
    assert_eq!(error.path.as_deref(), Some("/1/0"));
    assert!(error.message.contains("missing field `0`"), "{error}");
}

#[test]
fn an_error_at_the_top_has_the_root_path() {
    let error = read::<Vec<f64>>(&Cos::Integer(1)).unwrap_err();
    assert_eq!(error.path.as_deref(), Some("/"));
}

#[test]
fn arrays_of_the_wrong_length_are_refused() {
    let cos = Cos::Array(vec![Cos::Real(1.0); 3]);
    assert!(read::<[f64; 2]>(&cos).is_err());
}

#[test]
fn mismatch_prints_its_message() {
    let mismatch = <Mismatch as de::Error>::custom("bad");
    assert_eq!(mismatch.to_string(), "bad");
}
