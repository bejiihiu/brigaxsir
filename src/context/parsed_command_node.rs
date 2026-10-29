//! A node touched during parsing, with its input span. Java:
//! `com.mojang.brigadier.context.ParsedCommandNode`.

use std::{fmt, sync::Arc};

use parking_lot::RwLock;

use super::string_range::StringRange;
use crate::tree::CommandNode;

/// One entry of a context's node list: which tree node matched and the span
/// of input it consumed.
#[derive(Debug)]
pub struct ParsedCommandNode<S, R> {
    /// The node that matched.
    node: Arc<RwLock<CommandNode<S, R>>>,
    /// The span of input the node consumed.
    range: StringRange,
}

impl<S, R> Clone for ParsedCommandNode<S, R> {
    // manual: the derive would demand `S: Clone`/`R: Clone` although the
    // fields (an `Arc` handle and a `Copy` range) never do
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
            range: self.range,
        }
    }
}

impl<S, R> ParsedCommandNode<S, R> {
    /// Java: `ParsedCommandNode(node, range)`.
    pub fn new(node: Arc<RwLock<CommandNode<S, R>>>, range: StringRange) -> Self {
        Self { node, range }
    }

    /// The node that matched, as the shared `Arc` handle (Java: `getNode`).
    pub fn get_node(&self) -> Arc<RwLock<CommandNode<S, R>>> {
        self.node.clone()
    }

    /// The span of input the node consumed. Java: `getRange`.
    pub fn get_range(&self) -> StringRange {
        self.range
    }
}

impl<S, R> PartialEq for ParsedCommandNode<S, R> {
    /// Java: `Objects.equals(node, that.node) && Objects.equals(range,
    /// that.range)` — the node compares by value through
    /// [`CommandNode`]'s structural equality.
    fn eq(&self, other: &Self) -> bool {
        *self.node.read() == *other.node.read() && self.range == other.range
    }
}

impl<S, R> fmt::Display for ParsedCommandNode<S, R> {
    /// Java's `toString`: `node + "@" + range`. The node renders through its
    /// usage text (Java's per-subclass `toString` for the literal and
    /// argument kinds; parsed nodes never wrap the root).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.node.read().get_usage_text(), self.range)
    }
}
