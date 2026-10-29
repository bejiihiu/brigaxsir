//! The literal payload of a command node: an exact keyword the input must
//! match. Java: `com.mojang.brigadier.tree.LiteralCommandNode` — there it is
//! a full node subclass; here the tree keeps one `CommandNode` type and this
//! struct is the data of the literal kind (see
//! [`CommandNodeKind`](crate::tree::CommandNodeKind)).

use crate::string_reader::StringReader;

/// The data of a literal node: the keyword as written plus a lowercase copy
/// cached for suggestion prefix checks (Java caches `literalLowerCase` the
/// same way).
#[derive(Clone, Debug)]
pub struct LiteralCommandNode {
    literal: String,
    literal_lower_case: String,
}

impl LiteralCommandNode {
    /// A literal payload for the keyword `literal`. Java:
    /// `LiteralCommandNode(literal, ...)` constructor.
    pub fn new(literal: &str) -> Self {
        Self {
            literal: literal.to_owned(),
            // Java: literal.toLowerCase(Locale.ROOT); Rust's to_lowercase is
            // the same full Unicode mapping (locale-independent).
            literal_lower_case: literal.to_lowercase(),
        }
    }

    /// The keyword this literal matches. Java: `getLiteral`.
    pub fn get_literal(&self) -> &str {
        &self.literal
    }

    /// The cached lowercase keyword, feeding suggestion prefix checks. Java
    /// keeps `literalLowerCase` private; this stays crate-internal too.
    pub(crate) fn literal_lower_case(&self) -> &str {
        &self.literal_lower_case
    }

    /// Consume the keyword from `reader` if it matches exactly and ends at a
    /// word boundary, returning the end cursor. Java: the private
    /// `parse(StringReader)` returning the end cursor or `-1`.
    ///
    /// The cursor is char-based (crate-wide convention), so `literal.length`
    /// is the keyword's char count and the prefix comparison runs over
    /// chars — byte-identical to Java for ASCII, the documented divergence
    /// for astral input.
    pub(crate) fn parse(&self, reader: &mut StringReader) -> Option<usize> {
        let start = reader.cursor();
        if !reader.can_read_length(self.literal.chars().count()) {
            return None;
        }
        let mut matched = true;
        for c in self.literal.chars() {
            if reader.peek() == c {
                reader.skip();
            } else {
                matched = false;
                break;
            }
        }
        if !matched {
            // no match: Java never moved the cursor in this branch, so land
            // back at the start either way
            reader.set_cursor(start);
            return None;
        }
        let end = reader.cursor();
        if !reader.can_read() || reader.peek() == ' ' {
            return Some(end);
        }
        // word matched but not at a boundary — rewind like Java's
        // setCursor(start)
        reader.set_cursor(start);
        None
    }

    /// Whether `input` is exactly this literal (with an optional trailing
    /// space). Java: `isValidInput` — `parse(new StringReader(input)) > -1`.
    pub(crate) fn is_valid_input(&self, input: &str) -> bool {
        self.parse(&mut StringReader::from(input)).is_some()
    }

    /// The literal itself as the only example. Java: `getExamples` returns
    /// `Collections.singleton(literal)`.
    pub(crate) fn examples(&self) -> Vec<String> {
        vec![self.literal.clone()]
    }

    /// Sort key for the Comparable ordering. Java: `getSortedKey` returns
    /// `literal`.
    pub(crate) fn sorted_key(&self) -> &str {
        &self.literal
    }
}

impl PartialEq for LiteralCommandNode {
    /// Java: `literal.equals(that.literal)` — case-sensitive.
    fn eq(&self, other: &Self) -> bool {
        self.literal == other.literal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_matches_whole_word_and_stops_at_boundary() {
        let mut reader = StringReader::from("foo bar");
        assert_eq!(LiteralCommandNode::new("foo").parse(&mut reader), Some(3));
        assert_eq!(reader.remaining(), " bar");
    }

    #[test]
    fn parse_rejects_longer_word_and_rewinds() {
        let mut reader = StringReader::from("foobar");
        assert_eq!(LiteralCommandNode::new("foo").parse(&mut reader), None);
        assert_eq!(reader.cursor(), 0);
    }

    #[test]
    fn literal_lower_case_is_cached_full_unicode() {
        let node = LiteralCommandNode::new("ПРИВЕТ");
        assert_eq!(node.literal_lower_case(), "привет");
    }
}
