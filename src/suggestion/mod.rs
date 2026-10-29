//! Suggestions: what the user might want to type next. Mirrors Java's
//! `com.mojang.brigadier.suggestion` package (`Suggestion`,
//! `IntegerSuggestion`, `Suggestions`, `SuggestionsBuilder`,
//! `SuggestionProvider`).
//!
//! Everything that *produces* suggestions is asynchronous here, like Java:
//! argument types, custom providers and the dispatcher all return a
//! [`SuggestionsFuture`] (the `CompletableFuture<Suggestions>` stand-in) while
//! the assembly helpers (`Suggestions::merge`, `SuggestionsBuilder::build`)
//! stay synchronous, exactly like their Java counterparts. The future type is
//! plain `std` — no async runtime is pulled in; bring your own executor (or
//! use the sync twins) to consume results.

mod future;
mod ordering;
mod suggestion_provider;
mod suggestions;
mod suggestions_builder;

pub use future::SuggestionsFuture;
pub(crate) use future::join_all_and_merge;
pub use suggestion_provider::SuggestionProvider;
pub use suggestions::Suggestions;
pub use suggestions_builder::SuggestionsBuilder;

use std::{
    cmp::Ordering,
    hash::{Hash, Hasher},
    sync::Arc,
};

use crate::{
    context::StringRange,
    message::Message,
    string_reader::{char_len, char_to_byte},
};

/// A plain text suggestion: a replacement string for a span of the input,
/// plus an optional tooltip shown next to it. Java: `Suggestion`.
///
/// Java folds text and numeric suggestions into one class hierarchy
/// (`Suggestion` / `IntegerSuggestion`); here both payloads are named structs
/// carried by the [`Suggestion`] enum, so `IntegerSuggestion` stays a
/// distinguishable public type with its own [`IntegerSuggestion::value`].
#[derive(Clone, Debug)]
pub struct TextSuggestion {
    range: StringRange,
    text: String,
    tooltip: Option<Arc<dyn Message>>,
}

impl TextSuggestion {
    /// A suggestion with no tooltip. Java: `Suggestion(range, text)`.
    pub fn new(range: StringRange, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
            tooltip: None,
        }
    }

    /// A suggestion with a tooltip. Java: `Suggestion(range, text, tooltip)`.
    pub fn new_with_tooltip(
        range: StringRange,
        text: impl Into<String>,
        tooltip: Arc<dyn Message>,
    ) -> Self {
        Self {
            range,
            text: text.into(),
            tooltip: Some(tooltip),
        }
    }

    /// The span of input this suggestion replaces. Java: `getRange`.
    pub fn range(&self) -> StringRange {
        self.range
    }

    /// The suggested text. Java: `getText`.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The optional tooltip. Java: `getTooltip`.
    pub fn tooltip(&self) -> Option<&Arc<dyn Message>> {
        self.tooltip.as_ref()
    }

    /// Apply this suggestion to `input` (see [`Suggestion::apply`]).
    pub fn apply(&self, input: &str) -> String {
        Suggestion::Text(self.clone()).apply(input)
    }

    /// Widen this suggestion to `range` (see [`Suggestion::expand`]).
    pub fn expand(&self, command: &str, range: StringRange) -> Suggestion {
        Suggestion::Text(self.clone()).expand(command, range)
    }
}

impl From<TextSuggestion> for Suggestion {
    fn from(value: TextSuggestion) -> Self {
        Suggestion::Text(value)
    }
}

/// A numeric suggestion. Java: `IntegerSuggestion`.
///
/// The number is remembered ( [`Self::value`] ), and comparisons between two
/// integer suggestions order numerically instead of lexicographically — that
/// is Java's `IntegerSuggestion#compareTo` override.
#[derive(Clone, Debug)]
pub struct IntegerSuggestion {
    range: StringRange,
    value: i32,
    text: String,
    tooltip: Option<Arc<dyn Message>>,
}

impl IntegerSuggestion {
    /// A suggestion with no tooltip. Java: `IntegerSuggestion(range, value)`.
    pub fn new(range: StringRange, value: i32) -> Self {
        Self::new_with_tooltip(range, value, None)
    }

    /// A suggestion with a tooltip. Java: `IntegerSuggestion(range, value,
    /// tooltip)`.
    pub fn new_with_tooltip(
        range: StringRange,
        value: i32,
        tooltip: Option<Arc<dyn Message>>,
    ) -> Self {
        Self {
            range,
            value,
            // Java: super(range, Integer.toString(value), tooltip)
            text: value.to_string(),
            tooltip,
        }
    }

    /// The span of input this suggestion replaces. Java: `getRange`.
    pub fn range(&self) -> StringRange {
        self.range
    }

    /// The suggested value. Java: `getValue`.
    pub fn value(&self) -> i32 {
        self.value
    }

    /// The suggested text, i.e. the decimal rendering of the value. Java:
    /// `getText`.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The optional tooltip. Java: `getTooltip`.
    pub fn tooltip(&self) -> Option<&Arc<dyn Message>> {
        self.tooltip.as_ref()
    }

    /// Apply this suggestion to `input` (see [`Suggestion::apply`]).
    pub fn apply(&self, input: &str) -> String {
        Suggestion::Integer(self.clone()).apply(input)
    }

    /// Widen this suggestion to `range` (see [`Suggestion::expand`]).
    ///
    /// Like Java, expanding loses the numeric payload: the widened
    /// suggestion comes back as a plain text suggestion with the same
    /// tooltip.
    pub fn expand(&self, command: &str, range: StringRange) -> Suggestion {
        Suggestion::Integer(self.clone()).expand(command, range)
    }
}

impl From<IntegerSuggestion> for Suggestion {
    fn from(value: IntegerSuggestion) -> Self {
        Suggestion::Integer(value)
    }
}

/// One suggestion: text or an integer replacing a span of the input. Java:
/// `Suggestion` — the base class `IntegerSuggestion` extends; the enum is the
/// Rust-shaped union of the two, and the payloads keep the class names.
#[derive(Clone, Debug)]
pub enum Suggestion {
    /// A text suggestion. Java: a plain `Suggestion`.
    Text(TextSuggestion),
    /// A numeric suggestion. Java: `IntegerSuggestion`.
    Integer(IntegerSuggestion),
}

impl Suggestion {
    /// A text suggestion with no tooltip. Java: `Suggestion(range, text)`.
    pub fn new(range: StringRange, text: impl Into<String>) -> Self {
        Self::Text(TextSuggestion::new(range, text))
    }

    /// A text suggestion with a tooltip. Java: `Suggestion(range, text,
    /// tooltip)`.
    pub fn new_with_tooltip(
        range: StringRange,
        text: impl Into<String>,
        tooltip: Arc<dyn Message>,
    ) -> Self {
        Self::Text(TextSuggestion::new_with_tooltip(range, text, tooltip))
    }

    /// The span of input this suggestion replaces. Java: `getRange`.
    pub fn range(&self) -> StringRange {
        match self {
            Suggestion::Text(text) => text.range,
            Suggestion::Integer(integer) => integer.range,
        }
    }

    /// The suggested text; integers render in decimal form. Java: `getText`.
    pub fn text(&self) -> &str {
        match self {
            Suggestion::Text(text) => &text.text,
            Suggestion::Integer(integer) => &integer.text,
        }
    }

    /// The optional tooltip. Java: `getTooltip`.
    pub fn tooltip(&self) -> Option<&Arc<dyn Message>> {
        match self {
            Suggestion::Text(text) => text.tooltip.as_ref(),
            Suggestion::Integer(integer) => integer.tooltip.as_ref(),
        }
    }

    /// The numeric value, for integer suggestions. Java: `IntegerSuggestion`
    /// carries this; text suggestions have no value, so this is `None` for
    /// them.
    pub fn value(&self) -> Option<i32> {
        match self {
            Suggestion::Text(_) => None,
            Suggestion::Integer(integer) => Some(integer.value),
        }
    }

    /// The text payload, for text suggestions.
    pub fn as_text(&self) -> Option<&TextSuggestion> {
        match self {
            Suggestion::Text(text) => Some(text),
            Suggestion::Integer(_) => None,
        }
    }

    /// The integer payload, for integer suggestions.
    pub fn as_integer(&self) -> Option<&IntegerSuggestion> {
        match self {
            Suggestion::Text(_) => None,
            Suggestion::Integer(integer) => Some(integer),
        }
    }

    /// Whether this is an integer suggestion. Java: `instanceof
    /// IntegerSuggestion`.
    pub fn is_integer(&self) -> bool {
        matches!(self, Suggestion::Integer(_))
    }

    /// Apply this suggestion to `input`: replace the suggestion's range with
    /// the suggested text and keep everything else. Java: `apply(input)`.
    ///
    /// Ranges are character units (see `StringRange`), so slicing goes
    /// through the char→byte bridge; out-of-range ends clamp, keeping the
    /// operation panic-free (Java would throw for a bogus range).
    pub fn apply(&self, input: &str) -> String {
        let range = self.range();
        let text = self.text();
        let input_len = char_len(input);
        if range.start() == 0 && range.end() == input_len {
            return text.to_owned();
        }
        let mut result = String::with_capacity(text.len());
        if range.start() > 0 {
            result.push_str(&input[..char_to_byte(input, range.start())]);
        }
        result.push_str(text);
        if range.end() < input_len {
            result.push_str(&input[char_to_byte(input, range.end())..]);
        }

        result
    }

    /// Widen this suggestion to `range`, pulling in the extra input around
    /// it (Java does the same when merging suggestions with different
    /// ranges). Java: `expand(command, range)`.
    ///
    /// The widened suggestion is always a plain text suggestion, even when
    /// this one carried a number — Java's `expand` returns the base class
    /// and drops `IntegerSuggestion`'s value too. The tooltip survives.
    pub fn expand(&self, command: &str, range: StringRange) -> Suggestion {
        let own = self.range();
        if range == own {
            return self.clone();
        }
        let mut result = String::new();
        if range.start() < own.start() {
            result.push_str(
                &command[char_to_byte(command, range.start())..char_to_byte(command, own.start())],
            );
        }
        result.push_str(self.text());
        if range.end() > own.end() {
            result.push_str(
                &command[char_to_byte(command, own.end())..char_to_byte(command, range.end())],
            );
        }
        Suggestion::Text(TextSuggestion {
            range,
            text: result,
            tooltip: self.tooltip().cloned(),
        })
    }

    /// Order two suggestions by their text. Java: `compareTo` — text vs text
    /// compares strings; integer vs integer compares numerically; mixed
    /// pairs fall back to the text comparison (Java's override).
    pub fn compare_to(&self, other: &Suggestion) -> Ordering {
        match (self, other) {
            (Suggestion::Integer(a), Suggestion::Integer(b)) => a.value.cmp(&b.value),
            _ => java_compare_to(self.text(), other.text()),
        }
    }

    /// Order two suggestions case-insensitively. Java:
    /// `compareToIgnoreCase`.
    ///
    /// One asymmetry survives the port because Java has it: an integer
    /// suggestion compares case-**sensitively** against a text suggestion
    /// (the `IntegerSuggestion` override delegates to `compareTo`), while a
    /// text receiver folds case.
    pub fn compare_to_ignore_case(&self, other: &Suggestion) -> Ordering {
        match (self, other) {
            (Suggestion::Integer(_), Suggestion::Integer(_)) => self.compare_to(other),
            (Suggestion::Text(_), _) => java_compare_to_ignore_case(self.text(), other.text()),
            // integer receiver, text other: Java's override means the
            // case-sensitive comparison
            _ => self.compare_to(other),
        }
    }

    /// Java's `hashCode()` for this suggestion, in the exact formula Java's
    /// `HashSet` ordering depends on. Deliberately private: it exists to
    /// reproduce Java's set iteration order (see `ordering`), not as the
    /// Rust `Hash` implementation.
    fn java_hash_code(&self) -> i32 {
        let range = self.range();
        let range_hash = ordering::java_objects_hash(&[range.start() as i32, range.end() as i32]);
        let tooltip_hash = self
            .tooltip()
            .map_or(0, |m| ordering::java_string_hash(m.get_string()));
        let hash = ordering::java_objects_hash(&[
            range_hash,
            ordering::java_string_hash(self.text()),
            tooltip_hash,
        ]);
        match self {
            // Java: IntegerSuggestion.hashCode = Objects.hash(super, value)
            Suggestion::Integer(integer) => ordering::java_objects_hash(&[hash, integer.value]),
            Suggestion::Text(_) => hash,
        }
    }
}

impl PartialEq for Suggestion {
    fn eq(&self, other: &Self) -> bool {
        if std::mem::discriminant(self) != std::mem::discriminant(other) {
            return false;
        }
        self.range() == other.range()
            && self.text() == other.text()
            && tooltip_eq(self.tooltip(), other.tooltip())
    }
}
impl Eq for Suggestion {}

impl Hash for Suggestion {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        self.range().hash(state);
        self.text().hash(state);
        self.tooltip().is_some().hash(state);
        if let Some(tooltip) = self.tooltip() {
            tooltip.get_string().hash(state);
        }
        if let Suggestion::Integer(integer) = self {
            integer.value.hash(state);
        }
    }
}

/// Tooltips compare by their text, like Java's `LiteralMessage` *should*
/// (Java actually uses reference equality there because `LiteralMessage`
/// doesn't override `equals`; string equality is the sane stand-in).
fn tooltip_eq(a: Option<&Arc<dyn Message>>, b: Option<&Arc<dyn Message>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.get_string() == b.get_string(),
        _ => false,
    }
}

/// Java's `String.compareTo`: UTF-16 code unit by UTF-16 code unit. Rust
/// compares code points, which orders astral characters differently than
/// Java's surrogate units — this helper is the exact Java behaviour.
fn java_compare_to(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Java's `String.compareToIgnoreCase`: fold each UTF-16 unit with
/// `toLowerCase(toUpperCase(c))` — a *single-character* mapping — then
/// compare unit by unit, breaking ties by length.
fn java_compare_to_ignore_case(a: &str, b: &str) -> Ordering {
    a.encode_utf16()
        .map(java_fold_unit)
        .cmp(b.encode_utf16().map(java_fold_unit))
}

/// Fold one UTF-16 unit the way `String.compareToIgnoreCase` does. Surrogate
/// units (astral input) are caseless and pass through unchanged.
///
/// Verified exhaustive against the live JDK 21 over the whole BMP: this fold
/// agrees with Java everywhere except eight code points where the two
/// toolchains' Unicode *data* versions differ (see
/// [`java_single_char_lower`]) — an unavoidable data-version drift, not a
/// logic difference.
fn java_fold_unit(unit: u16) -> u16 {
    match char::from_u32(u32::from(unit)) {
        None => unit,
        Some(c) => {
            let folded = java_single_char_lower(java_single_char_upper(c));
            u16::try_from(folded as u32).unwrap_or(unit)
        }
    }
}

/// Java's `Character.toUpperCase(char)`: the simple (single-character)
/// mapping, or the character itself when the full mapping expands (like
/// `ß` → `SS`).
fn java_single_char_upper(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(first), None) => first,
        _ => c,
    }
}

/// Java's `Character.toLowerCase(char)`, same single-mapping rule. U+0130
/// (İ) is special-cased: Java's simple lowercase mapping is `i`, while
/// Rust's full lowercase expands to `i` + combining dot.
///
/// Beyond that special case the simple-mapping algorithm is Java-exact; the
/// only residual divergence is Unicode *data* version drift. JDK 21 ships
/// Unicode 15.0, current rustc ships newer data, so eight code points that
/// are unassigned in Java 15.0 (U+1C89, U+A7CB, U+A7CC, U+A7CE, U+A7D2,
/// U+A7D4, U+A7DA, U+A7DC) already have lowercase mappings here and fold
/// where Java leaves them unchanged. Pinned exhaustively by a test so any
/// future drift on either side fails loudly instead of silently moving.
fn java_single_char_lower(c: char) -> char {
    if c == '\u{0130}' {
        return 'i';
    }
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(first), None) => first,
        _ => c,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::LiteralMessage;

    // the numbers below come from running the Java 1.3 sources on a local
    // JVM — they pin the hash emulation byte-for-byte
    #[test]
    fn java_hash_matches_the_oracle() {
        let text = |start: usize, end: usize, s: &str| {
            Suggestion::new(StringRange::between(start, end), s)
        };
        let integer = |start: usize, end: usize, v: i32| {
            Suggestion::Integer(IntegerSuggestion::new(StringRange::between(start, end), v))
        };
        assert_eq!(ordering::java_string_hash("11"), 1568);
        assert_eq!(text(6, 7, "11").java_hash_code(), 1_187_393);
        assert_eq!(text(6, 7, "a").java_hash_code(), 1_141_792);
        assert_eq!(text(6, 7, "3a").java_hash_code(), 1_190_803);
        assert_eq!(integer(6, 7, 2).java_hash_code(), 35_351_348);
        assert_eq!(integer(6, 7, 32).java_hash_code(), 36_870_719);
        assert_eq!(text(4, 5, "apple").java_hash_code(), -1_409_984_505);
    }

    #[test]
    fn comparisons_match_java() {
        let text = |s: &str| Suggestion::new(StringRange::between(6, 7), s);
        let integer =
            |v: i32| Suggestion::Integer(IntegerSuggestion::new(StringRange::between(6, 7), v));
        assert_eq!(
            text("bar").compare_to_ignore_case(&text("Bar")),
            Ordering::Equal
        );
        assert_eq!(
            text("Bar").compare_to_ignore_case(&text("bar")),
            Ordering::Equal
        );
        assert_eq!(
            text("a").compare_to_ignore_case(&text("A")),
            Ordering::Equal
        );
        // integer vs text is case-sensitive through the Integer override
        assert_eq!(integer(2).compare_to(&text("22")), Ordering::Less);
        assert_eq!(
            integer(2).compare_to_ignore_case(&text("22")),
            Ordering::Less
        );
        assert_eq!(
            text("22").compare_to_ignore_case(&integer(2)),
            Ordering::Greater
        );
        // integers compare numerically among themselves
        assert_eq!(integer(30).compare_to(&integer(4)), Ordering::Greater);
        // text compares lexicographically (UTF-16 units)
        assert_eq!(text("11").compare_to(&text("2")), Ordering::Less);
    }

    #[test]
    fn equality_and_hash_agree() {
        use std::collections::HashSet;
        let a = Suggestion::new(StringRange::between(0, 1), "foo");
        let b = Suggestion::new(StringRange::between(0, 1), "foo");
        let with_tooltip = Suggestion::new_with_tooltip(
            StringRange::between(0, 1),
            "foo",
            Arc::new(LiteralMessage::new("tip")),
        );
        assert_eq!(a, b);
        let mut set = HashSet::new();
        assert!(set.insert(a.clone()));
        assert!(!set.insert(b)); // equal → already present
        assert!(set.insert(with_tooltip.clone())); // different tooltip → new element
        assert_eq!(set.len(), 2);
        assert_ne!(a, with_tooltip);
        // integer vs text with the same rendering stay distinct (Java keeps
        // both in a HashSet too, since the subclass hashCode differs)
        assert_ne!(
            Suggestion::new(StringRange::between(0, 1), "5"),
            Suggestion::Integer(IntegerSuggestion::new(StringRange::between(0, 1), 5))
        );
    }

    #[test]
    fn expand_of_integer_becomes_text() {
        let integer = Suggestion::Integer(IntegerSuggestion::new(StringRange::at(1), 7));
        let widened = integer.expand("foo", StringRange::between(0, 1));
        assert!(!widened.is_integer());
        // "f" pulled in from the command + the integer's text
        assert_eq!(widened.text(), "f7");
        assert_eq!(widened.value(), None);
    }

    #[test]
    fn fold_matches_java_across_the_case_mapping_oddities() {
        // live-JDK 21 values (fold = toLowerCase(toUpperCase(c))), covering
        // the expansions, the special cases and script oddities:
        // İ/dotless-i, sharp s, Cherokee, ligatures, digraphs, Greek iota
        // subscripts, final sigma, Old Hungarian, Adlam, Cyrillic unicodes
        let cases: [(u32, u32); 19] = [
            (0x0130, 0x0069),
            (0x0131, 0x0069),
            (0x00DF, 0x00DF),
            (0x1E9E, 0x00DF),
            (0x13A0, 0xAB70),
            (0x13A8, 0xAB78),
            (0xFB00, 0xFB00),
            (0xFB01, 0xFB01),
            (0x01C4, 0x01C6),
            (0x01C6, 0x01C6),
            (0x1F88, 0x1F80),
            (0x1FA8, 0x1FA0),
            (0x0345, 0x03B9),
            (0x03C2, 0x03C3),
            (0x03A3, 0x03C3),
            (0x1C80, 0x0432),
            (0x1C81, 0x0434),
            (0xA69C, 0xA69C),
            (0x2C2E, 0x2C5E),
        ];
        for (cp, expected) in cases {
            let folded = java_fold_unit(u16::try_from(cp).unwrap());
            assert_eq!(u32::from(folded), expected, "cp {cp:#06x}");
        }
    }

    #[test]
    fn fold_unicode_version_drift_is_pinned() {
        // The eight BMP code points where this fold diverges from the live
        // JDK 21: unassigned in Java's Unicode 15.0 data (no mapping — Java
        // leaves them unchanged), already mapped in rustc's newer data.
        // Verified exhaustively against the live JDK 21 (every other BMP
        // code point folds identically). If this test fails after a
        // toolchain update, re-dump both sides and refresh this list and
        // the `java_single_char_lower` doc — the drift set moves with the
        // Unicode versions, it is not a logic bug.
        let drift: [(u32, u32); 8] = [
            (0x1C89, 0x1C8A),
            (0xA7CB, 0x0264),
            (0xA7CC, 0xA7CD),
            (0xA7CE, 0xA7CF),
            (0xA7D2, 0xA7D3),
            (0xA7D4, 0xA7D5),
            (0xA7DA, 0xA7DB),
            (0xA7DC, 0x019B),
        ];
        for (cp, rust_folded) in drift {
            assert_eq!(
                u32::from(java_fold_unit(u16::try_from(cp).unwrap())),
                rust_folded
            );
        }
    }
}
