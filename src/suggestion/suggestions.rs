//! A set of suggestions for one position in the input. Java:
//! `com.mojang.brigadier.suggestion.Suggestions`.

use super::{Suggestion, SuggestionsFuture, ordering};
use crate::{context::StringRange, errors::CommandSyntaxException};

/// The merged output of one or more suggestion providers: a common range and
/// the suggestions covering it, ordered the way Java's
/// `Suggestion#compareToIgnoreCase` ordering (over a `HashSet`-derived
/// sequence) produces.
///
/// The `EMPTY` singleton Java keeps around surfaces here as
/// [`Default::default`] and as [`Self::empty`] (the future-wrapped form Java
/// actually exposes).
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct Suggestions {
    range: StringRange,
    suggestions: Vec<Suggestion>,
}

impl Suggestions {
    /// A ready-made result. Java: `Suggestions(range, list)`.
    pub fn new(range: StringRange, suggestions: Vec<Suggestion>) -> Self {
        Self { range, suggestions }
    }

    /// An empty result wrapped in an already-completed future. Java:
    /// `Suggestions.empty()`, the default answer of argument types and
    /// literal nodes that have nothing to offer.
    pub fn empty() -> SuggestionsFuture {
        Self::completed_future(Self::default())
    }

    /// Wrap a ready result into a completed future, the way Java code writes
    /// `CompletableFuture.completedFuture(suggestions)` inside a provider.
    pub fn completed_future(suggestions: Suggestions) -> SuggestionsFuture {
        Box::pin(std::future::ready(Ok(suggestions)))
    }

    /// Wrap an error into a completed future, for providers that want to
    /// surface a [`CommandSyntaxException`] instead of suggestions.
    pub fn failed_future(error: CommandSyntaxException) -> SuggestionsFuture {
        Box::pin(std::future::ready(Err(error)))
    }

    /// Merge suggestion sets from several nodes into one, matching Java:
    /// empty input collapses to nothing, a single set is reused as-is, and
    /// multiple sets get their suggestions widened to a common range, then
    /// deduplicated and ordered. Java: `Suggestions.merge(command, input)`.
    ///
    /// Java feeds the concatenated lists through a `HashSet` before creating
    /// the result, and that set's iteration order is what the sort sees — it
    /// is reproduced exactly (see the `ordering` module docs).
    pub fn merge(command: &str, input: &[Suggestions]) -> Self {
        if input.is_empty() {
            return Suggestions::default();
        } else if input.len() == 1 {
            return input[0].clone();
        }

        let combined = input
            .iter()
            .flat_map(|suggestions| suggestions.list().iter().cloned());
        Suggestions::create(command, &ordering::java_set_order(combined))
    }

    /// Build the final result from raw suggestions: widen each one to the
    /// range that covers all of them, deduplicate, and order like Java.
    /// Java: `Suggestions.create(command, suggestions)`.
    ///
    /// The slice is treated as the collection's iteration order; the
    /// expanded suggestions go through the Java-`HashSet` order emulation
    /// before sorting, so the outcome matches Java's `create` for any input
    /// order.
    pub fn create(command: &str, suggestions: &[Suggestion]) -> Self {
        if suggestions.is_empty() {
            return Suggestions::default();
        }
        let mut start = usize::MAX;
        let mut end = 0usize;
        for suggestion in suggestions {
            start = suggestion.range().start().min(start);
            end = suggestion.range().end().max(end);
        }
        let range = StringRange::new(start, end);
        let expanded =
            ordering::java_set_order(suggestions.iter().map(|s| s.expand(command, range)));
        let mut sorted = expanded;
        ordering::java_sort_ignore_case(&mut sorted);
        Suggestions {
            range,
            suggestions: sorted,
        }
    }

    /// Whether there is nothing to suggest. Java: `isEmpty`.
    pub fn is_empty(&self) -> bool {
        self.suggestions.is_empty()
    }

    /// The suggestions, in final order. Java: `getList`.
    pub fn list(&self) -> &[Suggestion] {
        &self.suggestions
    }

    /// The input span the suggestions apply to. Java: `getRange`.
    pub fn range(&self) -> StringRange {
        self.range
    }
}
