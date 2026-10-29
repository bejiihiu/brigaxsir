//! The port of Java's `tree/LiteralCommandNodeTest` (including the inherited
//! `AbstractCommandNodeTest` cases).

use std::sync::Arc;

use super::abstract_command_node_test::{
    SharedNode, Source, literal_with_command, run_test_add_child,
    run_test_add_child_merges_grandchildren, run_test_add_child_overwrites_command,
    run_test_add_child_preserves_command,
};
use brigaxsir::{
    builder::literal_argument_builder::literal,
    command_dispatcher::CommandDispatcher,
    context::{CommandContext, CommandContextBuilder, StringRange},
    errors::{BuiltInExceptionProvider, built_in_exceptions},
    string_reader::StringReader,
    suggestion::{Suggestion, SuggestionsBuilder},
    tree::CommandNode,
};
use parking_lot::RwLock;

/// Minimal `block_on` for the tests, std-only: a channel-based waker (same
/// helper the other test binaries carry; the library's own futures complete
/// on the first poll).
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::{
        sync::mpsc,
        task::{Context, Poll, Wake, Waker},
    };

    struct ThreadWaker(mpsc::Sender<()>);

    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            let _ = self.0.send(());
        }
    }

    let mut future = Box::pin(future);
    let (tx, rx) = mpsc::channel();
    let waker = Waker::from(Arc::new(ThreadWaker(tx)));
    let mut cx = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => {
                if rx.recv().is_err() {
                    panic!("waker sender dropped before the future completed");
                }
            }
        }
    }
}

fn block_on_suggestions(
    future: brigaxsir::suggestion::SuggestionsFuture,
) -> brigaxsir::suggestion::Suggestions {
    block_on(future).expect("suggestion future failed")
}

fn new_context_builder(
    dispatcher: &CommandDispatcher<Source, i32>,
) -> CommandContextBuilder<'_, Source, i32> {
    CommandContextBuilder::new(
        dispatcher,
        Arc::new(()),
        Arc::new(RwLock::new(CommandNode::root())),
        0,
    )
}

#[test]
fn test_add_child() {
    run_test_add_child(|| Arc::new(RwLock::new(literal("foo").build())));
}

#[test]
fn test_add_child_merges_grandchildren() {
    run_test_add_child_merges_grandchildren(|| Arc::new(RwLock::new(literal("foo").build())));
}

#[test]
fn test_add_child_preserves_command() {
    run_test_add_child_preserves_command(|| Arc::new(RwLock::new(literal("foo").build())));
}

#[test]
fn test_add_child_overwrites_command() {
    run_test_add_child_overwrites_command(|| Arc::new(RwLock::new(literal("foo").build())));
}

#[test]
fn test_parse() {
    let node: SharedNode = Arc::new(RwLock::new(literal("foo").build()));
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let mut context_builder = new_context_builder(&dispatcher);

    let mut reader = StringReader::from("foo bar");
    CommandNode::parse_with_context(&node, &mut reader, &mut context_builder).unwrap();
    assert_eq!(reader.remaining(), " bar");
}

#[test]
fn test_parse_exact() {
    let node: SharedNode = Arc::new(RwLock::new(literal("foo").build()));
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let mut context_builder = new_context_builder(&dispatcher);

    let mut reader = StringReader::from("foo");
    CommandNode::parse_with_context(&node, &mut reader, &mut context_builder).unwrap();
    assert_eq!(reader.remaining(), "");
}

#[test]
fn test_parse_similar() {
    let node: SharedNode = Arc::new(RwLock::new(literal("foo").build()));
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let mut context_builder = new_context_builder(&dispatcher);

    let mut reader = StringReader::from("foobar");
    let error = CommandNode::parse_with_context(&node, &mut reader, &mut context_builder)
        .expect_err("parsing `foobar` as literal `foo` must fail");
    assert!(
        error.is_type(built_in_exceptions().literal_incorrect().as_type()),
        "wrong exception type"
    );
    assert_eq!(error.get_cursor(), Some(0));
}

#[test]
fn test_parse_invalid() {
    let node: SharedNode = Arc::new(RwLock::new(literal("foo").build()));
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let mut context_builder = new_context_builder(&dispatcher);

    let mut reader = StringReader::from("bar");
    let error = CommandNode::parse_with_context(&node, &mut reader, &mut context_builder)
        .expect_err("parsing `bar` as literal `foo` must fail");
    assert!(error.is_type(built_in_exceptions().literal_incorrect().as_type()));
    assert_eq!(error.get_cursor(), Some(0));
}

#[test]
fn test_usage() {
    let node: SharedNode = Arc::new(RwLock::new(literal("foo").build()));
    assert_eq!(node.read().get_usage_text(), "foo");
}

#[test]
fn test_suggestions() {
    let node: SharedNode = Arc::new(RwLock::new(literal("foo").build()));
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let context_builder = new_context_builder(&dispatcher);

    let empty = block_on_suggestions(
        node.read()
            .list_suggestions(context_builder.build(""), SuggestionsBuilder::new("", 0)),
    );
    assert_eq!(
        empty.list(),
        [Suggestion::new(StringRange::at(0), "foo")].as_slice()
    );

    let foo = block_on_suggestions(node.read().list_suggestions(
        context_builder.build("foo"),
        SuggestionsBuilder::new("foo", 0),
    ));
    assert!(foo.is_empty());

    let food = block_on_suggestions(node.read().list_suggestions(
        context_builder.build("food"),
        SuggestionsBuilder::new("food", 0),
    ));
    assert!(food.is_empty());

    let _b = block_on_suggestions(
        node.read()
            .list_suggestions(context_builder.build("b"), SuggestionsBuilder::new("b", 0)),
    );
    // Java asserts `food` twice in this test (the `b` case asserts on the
    // wrong variable); ported 1:1 instead of silently fixing it
    assert!(food.is_empty());
}

#[test]
fn test_equals() {
    // Java's EqualsTester with one shared mock Command: within a group every
    // pair is equal, across groups every pair differs
    let command: super::abstract_command_node_test::SharedCommand =
        Arc::new(|_: &CommandContext<Source, i32>| Ok(0));
    let node_arc = |node: brigaxsir::tree::CommandNode<Source, i32>| -> SharedNode {
        Arc::new(RwLock::new(node))
    };

    let group1 = [
        node_arc(literal("foo").build()),
        node_arc(literal("foo").build()),
    ];
    let group2 = [
        literal_with_command("bar", Some(command.clone())),
        literal_with_command("bar", Some(command.clone())),
    ];
    let group3 = [
        node_arc(literal("bar").build()),
        node_arc(literal("bar").build()),
    ];
    let group4 = [
        node_arc(literal("foo").then(literal("bar")).build()),
        node_arc(literal("foo").then(literal("bar")).build()),
    ];

    for group in [&group1, &group2, &group3, &group4] {
        assert_eq!(*group[0].read(), *group[1].read());
    }
    for (i, a) in [&group1, &group2, &group3, &group4].iter().enumerate() {
        for (j, b) in [&group1, &group2, &group3, &group4].iter().enumerate() {
            if i != j {
                assert_ne!(*a[0].read(), *b[0].read());
            }
        }
    }
}

#[test]
fn test_create_builder() {
    let node: SharedNode = Arc::new(RwLock::new(literal("foo").build()));
    let builder = node.read().create_builder();

    assert_eq!(builder.get_literal(), node.read().get_literal());
    // Java compares the requirement/command references (`is`); the shared
    // Arc is the same identity here
    assert!(Arc::ptr_eq(
        builder.get_requirement(),
        node.read().get_requirement()
    ));
    assert!(builder.get_command().is_none());
    assert!(node.read().get_command().is_none());
}
