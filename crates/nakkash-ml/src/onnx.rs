//! The check every graph passes before it is kept ([D84]): nakkash reads
//! the ONNX file itself, before any runtime sees it.
//!
//! An ONNX file is one protobuf message, `ModelProto`. The reader walks its
//! wire format directly from the file and seeks over everything it does
//! not need, so a gigabyte of weights costs a few reads. It refuses:
//!
//! - an operator set or a node in any domain but the standard one, the
//!   empty name or `ai.onnx` (no custom operators, which load native code);
//! - model-local functions, which define operators of their own;
//! - a tensor whose data lies outside the file (external data, whose paths
//!   have escaped their folder in past runtimes);
//! - a file larger than protobuf's 2 GiB, or a message that is malformed,
//!   nested deeper than [`MAX_DEPTH`], or runs past its parent's end.
//!
//! Subgraphs (the branches of `If`, the bodies of `Loop` and `Scan`) are
//! read like the main graph.
//!
//! [D84]: https://github.com/lefinehq/nakkash/blob/main/docs/design/decisions.md#d84-models-are-untrusted-input

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use nakkash_format::{Diagnostic, Kind, Result};

/// The largest file read: protobuf's limit for one message.
pub const MAX_BYTES: u64 = 2 << 30;

/// How deep messages may nest: graphs inside attributes inside nodes inside
/// graphs, a few levels for any real model.
pub const MAX_DEPTH: usize = 32;

/// What the check found in a graph that passed it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// The ONNX IR version.
    pub ir_version: i64,
    /// The standard operator set's version.
    pub opset: i64,
    /// Nodes, subgraphs' included.
    pub nodes: usize,
    /// Initialisers, subgraphs' included.
    pub initializers: usize,
}

/// Checks the ONNX file at `path`.
///
/// # Errors
/// A [`Kind::Io`] diagnostic when it cannot be read, and a
/// [`Kind::Invalid`] one, naming the file and what was refused, when it
/// fails the check.
pub fn check_file(path: &Path) -> Result<Summary> {
    let file = File::open(path).map_err(|e| Diagnostic::io("open", path, &e))?;
    let size = file
        .metadata()
        .map_err(|e| Diagnostic::io("read", path, &e))?
        .len();
    check(BufReader::new(file), size).map_err(|e| Diagnostic {
        message: format!("`{}`: {}", path.display(), e.message),
        ..e
    })
}

/// Checks the ONNX model `reader` gives, `size` bytes long.
///
/// # Errors
/// As [`check_file`].
pub fn check(reader: impl Read + Seek, size: u64) -> Result<Summary> {
    if size > MAX_BYTES {
        return Err(refused(format!(
            "a graph of {size} bytes, over protobuf's {MAX_BYTES}"
        )));
    }
    let mut wire = Wire { reader, at: 0 };
    let mut summary = Summary::default();
    let mut opset = None;
    while wire.at < size {
        let (field, kind) = wire.key(size)?;
        match (field, kind) {
            (1, Value::Varint) => summary.ir_version = wire.varint(size)? as i64,
            (8, Value::Length) => {
                let end = wire.length(size)?;
                let (domain, version) = opset_import(&mut wire, end)?;
                if !standard(&domain) {
                    return Err(refused(format!("an operator set of domain `{domain}`")));
                }
                opset = Some(version);
            }
            (7, Value::Length) => {
                let end = wire.length(size)?;
                graph(&mut wire, end, 1, &mut summary)?;
            }
            (25, Value::Length) => return Err(refused("a model-local function".into())),
            _ => wire.skip(kind, size)?,
        }
    }
    summary.opset = opset.ok_or_else(|| refused("no standard operator set".into()))?;
    Ok(summary)
}

/// Whether `domain` names the standard operators.
fn standard(domain: &str) -> bool {
    domain.is_empty() || domain == "ai.onnx"
}

/// The error for a graph that fails the check.
fn refused(what: String) -> Diagnostic {
    Diagnostic::new(
        Kind::Invalid,
        format!(
            "{what}: nakkash runs only standard ONNX operators with every weight in the file (D84)"
        ),
    )
}

/// The error for bytes that are not a protobuf message.
fn malformed(what: &str) -> Diagnostic {
    Diagnostic::new(Kind::Invalid, format!("not an ONNX model: {what}"))
}

/// The kinds of value the wire format carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Value {
    Varint,
    Fixed64,
    Length,
    Fixed32,
}

/// A protobuf reader over a file, counting where it is.
struct Wire<R> {
    reader: R,
    at: u64,
}

impl<R: Read + Seek> Wire<R> {
    fn byte(&mut self, end: u64) -> Result<u8> {
        if self.at >= end {
            return Err(malformed("a value runs past its message"));
        }
        let mut b = [0];
        self.reader
            .read_exact(&mut b)
            .map_err(|_| malformed("the file ends inside a value"))?;
        self.at += 1;
        Ok(b[0])
    }

    fn varint(&mut self, end: u64) -> Result<u64> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.byte(end)?;
            value |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(malformed("a varint longer than ten bytes"))
    }

    fn key(&mut self, end: u64) -> Result<(u64, Value)> {
        let key = self.varint(end)?;
        let kind = match key & 7 {
            0 => Value::Varint,
            1 => Value::Fixed64,
            2 => Value::Length,
            5 => Value::Fixed32,
            _ => return Err(malformed("a group or an unknown wire type")),
        };
        match key >> 3 {
            0 => Err(malformed("field number 0")),
            field => Ok((field, kind)),
        }
    }

    /// Reads a length and returns where its value ends.
    fn length(&mut self, end: u64) -> Result<u64> {
        let len = self.varint(end)?;
        match self.at.checked_add(len) {
            Some(stop) if stop <= end => Ok(stop),
            _ => Err(malformed("a length that runs past its message")),
        }
    }

    fn seek_to(&mut self, to: u64) -> Result<()> {
        let by = i64::try_from(to - self.at).map_err(|_| malformed("a length too large"))?;
        self.reader
            .seek(SeekFrom::Current(by))
            .map_err(|_| malformed("the file ends inside a value"))?;
        self.at = to;
        Ok(())
    }

    fn skip(&mut self, kind: Value, end: u64) -> Result<()> {
        let to = match kind {
            Value::Varint => return self.varint(end).map(drop),
            Value::Fixed64 => self.at + 8,
            Value::Fixed32 => self.at + 4,
            Value::Length => self.length(end)?,
        };
        if to > end {
            return Err(malformed("a value runs past its message"));
        }
        self.seek_to(to)
    }

    fn string(&mut self, end: u64) -> Result<String> {
        let stop = self.length(end)?;
        let len = usize::try_from(stop - self.at).map_err(|_| malformed("a string too long"))?;
        if len > 1 << 16 {
            return Err(malformed("a name longer than 64 KiB"));
        }
        let mut bytes = vec![0; len];
        self.reader
            .read_exact(&mut bytes)
            .map_err(|_| malformed("the file ends inside a string"))?;
        self.at = stop;
        String::from_utf8(bytes).map_err(|_| malformed("a name that is not UTF-8"))
    }
}

/// `OperatorSetIdProto`: domain (1) and version (2).
fn opset_import<R: Read + Seek>(wire: &mut Wire<R>, end: u64) -> Result<(String, i64)> {
    let (mut domain, mut version) = (String::new(), 0);
    while wire.at < end {
        match wire.key(end)? {
            (1, Value::Length) => domain = wire.string(end)?,
            (2, Value::Varint) => version = wire.varint(end)? as i64,
            (_, kind) => wire.skip(kind, end)?,
        }
    }
    Ok((domain, version))
}

/// `GraphProto`: nodes (1), initialisers (5), sparse initialisers (15).
fn graph<R: Read + Seek>(
    wire: &mut Wire<R>,
    end: u64,
    depth: usize,
    summary: &mut Summary,
) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(malformed("graphs nested too deep"));
    }
    while wire.at < end {
        match wire.key(end)? {
            (1, Value::Length) => {
                let stop = wire.length(end)?;
                node(wire, stop, depth, summary)?;
            }
            (5, Value::Length) => {
                let stop = wire.length(end)?;
                tensor(wire, stop)?;
                summary.initializers += 1;
            }
            (15, Value::Length) => {
                let stop = wire.length(end)?;
                sparse_tensor(wire, stop)?;
                summary.initializers += 1;
            }
            (_, kind) => wire.skip(kind, end)?,
        }
    }
    Ok(())
}

/// `NodeProto`: domain (7) and attributes (5).
fn node<R: Read + Seek>(
    wire: &mut Wire<R>,
    end: u64,
    depth: usize,
    summary: &mut Summary,
) -> Result<()> {
    summary.nodes += 1;
    while wire.at < end {
        match wire.key(end)? {
            (7, Value::Length) => {
                let domain = wire.string(end)?;
                if !standard(&domain) {
                    return Err(refused(format!("a node of domain `{domain}`")));
                }
            }
            (5, Value::Length) => {
                let stop = wire.length(end)?;
                attribute(wire, stop, depth, summary)?;
            }
            (_, kind) => wire.skip(kind, end)?,
        }
    }
    Ok(())
}

/// `AttributeProto`: a tensor (5), a graph (6), tensors (10), graphs (11),
/// a sparse tensor (22) and sparse tensors (23); everything else skipped.
fn attribute<R: Read + Seek>(
    wire: &mut Wire<R>,
    end: u64,
    depth: usize,
    summary: &mut Summary,
) -> Result<()> {
    while wire.at < end {
        match wire.key(end)? {
            (5 | 10, Value::Length) => {
                let stop = wire.length(end)?;
                tensor(wire, stop)?;
            }
            (22 | 23, Value::Length) => {
                let stop = wire.length(end)?;
                sparse_tensor(wire, stop)?;
            }
            (6 | 11, Value::Length) => {
                let stop = wire.length(end)?;
                graph(wire, stop, depth + 1, summary)?;
            }
            (_, kind) => wire.skip(kind, end)?,
        }
    }
    Ok(())
}

/// `TensorProto`: refused when its data is external, by its location (14)
/// or its external data entries (13).
fn tensor<R: Read + Seek>(wire: &mut Wire<R>, end: u64) -> Result<()> {
    while wire.at < end {
        match wire.key(end)? {
            (14, Value::Varint) => {
                if wire.varint(end)? != 0 {
                    return Err(refused("a tensor whose data lies outside the file".into()));
                }
            }
            (13, Value::Length) => {
                return Err(refused("a tensor whose data lies outside the file".into()));
            }
            (_, kind) => wire.skip(kind, end)?,
        }
    }
    Ok(())
}

/// `SparseTensorProto`: values (1) and indices (2), both tensors.
fn sparse_tensor<R: Read + Seek>(wire: &mut Wire<R>, end: u64) -> Result<()> {
    while wire.at < end {
        match wire.key(end)? {
            (1 | 2, Value::Length) => {
                let stop = wire.length(end)?;
                tensor(wire, stop)?;
            }
            (_, kind) => wire.skip(kind, end)?,
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/onnx.rs"]
mod tests;
