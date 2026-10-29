//! Port of Java's `builder/ArgumentBuilderTest` — the shared builder surface
//! exercised on a bare builder, standing in for Java's
//! `TestableArgumentBuilder<S> extends ArgumentBuilder<S, ...>` subclass
//! (whose `build()` is never called).

use std::sync::Arc;

use brigaxsir::{
    builder::argument_builder::ArgumentBuilder,
    context::{CommandContext, CommandContextBuilder},
    prelude::*,
    tree::CommandNode,
};
use parking_lot::RwLock;

/// Java's `TestableArgumentBuilder<S>`: a builder with no node kind of its
/// own — only the shared surface is under test and `build` is never called.
/// The port's shared struct takes any kind data, so a bare unit struct plays
/// the subclass.
struct TestKind;

type TestableBuilder = ArgumentBuilder<(), i32, TestKind>;

/// A stand-in for the Mockito `CommandNode` mocks: any node handle works as
/// a redirect target.
fn target_node() -> Arc<RwLock<CommandNode<(), i32>>> {
    Arc::new(RwLock::new(literal("target").build()))
}

/// A bare context to invoke a stored command or modifier through (Java's
/// tests compare object identity; closures have none in Rust, so the port
/// proves the wiring by running it). The source is a parameter so identity
/// checks against it can pass.
fn dummy_context(source: Arc<()>) -> CommandContext<(), i32> {
    let dispatcher = CommandDispatcher::<(), i32>::new();
    CommandContextBuilder::new(&dispatcher, source, dispatcher.get_root(), 0).build("")
}

#[test]
fn test_arguments() {
    let builder: TestableBuilder = ArgumentBuilder::new(TestKind);

    let argument = argument("bar", integer());
    let builder = builder.then(argument.clone());

    assert_eq!(builder.get_arguments().len(), 1);
    // Java: `hasItem(argument.build())` — the builder is not consumed there,
    // it builds a second node and compares structurally; the clone keeps the
    // original alive for the same comparison here
    let built = argument.build();
    assert!(builder.get_arguments().iter().any(|e| *e.read() == built));
}

#[test]
fn test_redirect() {
    let target = target_node();
    let builder: TestableBuilder = ArgumentBuilder::new(TestKind);
    let builder = builder.redirect(target.clone());

    assert!(Arc::ptr_eq(
        builder.get_redirect().as_ref().unwrap(),
        &target
    ));
    // Java's plain redirect stores no modifier
    assert!(builder.get_redirect_modifier().is_none());
    assert!(!builder.is_fork());
}

#[test]
#[should_panic(expected = "Cannot forward a node with children")]
fn test_redirect_with_child() {
    let target = target_node();
    let builder: TestableBuilder = ArgumentBuilder::new(TestKind);
    let builder = builder.then(literal("foo"));
    let _ = builder.redirect(target);
}

#[test]
#[should_panic(expected = "Cannot add children to a redirected node")]
fn test_then_with_redirect() {
    let target = target_node();
    let builder: TestableBuilder = ArgumentBuilder::new(TestKind);
    let builder = builder.redirect(target);
    let _ = builder.then(literal("foo"));
}

/// Not in Java's test file — a pin for the singleton wrapping the port adds
/// around `SingleRedirectModifier` (Java's `Collections.singleton(...)`).
#[test]
fn redirect_with_wraps_the_modifier_to_a_single_source() {
    let target = target_node();
    let source = Arc::new(());
    let builder: TestableBuilder = ArgumentBuilder::new(TestKind);
    let builder = builder.redirect_with(
        target,
        Arc::new(|ctx: &CommandContext<(), i32>| Ok(ctx.get_source_arc())),
    );

    assert!(builder.get_redirect().is_some());
    assert!(!builder.is_fork());

    let context = dummy_context(source.clone());
    let sources = (builder.get_redirect_modifier().unwrap())(&context).unwrap();
    assert_eq!(sources.len(), 1);
    assert!(Arc::ptr_eq(&sources[0], &source));
}

/// Not in Java's test file — a pin for Java's merge-on-then behavior: the
/// builder's `arguments` container is a `RootCommandNode` and `then` adds
/// through its `addChild`, so same-name children merge immediately (the
/// later non-null command wins, grandchildren are absorbed, the original
/// insertion position is kept) and `getArguments()` reports one child
/// between the calls, not two. The built tree carries the same merged state.
#[test]
fn then_merges_same_name_children_immediately_like_java() {
    let first = Arc::new(RwLock::new(
        literal("dup").executes(|_: &CommandContext<()>| 1).build(),
    ));
    let second = Arc::new(RwLock::new(
        literal("dup")
            .executes(|_: &CommandContext<()>| 2)
            .then(literal("sub").executes(|_: &CommandContext<()>| 2))
            .build(),
    ));

    let builder: TestableBuilder = ArgumentBuilder::new(TestKind);
    let builder = builder.then_built(first).then_built(second);

    let arguments = builder.get_arguments();
    assert_eq!(arguments.len(), 1);
    let merged = arguments[0].clone();
    // the later command replaced the earlier one — run it to prove which
    let context = dummy_context(Arc::new(()));
    let result = (merged.read().get_command().as_ref().unwrap())(&context).unwrap();
    assert_eq!(result, 2);
    // the grandchild came along through the merge
    assert!(merged.read().get_child("sub").is_some());

    // and build() carries the merged child over into the node
    let built = literal("root").then_built(merged).build();
    let dup = built.get_child("dup").unwrap();
    assert!(dup.read().get_child("sub").is_some());
    let context = dummy_context(Arc::new(()));
    let result = (dup.read().get_command().as_ref().unwrap())(&context).unwrap();
    assert_eq!(result, 2);
}
