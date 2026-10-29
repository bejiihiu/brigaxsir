//! The result of parsing: a context, the leftover reader state, and per-node
//! errors. Mirrors Java's `com.mojang.brigadier.ParseResults`.

use std::fmt::{self, Debug};

use indexmap::IndexMap;

use crate::{
    context::CommandContextBuilder, errors::CommandSyntaxException, string_reader::StringReader,
};

/// Everything [`CommandDispatcher`](crate::command_dispatcher::CommandDispatcher)
/// learned while parsing: the partially built context, the reader as it was
/// left, and the errors collected from candidate nodes (at most one of these
/// gets surfaced when executing, mirroring Java's behaviour of reporting the
/// single exception, or the generic unknown-command/unknown-argument error).
///
/// Java keeps the three fields private behind getters; here they are public,
/// which is also how the dispatcher constructs the struct. Java's two
/// constructors and three getters are mirrored as [`Self::new`],
/// [`Self::new_with_defaults`], [`Self::get_context`], [`Self::get_reader`]
/// and [`Self::get_exceptions`], so code ported from Java keeps its shape.
///
/// The struct is [`Clone`]: a clone is an independent snapshot of the same
/// parse, each clone executable on its own (Java has no `clone` method
/// because every Java reference already behaves like a shared handle; this
/// makes the re-execute-the-same-results pattern, which Java permits since
/// `execute` never mutates the results, expressible — including from
/// several threads at once).
pub struct ParseResults<'a, S, R> {
    /// The context built up during parsing. Java: the `context` field /
    /// `getContext()`.
    pub context: CommandContextBuilder<'a, S, R>,
    /// The reader as parsing left it; its cursor marks how far the best match
    /// got. An empty remainder means the whole input was consumed. Java:
    /// the `reader` field / `getReader()`.
    pub reader: StringReader,
    /// Errors per candidate node. Java keys these by the `CommandNode`
    /// object in a `LinkedHashMap` — insertion order observable; every
    /// candidate comes from one parent's child map, so names are unique
    /// there and a name-keyed [`IndexMap`] produces the identical observable
    /// map (same size, same exceptions, same insertion order) without
    /// hashing a lock-protected recursive structure. Java: the `exceptions`
    /// field / `getExceptions()`.
    pub exceptions: IndexMap<String, CommandSyntaxException>,
}

impl<'a, S, R> ParseResults<'a, S, R> {
    /// The full constructor: a context, the reader as parsing left it, and
    /// the per-candidate errors. Java:
    /// `new ParseResults<>(context, reader, exceptions)`.
    pub fn new(
        context: CommandContextBuilder<'a, S, R>,
        reader: StringReader,
        exceptions: IndexMap<String, CommandSyntaxException>,
    ) -> Self {
        Self {
            context,
            reader,
            exceptions,
        }
    }

    /// The convenience constructor: just a context, with an empty reader and
    /// no per-node errors. Java: `new ParseResults<>(context)` (which fills
    /// in `new StringReader("")` and an empty map).
    pub fn new_with_defaults(context: CommandContextBuilder<'a, S, R>) -> Self {
        Self::new(context, StringReader::from(""), IndexMap::new())
    }

    /// The context built up during parsing. Java: `getContext()`.
    pub fn get_context(&self) -> &CommandContextBuilder<'a, S, R> {
        &self.context
    }

    /// The reader as parsing left it. Java: `getReader()`, which returns the
    /// read-only [`ImmutableStringReader`](crate::string_reader::ImmutableStringReader)
    /// there. The concrete [`StringReader`] returned here implements that
    /// trait too, so read-only callers coerce the same way and callers that
    /// want the cursor get it.
    pub fn get_reader(&self) -> &StringReader {
        &self.reader
    }

    /// The collected per-candidate-node errors. Java: `getExceptions()`.
    pub fn get_exceptions(&self) -> &IndexMap<String, CommandSyntaxException> {
        &self.exceptions
    }
}

impl<S, R> Clone for ParseResults<'_, S, R> {
    // no `S`/`R` bounds: every field is shared handles or plain data, so a
    // clone of any parse is always available
    fn clone(&self) -> Self {
        Self {
            context: self.context.clone(),
            reader: self.reader.clone(),
            exceptions: self.exceptions.clone(),
        }
    }
}

impl<S, R> Debug for ParseResults<'_, S, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParseResults")
            .field("context", &self.context)
            .field("exceptions", &self.exceptions)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{command_dispatcher::CommandDispatcher, context::CommandContextBuilder};
    use std::sync::Arc;

    fn context<'a>(
        dispatcher: &'a CommandDispatcher<()>,
        start: usize,
    ) -> CommandContextBuilder<'a, (), i32> {
        CommandContextBuilder::new(dispatcher, Arc::new(()), dispatcher.get_root(), start)
    }

    #[test]
    fn new_mirrors_javas_full_constructor() {
        let dispatcher = CommandDispatcher::<()>::new();
        let reader = StringReader::from("hello");
        let results = ParseResults::new(context(&dispatcher, 0), reader, IndexMap::new());

        assert_eq!(results.get_reader().string(), "hello");
        assert_eq!(results.get_reader().cursor(), 0);
        assert!(results.get_exceptions().is_empty());
        assert_eq!(results.get_context().get_range().start(), 0);
    }

    #[test]
    fn new_with_defaults_mirrors_javas_single_arg_constructor() {
        // Java: new ParseResults<>(context) == empty reader + empty error map
        let dispatcher = CommandDispatcher::<()>::new();
        let results = ParseResults::new_with_defaults(context(&dispatcher, 3));

        assert_eq!(results.get_reader().string(), "");
        assert_eq!(results.get_reader().cursor(), 0);
        assert!(results.get_exceptions().is_empty());
        assert_eq!(results.get_context().get_range().start(), 3);
    }
}
