//! Where a suggestion request is anchored in the tree. Java:
//! `com.mojang.brigadier.context.SuggestionContext`.

use std::sync::Arc;

use parking_lot::RwLock;

use super::command_context_builder::CommandContextBuilder;
use crate::tree::CommandNode;

/// The node whose children should be suggested at a given cursor, the
/// context builder at the level that node was found at, and the input
/// position suggestions start replacing from. Java: `SuggestionContext<T>`
/// with `context`/`parent` and `startPos`.
///
/// The context is borrowed (Java holds a reference too); the borrow lives as
/// long as the builder chain it came from.
#[derive(Debug)]
pub struct SuggestionContext<'b, 'a, S, R> {
    /// The context builder at the level the parent node was found at; the
    /// dispatcher builds the suggestion request from it. Java: the `context`
    /// field.
    pub context: &'b CommandContextBuilder<'a, S, R>,
    /// The node providing the suggestions (usually the last node parsed
    /// before the cursor, or the root when nothing matched yet). Java: the
    /// `parent` field.
    pub parent: Arc<RwLock<CommandNode<S, R>>>,
    /// The input position suggestions begin at. Java: `startPos`.
    pub start_pos: usize,
}
