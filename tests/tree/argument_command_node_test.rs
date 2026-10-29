//! The port of Java's `tree/ArgumentCommandNodeTest` (including the
//! inherited `AbstractCommandNodeTest` cases).

use std::sync::Arc;

use super::abstract_command_node_test::{
    SharedCommand, SharedNode, Source, run_test_add_child, run_test_add_child_merges_grandchildren,
    run_test_add_child_overwrites_command, run_test_add_child_preserves_command,
};
use brigaxsir::{
    arguments::integer_argument_type::{integer, integer_range},
    builder::required_argument_builder::argument,
    command_dispatcher::CommandDispatcher,
    context::{CommandContext, CommandContextBuilder},
    string_reader::StringReader,
    suggestion::SuggestionsBuilder,
    tree::{ArgumentCommandNode, CommandNode, CommandNodeKind},
};
use parking_lot::RwLock;

/// Minimal `block_on` for the tests, std-only: a channel-based waker.
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

fn new_node() -> SharedNode {
    Arc::new(RwLock::new(argument("foo", integer()).build()))
}

/// An argument node carrying an explicitly shared command — the mock-command
/// stand-in, like `literal_with_command` in the shared test module.
fn argument_with_command(
    name: &str,
    parser: brigaxsir::arguments::IntegerArgumentType,
    command: SharedCommand,
) -> SharedNode {
    Arc::new(RwLock::new(CommandNode::new(
        CommandNodeKind::Argument(ArgumentCommandNode::new(name, Arc::new(parser), None)),
        Some(command),
        Arc::new(|_| true),
        None,
        None,
        false,
    )))
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
    run_test_add_child(new_node);
}

#[test]
fn test_add_child_merges_grandchildren() {
    run_test_add_child_merges_grandchildren(new_node);
}

#[test]
fn test_add_child_preserves_command() {
    run_test_add_child_preserves_command(new_node);
}

#[test]
fn test_add_child_overwrites_command() {
    run_test_add_child_overwrites_command(new_node);
}

#[test]
fn test_parse() {
    let node = new_node();
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let mut context_builder = new_context_builder(&dispatcher);

    let mut reader = StringReader::from("123 456");
    CommandNode::parse_with_context(&node, &mut reader, &mut context_builder).unwrap();

    let arguments = context_builder.get_arguments();
    assert!(arguments.contains_key("foo"));
    assert_eq!(
        arguments
            .get("foo")
            .unwrap()
            .get_result()
            .as_any()
            .downcast_ref::<i32>(),
        Some(&123)
    );
}

#[test]
fn test_usage() {
    let node = new_node();
    assert_eq!(node.read().get_usage_text(), "<foo>");
}

#[test]
fn test_suggestions() {
    let node = new_node();
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let context_builder = new_context_builder(&dispatcher);

    let result = block_on(
        node.read()
            .list_suggestions(context_builder.build(""), SuggestionsBuilder::new("", 0)),
    )
    .expect("suggestion future failed");
    assert!(result.is_empty());
}

#[test]
fn test_equals() {
    let command: SharedCommand = Arc::new(|_: &CommandContext<Source, i32>| Ok(0));
    let node_arc = |node: CommandNode<Source, i32>| -> SharedNode { Arc::new(RwLock::new(node)) };

    let group1 = [
        node_arc(argument("foo", integer()).build()),
        node_arc(argument("foo", integer()).build()),
    ];
    let group2 = [
        argument_with_command("foo", integer(), command.clone()),
        argument_with_command("foo", integer(), command.clone()),
    ];
    let group3 = [
        node_arc(argument("bar", integer_range(-100, 100)).build()),
        node_arc(argument("bar", integer_range(-100, 100)).build()),
    ];
    let group4 = [
        node_arc(argument("foo", integer_range(-100, 100)).build()),
        node_arc(argument("foo", integer_range(-100, 100)).build()),
    ];
    let group5 = [
        node_arc(
            argument("foo", integer())
                .then(argument("bar", integer()))
                .build(),
        ),
        node_arc(
            argument("foo", integer())
                .then(argument("bar", integer()))
                .build(),
        ),
    ];

    for group in [&group1, &group2, &group3, &group4, &group5] {
        assert_eq!(*group[0].read(), *group[1].read());
    }
    for (i, a) in [&group1, &group2, &group3, &group4, &group5]
        .iter()
        .enumerate()
    {
        for (j, b) in [&group1, &group2, &group3, &group4, &group5]
            .iter()
            .enumerate()
        {
            if i != j {
                assert_ne!(*a[0].read(), *b[0].read());
            }
        }
    }
}

#[test]
fn test_create_builder() {
    let node = new_node();
    let builder = node.read().create_builder();

    assert_eq!(builder.get_name(), node.read().get_name());
    // Java compares the type/requirement/command references (`is`); the
    // parser Arc is shared between node and builder, the command is absent
    assert!(Arc::ptr_eq(builder.get_type(), node.read().get_type()));
    assert!(Arc::ptr_eq(
        builder.get_requirement(),
        node.read().get_requirement()
    ));
    assert!(builder.get_command().is_none());
    assert!(node.read().get_command().is_none());
}
