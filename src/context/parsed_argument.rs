//! A single parsed argument: the span of input it came from plus the
//! type-erased parse result. Java: `com.mojang.brigadier.context.ParsedArgument`.

use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

use super::{ParsedValue, string_range::StringRange};

/// One successfully parsed argument. The result is stored as
/// `Arc<dyn ParsedValue>` — type-erased, but self-describing (Java's
/// `ParsedArgument<S, T>` keeps the value type as a generic; the crate needs
/// one heterogeneous map per context, so the type lives inside the value).
#[derive(Clone)]
pub struct ParsedArgument {
    /// Where in the input this argument was read from.
    range: StringRange,
    /// The parsed value, type-erased.
    result: Arc<dyn ParsedValue>,
}

impl ParsedArgument {
    /// Java: `ParsedArgument(int start, int end, T result)`.
    pub fn new<V: ParsedValue>(start: usize, end: usize, result: V) -> Self {
        Self {
            range: StringRange::between(start, end),
            result: Arc::new(result),
        }
    }

    /// The erased-value companion of [`Self::new`], for results that already
    /// went through
    /// [`ArgumentType::parse`](crate::arguments::ArgumentType::parse) — the
    /// generic `T` is gone by then, so the pre-erased handle is stored as is.
    pub fn new_erased(start: usize, end: usize, result: Arc<dyn ParsedValue>) -> Self {
        Self {
            range: StringRange::between(start, end),
            result,
        }
    }

    /// Where in the input this argument was read from. Java: `getRange`.
    pub fn get_range(&self) -> StringRange {
        self.range
    }

    /// The parsed value, type-erased. Java: `getResult` — there it is the
    /// typed `T`; here downcast through [`ParsedValue::as_any`], or use
    /// the typed accessors (`get_integer`, `get_string`, ...) or
    /// [`CommandContext::get_argument`](crate::context::CommandContext::get_argument)
    /// directly.
    pub fn get_result(&self) -> &dyn ParsedValue {
        self.result.as_ref()
    }
}

impl PartialEq for ParsedArgument {
    /// Java: `Objects.equals(range, that.range) && Objects.equals(result,
    /// that.result)` — the value comparison rides the erased-value hook
    /// ([`ParsedValue::value_eq`]).
    fn eq(&self, other: &Self) -> bool {
        self.range == other.range && self.result.value_eq(other.result.as_ref())
    }
}

impl Eq for ParsedArgument {}

impl Hash for ParsedArgument {
    /// Java: `Objects.hash(range, result)`.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.range.hash(state);
        self.result.value_hash(state);
    }
}

impl std::fmt::Debug for ParsedArgument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ParsedArgument")
            .field("range", &self.range)
            .field("result_type", &self.result.type_name())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_mismatch_message_names_both_types() {
        // pins the names `try_get_argument` renders into the Java message
        let argument = ParsedArgument::new(0, 1, 123);
        assert_eq!(argument.get_result().type_name(), "Integer");
    }
}
