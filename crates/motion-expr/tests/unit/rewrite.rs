//! Unit tests for `src/rewrite.rs`.

use pretty_assertions::assert_eq;

use super::*;

fn modern(source: &str) -> String {
    rewrite(source, Dialect::Modern)
        .unwrap()
        .source
        .replace(['\n', ' '], "")
}

#[test]
fn arithmetic_becomes_helper_calls() {
    assert_eq!(modern("value + [10, 0]"), "$add(value,[10,0]);");
    assert_eq!(
        modern("a - b * c / d % e"),
        "$sub(a,$mod($div($mul(b,c),d),e));"
    );
    assert_eq!(modern("-x"), "$neg(x);");
    assert_eq!(modern("2 ** 3"), "2**3;");
}

#[test]
fn compound_assignments_too() {
    assert_eq!(modern("x += [1, 2]"), "x=$add(x,[1,2]);");
    assert_eq!(modern("o.p *= 2"), "o.p=$mul(o.p,2);");
}

#[test]
fn comparisons_and_logic_stay() {
    let out = rewrite("if (a == 1 && b < 2) c; else d", Dialect::Modern)
        .unwrap()
        .source;
    assert!(!out.contains('$'), "{out}");
}

#[test]
fn legacy_else_on_one_line_is_repaired() {
    let source = "x = effect(\"S\")(1);\nif (x == 1) 100 else 0";
    let out = rewrite(source, Dialect::Legacy).unwrap();
    assert_eq!(out.repairs, ["a semicolon before `else` on line 2"]);
    assert!(rewrite(source, Dialect::Modern).is_err());
}

#[test]
fn legacy_this_call_is_this_layer() {
    let out = rewrite("this(\"ADBE Transform Group\")", Dialect::Legacy).unwrap();
    assert!(
        out.source.contains("thisLayer(\"ADBE Transform Group\")"),
        "{}",
        out.source
    );
}

#[test]
fn a_source_that_does_not_parse_is_a_syntax_error() {
    let error = rewrite("value +", Dialect::Legacy).unwrap_err();
    assert_eq!(error.kind, Kind::Syntax);
}
