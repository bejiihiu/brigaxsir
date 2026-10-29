//! The string argument type. Java:
//! `com.mojang.brigadier.arguments.StringArgumentType`.

use std::{any::Any, fmt, sync::Arc};

use super::ArgumentType;
use crate::{
    context::{CommandContext, ParsedValue},
    errors::CommandSyntaxException,
    string_reader::StringReader,
};

/// The three string-reading modes Java's `StringArgumentType` offers, each
/// with its own example values. Java: the `StringType` enum.
///
/// Java enums compare by constant identity, which for a fieldless enum is
/// exactly value equality — `PartialEq`/`Eq`/`Hash`/`Copy` are faithful.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StringType {
    /// Match up until the next space. Java: `SINGLE_WORD`.
    SingleWord,
    /// Same as single word unless the argument is wrapped in quotes, in which
    /// case it can contain spaces. Java: `QUOTABLE_PHRASE`.
    QuotablePhrase,
    /// Match the rest of the input. Java: `GREEDY_PHRASE`.
    GreedyPhrase,
}

impl StringType {
    /// The example values advertised for this mode, in Java's order. Java:
    /// `StringType.getExamples`.
    pub fn get_examples(&self) -> Vec<String> {
        match self {
            StringType::SingleWord => vec!["word", "words_with_underscores"],
            StringType::QuotablePhrase => vec!["\"quoted phrase\"", "word", "\"\""],
            StringType::GreedyPhrase => vec!["word", "words with spaces", "\"and symbols\""],
        }
        .into_iter()
        .map(|s| s.to_owned())
        .collect()
    }
}

/// An argument parsing a string in one of three modes. Java:
/// `StringArgumentType` — private field and constructor, public static
/// factories. Construct it through [`word`], [`string`] or
/// [`greedy_string`].
///
/// No [`PartialEq`]/[`Hash`]/`Eq`: Java's `StringArgumentType` does not
/// override `equals`/`hashCode`, so two `word()` calls are distinct objects
/// there. Value equality would silently diverge from that (e.g. dedup in
/// sets), so the port leaves it out on purpose. `Display` is Java's
/// `toString` — always `"string()"`, regardless of the mode.
#[derive(Clone, Copy, Debug)]
pub struct StringArgumentType {
    type_: StringType,
}

impl StringArgumentType {
    /// The mode this argument reads in. Java: `getType`.
    pub fn get_type(&self) -> StringType {
        self.type_
    }
}

impl ArgumentType for StringArgumentType {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
        let result = match self.type_ {
            StringType::SingleWord => reader.read_unquoted_string().to_owned(),
            StringType::QuotablePhrase => reader.read_string()?,
            StringType::GreedyPhrase => {
                let text = reader.remaining().to_owned();
                reader.set_cursor(reader.total_length());
                text
            }
        };
        Ok(Arc::new(result))
    }

    fn examples(&self) -> Vec<String> {
        self.type_.get_examples()
    }
}

impl fmt::Display for StringArgumentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Java hardcodes this for every mode — word() also prints "string()".
        write!(f, "string()")
    }
}

/// Match up until the next space. Java: `StringArgumentType.word()`.
pub fn word() -> StringArgumentType {
    StringArgumentType {
        type_: StringType::SingleWord,
    }
}

/// Same as single word unless the argument is wrapped in quotes, in which case
/// it can contain spaces. Java: `StringArgumentType.string()`.
pub fn string() -> StringArgumentType {
    StringArgumentType {
        type_: StringType::QuotablePhrase,
    }
}

/// Match the rest of the input. Java: `StringArgumentType.greedyString()`.
pub fn greedy_string() -> StringArgumentType {
    StringArgumentType {
        type_: StringType::GreedyPhrase,
    }
}

/// Wrap `input` in quotes unless it is already safe unquoted. Java:
/// `StringArgumentType.escapeIfRequired(String)` — a string needs quoting
/// when it contains any character `StringReader.isAllowedInUnquotedString`
/// rejects; otherwise it passes through unchanged.
pub fn escape_if_required(input: &str) -> String {
    if input
        .chars()
        .any(|c| !StringReader::is_allowed_in_unquoted_string(c))
    {
        escape(input)
    } else {
        input.to_owned()
    }
}

/// Quote and escape `input`: wrap it in double quotes, backslash-escaping
/// every `\` and `"` inside. Java: the private `StringArgumentType.escape`.
fn escape(input: &str) -> String {
    let mut result = String::with_capacity(input.len() + 2);
    result.push('"');
    for c in input.chars() {
        if c == '\\' || c == '"' {
            result.push('\\');
        }
        result.push(c);
    }
    result.push('"');
    result
}

/// Read a previously parsed string argument named `name` from `context`.
/// Java: `StringArgumentType.getString(context, name)`.
///
/// # Panics
///
/// Panics with Java's exact `IllegalArgumentException` messages from
/// `CommandContext#getArgument` when no argument named `name` was parsed
/// (`No such argument '<name>' exists on this command`) or when it holds a
/// different value type (`Argument '<name>' is defined as ..., not ...`).
/// Fallible callers: [`CommandContext::try_get_argument`].
pub fn get_string<S, R>(context: &CommandContext<S, R>, name: &str) -> String {
    context.get_argument::<String>(name).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_quotes_and_backslashes_like_java() {
        // oracle-verified against Java 21 via StringArgumentTypeTest
        assert_eq!(escape("hello \"world\"!"), "\"hello \\\"world\\\"!\"");
        assert_eq!(escape("\\"), "\"\\\\\"");
        assert_eq!(escape("\""), "\"\\\"\"");
    }
}
