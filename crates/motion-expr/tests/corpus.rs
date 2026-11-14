//! The local corpus: every distinct expression of the local projects
//! compiles. Ignored: the sources stay local. Run it with
//! `MOTION_EXPR_SOURCES=unique.json cargo nextest run -p motion-expr --run-ignored only`,
//! where the file is a JSON list of `{"expr": source, "engine": name}`.

use std::rc::Rc;

use motion_expr::{
    Dialect, Engine, Host, Limits, Missing, NodeId, NodeKind, STACK_SIZE, Select, Val,
};

/// A host with nothing in it: compiling needs none.
struct Empty;

impl Host for Empty {
    fn project(&self) -> NodeId {
        0
    }
    fn kind(&self, _: NodeId) -> NodeKind {
        NodeKind::Project
    }
    fn up(&self, _: NodeId) -> Option<NodeId> {
        None
    }
    fn child(&self, _: NodeId, _: Select<'_>) -> Option<NodeId> {
        None
    }
    fn link(&self, _: NodeId, _: &str) -> Option<NodeId> {
        None
    }
    fn attribute(&self, _: NodeId, _: &str) -> Option<Val> {
        None
    }
    fn value_at(&self, _: NodeId, _: f64, _: bool) -> Result<Val, Missing> {
        Ok(Val::Number(0.0))
    }
    fn key_count(&self, _: NodeId) -> usize {
        0
    }
    fn key(&self, _: NodeId, _: usize) -> Option<(f64, Val)> {
        None
    }
}

#[test]
#[ignore = "needs the local corpus"]
fn every_distinct_corpus_expression_compiles() {
    std::thread::Builder::new()
        .stack_size(STACK_SIZE)
        .spawn(compile_all)
        .unwrap()
        .join()
        .unwrap();
}

fn compile_all() {
    let path = std::env::var("MOTION_EXPR_SOURCES").expect("MOTION_EXPR_SOURCES names the sources");
    let text = std::fs::read_to_string(path).unwrap();
    let mut failed = Vec::new();
    let mut repaired = 0;
    let mut encoded = 0;
    let mut total = 0;
    let mut engines = [Dialect::Modern, Dialect::Legacy].map(|d| {
        (
            d,
            Engine::new(Rc::new(Empty), d, Limits::default()).unwrap(),
        )
    });
    for entry in text.split("{\"expr\": ").skip(1) {
        let (source, rest) = entry.split_once(", \"engine\": ").unwrap();
        let source: String = serde_json_str(source);
        let dialect = if rest.starts_with("\"javascript") {
            Dialect::Modern
        } else {
            Dialect::Legacy
        };
        let engine = &mut engines.iter_mut().find(|(d, _)| *d == dialect).unwrap().1;
        total += 1;
        if std::env::var_os("MOTION_EXPR_TRACE").is_some() {
            eprintln!(
                "#{total} {}",
                source
                    .chars()
                    .take(70)
                    .collect::<String>()
                    .replace('\n', " ")
            );
        }
        match engine.compile(&source, dialect) {
            Ok(compiled) => repaired += usize::from(!compiled.repairs.is_empty()),
            Err(e) if e.message.contains("encoded") => encoded += 1,
            Err(e) => failed.push(format!(
                "{}: {}",
                e.message,
                source.chars().take(80).collect::<String>()
            )),
        }
    }
    eprintln!(
        "{total} sources: {} compile ({repaired} after legacy repairs), {encoded} are encoded scripts, {} fail",
        total - failed.len() - encoded,
        failed.len()
    );
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// A JSON string literal's text (the corpus file is plain JSON strings).
fn serde_json_str(literal: &str) -> String {
    let inner = literal.trim().trim_start_matches('"').trim_end_matches('"');
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                out.push(
                    char::from_u32(u32::from_str_radix(&hex, 16).unwrap_or(0xFFFD))
                        .unwrap_or('\u{FFFD}'),
                );
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}
