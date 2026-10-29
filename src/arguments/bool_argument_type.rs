//! The boolean argument type. Java: `com.mojang.brigadier.arguments.BoolArgumentType`.

use std::{any::Any, sync::Arc};

use super::ArgumentType;
use crate::{
    context::{CommandContext, ParsedValue},
    errors::CommandSyntaxException,
    string_reader::StringReader,
    suggestion::{SuggestionsBuilder, SuggestionsFuture},
};

/// An argument parsing `true` or `false`. Java: `BoolArgumentType` — private
/// constructor, the `bool()` factory is the only way to build one.
///
/// No [`PartialEq`]/`Hash`/`Display`: Java's `BoolArgumentType` overrides
/// none of `equals`/`hashCode`/`toString`, so instances there compare and
/// print by identity. Adding value equality or a made-up `Display` string
/// would silently diverge, so the port leaves all three out on purpose.
#[derive(Clone, Copy, Debug, Default)]
pub struct BoolArgumentType;

impl ArgumentType for BoolArgumentType {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
        Ok(Arc::new(reader.read_boolean()?))
    }

    fn list_suggestions(&self, mut builder: SuggestionsBuilder) -> SuggestionsFuture {
        if "true".starts_with(builder.remaining_lowercase()) {
            builder = builder.suggest("true");
        }
        if "false".starts_with(builder.remaining_lowercase()) {
            builder = builder.suggest("false");
        }
        builder.build_future()
    }

    fn examples(&self) -> Vec<String> {
        EXAMPLES.iter().map(|s| s.to_string()).collect()
    }
}

/// The shared example list. Java: the `EXAMPLES` constant.
const EXAMPLES: [&str; 2] = ["true", "false"];

/// An argument parsing `true` or `false`. Java: `BoolArgumentType.bool()`.
pub fn bool() -> BoolArgumentType {
    BoolArgumentType
}

/// Read a previously parsed bool argument named `name` from `context`.
/// Java: `BoolArgumentType.getBool(context, name)`.
///
/// # Panics
///
/// Panics with Java's exact `IllegalArgumentException` messages from
/// `CommandContext#getArgument` when no argument named `name` was parsed
/// (`No such argument '<name>' exists on this command`) or when it holds a
/// different value type (`Argument '<name>' is defined as ..., not ...`).
/// Fallible callers: [`CommandContext::try_get_argument`].
pub fn get_bool<S, R>(context: &CommandContext<S, R>, name: &str) -> bool {
    *context.get_argument::<bool>(name)
}
