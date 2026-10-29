//! The trait for custom suggestion sources. Java:
//! `com.mojang.brigadier.suggestion.SuggestionProvider`.

use super::{SuggestionsBuilder, SuggestionsFuture};
use crate::context::CommandContext;

/// Produces suggestions for an argument node, given the parse context and a
/// builder seeded with the remaining input. Java:
/// `SuggestionProvider#getSuggestions(context, builder)` returning a
/// `CompletableFuture<Suggestions>` that may throw `CommandSyntaxException`.
///
/// A sync provider returns [`Suggestions::completed_future`] (or
/// `builder.build_future()`); a genuinely async one returns any boxed future:
///
/// ```text
/// Box::pin(async move { Ok(builder.suggest("later").build()) })
/// ```
///
/// The error case is `Suggestions::failed_future(error)`. Errors surface
/// when the future is polled (see [`SuggestionsFuture`]); sync callers that
/// never await only ever see the completed form.
///
/// [`Suggestions::completed_future`]:
///     crate::suggestion::Suggestions::completed_future
/// [`Suggestions::failed_future`]:
///     crate::suggestion::Suggestions::failed_future
pub trait SuggestionProvider<S, R> {
    /// Produce suggestions for the argument the provider is attached to,
    /// seeded with the parse context and the builder covering the remaining
    /// input. Java: `getSuggestions(CommandContext<S>, SuggestionsBuilder)`.
    ///
    /// Returning
    /// [`Suggestions::completed_future`](crate::suggestion::Suggestions::completed_future)
    /// (or `builder.build_future()`) makes the provider synchronous; anything
    /// async goes through the same boxed-future type, and its `Err` surfaces
    /// when the future is polled.
    fn get_suggestions(
        &self,
        context: CommandContext<S, R>,
        builder: SuggestionsBuilder,
    ) -> SuggestionsFuture;
}
