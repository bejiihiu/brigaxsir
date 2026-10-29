//! The literal builder, mirroring Java's
//! `com.mojang.brigadier.builder.LiteralArgumentBuilder`.
//!
//! Java subclasses the abstract `ArgumentBuilder` to carry the literal
//! keyword; the port's shared builder ([`ArgumentBuilder`]) is generic over
//! that kind data, so this concrete builder is a type alias with `K` pinned
//! to the literal payload. The shared fluent surface (`then`, `executes`,
//! `requires`, `redirect`, ...) comes from there.

use std::sync::Arc;

use parking_lot::RwLock;

use super::argument_builder::{ArgumentBuilder, IntoCommandNode};
use crate::tree::{CommandNode, CommandNodeKind, LiteralCommandNode};

/// A builder for a literal keyword node. Java:
/// `LiteralArgumentBuilder<S>`.
pub type LiteralArgumentBuilder<S, R = i32> = ArgumentBuilder<S, R, LiteralCommandNode>;

/// Shortcut for creating a new literal builder. Java:
/// `LiteralArgumentBuilder.literal(name)`.
///
/// The literal matches the same word in the input (case-insensitively for
/// suggestion purposes) and contributes no parsed argument.
///
/// ```
/// # use brigaxsir::prelude::*;
/// # let mut subject = CommandDispatcher::<()>::new();
/// subject.register(literal("foo").executes(|_| 42));
/// ```
pub fn literal<S, R>(value: &str) -> LiteralArgumentBuilder<S, R> {
    ArgumentBuilder::new(LiteralCommandNode::new(value))
}

impl<S, R> ArgumentBuilder<S, R, LiteralCommandNode> {
    /// The keyword being built. Java: `LiteralArgumentBuilder#getLiteral`.
    pub fn get_literal(&self) -> &str {
        self.kind.get_literal()
    }

    /// Build into a literal node, copying the collected children in
    /// registration order (Java: `build()` looping `addChild` over the
    /// container's merged children).
    /// Java: `LiteralArgumentBuilder#build`.
    pub fn build(self) -> CommandNode<S, R> {
        let mut result = CommandNode::new(
            CommandNodeKind::Literal(self.kind),
            self.command,
            self.requirement,
            self.target,
            self.modifier,
            self.forks,
        );

        for child in self.arguments.read().get_children() {
            result.add_child(&child);
        }

        result
    }
}

impl<S, R> IntoCommandNode<S, R> for ArgumentBuilder<S, R, LiteralCommandNode> {
    fn into_node(self) -> Arc<RwLock<CommandNode<S, R>>> {
        Arc::new(RwLock::new(self.build()))
    }
}
