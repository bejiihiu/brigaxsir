//! The collector used while producing suggestions. Java:
//! `com.mojang.brigadier.suggestion.SuggestionsBuilder`.

use std::sync::Arc;

use super::{IntegerSuggestion, Suggestion, Suggestions, SuggestionsFuture};
use crate::{
    context::StringRange,
    message::Message,
    string_reader::{char_len, char_to_byte},
};

/// Accumulates suggestions for the tail of the input starting at `start`.
/// Consumers call `suggest` for every candidate and finish with `build`.
///
/// `start` (like every position in this library) is a **character** index —
/// see `StringReader` for what that means. A lowercase copy of the input is
/// kept alongside the original because the library matches suggestion
/// prefixes case-insensitively, like Java's `remainingLowerCase` field; the
/// two are sliced independently, since lowercasing can change how many
/// characters a string has (same latent quirk as Java's builder).
///
/// Like Java, the collected suggestions live in a plain list — duplicates are
/// fine while collecting; `build` deduplicates.
///
/// ```
/// # use brigaxsir::suggestion::SuggestionsBuilder;
/// let suggestions = SuggestionsBuilder::new("f", 0)
///     .suggest("foo")
///     .suggest("bar")
///     .build();
/// let texts: Vec<&str> = suggestions.list().iter().map(|s| s.text()).collect();
/// // ordered case-insensitively, exactly like Java's `compareToIgnoreCase`
/// assert_eq!(texts, ["bar", "foo"]);
/// ```
#[derive(Debug, PartialEq)]
pub struct SuggestionsBuilder {
    input: String,
    input_lowercase: String,
    start: usize,
    remaining: String,
    remaining_lowercase: String,
    result: Vec<Suggestion>,
}

impl SuggestionsBuilder {
    /// A builder over `input`, with suggestions anchored at `start`.
    /// Java: `SuggestionsBuilder(input, start)` — which lowercases the input
    /// itself (Java with `Locale.ROOT`, Rust with `str::to_lowercase`; both
    /// apply the Unicode default case conversion).
    pub fn new(input: &str, start: usize) -> Self {
        Self::new_with_lowercase(input, input.to_lowercase().as_str(), start)
    }

    /// Like [`Self::new`], but with a precomputed lowercase input (the
    /// dispatcher already has one, so it is reused). Java carries the same
    /// pair of fields internally.
    pub fn new_with_lowercase(input: &str, input_lowercase: &str, start: usize) -> Self {
        Self {
            start,
            input: input.to_owned(),
            input_lowercase: input_lowercase.to_owned(),
            remaining: input[char_to_byte(input, start)..].to_owned(),
            remaining_lowercase: input_lowercase[char_to_byte(input_lowercase, start)..].to_owned(),
            result: Vec::new(),
        }
    }

    /// The full input being suggested for. Java: `getInput`.
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Where suggestions start replacing input. Java: `getStart`.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The input after `start`, i.e. what's being completed. Java:
    /// `getRemaining`.
    pub fn remaining(&self) -> &str {
        &self.remaining
    }

    /// Lowercase view of [`Self::remaining`], used for prefix matching.
    /// Java: `getRemainingLowerCase`.
    pub fn remaining_lowercase(&self) -> &str {
        &self.remaining_lowercase
    }

    /// Finish and produce the ordered, deduplicated suggestions.
    /// Java: `build`.
    pub fn build(&self) -> Suggestions {
        Suggestions::create(&self.input, &self.result)
    }

    /// [`Self::build`] wrapped in a completed future. Java: `buildFuture`
    /// (`CompletableFuture.completedFuture(build())`).
    pub fn build_future(&self) -> SuggestionsFuture {
        Suggestions::completed_future(self.build())
    }

    /// Suggest `text`, unless it equals the remaining input exactly (no-op,
    /// matching Java). Java: `suggest(String)`.
    pub fn suggest(mut self, text: impl Into<String>) -> Self {
        let text = text.into();
        if text == self.remaining {
            return self;
        }
        self.result.push(Suggestion::new(
            StringRange::between(self.start, char_len(&self.input)),
            text,
        ));
        self
    }

    /// Like [`Self::suggest`], with a tooltip. Java: `suggest(text, tooltip)`.
    pub fn suggest_with_tooltip(
        mut self,
        text: impl Into<String>,
        tooltip: Arc<dyn Message>,
    ) -> Self {
        let text = text.into();
        if text == self.remaining {
            return self;
        }
        self.result.push(Suggestion::new_with_tooltip(
            StringRange::between(self.start, char_len(&self.input)),
            text,
            tooltip,
        ));
        self
    }

    /// Suggest an integer value. Java: `suggest(int)` — note there is no
    /// remaining-input check for integers, matching Java.
    pub fn suggest_integer(mut self, value: i32) -> Self {
        self.result.push(Suggestion::from(IntegerSuggestion::new(
            StringRange::between(self.start, char_len(&self.input)),
            value,
        )));
        self
    }

    /// Like [`Self::suggest_integer`], with a tooltip. Java: `suggest(int,
    /// message)`.
    pub fn suggest_integer_with_tooltip(mut self, value: i32, tooltip: Arc<dyn Message>) -> Self {
        self.result
            .push(Suggestion::from(IntegerSuggestion::new_with_tooltip(
                StringRange::between(self.start, char_len(&self.input)),
                value,
                Some(tooltip),
            )));
        self
    }

    /// Pull every suggestion from `other` into this builder. Java: `add`.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, other: SuggestionsBuilder) -> Self {
        self.result.extend(other.result);
        self
    }

    /// A fresh builder over the same input but a new `start`. Java:
    /// `createOffset(start)`.
    pub fn create_offset(&self, start: usize) -> SuggestionsBuilder {
        SuggestionsBuilder::new_with_lowercase(&self.input, &self.input_lowercase, start)
    }

    /// Reset this builder to its original `start`, discarding suggestions.
    /// Java: `restart`.
    pub fn restart(&self) -> SuggestionsBuilder {
        self.create_offset(self.start)
    }
}
