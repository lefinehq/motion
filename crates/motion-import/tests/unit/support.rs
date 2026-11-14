//! Byte builders shared by the unit tests: chunks, lists and whole files,
//! laid out as the spec says, so each test states only the bytes it is about.

/// A chunk with `tag` and `body`, padded to an even length.
pub(crate) fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let size = u32::try_from(body.len()).expect("a small test body");
    let pad: &[u8] = if body.len() % 2 == 1 { &[0] } else { &[] };
    [tag.as_slice(), &size.to_be_bytes(), body, pad].concat()
}

/// A `LIST` of type `kind` holding `children`, each already built.
pub(crate) fn list(kind: &[u8; 4], children: &[Vec<u8>]) -> Vec<u8> {
    chunk(b"LIST", &[kind.as_slice(), &children.concat()].concat())
}

/// A whole file: the `RIFX` header, the `Egg!` form and `children`.
pub(crate) fn file(children: &[Vec<u8>]) -> Vec<u8> {
    chunk(b"RIFX", &[b"Egg!".as_slice(), &children.concat()].concat())
}
