//! The port of Java's `context/CommandContextTest`.

use std::sync::Arc;

use brigaxsir::{
    builder::literal_argument_builder::literal,
    command_dispatcher::CommandDispatcher,
    context::{CommandContextBuilder, ParsedArgument, StringRange},
    tree::{Command, CommandNode, CommandNodeKind, LiteralCommandNode},
};
use parking_lot::RwLock;

// Java's `@Mock Object source`: one shared instance drives the identity
// comparisons, a fresh allocation stands in for `otherSource`. The dispatcher
// mock becomes a real dispatcher, the node mocks real nodes.
#[derive(Debug)]
struct Source;

type SharedCommand = Command<Source, i32>;
type SharedNode = Arc<RwLock<CommandNode<Source, i32>>>;

fn new_builder<'a>(
    dispatcher: &'a CommandDispatcher<Source, i32>,
    source: Arc<Source>,
    root_node: SharedNode,
) -> CommandContextBuilder<'a, Source, i32> {
    CommandContextBuilder::new(dispatcher, source, root_node, 0)
}

// Java: `@Test(expected = IllegalArgumentException.class)
// testGetArgument_nonexistent` — it asks for Object.class; in Rust there is
// no universal base type, and any requested `V` hits the same missing-name
// check first, so String stands in.
#[test]
#[should_panic(expected = "No such argument 'foo' exists on this command")]
fn test_get_argument_nonexistent() {
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let builder = new_builder(
        &dispatcher,
        Arc::new(Source),
        Arc::new(RwLock::new(literal("root").build())),
    );
    let _ = builder.build("").get_argument::<String>("foo");
}

// Java: `@Test(expected = IllegalArgumentException.class)
// testGetArgument_wrongType` — the parsed Integer is asked for as String.
#[test]
#[should_panic(expected = "Argument 'foo' is defined as Integer, not String")]
fn test_get_argument_wrong_type() {
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let mut builder = new_builder(
        &dispatcher,
        Arc::new(Source),
        Arc::new(RwLock::new(literal("root").build())),
    );
    let context = builder
        .with_argument("foo", ParsedArgument::new(0, 1, 123))
        .build("123");
    let _ = context.get_argument::<String>("foo");
}

#[test]
fn test_get_argument() {
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let mut builder = new_builder(
        &dispatcher,
        Arc::new(Source),
        Arc::new(RwLock::new(literal("root").build())),
    );
    let context = builder
        .with_argument("foo", ParsedArgument::new(0, 1, 123))
        .build("123");

    // Java: `assertThat(context.getArgument("foo", int.class), is(123))`
    assert_eq!(*context.get_argument::<i32>("foo"), 123);
    // the crate's fallible extension reads the same value
    assert_eq!(*context.try_get_argument::<i32>("foo").unwrap(), 123);
}

// the crate's extension, pinned: the error carries Java's exact strings
#[test]
fn try_get_argument_missing_name_has_javas_message() {
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let builder = new_builder(
        &dispatcher,
        Arc::new(Source),
        Arc::new(RwLock::new(literal("root").build())),
    );
    let error = builder
        .build("")
        .try_get_argument::<String>("foo")
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "No such argument 'foo' exists on this command"
    );
}

#[test]
fn try_get_argument_type_mismatch_has_javas_message() {
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let mut builder = new_builder(
        &dispatcher,
        Arc::new(Source),
        Arc::new(RwLock::new(literal("root").build())),
    );
    let context = builder
        .with_argument("foo", ParsedArgument::new(0, 1, 123))
        .build("123");
    let error = context.try_get_argument::<String>("foo").unwrap_err();
    assert_eq!(
        error.to_string(),
        "Argument 'foo' is defined as Integer, not String"
    );
}

#[test]
fn test_source() {
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let source = Arc::new(Source);
    let builder = new_builder(
        &dispatcher,
        source.clone(),
        Arc::new(RwLock::new(literal("root").build())),
    );
    // Java: `assertThat(builder.build("").getSource(), is(source))` — the
    // mock identity is the shared Arc here
    assert!(Arc::ptr_eq(&builder.build("").get_source_arc(), &source));
}

#[test]
fn test_root_node() {
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let root_node = Arc::new(RwLock::new(literal("root").build()));
    let builder = new_builder(&dispatcher, Arc::new(Source), root_node.clone());
    // Java: `assertThat(builder.build("").getRootNode(), is(rootNode))`
    assert!(Arc::ptr_eq(builder.build("").get_root_node(), &root_node));
}

// Java's EqualsTester groups, flattened into the crate's
// within-group-equal / cross-group-unequal loops (the mock objects became
// hand-rolled fakes: the shared source Arc is the identity `source`, fresh
// allocations are the `other` objects, and the command/root/node mocks are
// value-distinct real objects).
#[test]
fn test_equals() {
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let source = Arc::new(Source);
    let other_source = Arc::new(Source);
    let command: SharedCommand = Some(Arc::new(|_| Ok(0)));
    let other_command: SharedCommand = Some(Arc::new(|_| Ok(0)));
    let root_node = Arc::new(RwLock::new(literal("root").build()));
    let other_root_node = Arc::new(RwLock::new(literal("other_root").build()));
    let node = Arc::new(RwLock::new(literal("node").build()));
    let other_node = Arc::new(RwLock::new(literal("other_node").build()));

    let group1 = [
        new_builder(&dispatcher, source.clone(), root_node.clone()).build(""),
        new_builder(&dispatcher, source.clone(), root_node.clone()).build(""),
    ];
    let group2 = [
        new_builder(&dispatcher, source.clone(), other_root_node.clone()).build(""),
        new_builder(&dispatcher, source.clone(), other_root_node.clone()).build(""),
    ];
    let group3 = [
        new_builder(&dispatcher, other_source.clone(), root_node.clone()).build(""),
        new_builder(&dispatcher, other_source.clone(), root_node.clone()).build(""),
    ];
    let group4 = [
        new_builder(&dispatcher, source.clone(), root_node.clone())
            .with_command(&command)
            .build(""),
        new_builder(&dispatcher, source.clone(), root_node.clone())
            .with_command(&command)
            .build(""),
    ];
    let group5 = [
        new_builder(&dispatcher, source.clone(), root_node.clone())
            .with_command(&other_command)
            .build(""),
        new_builder(&dispatcher, source.clone(), root_node.clone())
            .with_command(&other_command)
            .build(""),
    ];
    let group6 = [
        new_builder(&dispatcher, source.clone(), root_node.clone())
            .with_argument("foo", ParsedArgument::new(0, 1, 123))
            .build("123"),
        new_builder(&dispatcher, source.clone(), root_node.clone())
            .with_argument("foo", ParsedArgument::new(0, 1, 123))
            .build("123"),
    ];
    let group7 = [
        new_builder(&dispatcher, source.clone(), root_node.clone())
            .with_node(node.clone(), StringRange::between(0, 3))
            .with_node(other_node.clone(), StringRange::between(4, 6))
            .build("123 456"),
        new_builder(&dispatcher, source.clone(), root_node.clone())
            .with_node(node.clone(), StringRange::between(0, 3))
            .with_node(other_node.clone(), StringRange::between(4, 6))
            .build("123 456"),
    ];
    let group8 = [
        new_builder(&dispatcher, source.clone(), root_node.clone())
            .with_node(other_node.clone(), StringRange::between(0, 3))
            .with_node(node.clone(), StringRange::between(4, 6))
            .build("123 456"),
        new_builder(&dispatcher, source.clone(), root_node.clone())
            .with_node(other_node.clone(), StringRange::between(0, 3))
            .with_node(node.clone(), StringRange::between(4, 6))
            .build("123 456"),
    ];

    let groups = [
        &group1, &group2, &group3, &group4, &group5, &group6, &group7, &group8,
    ];
    for group in groups {
        assert_eq!(group[0], group[1]);
    }
    for (i, a) in groups.iter().enumerate() {
        for (j, b) in groups.iter().enumerate() {
            if i != j {
                assert_ne!(a[0], b[0]);
            }
        }
    }
}

// Java has no test for `CommandContextBuilder#copy`, but its exact semantics
// are load-bearing for the parse and easy to regress, so this pins them: the
// redirect modifier is NOT carried over (fresh null until the next withNode),
// the child stays the same shared builder, and command/arguments/nodes/range/
// forks do. The modifier is only observable through the built context —
// Java's builder has no modifier getter either.
#[test]
fn copy_matches_java_semantics() {
    let dispatcher = CommandDispatcher::<Source, i32>::new();
    let source = Arc::new(Source);
    let root_node = Arc::new(RwLock::new(literal("root").build()));

    // a node carrying a redirect modifier and the fork flag, so withNode has
    // both to install
    let modifier: Arc<brigaxsir::modifier::RedirectModifier<Source, i32>> =
        Arc::new(|_| Ok(vec![]));
    let node = Arc::new(RwLock::new(CommandNode::new(
        CommandNodeKind::Literal(LiteralCommandNode::new("node")),
        None,
        Arc::new(|_: &Source| true),
        None,
        Some(modifier.clone()),
        true,
    )));

    let child = Arc::new(CommandContextBuilder::new(
        &dispatcher,
        source.clone(),
        root_node.clone(),
        0,
    ));

    let mut builder = new_builder(&dispatcher, source.clone(), root_node.clone());
    builder
        .with_child(child.clone())
        .with_argument("foo", ParsedArgument::new(0, 1, 123))
        .with_node(node, StringRange::between(0, 4));

    // sanity: the original really carries modifier + fork
    assert!(builder.build("x").get_redirect_modifier().is_some());
    assert!(builder.build("x").is_forked());

    let copy = builder.copy();

    // command/arguments/nodes/range/forks carry over...
    assert!(copy.get_command().is_none());
    assert_eq!(copy.get_arguments().len(), 1);
    assert_eq!(copy.get_nodes().len(), 1);
    assert_eq!(copy.get_range(), StringRange::between(0, 4));
    // ...the fork flag does too (Java assigns it)...
    assert!(copy.build("x").is_forked());
    // ...the child stays the SAME shared builder...
    assert!(Arc::ptr_eq(&copy.get_child().unwrap(), &child));
    // ...and the modifier does not: Java's copy() leaves it null until the
    // next withNode, while the original keeps its own
    assert!(copy.build("x").get_redirect_modifier().is_none());
    assert!(builder.build("x").get_redirect_modifier().is_some());
}
