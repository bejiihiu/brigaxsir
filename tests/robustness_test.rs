//! Recursion-depth and huge-input robustness pins (agent-24).
//!
//! What the surrounding probing established (Windows-gnu host, 2 MiB default
//! stack reserve; live Java 21 oracle probed with the same tree shapes):
//!
//! - `parse_nodes` / redirect chains recurse once per tree level. Release
//!   builds survive 1000-deep chains (parse+execute ~17 ms) and overflow
//!   between 1000 and 2000 frames on a 2 MiB stack; debug builds carry much
//!   larger frames (~6 KiB) and overflow around 300-500 frames on the same
//!   stack. Java 21 behaves the same way in kind: with a 2 MiB stack it
//!   survives 4000-deep chains and overflows at 8000 (default 1 MiB stack:
//!   survives 2000, overflows at 4000). Both implementations are recursive
//!   and both hit the platform stack limit on pathologically deep trees —
//!   this is Java parity, not a port defect.
//! - `get_all_usage` recursion survives ~4000 frames in release; `get_path`
//!   (which materializes every root-to-leaf path) survives ~8000 and its
//!   output is inherently O(depth²) node handles for a chain — exactly the
//!   same shape Java's `addPaths` produces, since every path of a chain is
//!   stored. `find_ambiguities` survives ~4000.
//! - Huge (1 MB+) inputs are linear in every string mode (greedy/word/
//!   quoted/error paths measured 18-53 ms in debug, 1.4-4.8 ms in release at
//!   1 MB, scaling linearly to 8 MB).
//!
//! The pinned tests run the moderate depth (500) on an explicitly sized
//! 8 MiB thread — comfortable headroom in both profiles (debug needs
//! ~3.3 MiB at 500). On default 2 MiB threads keep recursion-heavy inputs
//! below ~300 levels in debug / ~900 in release, or give the calling thread
//! a bigger stack (the same advice as Java's `-Xss`).

use std::sync::Arc;

use parking_lot::RwLock;

use brigaxsir::prelude::*;
use brigaxsir::tree::CommandNode;

/// The pinned moderate depth: green in debug AND release on the 8 MiB test
/// threads below, and two-plus orders of magnitude beyond any realistic
/// command tree.
const DEPTH: usize = 500;

/// The stack the depth tests run on. Rust's own threads default to 2 MiB;
/// the test doubles down to 8 MiB because debug builds need ~3.3 MiB at this
/// depth. This mirrors what real embedders do for recursive parsers.
const TEST_STACK: usize = 8 * 1024 * 1024;

/// Run `f` on a thread with an explicit stack size (see [`TEST_STACK`]).
fn with_big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(TEST_STACK)
        .spawn(f)
        .expect("spawn test thread")
        .join()
        .expect("test thread panicked")
}

/// A literal chain `a0 -> a1 -> ... -> a{DEPTH-1} -> <x: int>` with the
/// command on the argument node. Intermediates are NOT registered at the
/// root (that would add every node as a root child and turn the usage/path
/// walks into quadratic fan-out); the chain head is attached to the root
/// directly.
fn build_chain(d: usize) -> (CommandDispatcher<i32>, Arc<RwLock<CommandNode<i32>>>) {
    let subject = CommandDispatcher::<i32>::new();
    let mut node = Arc::new(RwLock::new(
        literal(&format!("a{}", d - 1))
            .then(argument("x", integer()).executes(|_| 1))
            .build(),
    ));
    for i in (0..d - 1).rev() {
        node = Arc::new(RwLock::new(
            literal(&format!("a{}", i)).then_built(node).build(),
        ));
    }
    subject.get_root().write().add_child(&node);
    (subject, node)
}

fn chain_input(d: usize) -> String {
    let mut s = String::new();
    for i in 0..d {
        s.push_str(&format!("a{} ", i));
    }
    s.push_str("42");
    s
}

/// A redirect chain `r0 -> r1 -> ... -> r{DEPTH-1}` where each hop's target
/// carries the next hop as its only child, ending on an executable argument.
/// Every redirect hop costs one `parse_nodes` recursion frame, same as a
/// literal chain.
fn build_redirect_chain(d: usize) -> CommandDispatcher<i32> {
    let mut subject = CommandDispatcher::<i32>::new();
    let mut t: Arc<RwLock<CommandNode<i32>>> = Arc::new(RwLock::new(
        literal(&format!("w{}", d - 1))
            .then(argument("x", integer()).executes(|_| 1))
            .build(),
    ));
    for i in (0..d - 1).rev() {
        let c = Arc::new(RwLock::new(
            literal(&format!("r{}", i + 1)).redirect(t).build(),
        ));
        t = Arc::new(RwLock::new(
            literal(&format!("w{}", i)).then_built(c).build(),
        ));
    }
    subject.register(literal("r0").redirect(t));
    subject
}

fn redirect_input(d: usize) -> String {
    let mut s = String::new();
    for i in 0..d {
        s.push_str(&format!("r{} ", i));
    }
    s.push_str("42");
    s
}

#[test]
fn deep_chain_parse_and_execute() {
    with_big_stack(move || {
        let (subject, _) = build_chain(DEPTH);
        assert_eq!(subject.execute(chain_input(DEPTH), 1).unwrap(), 1);
    });
}

#[test]
fn deep_redirect_chain_parse_and_execute() {
    with_big_stack(move || {
        let subject = build_redirect_chain(DEPTH);
        assert_eq!(subject.execute(redirect_input(DEPTH), 1).unwrap(), 1);
    });
}

#[test]
fn deep_chain_usage_and_paths() {
    with_big_stack(move || {
        let (subject, head) = build_chain(DEPTH);
        let root = subject.get_root();
        // only the argument node carries a command, so exactly one usage line
        let usage = subject.get_all_usage(&root.read(), &1, false);
        assert_eq!(usage.len(), 1);
        assert!(usage[0].starts_with("a0 a1 a2 "));
        assert!(usage[0].ends_with("<x>"));

        // get_path for the head, then a find_node round trip back to identity
        assert_eq!(subject.get_path(&head), vec!["a0"]);
        let a2 = head
            .read()
            .get_child("a1")
            .and_then(|a1| a1.read().get_child("a2"))
            .expect("a2 node");
        let found = subject.find_node(&["a0", "a1", "a2"]).expect("resolves");
        assert!(Arc::ptr_eq(&found, &a2));
    });
}

#[test]
fn deep_chain_ambiguity_scan() {
    with_big_stack(move || {
        let (subject, _) = build_chain(DEPTH);
        struct NoAmbiguity;
        impl brigaxsir::ambiguity_consumer::AmbiguityConsumer<i32, i32> for NoAmbiguity {
            fn ambiguous(
                &self,
                _parent: &Arc<RwLock<CommandNode<i32>>>,
                _child: &Arc<RwLock<CommandNode<i32>>>,
                _sibling: &Arc<RwLock<CommandNode<i32>>>,
                _inputs: &std::collections::HashSet<String>,
            ) {
                panic!("a strict chain has no ambiguous siblings");
            }
        }
        subject.find_ambiguities(&NoAmbiguity);
    });
}

#[test]
fn deep_chain_completion_suggestions() {
    with_big_stack(move || {
        let (subject, _) = build_chain(DEPTH);
        // fully consumed parse: no children left to suggest
        let parse = subject.parse(chain_input(DEPTH), 1);
        assert!(parse.get_exceptions().is_empty());
        let mut future = CommandDispatcher::get_completion_suggestions(parse);
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        match std::future::Future::poll(std::pin::Pin::new(&mut future), &mut cx) {
            std::task::Poll::Ready(Ok(suggestions)) => {
                assert!(suggestions.list().is_empty());
            }
            _ => panic!("literal-only merge future completes on the first poll"),
        }
    });
}

#[test]
fn huge_inputs_stay_linear_and_panic_free() {
    // 1 MB of input in every string mode; the wall-clock caps are generous
    // regression guards for accidental O(n^2) (measured values are ~50 ms
    // debug / ~5 ms release per op on the dev box).
    let n = 1_000_000;
    let t0 = std::time::Instant::now();

    let mut subject = CommandDispatcher::<i32>::new();
    subject
        .register(literal("g").then(
            argument("s", greedy_string()).executes(|ctx| get_string(ctx, "s").len() as i32),
        ));
    subject.register(
        literal("w").then(argument("s", word()).executes(|ctx| get_string(ctx, "s").len() as i32)),
    );
    subject.register(
        literal("q")
            .then(argument("s", string()).executes(|ctx| get_string(ctx, "s").len() as i32)),
    );
    subject.register(literal("i").then(argument("n", integer()).executes(|_| 1)));

    // greedy takes the whole rest of the input, spaces and quotes included
    let content: String = "x y_z-\"q\"\\.".repeat(n / 10 + 2);
    let content = &content[..n];
    assert_eq!(
        subject.execute(format!("g {}", content), 1).unwrap(),
        n as i32
    );

    // word: one huge unquoted token
    let token: String = "a".repeat(n);
    assert_eq!(
        subject.execute(format!("w {}", token), 1).unwrap(),
        n as i32
    );

    // quoted: 1 MB body with escapes (\" pairs) and embedded spaces
    let body: String = "ab\\\"cd ".repeat(n / 7 + 2);
    let body = &body[..n];
    let parsed = subject.execute(format!("q \"{}\"", body), 1).unwrap();
    // every escape pair drops exactly one backslash from the parsed value
    assert_eq!(parsed, (body.len() - body.matches("\\\"").count()) as i32);

    // error paths stay cheap and panic-free; the context window in the
    // message stays bounded (Java parity: the snippet is CONTEXT_AMOUNT
    // chars, never the whole input)
    let err = subject
        .execute(format!("q \"{}", "a".repeat(n)), 1)
        .unwrap_err();
    assert_eq!(err.get_raw_message().get_string(), "Unclosed quoted string");
    assert!(err.get_message().len() < 100);

    // a 1 MB integer token overflows -> Java's exact invalid-integer error,
    // which (like Java) embeds the whole token in the message
    let digits: String = "9".repeat(n);
    let err = subject.execute(format!("i {}", digits), 1).unwrap_err();
    assert!(
        err.get_raw_message()
            .get_string()
            .starts_with("Invalid integer '9999")
    );

    let elapsed = t0.elapsed();
    assert!(
        elapsed.as_secs() < 10,
        "1 MB battery took {:?} — quadratic regression suspected",
        elapsed
    );
}
