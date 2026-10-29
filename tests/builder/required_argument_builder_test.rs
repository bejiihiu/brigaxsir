//! Port of Java's `builder/RequiredArgumentBuilderTest`.

use std::sync::Arc;

use brigaxsir::{
    context::{CommandContext, CommandContextBuilder},
    prelude::*,
    tree::CommandNode,
};

/// A bare context to invoke a stored command through (Java's test compares
/// the Mockito mock's identity; closures have no identity in Rust, so the
/// port proves the wiring by running the command).
fn dummy_context() -> CommandContext<(), i32> {
    let dispatcher = CommandDispatcher::<(), i32>::new();
    CommandContextBuilder::new(&dispatcher, Arc::new(()), dispatcher.get_root(), 0).build("")
}

#[test]
fn test_build() {
    let node: CommandNode<(), i32> = argument("foo", integer()).build();

    assert_eq!(node.get_name(), "foo");
    // Java: `node.getType() is type` — the mocked ArgumentType's identity.
    // The built-in integer type has no identity to preserve (a fresh value
    // per factory call), so the port compares the parser through the erased
    // equality hook instead.
    assert!(node.get_type().type_equals(&integer()));
}

#[test]
fn test_build_with_executor() {
    let node: CommandNode<(), i32> = argument("foo", integer()).executes(|_ctx| 42).build();

    assert_eq!(node.get_name(), "foo");
    assert!(node.get_type().type_equals(&integer()));
    // Java: `node.getCommand() is command` — the mock's object identity. The
    // builder wraps the closure in its own `Arc`, so identity doesn't exist
    // here; running the command proves it landed on the node instead.
    let command = node.get_command().as_ref().unwrap();
    assert_eq!(command(&dummy_context()).unwrap(), 42);
}

#[test]
fn test_build_with_children() {
    let node: CommandNode<(), i32> = argument("foo", integer())
        .then(argument("bar", integer()))
        .then(argument("baz", integer()))
        .build();

    assert_eq!(node.get_children().len(), 2);
}
