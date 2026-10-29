//! A cursor-based reader over command input, mirroring Java's
//! `com.mojang.brigadier.StringReader` and its read-only twin
//! `com.mojang.brigadier.ImmutableStringReader` ([`ImmutableStringReader`]).

use std::{fmt, str::FromStr, sync::Arc};

use crate::errors::{BuiltInExceptionProvider, CommandSyntaxException, built_in_exceptions};

/// The character index `index` refers to, as a byte offset into `s`.
///
/// Positions at or past the last character clamp to `s.len()` (the exclusive
/// end), so the result is always a valid slice boundary. This is the bridging
/// helper every char-index consumer uses (`StringRange::get`, error context
/// snippets, suggestion splicing); the reader itself tracks its own byte
/// offset incrementally instead.
pub(crate) fn char_to_byte(s: &str, index: usize) -> usize {
    s.char_indices()
        .nth(index)
        .map_or(s.len(), |(byte, _)| byte)
}

/// The number of characters (Unicode scalar values) in `s`.
pub(crate) fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// A `String` plus a cursor. All the primitive scanning the parser needs:
/// numbers, quoted strings, whitespace, and single-character expectations.
///
/// The cursor counts **characters** (Unicode scalar values), not bytes or
/// UTF-16 code units. That is the one deliberate divergence from Java, whose
/// cursors are UTF-16 indices: for any input made only of BMP characters the
/// two agree, so plain ASCII behaves 1:1 with Java, while astral-plane input
/// (emoji and friends) counts one per character instead of Java's two. Rust
/// `String`s cannot hold lone surrogates at all, which is what makes the
/// cleaner indexing possible.
///
/// Everything is safe on arbitrary UTF-8: length and slicing operations
/// saturate at the end of the input where Java would throw
/// `StringIndexOutOfBoundsException`, and only [`Self::peek`],
/// [`Self::peek_offset`] and [`Self::read`] panic — the same spots where Java
/// throws `StringIndexOutOfBoundsException` on an exhausted reader.
///
/// ```
/// # use brigaxsir::string_reader::StringReader;
/// let mut reader = StringReader::from("tell 42");
/// reader.skip(); // one char at a time: "t"
/// while reader.peek().is_ascii_alphabetic() {
///     reader.skip();
/// }
/// reader.skip_whitespace();
/// assert_eq!(reader.read_int().unwrap(), 42);
/// assert!(!reader.can_read());
/// ```
#[derive(Clone)]
pub struct StringReader {
    /// The full input. Shared, so cloning a reader is a reference bump like
    /// Java's copy constructor sharing the string, not a copy.
    string: Arc<str>,
    /// The cursor: index of the next character to read, in characters.
    cursor: usize,
    /// Byte offset of `cursor` inside `string`, kept in sync by every cursor
    /// mutation so slicing at the cursor stays O(1). Always a char boundary;
    /// saturates at `string.len()` when the cursor runs past the end.
    byte: usize,
    /// Cached `string.chars().count()`, the exclusive end of the cursor.
    len: usize,
}

// Java keeps these as private static final chars with the same names.
const SYNTAX_ESCAPE: char = '\\';
const SYNTAX_DOUBLE_QUOTE: char = '"';
const SYNTAX_SINGLE_QUOTE: char = '\'';

impl From<String> for StringReader {
    fn from(string: String) -> Self {
        Self::from_shared(string.into())
    }
}
impl From<&str> for StringReader {
    fn from(string: &str) -> Self {
        Self::from_shared(string.into())
    }
}

impl fmt::Debug for StringReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StringReader")
            .field("string", &self.string)
            .field("cursor", &self.cursor)
            .finish()
    }
}

impl StringReader {
    fn from_shared(string: Arc<str>) -> Self {
        let len = char_len(&string);
        Self {
            string,
            cursor: 0,
            byte: 0,
            len,
        }
    }

    /// The whole input this reader works on, from position 0.
    pub fn string(&self) -> &str {
        &self.string
    }

    /// How much is left to read, in characters. Java: `getRemainingLength`.
    /// Saturates at zero where Java's `int` would go negative when the cursor
    /// was pushed past the end via [`Self::skip`] or [`Self::set_cursor`].
    pub fn remaining_length(&self) -> usize {
        self.len.saturating_sub(self.cursor)
    }

    /// The full length of the input, in characters. Java: `getTotalLength`.
    pub fn total_length(&self) -> usize {
        self.len
    }

    /// The portion already consumed, `string[0..cursor]`. Java: `getRead`.
    pub fn get_read(&self) -> &str {
        &self.string[..self.byte]
    }

    /// The portion not yet consumed. Java: `getRemaining`.
    pub fn remaining(&self) -> &str {
        &self.string[self.byte..]
    }

    /// Whether `length` more characters can be read. Java: `canRead(length)`.
    pub fn can_read_length(&self, length: usize) -> bool {
        self.cursor
            .checked_add(length)
            .is_some_and(|end| end <= self.len)
    }

    /// Whether there is at least one more character to read. Java: `canRead`.
    pub fn can_read(&self) -> bool {
        self.can_read_length(1)
    }

    /// The character at the cursor without consuming it.
    ///
    /// # Panics
    ///
    /// Panics if nothing is left to read; gate with [`Self::can_read`] first.
    /// Java throws `StringIndexOutOfBoundsException` in the same situation.
    pub fn peek(&self) -> char {
        self.string[self.byte..]
            .chars()
            .next()
            .expect("peeked past the end of the input; check can_read() first")
    }

    /// The character `offset` positions past the cursor, without consuming.
    /// Java: `peek(offset)`.
    ///
    /// # Panics
    ///
    /// Panics if `offset` reaches past the end of the input. Java throws
    /// `StringIndexOutOfBoundsException` in the same situation.
    pub fn peek_offset(&self, offset: usize) -> char {
        self.string[self.byte..]
            .chars()
            .nth(offset)
            .expect("peeked past the end of the input; check can_read_length() first")
    }

    /// The current cursor position, in characters. Java: `getCursor`.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Move the cursor to `pos` characters into the input. Java:
    /// `setCursor`. Java accepts out-of-range values and only fails later on
    /// use; this does the same, with positions past the end saturating all
    /// length/slicing operations instead of throwing.
    ///
    /// Re-deriving the byte offset of `pos` is O(`pos`); the parser hot path
    /// moves the cursor with [`Self::skip`]/[`Self::read`] instead, which are
    /// O(1).
    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos;
        self.byte = char_to_byte(&self.string, pos);
    }

    /// Consume and return the character at the cursor. Java: `read`.
    ///
    /// # Panics
    ///
    /// Panics if nothing is left to read (via [`Self::peek`]). Java throws
    /// `StringIndexOutOfBoundsException` in the same situation.
    pub fn read(&mut self) -> char {
        let c = self.peek();
        self.skip();
        c
    }

    /// Consume one character, discarding it. Java: `skip`. Unlike Java this
    /// does not lose track of the input when skipping past the end: the
    /// cursor may exceed the length, and the length/slicing operations then
    /// saturate instead of throwing `StringIndexOutOfBoundsException`.
    pub fn skip(&mut self) {
        if let Some(c) = self.string[self.byte..].chars().next() {
            self.byte += c.len_utf8();
        }
        self.cursor += 1;
    }

    /// Whether `c` may appear in a number: ASCII digits, `.` and `-`.
    /// Java: `isAllowedNumber`.
    pub fn is_allowed_number(c: char) -> bool {
        c.is_ascii_digit() || c == '.' || c == '-'
    }

    /// Whether `c` opens a quoted string: `"` or `'`. Java: `isQuotedStringStart`.
    pub fn is_quoted_string_start(c: char) -> bool {
        c == SYNTAX_DOUBLE_QUOTE || c == SYNTAX_SINGLE_QUOTE
    }

    /// Consume whitespace until the next non-whitespace character.
    /// Java: `skipWhitespace`.
    ///
    /// Whitespace is Java's exact `Character.isWhitespace` set, reproduced
    /// range for range (see `java_is_whitespace`) — not Rust's
    /// `char::is_whitespace`, which would additionally swallow the
    /// non-breaking spaces U+00A0/U+2007/U+202F and NEL U+0085 and would
    /// miss the C0 separators U+001C..U+001F. In Java an NBSP is ordinary
    /// token material, so `parse` treats `foo\u{00A0}bar` as two separate
    /// words; this port now does too.
    pub fn skip_whitespace(&mut self) {
        while self.can_read() && Self::java_is_whitespace(self.peek()) {
            self.skip();
        }
    }

    /// Java's `Character.isWhitespace`, as an exact code point set.
    ///
    /// Deliberately *not* `char::is_whitespace` (the Unicode `White_Space`
    /// property): Java's set differs on eight code points. Java skips the C0
    /// separators U+001C..U+001F but not the non-breaking spaces U+00A0
    /// (no-break space), U+2007 (figure space), U+202F (narrow no-break
    /// space) or NEL U+0085; the Unicode property does the opposite. The
    /// ranges below were dumped exhaustively (0..=0x10FFFF) from the live
    /// JDK 21 and pinned by an exhaustive test, so any future Unicode drift
    /// on either side fails loudly.
    fn java_is_whitespace(c: char) -> bool {
        matches!(
            c,
            '\u{0009}'..='\u{000D}'
                | '\u{001C}'..='\u{0020}'
                | '\u{1680}'
                | '\u{2000}'..='\u{2006}'
                | '\u{2008}'..='\u{200A}'
                | '\u{2028}'..='\u{2029}'
                | '\u{205F}'
                | '\u{3000}'
        )
    }

    /// Read an `i32`, restoring the cursor on failure. Errors are the Java
    /// `ReaderExpectedInt` / `ReaderInvalidInt` messages. Java: `readInt`.
    pub fn read_int(&mut self) -> Result<i32, CommandSyntaxException> {
        let (start, start_byte) = (self.cursor, self.byte);
        while self.can_read() && StringReader::is_allowed_number(self.peek()) {
            self.skip();
        }
        let number = &self.string[start_byte..self.byte];
        if number.is_empty() {
            return Err(built_in_exceptions()
                .reader_expected_int()
                .create_with_context(&*self));
        }
        match i32::from_str(number) {
            Ok(result) => Ok(result),
            Err(_) => {
                self.cursor = start;
                self.byte = start_byte;
                Err(built_in_exceptions()
                    .reader_invalid_int()
                    .create_with_context(&*self, number.to_owned()))
            }
        }
    }

    /// Read an `i64` the same way [`Self::read_int`] reads an `i32`.
    /// Java: `readLong`.
    pub fn read_long(&mut self) -> Result<i64, CommandSyntaxException> {
        let (start, start_byte) = (self.cursor, self.byte);
        while self.can_read() && StringReader::is_allowed_number(self.peek()) {
            self.skip();
        }
        let number = &self.string[start_byte..self.byte];
        if number.is_empty() {
            return Err(built_in_exceptions()
                .reader_expected_long()
                .create_with_context(&*self));
        }
        match i64::from_str(number) {
            Ok(result) => Ok(result),
            Err(_) => {
                self.cursor = start;
                self.byte = start_byte;
                Err(built_in_exceptions()
                    .reader_invalid_long()
                    .create_with_context(&*self, number.to_owned()))
            }
        }
    }

    /// Read an `f64` the same way [`Self::read_int`] reads an `i32`.
    /// Java: `readDouble`.
    pub fn read_double(&mut self) -> Result<f64, CommandSyntaxException> {
        let (start, start_byte) = (self.cursor, self.byte);
        while self.can_read() && StringReader::is_allowed_number(self.peek()) {
            self.skip();
        }
        let number = &self.string[start_byte..self.byte];
        if number.is_empty() {
            return Err(built_in_exceptions()
                .reader_expected_double()
                .create_with_context(&*self));
        }
        match f64::from_str(number) {
            Ok(result) => Ok(result),
            Err(_) => {
                self.cursor = start;
                self.byte = start_byte;
                Err(built_in_exceptions()
                    .reader_invalid_double()
                    .create_with_context(&*self, number.to_owned()))
            }
        }
    }

    /// Read an `f32` the same way [`Self::read_int`] reads an `i32`.
    /// Java: `readFloat`.
    pub fn read_float(&mut self) -> Result<f32, CommandSyntaxException> {
        let (start, start_byte) = (self.cursor, self.byte);
        while self.can_read() && StringReader::is_allowed_number(self.peek()) {
            self.skip();
        }
        let number = &self.string[start_byte..self.byte];
        if number.is_empty() {
            return Err(built_in_exceptions()
                .reader_expected_float()
                .create_with_context(&*self));
        }
        match f32::from_str(number) {
            Ok(result) => Ok(result),
            Err(_) => {
                self.cursor = start;
                self.byte = start_byte;
                Err(built_in_exceptions()
                    .reader_invalid_float()
                    .create_with_context(&*self, number.to_owned()))
            }
        }
    }

    /// Whether `c` may appear in an unquoted string: ASCII alphanumeric plus
    /// `_`, `-`, `.`, `+`. Java: `isAllowedInUnquotedString`.
    pub fn is_allowed_in_unquoted_string(c: char) -> bool {
        c.is_ascii_digit()
            || c.is_ascii_uppercase()
            || c.is_ascii_lowercase()
            || c == '_'
            || c == '-'
            || c == '.'
            || c == '+'
    }

    /// Consume and return the longest run of allowed unquoted characters.
    /// Java: `readUnquotedString`.
    pub fn read_unquoted_string(&mut self) -> &str {
        let start_byte = self.byte;
        while self.can_read() && StringReader::is_allowed_in_unquoted_string(self.peek()) {
            self.skip();
        }
        &self.string[start_byte..self.byte]
    }

    /// Read a quoted string. If the next character is not a quote, this
    /// errors with `Expected quote to start a string`; at end of input it
    /// returns an empty string, matching Java. Java: `readQuotedString`.
    pub fn read_quoted_string(&mut self) -> Result<String, CommandSyntaxException> {
        if !self.can_read() {
            return Ok(String::new());
        }
        let next = self.peek();
        if !StringReader::is_quoted_string_start(next) {
            return Err(built_in_exceptions()
                .reader_expected_start_of_quote()
                .create_with_context(&*self));
        }
        self.skip();
        self.read_string_until(next)
    }

    /// Consume characters up to (and including) `terminator`, handling `\`
    /// escapes. Only `terminator` and `\` itself may be escaped; anything
    /// else after a backslash is the Java `Invalid escape sequence` error,
    /// reported at the offending character. Java: `readStringUntil`.
    pub fn read_string_until(
        &mut self,
        terminator: char,
    ) -> Result<String, CommandSyntaxException> {
        let mut result = String::new();
        let mut escaped = false;
        while self.can_read() {
            let c = self.read();
            if escaped {
                if c == terminator || c == SYNTAX_ESCAPE {
                    result.push(c);
                    escaped = false;
                } else {
                    // step back over the offending char so the error points
                    // at it, like Java's setCursor(getCursor() - 1)
                    self.cursor -= 1;
                    self.byte -= c.len_utf8();
                    return Err(built_in_exceptions()
                        .reader_invalid_escape()
                        .create_with_context(&*self, c));
                }
            } else if c == SYNTAX_ESCAPE {
                escaped = true;
            } else if c == terminator {
                return Ok(result);
            } else {
                result.push(c);
            }
        }

        Err(built_in_exceptions()
            .reader_expected_end_of_quote()
            .create_with_context(&*self))
    }

    /// Read either a quoted string or an unquoted string, depending on what
    /// comes next. Java: `readString`.
    pub fn read_string(&mut self) -> Result<String, CommandSyntaxException> {
        if !self.can_read() {
            return Ok(String::new());
        }
        let next = self.peek();
        if StringReader::is_quoted_string_start(next) {
            self.skip();
            return self.read_string_until(next);
        }
        Ok(self.read_unquoted_string().to_owned())
    }

    /// Read `true` or `false`, restoring the cursor on anything else.
    /// Java: `readBoolean`.
    pub fn read_boolean(&mut self) -> Result<bool, CommandSyntaxException> {
        let (start, start_byte) = (self.cursor, self.byte);
        let value = self.read_string()?;
        if value.is_empty() {
            return Err(built_in_exceptions()
                .reader_expected_bool()
                .create_with_context(&*self));
        }

        match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => {
                self.cursor = start;
                self.byte = start_byte;
                Err(built_in_exceptions()
                    .reader_invalid_bool()
                    .create_with_context(&*self, value))
            }
        }
    }

    /// Require the exact character `c` at the cursor and consume it.
    /// Java: `expect(char)`.
    pub fn expect(&mut self, c: char) -> Result<(), CommandSyntaxException> {
        if !self.can_read() || self.peek() != c {
            return Err(built_in_exceptions()
                .reader_expected_symbol()
                .create_with_context(&*self, c));
        }
        self.skip();
        Ok(())
    }
}

/// The read-only view of a reader. Java:
/// `com.mojang.brigadier.ImmutableStringReader` — the interface the
/// `createWithContext(reader, ...)` constructors accept, so they can be fed
/// anything cursor-shaped without granting mutation. [`StringReader`]
/// implements it; a custom read-only view only needs these ten methods.
pub trait ImmutableStringReader {
    /// The whole input this reader works on. Java: `getString()`.
    fn string(&self) -> &str;
    /// How much is left to read. Java: `getRemainingLength()`.
    fn remaining_length(&self) -> usize;
    /// The full input length. Java: `getTotalLength()`.
    fn total_length(&self) -> usize;
    /// The current cursor position. Java: `getCursor()`.
    fn cursor(&self) -> usize;
    /// The portion already consumed. Java: `getRead()`.
    fn get_read(&self) -> &str;
    /// The portion not yet consumed. Java: `getRemaining()`.
    fn remaining(&self) -> &str;
    /// Whether `length` more characters can be read. Java: `canRead(int)`.
    fn can_read_length(&self, length: usize) -> bool;
    /// Whether there is at least one more character. Java: `canRead()`.
    fn can_read(&self) -> bool;
    /// The character at the cursor, without consuming. Java: `peek()`.
    ///
    /// # Panics
    ///
    /// Same contract as [`StringReader::peek`]: panics past the end of the
    /// input (Java throws `StringIndexOutOfBoundsException`).
    fn peek(&self) -> char;
    /// The character `offset` positions past the cursor. Java: `peek(int)`.
    ///
    /// # Panics
    ///
    /// Same contract as [`StringReader::peek_offset`]: panics past the end
    /// (Java throws `StringIndexOutOfBoundsException`).
    fn peek_offset(&self, offset: usize) -> char;
}

impl ImmutableStringReader for StringReader {
    fn string(&self) -> &str {
        StringReader::string(self)
    }
    fn remaining_length(&self) -> usize {
        StringReader::remaining_length(self)
    }
    fn total_length(&self) -> usize {
        StringReader::total_length(self)
    }
    fn cursor(&self) -> usize {
        StringReader::cursor(self)
    }
    fn get_read(&self) -> &str {
        StringReader::get_read(self)
    }
    fn remaining(&self) -> &str {
        StringReader::remaining(self)
    }
    fn can_read_length(&self, length: usize) -> bool {
        StringReader::can_read_length(self, length)
    }
    fn can_read(&self) -> bool {
        StringReader::can_read(self)
    }
    fn peek(&self) -> char {
        StringReader::peek(self)
    }
    fn peek_offset(&self, offset: usize) -> char {
        StringReader::peek_offset(self, offset)
    }
}
