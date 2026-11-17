//! The built-in operators, one table of names and functions, and the
//! standard dictionaries `systemdict` starts with.
//!
//! The families live in their own modules: the stacks, arithmetic,
//! composite objects and dictionaries, control, conversion, files, the
//! graphics state and matrices, paths and painting, colour, images, fonts
//! and text, resources, and the device and memory queries a program asks
//! before it draws.

pub(crate) mod compose;
mod control;
mod device;
mod file;
mod font;
mod math;
mod paint;
mod resource;
mod stack;
mod state;

use crate::interp::{Interp, PsError, R};
use crate::object::{Object, Value};

/// One operator: its name and what it does.
pub(crate) struct OpDef {
    pub name: &'static str,
    pub run: fn(&mut Interp) -> R,
}

/// An operator entry.
macro_rules! op {
    ($name:literal, $run:path) => {
        OpDef {
            name: $name,
            run: $run,
        }
    };
}

/// Every operator, by its index.
pub(crate) static OPERATORS: &[OpDef] = &[
    // The operand stack.
    op!("pop", stack::pop),
    op!("exch", stack::exch),
    op!("dup", stack::dup),
    op!("copy", compose::copy),
    op!("index", stack::index),
    op!("roll", stack::roll),
    op!("clear", stack::clear),
    op!("count", stack::count),
    op!("mark", stack::mark),
    op!("[", stack::mark),
    op!("<<", stack::mark),
    op!("cleartomark", stack::cleartomark),
    op!("counttomark", stack::counttomark),
    // Arithmetic.
    op!("add", math::add),
    op!("sub", math::sub),
    op!("mul", math::mul),
    op!("div", math::div),
    op!("idiv", math::idiv),
    op!("mod", math::modulo),
    op!("abs", math::abs),
    op!("neg", math::neg),
    op!("ceiling", math::ceiling),
    op!("floor", math::floor),
    op!("round", math::round),
    op!("truncate", math::truncate),
    op!("sqrt", math::sqrt),
    op!("atan", math::atan),
    op!("cos", math::cos),
    op!("sin", math::sin),
    op!("exp", math::exp),
    op!("ln", math::ln),
    op!("log", math::log),
    op!("rand", math::rand),
    op!("srand", math::srand),
    op!("rrand", math::rrand),
    op!("eq", math::eq),
    op!("ne", math::ne),
    op!("ge", math::ge),
    op!("gt", math::gt),
    op!("le", math::le),
    op!("lt", math::lt),
    op!("and", math::and),
    op!("or", math::or),
    op!("xor", math::xor),
    op!("not", math::not),
    op!("bitshift", math::bitshift),
    // Arrays, strings and dictionaries.
    op!("array", compose::array),
    op!("]", compose::array_close),
    op!("aload", compose::aload),
    op!("astore", compose::astore),
    op!("packedarray", compose::packedarray),
    op!("setpacking", compose::setpacking),
    op!("currentpacking", compose::currentpacking),
    op!("length", compose::length),
    op!("get", compose::get),
    op!("put", compose::put),
    op!("getinterval", compose::getinterval),
    op!("putinterval", compose::putinterval),
    op!("string", compose::string),
    op!("search", compose::search),
    op!("anchorsearch", compose::anchorsearch),
    op!("dict", compose::dict),
    op!(">>", compose::dict_close),
    op!("maxlength", compose::maxlength),
    op!("begin", compose::begin),
    op!("end", compose::end),
    op!("def", compose::def),
    op!("load", compose::load),
    op!("store", compose::store),
    op!("known", compose::known),
    op!("undef", compose::undef),
    op!("where", compose::where_),
    op!("currentdict", compose::currentdict),
    op!("countdictstack", compose::countdictstack),
    op!("dictstack", compose::dictstack),
    op!("cleardictstack", compose::cleardictstack),
    op!("readonly", compose::readonly),
    op!("executeonly", compose::executeonly),
    op!("noaccess", compose::noaccess),
    op!("rcheck", compose::rcheck),
    op!("wcheck", compose::wcheck),
    op!("xcheck", compose::xcheck),
    op!("type", compose::type_),
    op!("cvlit", compose::cvlit),
    op!("cvx", compose::cvx),
    op!("cvi", compose::cvi),
    op!("cvr", compose::cvr),
    op!("cvn", compose::cvn),
    op!("cvs", compose::cvs),
    op!("cvrs", compose::cvrs),
    op!("bind", compose::bind),
    op!("null", compose::null),
    op!("true", compose::true_),
    op!("false", compose::false_),
    op!("setglobal", compose::setglobal),
    op!("currentglobal", compose::currentglobal),
    op!("gcheck", compose::gcheck),
    // Control.
    op!("exec", control::exec),
    op!("if", control::if_),
    op!("ifelse", control::ifelse),
    op!("for", control::for_),
    op!("repeat", control::repeat),
    op!("loop", control::loop_),
    op!("forall", control::forall),
    op!("exit", control::exit),
    op!("stop", control::stop),
    op!("stopped", control::stopped),
    op!("countexecstack", control::countexecstack),
    op!("execstack", control::execstack),
    op!("quit", control::quit),
    op!("start", control::nothing),
    op!(".error", control::error),
    op!("handleerror", control::nothing),
    // Files.
    op!("currentfile", file::currentfile),
    op!("file", file::file),
    op!("filter", file::filter),
    op!("closefile", file::closefile),
    op!("read", file::read),
    op!("readstring", file::readstring),
    op!("readhexstring", file::readhexstring),
    op!("readline", file::readline),
    op!("token", file::token),
    op!("bytesavailable", file::bytesavailable),
    op!("flushfile", file::flushfile),
    op!("resetfile", file::resetfile),
    op!("status", file::status),
    op!("write", file::write),
    op!("writestring", file::writestring),
    op!("writehexstring", file::writestring),
    op!("print", file::print),
    op!("=", file::print),
    op!("==", file::print),
    op!("stack", control::nothing),
    op!("pstack", control::nothing),
    op!("flush", control::nothing),
    op!("run", file::run),
    op!("deletefile", file::deletefile),
    op!("renamefile", file::renamefile),
    op!("eexec", file::eexec),
    op!("echo", stack::pop),
    // The graphics state and matrices.
    op!("gsave", state::gsave),
    op!("grestore", state::grestore),
    op!("grestoreall", state::grestoreall),
    op!("initgraphics", state::initgraphics),
    op!("gstate", state::gstate),
    op!("setgstate", state::setgstate),
    op!("currentgstate", state::currentgstate),
    op!("save", state::save),
    op!("restore", state::restore),
    op!("setlinewidth", state::setlinewidth),
    op!("currentlinewidth", state::currentlinewidth),
    op!("setlinecap", state::setlinecap),
    op!("currentlinecap", state::currentlinecap),
    op!("setlinejoin", state::setlinejoin),
    op!("currentlinejoin", state::currentlinejoin),
    op!("setmiterlimit", state::setmiterlimit),
    op!("currentmiterlimit", state::currentmiterlimit),
    op!("setdash", state::setdash),
    op!("currentdash", state::currentdash),
    op!("setflat", state::setflat),
    op!("currentflat", state::currentflat),
    op!("setsmoothness", state::setsmoothness),
    op!("currentsmoothness", state::currentsmoothness),
    op!("setstrokeadjust", state::setstrokeadjust),
    op!("currentstrokeadjust", state::currentstrokeadjust),
    op!("setoverprint", state::setoverprint),
    op!("currentoverprint", state::currentoverprint),
    op!("matrix", state::matrix),
    op!("initmatrix", state::initmatrix),
    op!("identmatrix", state::identmatrix),
    op!("defaultmatrix", state::defaultmatrix),
    op!("currentmatrix", state::currentmatrix),
    op!("setmatrix", state::setmatrix),
    op!("translate", state::translate),
    op!("scale", state::scale),
    op!("rotate", state::rotate),
    op!("concat", state::concat),
    op!("concatmatrix", state::concatmatrix),
    op!("transform", state::transform),
    op!("itransform", state::itransform),
    op!("dtransform", state::dtransform),
    op!("idtransform", state::idtransform),
    op!("invertmatrix", state::invertmatrix),
    // Device controls that do not change what is drawn.
    op!("sethalftone", stack::pop),
    op!("currenthalftone", device::currenthalftone),
    op!("setscreen", device::pop3),
    op!("currentscreen", device::currentscreen),
    op!("setcolorscreen", device::pop12),
    op!("currentcolorscreen", device::currentcolorscreen),
    op!("settransfer", stack::pop),
    op!("currenttransfer", device::identity_proc),
    op!("setcolortransfer", device::pop4),
    op!("currentcolortransfer", device::currentcolortransfer),
    op!("setblackgeneration", stack::pop),
    op!("currentblackgeneration", device::identity_proc),
    op!("setundercolorremoval", stack::pop),
    op!("currentundercolorremoval", device::identity_proc),
    op!("setcolorrendering", stack::pop),
    op!("currentcolorrendering", device::empty_dict),
    // Paths and painting.
    op!("newpath", paint::newpath),
    op!("currentpoint", paint::currentpoint),
    op!("moveto", paint::moveto),
    op!("rmoveto", paint::rmoveto),
    op!("lineto", paint::lineto),
    op!("rlineto", paint::rlineto),
    op!("curveto", paint::curveto),
    op!("rcurveto", paint::rcurveto),
    op!("arc", paint::arc),
    op!("arcn", paint::arcn),
    op!("arct", paint::arct),
    op!("arcto", paint::arcto),
    op!("closepath", paint::closepath),
    op!("flattenpath", paint::flattenpath),
    op!("reversepath", paint::reversepath),
    op!("strokepath", paint::strokepath),
    op!("clippath", paint::clippath),
    op!("pathbbox", paint::pathbbox),
    op!("pathforall", paint::pathforall),
    op!("initclip", paint::initclip),
    op!("clip", paint::clip),
    op!("eoclip", paint::eoclip),
    op!("rectclip", paint::rectclip),
    op!("fill", paint::fill),
    op!("eofill", paint::eofill),
    op!("stroke", paint::stroke),
    op!("rectfill", paint::rectfill),
    op!("rectstroke", paint::rectstroke),
    op!("erasepage", paint::erasepage),
    op!("showpage", paint::showpage),
    op!("copypage", control::nothing),
    op!("shfill", paint::shfill),
    op!("ufill", paint::upath_unsupported),
    op!("ueofill", paint::upath_unsupported),
    op!("ustroke", paint::upath_unsupported),
    op!("uappend", paint::upath_unsupported),
    op!("upath", paint::upath),
    op!("setbbox", device::pop4),
    op!("execform", paint::execform),
    // Colour.
    op!("setgray", paint::setgray),
    op!("currentgray", paint::currentgray),
    op!("setrgbcolor", paint::setrgbcolor),
    op!("currentrgbcolor", paint::currentrgbcolor),
    op!("sethsbcolor", paint::sethsbcolor),
    op!("currenthsbcolor", paint::currenthsbcolor),
    op!("setcmykcolor", paint::setcmykcolor),
    op!("currentcmykcolor", paint::currentcmykcolor),
    op!("setcolorspace", paint::setcolorspace),
    op!("currentcolorspace", paint::currentcolorspace),
    op!("setcolor", paint::setcolor),
    op!("currentcolor", paint::currentcolor),
    op!("setpattern", paint::setpattern),
    op!("makepattern", paint::makepattern),
    // Images.
    op!("image", paint::image),
    op!("colorimage", paint::colorimage),
    op!("imagemask", paint::imagemask),
    // Fonts and text.
    op!("findfont", font::findfont),
    op!("definefont", font::definefont),
    op!("undefinefont", stack::pop),
    op!("scalefont", font::scalefont),
    op!("makefont", font::makefont),
    op!("setfont", font::setfont),
    op!("currentfont", font::currentfont),
    op!("rootfont", font::currentfont),
    op!("selectfont", font::selectfont),
    op!("show", font::show),
    op!("ashow", font::ashow),
    op!("widthshow", font::widthshow),
    op!("awidthshow", font::awidthshow),
    op!("kshow", font::kshow),
    op!("xshow", font::xshow),
    op!("yshow", font::xshow),
    op!("xyshow", font::xshow),
    op!("glyphshow", font::glyphshow),
    op!("cshow", font::kshow),
    op!("stringwidth", font::stringwidth),
    op!("charpath", font::charpath),
    op!("setcachedevice", font::setcachedevice),
    op!("setcachedevice2", font::setcachedevice2),
    op!("setcharwidth", device::pop2),
    op!("setcachelimit", stack::pop),
    op!("cachestatus", device::cachestatus),
    op!("findencoding", resource::findencoding),
    // Resources.
    op!("findresource", resource::findresource),
    op!("defineresource", resource::defineresource),
    op!("undefineresource", resource::undefineresource),
    op!("resourcestatus", resource::resourcestatus),
    op!("resourceforall", resource::resourceforall),
    op!(".findresource", resource::native_find),
    op!(".defineresource", resource::native_define),
    op!(".undefineresource", resource::native_undefine),
    op!(".resourcestatus", resource::native_status),
    op!(".resourceforall", resource::native_forall),
    // The device, the memory and the job.
    op!("setpagedevice", stack::pop),
    op!("currentpagedevice", device::currentpagedevice),
    op!("nulldevice", control::nothing),
    op!("languagelevel", device::languagelevel),
    op!("version", device::version),
    op!("product", device::product),
    op!("revision", device::revision),
    op!("serialnumber", device::serialnumber),
    op!("realtime", device::realtime),
    op!("usertime", device::realtime),
    op!("vmstatus", device::vmstatus),
    op!("vmreclaim", stack::pop),
    op!("setvmthreshold", stack::pop),
    op!("setuserparams", stack::pop),
    op!("currentuserparams", device::currentuserparams),
    op!("setsystemparams", stack::pop),
    op!("currentsystemparams", device::currentsystemparams),
    op!("setdevparams", device::pop2),
    op!("currentdevparams", device::currentdevparams),
    op!("setucacheparams", stack::cleartomark),
    op!("ucachestatus", device::ucachestatus),
    op!("internaldict", device::internaldict),
    op!("setobjectformat", stack::pop),
    op!("currentobjectformat", device::zero),
    op!("defineuserobject", device::pop2),
    op!("undefineuserobject", stack::pop),
    op!("execuserobject", device::execuserobject),
    op!("cliprestore", state::grestore),
    op!("clipsave", state::gsave),
];

/// Fills `systemdict` and the other standard dictionaries.
pub(crate) fn install(interp: &mut Interp) -> R {
    let system = interp.systemdict;
    for (index, def) in OPERATORS.iter().enumerate() {
        let op = u16::try_from(index).map_err(|_| PsError::LimitCheck)?;
        let object = match def.name {
            // `true`, `false` and `null` are values, not operators, and so
            // is the language level: programs `get` it from `systemdict`
            // and compare it as a number.
            "languagelevel" => Object::int(2),
            "true" => Object::bool(true),
            "false" => Object::bool(false),
            "null" => Object::NULL,
            _ => Object::exe(Value::Operator(op)),
        };
        interp.vm.dict_put_name(system, def.name, object)?;
    }
    let entries = [
        ("systemdict", interp.systemdict),
        ("userdict", interp.userdict),
        ("globaldict", interp.globaldict),
        ("errordict", interp.errordict),
        ("$error", interp.error_info),
        ("statusdict", interp.statusdict),
        ("FontDirectory", interp.fontdirectory),
        ("GlobalFontDirectory", interp.fontdirectory),
        ("SharedFontDirectory", interp.fontdirectory),
    ];
    for (name, dict) in entries {
        interp
            .vm
            .dict_put_name(system, name, Object::lit(Value::Dict(dict)))?;
    }
    // The standard error handlers: each records its error and stops.
    let error = interp.lookup_str(".error").unwrap_or(Object::NULL);
    for e in crate::interp::PsError::ALL {
        let name = interp.name_obj(e.name());
        let body = interp.vm.new_array(vec![name, error])?;
        interp
            .vm
            .dict_put_name(interp.errordict, e.name(), Object::exe(Value::Array(body)))?;
    }
    let handle = interp.lookup_str("handleerror").unwrap_or(Object::NULL);
    interp
        .vm
        .dict_put_name(interp.errordict, "handleerror", handle)?;
    let info = interp.error_info;
    interp
        .vm
        .dict_put_name(info, "newerror", Object::bool(false))?;
    font::install(interp)?;
    resource::install(interp)?;
    Ok(())
}
