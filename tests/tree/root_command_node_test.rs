//! The port of Java's `tree/RootCommandNodeTest` (including the inherited
//! `AbstractCommandNodeTest` cases).

use std::sync::Arc;

use super::abstract_command_node_test::{
    SharedNode, Source, run_test_add_child, run_test_add_child_merges_grandchildren,
    run_test_add_child_overwrites_command, run_test_add_child_preserves_command,
};
use brigaxsir::{
    builder::literal_argument_builder::literal, command_dispatcher::CommandDispatcher,
    context::CommandContextBuilder, string_reader::StringReader, suggestion::SuggestionsBuilder,
    tree::CommandNode,
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
    Arc::new(RwLock::new(CommandNode::root()))
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
    let mut context_builder = CommandContextBuilder::new(
        &dispatcher,
        Arc::new(()),
        Arc::new(RwLock::new(CommandNode::root())),
        0,
    );

    // the root consumes nothing: Java's RootCommandNode#parse is empty
    let mut reader = StringReader::from("hello world");
    CommandNode::parse_with_context(&node, &mut reader, &mut context_builder).unwrap();
    assert_eq!(reader.cursor(), 0);
}

#[test]
#[should_panic(expected = "Cannot add a RootCommandNode as a child to any other CommandNode")]
fn test_add_child_no_root() {
    let node = new_node();
    node.write()
        .add_child(&Arc::new(RwLock::new(CommandNode::<Source, i32>::root())));
}

#[test]
fn test_usage() {
    let node = new_node();
    assert_eq!(node.read().get_usage_text(), "");
}

#[test]
fn test_suggestions() {
    let node = new_node();
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    // Java mocks a CommandContext here; a real one built from a fresh
    // builder is the hand-rolled fake
    let context = CommandContextBuilder::new(
        &dispatcher,
        Arc::new(()),
        Arc::new(RwLock::new(CommandNode::root())),
        0,
    )
    .build("");

    let result = block_on(
        node.read()
            .list_suggestions(context, SuggestionsBuilder::new("", 0)),
    )
    .expect("suggestion future failed");
    assert!(result.is_empty());
}

#[test]
#[should_panic(expected = "Cannot convert root into a builder")]
fn test_create_builder() {
    let _ = CommandNode::<Source, i32>::root().create_builder();
}

#[test]
fn test_equals() {
    let node_arc = |node: CommandNode<Source, i32>| -> SharedNode { Arc::new(RwLock::new(node)) };
    let root_with_foo = || {
        let root = new_node();
        root.write()
            .add_child(&Arc::new(RwLock::new(literal("foo").build())));
        root
    };

    let group1 = [node_arc(CommandNode::root()), node_arc(CommandNode::root())];
    let group2 = [root_with_foo(), root_with_foo()];

    // within a group everything equals everything, across groups nothing does
    assert_eq!(*group1[0].read(), *group1[1].read());
    assert_eq!(*group2[0].read(), *group2[1].read());
    assert_ne!(*group1[0].read(), *group2[0].read());
    assert_ne!(*group1[0].read(), *group2[1].read());
    assert_ne!(*group1[1].read(), *group2[0].read());
    assert_ne!(*group1[1].read(), *group2[1].read());
}
