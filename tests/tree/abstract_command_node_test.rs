//! The shared part of Java's `tree` test package:
//! `AbstractCommandNodeTest` runs its four tests once per node kind, so the
//! Rust port keeps them as functions the three concrete test files invoke
//! with their own node factory.

use std::sync::Arc;

use brigaxsir::{
    builder::literal_argument_builder::literal,
    context::CommandContext,
    errors::CommandSyntaxException,
    tree::{Command, CommandNode, CommandNodeKind, LiteralCommandNode},
};
use parking_lot::RwLock;

/// Java's tests use `Object` as the command source; the unit type stands in.
pub(crate) type Source = ();
pub(crate) type SharedNode = Arc<RwLock<CommandNode<Source, i32>>>;
/// The pre-wrapped command type, for tests that need one command object
/// shared between several nodes (Java's `@Mock Command`).
pub(crate) type SharedCommand =
    Arc<dyn Fn(&CommandContext<Source, i32>) -> Result<i32, CommandSyntaxException> + Send + Sync>;

/// A literal node carrying an explicitly shared command. Java's tests build
/// one mock `Command` and hand it to several builders; `executes` wraps each
/// closure in its own `Arc`, so the mock is constructed directly — the
/// Mockito-mock-to-fake translation.
pub(crate) fn literal_with_command(literal: &str, command: Command<Source, i32>) -> SharedNode {
    Arc::new(RwLock::new(CommandNode::new(
        CommandNodeKind::Literal(LiteralCommandNode::new(literal)),
        command,
        Arc::new(|_| true),
        None,
        None,
        false,
    )))
}

/// Java: `AbstractCommandNodeTest#testAddChild`.
pub(crate) fn run_test_add_child(get_command_node: fn() -> SharedNode) {
    let node = get_command_node();

    node.write()
        .add_child(&Arc::new(RwLock::new(literal("child1").build())));
    node.write()
        .add_child(&Arc::new(RwLock::new(literal("child2").build())));
    node.write()
        .add_child(&Arc::new(RwLock::new(literal("child1").build())));

    assert_eq!(node.read().get_children().len(), 2);
}

/// Java: `AbstractCommandNodeTest#testAddChildMergesGrandchildren`.
pub(crate) fn run_test_add_child_merges_grandchildren(get_command_node: fn() -> SharedNode) {
    let node = get_command_node();

    node.write().add_child(&Arc::new(RwLock::new(
        literal("child").then(literal("grandchild1")).build(),
    )));
    node.write().add_child(&Arc::new(RwLock::new(
        literal("child").then(literal("grandchild2")).build(),
    )));

    assert_eq!(node.read().get_children().len(), 1);
    assert_eq!(node.read().get_children()[0].read().get_children().len(), 2);
}

/// Java: `AbstractCommandNodeTest#testAddChildPreservesCommand`.
pub(crate) fn run_test_add_child_preserves_command(get_command_node: fn() -> SharedNode) {
    let node = get_command_node();

    let command: SharedCommand = Arc::new(|_| Ok(42));
    node.write()
        .add_child(&literal_with_command("child", Some(command.clone())));
    node.write().add_child(&literal_with_command("child", None));

    // the merge keeps the first node's command — the exact same object, like
    // Java's `is(command)` on the mock
    assert!(Arc::ptr_eq(
        node.read().get_children()[0]
            .read()
            .get_command()
            .as_ref()
            .unwrap(),
        &command
    ));
}

/// Java: `AbstractCommandNodeTest#testAddChildOverwritesCommand`.
pub(crate) fn run_test_add_child_overwrites_command(get_command_node: fn() -> SharedNode) {
    let node = get_command_node();

    let command: SharedCommand = Arc::new(|_| Ok(42));
    node.write().add_child(&literal_with_command("child", None));
    node.write()
        .add_child(&literal_with_command("child", Some(command.clone())));

    // the merge takes the second node's command — again the same object
    assert!(Arc::ptr_eq(
        node.read().get_children()[0]
            .read()
            .get_command()
            .as_ref()
            .unwrap(),
        &command
    ));
}
