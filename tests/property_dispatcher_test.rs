//! Property-based tests for the dispatcher over randomly generated literal
//! trees (sibling names are deduplicated, so no merge semantics complicate
//! the model): parsing arbitrary input never panics, executing a registered
//! path runs exactly that node's command (or fails with an error when the
//! node has none), `find_node`/`get_path` round-trip for every node, usage
//! strings are well-formed, and completion suggestions come back sorted
//! (Java's `compareToIgnoreCase` order) and deduplicated.
//!
//! Every generated tree also carries one integer argument node under the
//! root, so random garbage exercises the argument-parser path (parse
//! failures, unknown-argument execution) through the same surfaces. proptest's
//! defaults are kept: 256 cases per property with the deterministic built-in
//! RNG, comfortably inside the debug-mode time budget.

use std::{
    cmp::Ordering,
    collections::HashSet,
    future::Future,
    sync::{Arc, mpsc},
    task::{Context, Poll, Wake, Waker},
};

use parking_lot::RwLock;
use proptest::prelude::*;

use brigaxsir::{
    builder::{LiteralArgumentBuilder, argument, literal},
    command_dispatcher::CommandDispatcher,
    prelude::*,
    tree::CommandNode,
};

/// The literal-name pool: ASCII, Cyrillic and emoji names, so non-ASCII
/// paths get exercised by every property.
const POOL: [&str; 11] = [
    "alpha",
    "beta",
    "gamma",
    "delta",
    "epsilon",
    "zeta",
    "eta",
    "theta",
    "привет",
    "мир",
    "🚀",
];

/// Minimal `block_on` for the suggestion futures, std-only (the same helper
/// every other suggestion test uses; the library's futures complete on the
/// first poll).
fn block_on<F: Future>(future: F) -> F::Output {
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

/// A random subtree spec. Sibling names are deduplicated when the tree is
/// assembled (`prop_map` dedup is deterministic), so the tree never relies
/// on the `add_child` merge path — the model stays a plain literal tree.
#[derive(Debug, Clone)]
struct NodeSpec {
    name: String,
    has_cmd: bool,
    children: Vec<NodeSpec>,
}

fn node_spec(depth: u32) -> BoxedStrategy<NodeSpec> {
    let name_and_cmd = (0usize..POOL.len(), any::<bool>())
        .prop_map(|(idx, has_cmd)| (POOL[idx].to_owned(), has_cmd));
    if depth == 0 {
        name_and_cmd
            .prop_map(|(name, has_cmd)| NodeSpec {
                name,
                has_cmd,
                children: Vec::new(),
            })
            .boxed()
    } else {
        (
            name_and_cmd,
            proptest::collection::vec(node_spec(depth - 1), 0..=3),
        )
            .prop_map(|((name, has_cmd), kids)| {
                let mut seen = HashSet::new();
                let children = kids
                    .into_iter()
                    .filter(|kid| seen.insert(kid.name.clone()))
                    .collect();
                NodeSpec {
                    name,
                    has_cmd,
                    children,
                }
            })
            .boxed()
    }
}

/// The top-level list: one to five distinct top-level names.
fn tree_spec() -> BoxedStrategy<Vec<NodeSpec>> {
    proptest::collection::vec(node_spec(2), 1..=5)
        .prop_map(|specs| {
            let mut seen = HashSet::new();
            specs
                .into_iter()
                .filter(|spec| seen.insert(spec.name.clone()))
                .collect()
        })
        .boxed()
}

type Handle = Arc<RwLock<CommandNode<&'static str, i32>>>;

/// The built tree plus its model: every node's path (root excluded, in the
/// same order the tree stores) and the command id it must run, if any.
struct Built {
    dispatcher: CommandDispatcher<&'static str, i32>,
    /// `model[i]` pairs with `handles[i]` — both walks use the same
    /// insertion-ordered DFS.
    model: Vec<(Vec<String>, Option<i32>)>,
    handles: Vec<Handle>,
}

/// Build the subtree builder for `spec`, recording each node's path and
/// command id in the same DFS order the tree will store them in.
fn build_and_record(
    spec: &NodeSpec,
    path: &mut Vec<String>,
    model: &mut Vec<(Vec<String>, Option<i32>)>,
    next_id: &mut i32,
) -> LiteralArgumentBuilder<&'static str, i32> {
    path.push(spec.name.clone());
    let cmd = spec.has_cmd.then(|| {
        let id = *next_id;
        *next_id += 1;
        id
    });
    model.push((path.clone(), cmd));

    let mut builder = literal(spec.name.as_str());
    for child in &spec.children {
        let child_builder = build_and_record(child, path, model, next_id);
        builder = builder.then(child_builder);
    }
    path.pop();

    match cmd {
        Some(id) => builder.executes(move |_| id),
        None => builder,
    }
}

/// Collect the tree's handles in the same DFS order the model was recorded
/// in (tree children are insertion-ordered and the spec was deduplicated,
/// so the shapes line up one-to-one).
fn walk_handles(handle: &Handle, spec: &NodeSpec, out: &mut Vec<Handle>) {
    out.push(handle.clone());
    let children = handle.read().get_children();
    assert_eq!(
        children.len(),
        spec.children.len(),
        "tree shape diverged from the spec"
    );
    for (child, child_spec) in children.iter().zip(&spec.children) {
        walk_handles(child, child_spec, out);
    }
}

fn build(spec: &[NodeSpec]) -> Built {
    let mut subject = CommandDispatcher::<&'static str, i32>::new();

    // one integer argument directly under the root (registered through the
    // root's `add_child`, since the fluent `register` takes literals like
    // Java): any input whose first word matches no literal falls through to
    // it, so random garbage exercises the argument parser through
    // parse/execute/suggestions. It carries no command.
    let arg_handle = Arc::new(RwLock::new(
        argument::<&'static str, i32>("num", integer()).build(),
    ));
    subject.get_root().write().add_child(&arg_handle);

    let mut model = vec![(vec!["num".to_owned()], None)];
    let mut handles = vec![arg_handle];
    let mut next_id = 1;
    for node in spec {
        let mut path = Vec::new();
        let builder = build_and_record(node, &mut path, &mut model, &mut next_id);
        let handle = subject.register(builder);
        walk_handles(&handle, node, &mut handles);
    }

    Built {
        dispatcher: subject,
        model,
        handles,
    }
}

/// Mixed command-shaped input: full paths, prefixes, glued-on number tails
/// and fully random strings.
fn command_input() -> BoxedStrategy<String> {
    prop_oneof![
        3 => (0usize..POOL.len(), 0usize..POOL.len(), 0usize..POOL.len())
            .prop_map(|(a, b, c)| format!("{} {} {}", POOL[a], POOL[b], POOL[c])),
        3 => (0usize..POOL.len(), 0usize..POOL.len())
            .prop_map(|(a, b)| format!("{} {}", POOL[a], POOL[b])),
        2 => (0usize..POOL.len()).prop_map(|a| POOL[a].to_owned()),
        2 => (0usize..POOL.len(), 1usize..=4)
            .prop_map(|(a, n)| POOL[a].chars().take(n).collect()),
        2 => (0usize..POOL.len(), 0usize..=7)
            .prop_map(|(a, n)| format!("{} {}", POOL[a], "123456789".chars().take(n).collect::<String>())),
        3 => any::<String>(),
    ]
    .boxed()
}

proptest! {
    #[test]
    fn registered_paths_execute_their_own_command(spec in tree_spec()) {
        let built = build(&spec);
        for (path, cmd) in &built.model {
            let input = path.join(" ");
            // the terminal context's command is the deepest matched node's
            // command — Java's `withCommand` semantics — so a node without a
            // command must refuse to execute
            let result = built.dispatcher.execute(input.as_str(), "source");
            match (cmd, result) {
                (Some(id), Ok(got)) => {
                    prop_assert_eq!(*id, got, "path {:?} ran the wrong command", input);
                }
                (None, Ok(got)) => {
                    prop_assert_eq!(None::<i32>, Some(got), "path {:?} has no command but executed", input);
                }
                (_, Err(_)) => prop_assert!(cmd.is_none(), "path {:?} must run command {:?}", input, cmd),
            }
        }
    }

    #[test]
    fn find_node_and_get_path_round_trip(spec in tree_spec()) {
        let built = build(&spec);
        let root = built.dispatcher.get_root();

        // the root: empty path in both directions
        prop_assert!(built.dispatcher.get_path(&root).is_empty());
        let found_root = built.dispatcher.find_node(&[]).expect("root must be found");
        prop_assert!(Arc::ptr_eq(&found_root, &root));

        for ((path, cmd), handle) in built.model.iter().zip(&built.handles) {
            // get_path returns the node names, root excluded, matching the
            // registration path
            let got = built.dispatcher.get_path(handle);
            prop_assert_eq!(&got, path);
            // find_node walks the same names back to the very same handle
            let names: Vec<&str> = path.iter().map(String::as_str).collect();
            let found = built
                .dispatcher
                .find_node(&names)
                .unwrap_or_else(|| panic!("path {path:?} not found"));
            prop_assert!(Arc::ptr_eq(&found, handle), "path {path:?} found a different node");
            // the command bookkeeping agrees with the node
            prop_assert_eq!(found.read().get_command().is_some(), cmd.is_some());
        }

        // a node built but never registered has no path; a missing path is None
        let stray = Arc::new(RwLock::new(literal("stray").build()));
        prop_assert!(built.dispatcher.get_path(&stray).is_empty());
        prop_assert!(built.dispatcher.find_node(&["definitely", "not", "there"]).is_none());
    }

    #[test]
    fn usage_strings_are_clean(spec in tree_spec()) {
        let built = build(&spec);
        let root = built.dispatcher.get_root();
        let source = "source";

        let all = built.dispatcher.get_all_usage(&root.read(), &source, false);
        for usage in &all {
            // usage entries are built from non-empty node usage texts, so a
            // bare or `null`-containing entry would be a rendering bug
            prop_assert!(!usage.is_empty(), "empty usage string in {all:?}");
            prop_assert!(!usage.contains("null"), "`null` leaked into usage {usage:?}");
        }

        // every node is usable by this source, so the restricted scan
        // matches the unrestricted one
        let restricted = built.dispatcher.get_all_usage(&root.read(), &source, true);
        prop_assert_eq!(all, restricted);

        for (_, usage) in built.dispatcher.get_smart_usage(&root.read(), &source) {
            prop_assert!(!usage.is_empty(), "empty smart usage string");
            prop_assert!(!usage.contains("null"), "`null` leaked into smart usage {usage:?}");
        }
    }

    #[test]
    fn completion_suggestions_are_sorted_and_deduped(
        (spec, probe, extra) in (tree_spec(), command_input(), 0usize..=2),
    ) {
        let built = build(&spec);
        let parse = built.dispatcher.parse(probe.as_str(), "source");
        // the cursor sweep includes past-the-end positions: the truncation
        // saturates (documented divergence) instead of panicking
        let cursor = probe.chars().count() + extra;
        let suggestions = block_on(CommandDispatcher::get_completion_suggestions_with_cursor(
            parse, cursor,
        ))
        .expect("literal-only suggestion providers never fail");
        let list = suggestions.list();

        // sorted: every adjacent pair is in non-decreasing
        // compare_to_ignore_case order (text-only suggestions make the
        // comparator transitive, so full sortedness must hold)
        for pair in list.windows(2) {
            prop_assert!(
                pair[0].compare_to_ignore_case(&pair[1]) != Ordering::Greater,
                "unsorted suggestions: {:?} before {:?}",
                pair[0].text(),
                pair[1].text()
            );
        }

        // deduplicated: no two suggestions carry the same text (after the
        // common-range expansion, suggestions are equal exactly then)
        for i in 0..list.len() {
            for j in (i + 1)..list.len() {
                prop_assert_ne!(
                    list[i].text(),
                    list[j].text(),
                    "duplicate suggestion text {:?}",
                    list[i].text()
                );
            }
        }
    }

    #[test]
    fn arbitrary_input_never_panics((spec, input, extra) in (tree_spec(), any::<String>(), 0usize..=2)) {
        let built = build(&spec);
        // parse + execute: every failure must surface as an Err, never a
        // panic, whatever the input looks like
        let _ = built.dispatcher.execute(input.as_str(), "source");

        // completion suggestions at a random cursor, including the
        // past-the-end saturation branch
        let cursor = input.chars().count() + extra;
        let _ = block_on(CommandDispatcher::get_completion_suggestions_with_cursor(
            built.dispatcher.parse(input.as_str(), "source"),
            cursor,
        ));
    }

    #[test]
    fn structured_input_never_panics((spec, input) in (tree_spec(), command_input())) {
        let built = build(&spec);
        let _ = built.dispatcher.execute(input.as_str(), "source");
        let _ = block_on(CommandDispatcher::get_completion_suggestions(
            built.dispatcher.parse(input.as_str(), "source"),
        ));
    }
}
