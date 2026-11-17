//! Hostile and random programs: every run ends, within its limits, with a
//! page and notes, never a panic or a hang.

use std::time::{Duration, Instant};

use motion_postscript::{Limits, NoteKind, Page, read_with};
use proptest::prelude::*;
use rstest::rstest;

/// Limits small enough that a test reaches them quickly.
fn small() -> Limits {
    Limits {
        operations: 200_000,
        memory: 1 << 20,
        depth: 256,
        nesting: 8,
        operands: 1024,
        dictionaries: 64,
        gsaves: 64,
        path_elements: 4096,
        items: 1024,
        stored_elements: 16_384,
        image_pixels: 1 << 16,
    }
}

/// Runs `body` as an EPS program within the small limits.
fn run(body: &[u8]) -> Page {
    let mut program = b"%!PS-3.0 EPSF-3.0\n%%BoundingBox: 0 0 10 10\n".to_vec();
    program.extend_from_slice(body);
    let start = Instant::now();
    let page = read_with(&program, &small()).unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "took {:?}",
        start.elapsed()
    );
    page
}

/// The kind of the note that ended the run, if one did.
fn ending(page: &Page) -> Option<NoteKind> {
    page.notes
        .iter()
        .find(|n| matches!(n.kind, NoteKind::Error | NoteKind::Limit))
        .map(|n| n.kind)
}

#[rstest]
#[case::endless_loop("{} loop")]
#[case::endless_recursion("/a { a } def a")]
#[case::growing_recursion("/a { a 1 } def a")]
#[case::growing_stack("{ 1 } loop")]
#[case::endless_for("0 1 1e30 { pop } for")]
#[case::huge_repeat("2147483647 { } repeat")]
#[case::huge_string("2147483647 string")]
#[case::many_strings("{ 60000 string } loop")]
#[case::many_arrays("{ 60000 array } loop")]
#[case::growing_dict("/d 1 dict def 0 { dup d exch 1 put 1 add } loop")]
#[case::endless_path("0 0 moveto { 1 1 rlineto } loop")]
#[case::endless_fill("{ 0 0 1 1 rectfill } loop")]
#[case::endless_clip("{ 0 0 1 1 rectclip } loop")]
#[case::deep_gsave("{ gsave } loop")]
#[case::deep_begin("{ 1 dict begin } loop")]
#[case::self_named("/a /a cvx def a")]
#[case::nested_stopped("/s { { s } stopped } def s")]
#[case::tint_recursion("[/Separation /X /DeviceGray { 1 setcolor 0 }] setcolorspace 1 setcolor")]
#[case::proc_source_forever("{ (A) } /ASCIIHexDecode filter { dup read pop pop } loop")]
#[case::huge_image("10000 10000 8 [1 0 0 1 0 0] { (x) } image")]
#[case::bind_cycle("/p [ null ] cvx def p 0 p put p bind")]
#[case::exit_outside("exit")]
#[case::unmatched_brace("}")]
#[case::unterminated_string("(abc")]
#[case::unterminated_proc("{ 1 2")]
#[case::stop_at_top("stop")]
#[case::restore_unknown("save restore restore")]
fn hostile_programs_end(#[case] body: &str) {
    let page = run(body.as_bytes());
    assert!(ending(&page).is_some(), "{body}: {:?}", page.notes);
}

#[test]
fn the_operation_budget_is_counted() {
    let page = run(b"{} loop");
    assert!(page.operations > small().operations);
    assert_eq!(ending(&page), Some(NoteKind::Limit));
}

/// Pieces of programs, mixed at random.
fn token() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "1",
        "-2",
        "3.5",
        "0",
        "16#7F",
        "/a",
        "/b",
        "a",
        "b",
        "{",
        "}",
        "[",
        "]",
        "<<",
        ">>",
        "(s)",
        "<41>",
        "<~87~>",
        "dup",
        "exch",
        "pop",
        "add",
        "mul",
        "div",
        "idiv",
        "roll",
        "index",
        "copy",
        "def",
        "load",
        "get",
        "put",
        "exec",
        "if",
        "ifelse",
        "for",
        "repeat",
        "loop",
        "forall",
        "exit",
        "stop",
        "stopped",
        "array",
        "string",
        "dict",
        "begin",
        "end",
        "moveto",
        "lineto",
        "curveto",
        "arc",
        "closepath",
        "fill",
        "eofill",
        "stroke",
        "clip",
        "rectfill",
        "rectclip",
        "gsave",
        "grestore",
        "save",
        "restore",
        "translate",
        "scale",
        "rotate",
        "concat",
        "matrix",
        "setgray",
        "setrgbcolor",
        "setcmykcolor",
        "setcolorspace",
        "setcolor",
        "image",
        "colorimage",
        "imagemask",
        "currentfile",
        "readstring",
        "readline",
        "filter",
        "token",
        "cvx",
        "cvn",
        "cvs",
        "bind",
        "findresource",
        "defineresource",
        "findfont",
        "show",
        "pathforall",
        "strokepath",
        "flattenpath",
        "showpage",
        "quit",
        "%%EOF",
        "%%BeginBinary: 3",
        "\u{1}\u{9F}",
        "%comment",
        "\n",
    ])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn random_tokens_never_panic(tokens in prop::collection::vec(token(), 0..80)) {
        let body = tokens.join(" ");
        let page = run(body.as_bytes());
        prop_assert!(page.operations <= small().operations + 1);
    }

    #[test]
    fn random_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..400)) {
        run(&bytes);
    }
}
