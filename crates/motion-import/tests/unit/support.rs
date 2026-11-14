//! Byte builders shared by the unit tests: chunks, lists, fixed records and
//! whole files, laid out as the spec says, so each test states only the bytes
//! it is about.

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

/// A record of `size` zero bytes with `fields` written at their offsets, as
/// the spec's tables give them.
pub(crate) fn record(size: usize, fields: &[(usize, &[u8])]) -> Vec<u8> {
    fields
        .iter()
        .fold(vec![0; size], |mut bytes, (offset, field)| {
            bytes[*offset..offset + field.len()].copy_from_slice(field);
            bytes
        })
}

/// A file of version `major` with `items` in its root folder.
pub(crate) fn project(major: u16, items: &[Vec<u8>]) -> Vec<u8> {
    file(&[
        chunk(b"head", &record(20, &[(0, &major.to_be_bytes())])),
        chunk(b"nhed", &record(32, &[(0x0F, &[1])])),
        list(b"Fold", &[&[chunk(b"fdta", &[0; 14])], items].concat()),
    ])
}

/// A `LIST:Item` of `kind` with `id`, `name` and the chunks after the name.
pub(crate) fn item(kind: u16, id: u32, name: &str, rest: &[Vec<u8>]) -> Vec<u8> {
    let idta = record(84, &[(0, &kind.to_be_bytes()), (0x10, &id.to_be_bytes())]);
    let head = [chunk(b"idta", &idta), chunk(b"Utf8", name.as_bytes())];
    list(b"Item", &[&head, rest].concat())
}

/// A 204-byte `cdta`: `width` x `height` at `fps`, 24576 timebase units a
/// second, lasting `seconds`.
pub(crate) fn cdta(width: u16, height: u16, fps: u16, seconds: u32) -> Vec<u8> {
    let tb = 24576u32.to_be_bytes();
    chunk(
        b"cdta",
        &record(
            204,
            &[
                (0x20, &tb),
                (0x24, &u32::MAX.to_be_bytes()),
                (0x28, &600u32.to_be_bytes()),
                (0x2C, &(seconds * 24576).to_be_bytes()),
                (0x30, &tb),
                (0x34, &[255, 0, 51]),
                (0x8C, &width.to_be_bytes()),
                (0x8E, &height.to_be_bytes()),
                (0x90, &[0, 0, 0, 1, 0, 0, 0, 1]),
                (0x9C, &(u32::from(fps) << 16).to_be_bytes()),
            ],
        ),
    )
}
