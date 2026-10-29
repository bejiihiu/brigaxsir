//! The required-argument builder, mirroring Java's
//! `com.mojang.brigadier.builder.RequiredArgumentBuilder`.
//!
//! Java subclasses the abstract `ArgumentBuilder` to carry the argument name
//! and type; the port's shared builder ([`ArgumentBuilder`]) is generic over
//! that kind data, so this concrete builder is a type alias with `K` pinned
//! to the argument payload. The shared fluent surface (`then`, `executes`,
//! `requires`, `redirect`, ...) comes from there.

use std::sync::Arc;

use parking_lot::RwLock;

use super::argument_builder::{ArgumentBuilder, IntoCommandNode};
use crate::{
    arguments::ArgumentType,
    suggestion::SuggestionProvider,
    tree::{ArgumentCommandNode, CommandNode, CommandNodeKind},
};

/// A builder for a required-argument node. Java: `RequiredArgumentBuilder<S,
/// T>`.
///
/// The Java `T` (the parsed value's Java type) is dropped on purpose: the
/// port's argument types are type-erased (`Arc<dyn ArgumentType>` parsing
/// into `Arc<dyn ParsedValue>`), so a phantom `T` would force type
/// annotations onto every builder chain for no observable effect. Typed
/// reads go through
/// [`CommandContext::get_argument`](crate::context::CommandContext::get_argument).
pub type RequiredArgumentBuilder<S, R = i32> = ArgumentBuilder<S, R, ArgumentCommandNode<S, R>>;

/// Shortcut for creating a new argument builder. Java:
/// `RequiredArgumentBuilder.argument(name, type)`.
///
/// The node reads from `name` in the input and stores the parsed value under
/// that name; typed getters (`get_integer`, `get_string`, ...) read it back.
///
/// ```
/// # use brigaxsir::prelude::*;
/// # let mut subject = CommandDispatcher::<()>::new();
/// subject.register(
///     literal("echo")
///         .then(argument("value", integer()).executes(|ctx| get_integer(ctx, "value"))),
/// );
/// assert_eq!(subject.execute("echo 42", ()).unwrap(), 42);
/// ```
pub fn argument<S, R>(
    name: &str,
    parser: impl ArgumentType + Send + Sync + 'static,
) -> RequiredArgumentBuilder<S, R> {
    ArgumentBuilder::new(ArgumentCommandNode::new(name, Arc::new(parser), None))
}

impl<S, R> ArgumentBuilder<S, R, ArgumentCommandNode<S, R>> {
    /// Attach a custom suggestion provider to this argument node; it replaces
    /// the argument type's own suggestions. Java:
    /// `RequiredArgumentBuilder#suggests(SuggestionProvider)`.
    pub fn suggests<P>(mut self, provider: P) -> Self
    where
        P: SuggestionProvider<S, R> + Send + Sync + 'static,
    {
        self.kind.custom_suggestions = Some(Arc::new(provider));
        self
    }

    /// The custom suggestion provider set with [`Self::suggests`], if any.
    /// Java: `RequiredArgumentBuilder#getSuggestionsProvider`.
    pub fn get_suggestions_provider(
        &self,
    ) -> Option<&Arc<dyn SuggestionProvider<S, R> + Send + Sync>> {
        self.kind.get_custom_suggestions()
    }

    /// The argument name being built. Java: `RequiredArgumentBuilder#getName`.
    pub fn get_name(&self) -> &str {
        self.kind.get_name()
    }

    /// The argument parser being built. Java: `RequiredArgumentBuilder#getType`
    /// returning the `ArgumentType<T>`; the type is erased behind the same
    /// `Arc` the tree stores, so identity (`Arc::ptr_eq`) survives.
    pub fn get_type(&self) -> &Arc<dyn ArgumentType + Send + Sync> {
        self.kind.get_type()
    }

    /// Build into an argument node, copying the collected children in
    /// registration order (Java: `build()` looping `addChild` over the
    /// container's merged children).
    /// Java: `RequiredArgumentBuilder#build`.
    pub fn build(self) -> CommandNode<S, R> {
        let mut result = CommandNode::new(
            CommandNodeKind::Argument(self.kind),
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

impl<S, R> IntoCommandNode<S, R> for ArgumentBuilder<S, R, ArgumentCommandNode<S, R>> {
    fn into_node(self) -> Arc<RwLock<CommandNode<S, R>>> {
        Arc::new(RwLock::new(self.build()))
    }
}
