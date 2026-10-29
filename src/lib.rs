#![doc = include_str!("../README.md")]
// every public item is documented (task-18 audit) — keep it that way. This
// lives here rather than in the Cargo.toml [lints] table so it only governs
// the library target, not benches/tests where criterion's macro output and
// integration-test crates are undocumented by nature.
#![deny(missing_docs)]

pub mod ambiguity_consumer;
pub mod arguments;
pub mod builder;
pub mod command_dispatcher;
pub mod context;
pub mod errors;
pub mod message;
pub mod modifier;
pub mod parse_results;
pub mod result_consumer;
pub mod string_reader;
pub mod suggestion;
pub mod tree;

/// Everything a typical command needs, one glob import away: the built-in
/// argument types and their getters, the `literal`/`argument` builders, the
/// dispatcher, the error surface for custom commands and custom argument
/// types, the reader and suggestion-authoring types, and the typed context
/// reads. More specialized items (the tree internals, the context-builder
/// family, the ordering machinery, `Dynamic2/3/4/N` exceptions) live in
/// their own modules and are imported by path.
///
/// ```
/// # use brigaxsir::prelude::*;
/// # use std::sync::Arc;
/// # struct Session;
/// let mut dispatcher = CommandDispatcher::<Session>::new();
/// let denied = Arc::new(SimpleCommandExceptionType::new(Arc::new(LiteralMessage::new(
///     "Insufficient permission",
/// ))));
/// dispatcher.register(
///     literal("give")
///         .then(argument("amount", integer_range(1, 100)).executes_result(move |ctx| {
///             if *ctx.get_argument::<i32>("amount") > 32 {
///                 Err(denied.create())
///             } else {
///                 Ok(SINGLE_SUCCESS)
///             }
///         })),
/// );
///
/// assert_eq!(dispatcher.execute("give 8", Session).unwrap(), 1);
/// let error = dispatcher.execute("give 99", Session).unwrap_err();
/// assert_eq!(error.get_raw_message().get_string(), "Insufficient permission");
/// ```
pub mod prelude {
    pub use crate::{
        arguments::{
            ArgumentType,
            bool_argument_type::{BoolArgumentType, bool, get_bool},
            double_argument_type::{
                DoubleArgumentType, double, double_min, double_range, get_double,
            },
            float_argument_type::{FloatArgumentType, float, float_min, float_range, get_float},
            integer_argument_type::{
                IntegerArgumentType, get_integer, integer, integer_min, integer_range,
            },
            long_argument_type::{LongArgumentType, get_long, long, long_min, long_range},
            string_argument_type::{
                StringArgumentType, StringType, escape_if_required, get_string, greedy_string,
                string, word,
            },
        },
        builder::{argument, literal},
        command_dispatcher::CommandDispatcher,
        context::{ArgumentError, CommandContext, ParsedValue},
        errors::{
            CommandExceptionType, CommandSyntaxException, DynamicCommandExceptionType,
            SimpleCommandExceptionType,
        },
        message::{LiteralMessage, Message},
        modifier::{RedirectModifier, SingleRedirectModifier},
        parse_results::ParseResults,
        result_consumer::ResultConsumer,
        string_reader::StringReader,
        suggestion::{SuggestionProvider, Suggestions, SuggestionsBuilder, SuggestionsFuture},
        tree::{Command, CommandNode, SINGLE_SUCCESS},
    };
}
