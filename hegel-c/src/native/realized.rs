//! The realized form of a test case: its choice nodes with the constraints
//! they were drawn under, and its spans, encoded so that another process
//! can hand them back to the shrinker, which needs constraints and spans,
//! not only the values the database keeps.
//!
//! Format: a version byte, then a stream. A stream is a 4-byte
//! little-endian node count, that many nodes, a 4-byte span count and that
//! many spans. A node is a forced byte (0 or 1), a kind tag, the
//! constraint and then the value:
//!
//! - 0 Integer: `min_value`, `max_value`, `shrink_towards` and the value,
//!   each a 4-byte length and that many two's-complement little-endian
//!   bytes
//! - 1 Boolean: `p` as 8 little-endian bytes of the f64 bit pattern, then
//!   the value as one byte
//! - 2 Float: `min_value`, `max_value` (f64 bits), `allow_nan`,
//!   `allow_infinity` (one byte each), `smallest_nonzero_magnitude` (f64
//!   bits), then the value (f64 bits)
//! - 3 Bytes: `min_size`, `max_size` as 8-byte little-endian, then a 4-byte
//!   length and the bytes
//! - 4 String: `min_size`, `max_size` as 8-byte little-endian, a 4-byte
//!   interval count and that many `(u32, u32)` pairs, then a 4-byte
//!   codepoint count and the codepoints as u32
//! - 5 Clone: the cloned stream, recursively
//!
//! A span is `start`, `end` as 8-byte little-endian, `label` as 8 bytes,
//! `depth` as 4 bytes, a presence byte and 8 bytes for `parent`, and a
//! `discarded` byte.

use alloc::vec::Vec;

#[cfg(any(test, feature = "fuzz-driver"))]
use alloc::sync::Arc;

use crate::native::bignum::BigInt;
#[cfg(any(test, feature = "fuzz-driver"))]
use crate::native::core::choices::{
    BooleanChoice, BytesChoice, FloatChoice, IntegerChoice, RealizedStream, StringChoice,
};
use crate::native::core::choices::{ChoiceData, ChoiceNode};
use crate::native::core::{MAX_CLONE_DEPTH, Span};
#[cfg(any(test, feature = "fuzz-driver"))]
use crate::native::intervalsets::IntervalSet;

const VERSION: u8 = 1;

/// Encode `nodes` and `spans`. `None` if a clone nests deeper than
/// [`MAX_CLONE_DEPTH`], which the engine never produces.
pub fn serialize_realized(nodes: &[ChoiceNode], spans: &[Span]) -> Option<Vec<u8>> {
    let mut buf = Vec::with_capacity(1 + nodes.len() * 40 + spans.len() * 38);
    buf.push(VERSION);
    write_stream(&mut buf, nodes, spans, 0)?;
    Some(buf)
}

/// Decode what [`serialize_realized`] wrote. `None` if the bytes are
/// truncated, malformed, of another version, or nest clones too deep.
#[cfg(any(test, feature = "fuzz-driver"))]
pub fn deserialize_realized(bytes: &[u8]) -> Option<(Vec<ChoiceNode>, Vec<Span>)> {
    let mut reader = Reader { bytes, pos: 0 };
    if reader.u8()? != VERSION {
        return None;
    }
    let stream = read_stream(&mut reader, 0)?;
    (reader.pos == bytes.len()).then_some(stream)
}

fn write_stream(
    buf: &mut Vec<u8>,
    nodes: &[ChoiceNode],
    spans: &[Span],
    depth: usize,
) -> Option<()> {
    if depth > MAX_CLONE_DEPTH {
        return None;
    }
    write_u32(buf, nodes.len());
    for node in nodes {
        write_node(buf, node, depth)?;
    }
    write_u32(buf, spans.len());
    for span in spans {
        write_u64(buf, span.start as u64);
        write_u64(buf, span.end as u64);
        write_u64(buf, span.label);
        buf.extend_from_slice(&span.depth.to_le_bytes());
        buf.push(span.parent.is_some() as u8);
        write_u64(buf, span.parent.unwrap_or(0) as u64);
        buf.push(span.discarded as u8);
    }
    Some(())
}

fn write_node(buf: &mut Vec<u8>, node: &ChoiceNode, depth: usize) -> Option<()> {
    buf.push(node.was_forced as u8);
    match &node.data {
        ChoiceData::Integer(constraint, value) => {
            buf.push(0);
            write_bigint(buf, &constraint.min_value);
            write_bigint(buf, &constraint.max_value);
            write_bigint(buf, &constraint.shrink_towards);
            write_bigint(buf, value);
        }
        ChoiceData::Boolean(constraint, value) => {
            buf.push(1);
            write_f64(buf, constraint.p);
            buf.push(*value as u8);
        }
        ChoiceData::Float(constraint, value) => {
            buf.push(2);
            write_f64(buf, constraint.min_value);
            write_f64(buf, constraint.max_value);
            buf.push(constraint.allow_nan as u8);
            buf.push(constraint.allow_infinity as u8);
            write_f64(buf, constraint.smallest_nonzero_magnitude);
            write_f64(buf, *value);
        }
        ChoiceData::Bytes(constraint, value) => {
            buf.push(3);
            write_u64(buf, constraint.min_size as u64);
            write_u64(buf, constraint.max_size as u64);
            write_u32(buf, value.len());
            buf.extend_from_slice(value);
        }
        ChoiceData::String(constraint, value) => {
            buf.push(4);
            write_u64(buf, constraint.min_size as u64);
            write_u64(buf, constraint.max_size as u64);
            write_u32(buf, constraint.intervals.intervals.len());
            for &(lo, hi) in &constraint.intervals.intervals {
                buf.extend_from_slice(&lo.to_le_bytes());
                buf.extend_from_slice(&hi.to_le_bytes());
            }
            write_u32(buf, value.len());
            for cp in value {
                buf.extend_from_slice(&cp.to_le_bytes());
            }
        }
        ChoiceData::Clone(stream) => {
            buf.push(5);
            write_stream(buf, stream.nodes(), stream.spans(), depth + 1)?;
        }
    }
    Some(())
}

fn write_u32(buf: &mut Vec<u8>, n: usize) {
    buf.extend_from_slice(&(n as u32).to_le_bytes());
}

fn write_u64(buf: &mut Vec<u8>, n: u64) {
    buf.extend_from_slice(&n.to_le_bytes());
}

fn write_f64(buf: &mut Vec<u8>, f: f64) {
    buf.extend_from_slice(&f.to_bits().to_le_bytes());
}

fn write_bigint(buf: &mut Vec<u8>, v: &BigInt) {
    let bytes = v.to_signed_bytes_le();
    write_u32(buf, bytes.len());
    buf.extend_from_slice(&bytes);
}

#[cfg(any(test, feature = "fuzz-driver"))]
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

#[cfg(any(test, feature = "fuzz-driver"))]
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let slice = self.bytes.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn flag(&mut self) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    fn u32(&mut self) -> Option<usize> {
        let raw: [u8; 4] = self.take(4)?.try_into().ok()?;
        Some(u32::from_le_bytes(raw) as usize)
    }

    fn u64(&mut self) -> Option<u64> {
        let raw: [u8; 8] = self.take(8)?.try_into().ok()?;
        Some(u64::from_le_bytes(raw))
    }

    fn size(&mut self) -> Option<usize> {
        usize::try_from(self.u64()?).ok()
    }

    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_bits(self.u64()?))
    }

    fn bigint(&mut self) -> Option<BigInt> {
        let len = self.u32()?;
        Some(BigInt::from_signed_bytes_le(self.take(len)?))
    }
}

#[cfg(any(test, feature = "fuzz-driver"))]
fn read_stream(reader: &mut Reader<'_>, depth: usize) -> Option<(Vec<ChoiceNode>, Vec<Span>)> {
    if depth > MAX_CLONE_DEPTH {
        return None;
    }
    let count = reader.u32()?;
    let mut nodes = Vec::with_capacity(count.min(reader.bytes.len()));
    for _ in 0..count {
        nodes.push(read_node(reader, depth)?);
    }
    let count = reader.u32()?;
    let mut spans = Vec::with_capacity(count.min(reader.bytes.len()));
    for _ in 0..count {
        let start = reader.size()?;
        let end = reader.size()?;
        let label = reader.u64()?;
        let depth = u32::from_le_bytes(reader.take(4)?.try_into().ok()?);
        let has_parent = reader.flag()?;
        let parent = reader.size()?;
        let discarded = reader.flag()?;
        spans.push(Span {
            start,
            end,
            label,
            depth,
            parent: has_parent.then_some(parent),
            discarded,
        });
    }
    Some((nodes, spans))
}

#[cfg(any(test, feature = "fuzz-driver"))]
fn read_node(reader: &mut Reader<'_>, depth: usize) -> Option<ChoiceNode> {
    let was_forced = reader.flag()?;
    let node = match reader.u8()? {
        0 => {
            let constraint = IntegerChoice {
                min_value: reader.bigint()?,
                max_value: reader.bigint()?,
                shrink_towards: reader.bigint()?,
            };
            ChoiceNode::integer(constraint, reader.bigint()?, was_forced)
        }
        1 => {
            let constraint = BooleanChoice { p: reader.f64()? };
            ChoiceNode::boolean(constraint, reader.flag()?, was_forced)
        }
        2 => {
            let constraint = FloatChoice {
                min_value: reader.f64()?,
                max_value: reader.f64()?,
                allow_nan: reader.flag()?,
                allow_infinity: reader.flag()?,
                smallest_nonzero_magnitude: reader.f64()?,
            };
            ChoiceNode::float(constraint, reader.f64()?, was_forced)
        }
        3 => {
            let constraint = BytesChoice {
                min_size: reader.size()?,
                max_size: reader.size()?,
            };
            let len = reader.u32()?;
            ChoiceNode::bytes(constraint, reader.take(len)?.to_vec(), was_forced)
        }
        4 => {
            let min_size = reader.size()?;
            let max_size = reader.size()?;
            let count = reader.u32()?;
            let mut intervals = Vec::with_capacity(count.min(reader.bytes.len()));
            for _ in 0..count {
                let lo = u32::from_le_bytes(reader.take(4)?.try_into().ok()?);
                let hi = u32::from_le_bytes(reader.take(4)?.try_into().ok()?);
                intervals.push((lo, hi));
            }
            let constraint = StringChoice {
                intervals: Arc::new(IntervalSet::new(intervals).ok()?),
                min_size,
                max_size,
            };
            let count = reader.u32()?;
            let mut value = Vec::with_capacity(count.min(reader.bytes.len()));
            for _ in 0..count {
                value.push(u32::from_le_bytes(reader.take(4)?.try_into().ok()?));
            }
            ChoiceNode::string(constraint, value, was_forced)
        }
        5 => {
            let (nodes, spans) = read_stream(reader, depth + 1)?;
            ChoiceNode::clone_stream(Arc::new(RealizedStream::new(nodes, spans)), was_forced)
        }
        _ => return None,
    };
    Some(node)
}

#[cfg(test)]
#[path = "../../tests/embedded/native/realized_tests.rs"]
mod tests;
