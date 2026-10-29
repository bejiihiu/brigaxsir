//! `StringRange`, the Rust analogue of Java's
//! `com.mojang.brigadier.context.StringRange`.

use std::{cmp, fmt};

use crate::string_reader::{ImmutableStringReader, char_to_byte};

/// A half-open `[start, end)` span over the command input, in **characters**
/// (Unicode scalar values) — the same unit [`StringReader`](crate::string_reader::StringReader)
/// cursors use. Used to remember where an argument or node came from, so it
/// can be re-extracted or replaced (that's what suggestions use to overwrite
/// the current token).
///
/// Java's `StringRange` counts UTF-16 units; the two agree for BMP-only
/// input, so ASCII behavior is 1:1.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct StringRange {
    start: usize,
    end: usize,
}

impl StringRange {
    /// A span `[start, end)`.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// An empty span at exactly `pos`. Java: `StringRange.at(pos)`.
    pub fn at(pos: usize) -> Self {
        Self::new(pos, pos)
    }

    /// A span between two positions. Java: `StringRange.between`.
    pub fn between(start: usize, end: usize) -> Self {
        Self::new(start, end)
    }

    /// The smallest span covering both inputs. Java: `StringRange.encompassing`.
    pub fn encompassing(a: &Self, b: &Self) -> Self {
        Self::new(cmp::min(a.start, b.start), cmp::max(a.end, b.end))
    }

    /// The start of the span, inclusive.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The end of the span, exclusive.
    pub fn end(&self) -> usize {
        self.end
    }

    /// Slice `reader` to this span. Java: `get(String)`.
    ///
    /// Java's substring throws on out-of-range spans; here both ends clamp
    /// to the input bounds and an inverted span comes back empty, so no
    /// range can panic.
    pub fn get<'a>(&self, reader: &'a str) -> &'a str {
        &reader[char_to_byte(reader, self.start)..char_to_byte(reader, self.end)]
    }

    /// Slice the input of a read-only reader to this span. Java:
    /// `get(ImmutableStringReader)` — the overload used with actual readers.
    ///
    /// Same clamping contract as [`Self::get`]: Java's `substring` would
    /// throw on out-of-range spans, the port saturates at the input bounds.
    pub fn get_from_reader(&self, reader: &dyn ImmutableStringReader) -> String {
        self.get(reader.string()).to_owned()
    }

    /// Whether the span is empty (`start == end`). Java: `isEmpty`.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// `end - start`. Java: `getLength`.
    pub fn length(&self) -> usize {
        self.end - self.start
    }
}

impl fmt::Display for StringRange {
    /// Java's `toString`: `StringRange{start=0, end=3}`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StringRange{{start={}, end={}}}", self.start, self.end)
    }
}
