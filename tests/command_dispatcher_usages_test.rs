//! Port of Java's `CommandDispatcherUsagesTest` (1:1 in meaning), plus a
//! few integration pins for behaviors that Java's own file leaves
//! unexercised: the `restricted = false` usage path, usage of a subtree
//! starting below the root (relative prefixes), the restricted check on
//! the starting node itself, and the `canUse` filtering inside the
//! smart-usage recursion.

use std::sync::Arc;

use brigaxsir::{prelude::*, string_reader::StringReader, tree::CommandNode};
use parking_lot::RwLock;

type NodeHandle = Arc<RwLock<CommandNode<()>>>;

fn setup() -> CommandDispatcher<()> {
    let command = |_: &CommandContext<()>| 0;

    let mut subject = CommandDispatcher::new();
    subject.register(
        literal("a")
            .then(
                literal("1")
                    .then(literal("i").executes(command))
                    .then(literal("ii").executes(command)),
            )
            .then(
                literal("2")
                    .then(literal("i").executes(command))
                    .then(literal("ii").executes(command)),
            ),
    );
    subject.register(literal("b").then(literal("1").executes(command)));
    subject.register(literal("c").executes(command));
    subject.register(literal("d").requires(|_| false).executes(command));
    subject.register(
        literal("e").executes(command).then(
            literal("1")
                .executes(command)
                .then(literal("i").executes(command))
                .then(literal("ii").executes(command)),
        ),
    );
    subject.register(
        literal("f")
            .then(
                literal("1")
                    .then(literal("i").executes(command))
                    .then(literal("ii").executes(command).requires(|_| false)),
            )
            .then(
                literal("2")
                    .then(literal("i").executes(command).requires(|_| false))
                    .then(literal("ii").executes(command)),
            ),
    );
    subject.register(
        literal("g")
            .executes(command)
            .then(literal("1").then(literal("i").executes(command))),
    );
    subject.register(
        literal("h")
            .executes(command)
            .then(literal("1").then(literal("i").executes(command)))
            .then(literal("2").then(literal("i").then(literal("ii").executes(command))))
            .then(literal("3").executes(command)),
    );
    subject.register(
        literal("i")
            .executes(command)
            .then(literal("1").executes(command))
            .then(literal("2").executes(command)),
    );
    subject.register(literal("j").redirect(subject.get_root()));
    subject.register(literal("k").redirect(get(&subject, "h")));
    subject
}

// Java has two `get` overloads (String and StringReader); one generic
// parameter covers both here.
fn get(subject: &CommandDispatcher<()>, command: impl Into<StringReader>) -> NodeHandle {
    subject
        .parse(command, ())
        .context
        .get_nodes()
        .last()
        .unwrap()
        .get_node()
}

/// Java compares the `getSmartUsage` result map by entry — order-blind,
/// keyed by node identity. The result is built in child insertion order,
/// so the exact order is pinned here too; identity is `Arc::ptr_eq`,
/// Java's `==`.
fn assert_smart_usage(actual: Vec<(NodeHandle, String)>, expected: Vec<(NodeHandle, &str)>) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "smart usage entry count mismatch"
    );
    for (index, (node, usage)) in expected.iter().enumerate() {
        assert!(
            Arc::ptr_eq(&actual[index].0, node),
            "entry {index}: unexpected node (got {})",
            actual[index].0.read().get_name()
        );
        assert_eq!(actual[index].1, *usage, "entry {index}: usage mismatch");
    }
}

#[test]
fn test_all_usage_no_commands() {
    let subject = CommandDispatcher::<()>::new();
    let results = subject.get_all_usage(&subject.get_root().read(), &(), true);
    assert!(results.is_empty());
}

#[test]
fn test_smart_usage_no_commands() {
    let subject = CommandDispatcher::<()>::new();
    let results = subject.get_smart_usage(&subject.get_root().read(), &());
    assert!(results.is_empty());
}

#[test]
fn test_all_usage_root() {
    let subject = setup();
    // Java asserts the exact array — the walk order (children in
    // insertion order, depth first) is observable
    assert_eq!(
        subject.get_all_usage(&subject.get_root().read(), &(), true),
        [
            "a 1 i", "a 1 ii", "a 2 i", "a 2 ii", "b 1", "c", "e", "e 1", "e 1 i", "e 1 ii",
            "f 1 i", "f 2 ii", "g", "g 1 i", "h", "h 1 i", "h 2 i ii", "h 3", "i", "i 1", "i 2",
            "j ...", "k -> h",
        ]
    );
}

#[test]
fn test_smart_usage_root() {
    let subject = setup();
    let results = subject.get_smart_usage(&subject.get_root().read(), &());

    assert_smart_usage(
        results,
        vec![
            (get(&subject, "a"), "a (1|2)"),
            (get(&subject, "b"), "b 1"),
            (get(&subject, "c"), "c"),
            (get(&subject, "e"), "e [1]"),
            (get(&subject, "f"), "f (1|2)"),
            (get(&subject, "g"), "g [1]"),
            (get(&subject, "h"), "h [1|2|3]"),
            (get(&subject, "i"), "i [1|2]"),
            (get(&subject, "j"), "j ..."),
            (get(&subject, "k"), "k -> h"),
        ],
    );
}

#[test]
fn test_smart_usage_h() {
    let subject = setup();
    let results = subject.get_smart_usage(&get(&subject, "h").read(), &());

    assert_smart_usage(
        results,
        vec![
            (get(&subject, "h 1"), "[1] i"),
            (get(&subject, "h 2"), "[2] i ii"),
            (get(&subject, "h 3"), "[3]"),
        ],
    );
}

#[test]
fn test_smart_usage_offset_h() {
    let subject = setup();
    let mut offset_h = StringReader::from("/|/|/h");
    offset_h.set_cursor(5);

    // Java fetches the node by parsing the offset reader itself
    let results = subject.get_smart_usage(&get(&subject, offset_h).read(), &());

    assert_smart_usage(
        results,
        vec![
            (get(&subject, "h 1"), "[1] i"),
            (get(&subject, "h 2"), "[2] i ii"),
            (get(&subject, "h 3"), "[3]"),
        ],
    );
}

/// Integration pin: `restricted = false` keeps every node regardless of
/// `requires` — the `false` side of the flag Java's usages test never
/// exercises.
#[test]
fn test_all_usage_root_unrestricted() {
    let subject = setup();
    assert_eq!(
        subject.get_all_usage(&subject.get_root().read(), &(), false),
        [
            "a 1 i", "a 1 ii", "a 2 i", "a 2 ii", "b 1", "c", "d", "e", "e 1", "e 1 i", "e 1 ii",
            "f 1 i", "f 1 ii", "f 2 i", "f 2 ii", "g", "g 1 i", "h", "h 1 i", "h 2 i ii", "h 3",
            "i", "i 1", "i 2", "j ...", "k -> h",
        ]
    );
}

/// Integration pin: usage of a node below the root is relative to that
/// node — Java's `getAllUsage` doc promises the path to the node is not
/// prepended — and a starting node the source cannot use yields nothing
/// when restricted. (`d` cannot be fetched through `parse` — the parse
/// skips unusable children, Java included — so it comes from the tree.)
#[test]
fn test_all_usage_subtree_is_relative() {
    let subject = setup();

    assert_eq!(
        subject.get_all_usage(&get(&subject, "f").read(), &(), true),
        ["1 i", "2 ii"]
    );
    let d = subject.get_root().read().get_child("d").unwrap();
    assert!(subject.get_all_usage(&d.read(), &(), true).is_empty());
}

/// Integration pin: `getSmartUsage` filters the candidate children by
/// `canUse` before collapsing alternatives — the `f` subtree is the
/// smallest shape where that filter is observable.
#[test]
fn test_smart_usage_filtered_children() {
    let subject = setup();
    let results = subject.get_smart_usage(&get(&subject, "f").read(), &());

    assert_smart_usage(
        results,
        vec![
            (get(&subject, "f 1"), "1 i"),
            (get(&subject, "f 2"), "2 ii"),
        ],
    );
}
