//! Unit tests for `src/interp.rs`: the language's semantics, observed on
//! the operand stack a program leaves.

use kurbo::Rect;
use pretty_assertions::assert_eq;
use rstest::rstest;

use super::*;
use crate::ops::compose::format_real;

/// An object as the tests write it: close to the language's own syntax.
fn show(i: &Interp, object: Object) -> String {
    match object.value {
        Value::Null => "null".into(),
        Value::Int(int) => int.to_string(),
        Value::Real(real) => format_real(real),
        Value::Bool(b) => b.to_string(),
        Value::Name(name) if object.exec => i.vm.name_text(name),
        Value::Name(name) => format!("/{}", i.vm.name_text(name)),
        Value::Mark => "mark".into(),
        Value::String(s) => format!("({})", String::from_utf8_lossy(i.vm.str_bytes(s))),
        Value::Array(arr) | Value::Packed(arr) => {
            let inner: Vec<String> = i.vm.arr(arr).iter().map(|o| show(i, *o)).collect();
            if object.exec {
                format!("{{{}}}", inner.join(" "))
            } else {
                format!("[{}]", inner.join(" "))
            }
        }
        Value::Dict(_) => "-dict-".into(),
        Value::Operator(_) => format!("--{}--", i.describe(object)),
        _ => format!("-{}-", object.type_name()),
    }
}

/// Runs `program` and returns the interpreter and how the run ended.
fn run_program(program: &str) -> (Interp, R) {
    let builder = Builder::new(Rect::new(0.0, 0.0, 100.0, 100.0), 1 << 20);
    let mut interp = Interp::new(program.as_bytes(), Limits::default(), builder).unwrap();
    let result = interp.run();
    (interp, result)
}

/// The operand stack `program` leaves, bottom first.
fn stack(program: &str) -> String {
    let (interp, result) = run_program(program);
    assert!(
        matches!(result, Ok(()) | Err(Fault::Halt(Halt::Quit))),
        "{program}: {result:?}"
    );
    let shown: Vec<String> = interp.ostack.iter().map(|o| show(&interp, *o)).collect();
    shown.join(" ")
}

#[rstest]
// The operand stack.
#[case::pop("1 2 pop", "1")]
#[case::exch("1 2 exch", "2 1")]
#[case::dup("1 dup", "1 1")]
#[case::copy("1 2 3 2 copy", "1 2 3 2 3")]
#[case::index("1 2 3 2 index", "1 2 3 1")]
#[case::roll("1 2 3 3 1 roll", "3 1 2")]
#[case::roll_back("1 2 3 3 -1 roll", "2 3 1")]
#[case::clear("1 2 clear 3", "3")]
#[case::count("1 2 count", "1 2 2")]
#[case::counttomark("1 mark 2 3 counttomark", "1 mark 2 3 2")]
#[case::cleartomark("1 mark 2 3 cleartomark", "1")]
// Arithmetic.
#[case::add("1 2 add 1.5 1 add", "3 2.5")]
#[case::overflow("2147483647 1 add", "2147483600.0")]
#[case::sub_mul("7 2 sub 3 4 mul", "5 12")]
#[case::div("1 2 div 4 2 div", "0.5 2.0")]
#[case::idiv_mod("7 2 idiv -7 2 idiv 7 3 mod -7 3 mod", "3 -3 1 -1")]
#[case::rounding(
    "2.5 round -2.5 round 2.7 truncate -2.7 floor 2.1 ceiling",
    "3.0 -2.0 2.0 -3.0 3.0"
)]
#[case::neg_abs("3 neg -4 abs -4.5 abs", "-3 4 4.5")]
#[case::sqrt_exp("16 sqrt 2 10 exp", "4.0 1024.0")]
#[case::atan("1 1 atan 0 -1 atan -1 0 atan", "45.0 180.0 270.0")]
#[case::trig("90 sin 0 cos", "1.0 1.0")]
#[case::bits("5 3 and 5 3 or 5 3 xor 5 not", "1 7 6 -6")]
#[case::bitshift("1 4 bitshift -1 -28 bitshift", "16 15")]
#[case::radix("16#FF 8#17 16#FFFFFFFF", "255 15 -1")]
#[case::reals(".5 -.5 1e2 1.5E-1 5.", "0.5 -0.5 100.0 0.15 5.0")]
// Comparison.
#[case::eq("1 1.0 eq (a) /a eq (a) (b) eq", "true true false")]
#[case::order("1 2 lt (a) (b) lt 2 2 ge 3 2 le", "true true true false")]
#[case::bool_ops("true false and true not", "false false")]
// Conversion.
#[case::cvi_cvr("3.7 cvi (42) cvi 3 cvr (2.5) cvr", "3 42 3.0 2.5")]
#[case::cvn_cvs("(abc) cvn 123 10 string cvs", "/abc (123)")]
#[case::cvrs("255 16 10 string cvrs", "(FF)")]
#[case::types(
    "1 type 1.0 type (a) type /a type {} type",
    "integertype realtype stringtype nametype arraytype"
)]
#[case::xcheck("{} xcheck /a xcheck /a cvx xcheck", "true false true")]
fn evaluates(#[case] program: &str, #[case] expected: &str) {
    assert_eq!(stack(program), expected);
}

#[rstest]
// Arrays.
#[case::array("3 array length", "3")]
#[case::brackets("[1 2 3] 1 get", "2")]
#[case::put("[1 2 3] dup 0 9 put", "[9 2 3]")]
#[case::aload("[1 2] aload", "1 2 [1 2]")]
#[case::astore("1 2 2 array astore", "[1 2]")]
#[case::getinterval_shares("[1 2 3 4] dup 1 2 getinterval 0 9 put", "[1 9 3 4]")]
#[case::putinterval("[1 2 3 4] dup 1 [8 9] putinterval", "[1 8 9 4]")]
#[case::copy_array("[1 2] 3 array copy", "[1 2]")]
#[case::forall_array("0 [1 2 3] { add } forall", "6")]
// Strings.
#[case::string("3 string length", "3")]
#[case::string_get("(abc) 1 get", "98")]
#[case::string_put("(abc) dup 0 65 put", "(Abc)")]
#[case::search("(abcde) (cd) search", "(e) (cd) (ab) true")]
#[case::search_miss("(abc) (x) search", "(abc) false")]
#[case::anchorsearch("(abc) (ab) anchorsearch", "(c) (ab) true")]
#[case::escapes("(a\\nb\\050\\)) length", "5")]
#[case::nested_parens("(a(b)c)", "(a(b)c)")]
#[case::hex("<41 42 4> ", "(AB@)")]
#[case::base85("<~87cURD]i,\"Ebo80~>", "(Hello World!)")]
#[case::token_string("(12 abc) token", "(abc) 12 true")]
#[case::forall_string("0 (ab) { add } forall", "195")]
// Dictionaries.
#[case::def_load("/a 5 def a /a load", "5 5")]
#[case::dict("2 dict begin /x 1 def x end", "1")]
#[case::known("/d 1 dict def d /k 1 put d /k known d /z known", "true false")]
#[case::string_key("/d 1 dict def d (k) 2 put d /k get", "2")]
#[case::real_key("/d 1 dict def d 1.0 7 put d 1 get", "7")]
#[case::where_found(
    "/a 1 def /a where { pop true } { false } ifelse /zz where",
    "true false"
)]
#[case::store("/a 1 def 1 dict begin 2 /a exch store end a", "2")]
#[case::undef("/d 1 dict def d /k 1 put d /k undef d /k known", "false")]
#[case::double_angle("<< /a 1 /b 2 >> dup length exch /b get", "2 2")]
#[case::forall_dict("0 << /a 1 /b 2 >> { exch pop add } forall", "3")]
#[case::forall_order("<< /c 1 /a 2 /b 3 >> { pop } forall", "/c /a /b")]
#[case::currentdict("currentdict userdict eq", "true")]
#[case::countdictstack("countdictstack 1 dict begin countdictstack end", "3 4")]
fn composes(#[case] program: &str, #[case] expected: &str) {
    assert_eq!(stack(program), expected);
}

#[rstest]
#[case::if_true("true { 1 } if", "1")]
#[case::if_false("false { 1 } if", "")]
#[case::ifelse("false { 1 } { 2 } ifelse", "2")]
#[case::for_int("0 1 1 4 { add } for", "10")]
#[case::for_down("3 -1 1 { } for", "3 2 1")]
#[case::for_real("0 0.5 1 { } for", "0.0 0.5 1.0")]
#[case::repeat("0 3 { 1 add } repeat", "3")]
#[case::loop_exit("0 { 1 add dup 5 eq { exit } if } loop", "5")]
#[case::nested_exit("0 3 { 0 { 1 add dup 2 eq { exit } if } loop add } repeat", "6")]
#[case::exec("{ 1 2 add } exec", "3")]
#[case::exec_string("(3 4 mul) cvx exec", "12")]
#[case::procedure_deferred("/p { 1 { 2 } } def p", "1 {2}")]
#[case::stopped_false("{ 1 } stopped", "1 false")]
#[case::stopped_stop("{ 1 stop 2 } stopped", "1 true")]
#[case::stopped_error("{ 1 0 div } stopped", "1 0 true")]
#[case::stopped_undefined("{ nosuchname } stopped $error /errorname get", "true /undefined")]
#[case::error_handler("errordict /undefined { pop 42 } put nosuchname", "42")]
#[case::exit_in_forall("[1 2 3] { dup 2 eq { exit } if } forall", "1 2")]
#[case::bind("/f { add } bind def /add { sub } def 1 2 f", "3")]
#[case::bind_nested("/f { { add } } bind def /add { sub } def 1 2 f exec", "3")]
#[case::immediate("/a 1 def { //a } /a 2 def exec", "1")]
#[case::tail_call(
    "/n 0 def /f { /n n 1 add def n 100000 lt { f } if } def f n",
    "100000"
)]
#[case::countexecstack("countexecstack type", "integertype")]
#[case::quit("1 quit 2", "1")]
fn controls(#[case] program: &str, #[case] expected: &str) {
    assert_eq!(stack(program), expected);
}

#[rstest]
#[case::currentfile_read("currentfile read X", "88 true")]
#[case::readline("currentfile 99 string readline\nabc def\n", "(abc def) true")]
#[case::readstring("currentfile 3 string readstring ABC", "(ABC) true")]
#[case::readhexstring("currentfile 2 string readhexstring 4142 ", "(AB) true")]
#[case::subfile(
    "currentfile 0 (END) /SubFileDecode filter cvx exec 1 2 END 3",
    "1 2 3"
)]
#[case::subfile_all("(4 5 add) 0 () /SubFileDecode filter cvx exec", "9")]
#[case::hex_filter("(41424>) /ASCIIHexDecode filter 9 string readstring", "(AB@) false")]
#[case::base85_filter(
    "(87cURD]i,\"Ebo80~>) /ASCII85Decode filter 20 string readstring pop",
    "(Hello World!)"
)]
#[case::run_length(
    "<02414243FE44 80> /RunLengthDecode filter 9 string readstring",
    "(ABCDDD) false"
)]
#[case::proc_source(
    "/n 0 def { /n n 1 add def n 3 le { (4142) } { () } ifelse } /ASCIIHexDecode filter 9 string readstring",
    "(ABABAB) false"
)]
#[case::token_file("currentfile token 7", "7 true")]
fn reads_files(#[case] program: &str, #[case] expected: &str) {
    assert_eq!(stack(program), expected);
}

#[test]
fn deflate_filter_inflates() {
    // "hello" compressed with zlib, written in hexadecimal.
    let program = "<789ccb48cdc9c90700062c0215> /FlateDecode filter 9 string readstring";
    assert_eq!(stack(program), "(hello) false");
}

#[rstest]
#[case::matrix("matrix", "[1.0 0.0 0.0 1.0 0.0 0.0]")]
#[case::translate("10 20 matrix translate", "[1.0 0.0 0.0 1.0 10.0 20.0]")]
#[case::scale("2 3 matrix scale", "[2.0 0.0 0.0 3.0 0.0 0.0]")]
#[case::rotate("90 matrix rotate", "[0.0 1.0 -1.0 0.0 0.0 0.0]")]
#[case::concatmatrix(
    "2 2 matrix scale 10 0 matrix translate matrix concatmatrix",
    "[2.0 0.0 0.0 2.0 10.0 0.0]"
)]
#[case::invert("2 4 matrix scale matrix invertmatrix", "[0.5 -0.0 -0.0 0.25 0.0 0.0]")]
#[case::transform_given("1 1 2 3 matrix scale transform", "2.0 3.0")]
#[case::itransform("4 9 2 3 matrix scale itransform", "2.0 3.0")]
#[case::dtransform("1 1 10 10 matrix translate dtransform", "1.0 1.0")]
#[case::currentpoint("initmatrix 10 20 translate 1 2 moveto currentpoint", "1.0 2.0")]
#[case::rmoveto("0 0 moveto 3 4 rmoveto currentpoint", "3.0 4.0")]
#[case::pathbbox("newpath 1 2 moveto 5 6 lineto 3 0 lineto pathbbox", "1.0 0.0 5.0 6.0")]
#[case::arc_bbox(
    "newpath 0 0 10 0 90 arc pathbbox 4 { 1000 mul round cvi 4 1 roll } repeat",
    "0 0 10000 10000"
)]
#[case::pathforall(
    "newpath 1 2 moveto 3 4 lineto closepath { (m) } { (l) } { (c) } { (h) } pathforall",
    "1.0 2.0 (m) 3.0 4.0 (l) (h)"
)]
#[case::gsave_restore("1 setlinewidth gsave 5 setlinewidth grestore currentlinewidth", "1.0")]
#[case::save_restore("save 3 setlinewidth restore currentlinewidth", "1.0")]
#[case::colour_gray("0.5 setgray currentgray currentrgbcolor", "0.5 0.5 0.5 0.5")]
#[case::colour_cmyk("0 0 0 1 setcmykcolor currentrgbcolor", "0.0 0.0 0.0")]
#[case::colour_space("[/DeviceCMYK] setcolorspace currentcolor", "0.0 0.0 0.0 1.0")]
#[case::dash("[3 1] 2 setdash currentdash", "[3 1] 2")]
#[case::level("languagelevel systemdict /languagelevel get", "2 2")]
fn places_and_states(#[case] program: &str, #[case] expected: &str) {
    assert_eq!(stack(program), expected);
}

#[rstest]
#[case::category("/Generic /Category findresource type", "dicttype")]
#[case::own_category(
    "/Generic /Category findresource dup length dict copy /Mine exch /Category defineresource pop \
     /x 5 /Mine defineresource pop /x /Mine findresource /y /Mine resourcestatus",
    "5 false"
)]
#[case::procset(
    "/P << /a 1 >> /ProcSet defineresource pop /P /ProcSet findresource /a get",
    "1"
)]
#[case::status(
    "/P 1 /ProcSet defineresource pop /P /ProcSet resourcestatus",
    "1 -1 true"
)]
#[case::missing("{ /Nope /ProcSet findresource } stopped", "/Nope true")]
#[case::filter_category(
    "/FlateDecode /Filter resourcestatus { pop pop true } { false } ifelse",
    "true"
)]
#[case::forall_resources(
    "/Ab 1 /ProcSet defineresource pop /Ac 2 /ProcSet defineresource pop /Bd 3 /ProcSet defineresource pop \
     (A*) { cvn } 9 string /ProcSet resourceforall",
    "/Ab /Ac"
)]
#[case::implementation_dict(
    "/ProcSet /Category findresource begin /Nope ResourceStatus end",
    "false"
)]
#[case::fonts(
    "/Anything findfont 12 scalefont setfont currentfont /FontMatrix get 0 get",
    "0.012"
)]
fn finds_resources(#[case] program: &str, #[case] expected: &str) {
    assert_eq!(stack(program), expected);
}

#[test]
fn uncaught_errors_end_the_run_with_their_name() {
    let (_, result) = run_program("1 2 nosuchname 3");
    let Err(Fault::Halt(Halt::Error { name, command })) = result else {
        panic!("{result:?}");
    };
    assert_eq!(name, "undefined");
    assert!(command.ends_with("nosuchname"), "{command}");
}

#[test]
fn the_end_of_file_comment_ends_the_program() {
    assert_eq!(stack("1\n%%EOF\n2"), "1");
    // Inside an embedded document it does not.
    assert_eq!(
        stack("1\n%%BeginDocument: x\n%%EOF\n%%EndDocument\n2\n%%EOF\n3"),
        "1 2"
    );
}

#[test]
fn declared_binary_sections_are_stepped_over() {
    let mut program = b"1\n%%BeginBinary: 4\n".to_vec();
    program.extend_from_slice(&[0x01, 0x9F, 0xFE, 0x00]);
    program.extend_from_slice(b"\n%%EndBinary\n2");
    let builder = Builder::new(Rect::new(0.0, 0.0, 1.0, 1.0), 1024);
    let mut interp = Interp::new(&program, Limits::default(), builder).unwrap();
    interp.run().unwrap();
    let shown: Vec<String> = interp.ostack.iter().map(|o| show(&interp, *o)).collect();
    assert_eq!(shown, ["1", "2"]);
    assert!(interp.unsupported.keys().any(|k| k.contains("binary data")));
}

#[test]
fn encrypted_sections_are_skipped_not_decoded() {
    let zeros = "0".repeat(64);
    let program = format!("mark currentfile eexec\nA1B2C3D4E5F6\n{zeros}\ncleartomark 7");
    assert_eq!(stack(&program), "7");
}
