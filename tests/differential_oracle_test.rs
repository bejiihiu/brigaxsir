//! Differential oracle test against the live Java Brigadier library (task 20).
//!
//! The test defines a fixed battery of command-tree scenarios, feeds them to
//! the real Java 1.3 library through a harness subprocess
//! (`tests/java_oracle/OracleHarness.java`, compiled on demand into
//! `target/java-oracle/classes`), runs the identical scenarios through the
//! Rust port, and compares the results field by field: parse error maps
//! (node name + raw message + context + cursor), execute results and
//! exceptions, completion suggestions (range + ordered texts + tooltips),
//! usage renderings, smart usage, `get_path`/`find_node`.
//!
//! Comparison policy (cursor-unit divergence):
//!
//! * Scenario inputs entirely inside the BMP have char cursors identical to
//!   Java's UTF-16 cursors, so those scenarios are compared exactly,
//!   cursor-for-cursor (`Mode::Exact`).
//! * Scenarios with astral-plane input (emoji) are compared semantically
//!   (`Mode::Semantic`): messages, node names, suggestion texts and result
//!   values must match byte-for-byte, but numeric cursors/ranges and the
//!   context snippet are skipped, because Rust cursors count characters while
//!   Java counts UTF-16 units (the documented crate divergence, notes.md
//!   §6.8 #7). Command results must also be position-free there: a `strlen:`
//!   command counts UTF-16 units in Java but characters in Rust, so astral
//!   scenarios use constant commands.
//!
//! The test skips itself (with a clear message, not `#[ignore]`) when no
//! `java` runtime is on PATH, or when classes are missing and no `javac`
//! exists. On this machine Java 21 is installed, so the oracle runs.

use std::{
    collections::HashMap,
    future::Future,
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
    process::{ChildStdout, Command, Stdio},
    sync::{Arc, mpsc},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

use parking_lot::RwLock;

use brigaxsir::{
    builder::{ArgumentBuilder, RequiredArgumentBuilder},
    command_dispatcher::CommandDispatcher,
    context::CommandContext,
    message::{LiteralMessage, Message},
    prelude::*,
    suggestion::{SuggestionProvider, SuggestionsBuilder, SuggestionsFuture},
    tree::CommandNode,
};

// ---------------------------------------------------------------------------
// block_on (std-only, same shape as the other test files)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// scenario model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mod {
    None,
    Identity,
    Dup(u32),
    Empty,
}

#[derive(Clone, Copy)]
enum Exec {
    Const(i32),
    Int(&'static str),
    Long(&'static str),
    StrLen(&'static str),
    Bool(&'static str),
}

#[derive(Clone, Copy)]
enum Req {
    /// `s -> true` — always usable.
    Always,
    Never,
    Eq(i32),
}

struct NodeSpec {
    path: &'static str,
    exec: Option<Exec>,
    req: Option<Req>,
    red: Option<&'static str>,
    modifier: Mod,
    fork: bool,
    sug: Option<&'static str>,
}

#[derive(Clone, Copy)]
struct ArgSpec {
    ty: &'static str,
    name: &'static str,
    min: &'static str,
    max: &'static str,
}

enum Seg {
    Lit(&'static str),
    Arg(ArgSpec),
}

impl Seg {
    fn name(&self) -> &'static str {
        match self {
            Seg::Lit(name) => name,
            Seg::Arg(spec) => spec.name,
        }
    }
}

enum Op {
    Parse {
        src: i32,
        input: String,
    },
    Exec {
        src: i32,
        input: String,
    },
    Sug {
        src: i32,
        cursor: usize,
        input: String,
    },
    Usage {
        restricted: bool,
        src: i32,
        node: &'static str,
    },
    Smart {
        src: i32,
        node: &'static str,
    },
    Path {
        node: &'static str,
    },
    Find {
        path: String,
    },
}

struct Scenario {
    id: &'static str,
    nodes: Vec<NodeSpec>,
    ops: Vec<Op>,
}

// builder helpers -----------------------------------------------------------

fn n(path: &'static str) -> NodeSpec {
    NodeSpec {
        path,
        exec: None,
        req: None,
        red: None,
        modifier: Mod::None,
        fork: false,
        sug: None,
    }
}

fn ex(mut node: NodeSpec, exec: Exec) -> NodeSpec {
    node.exec = Some(exec);
    node
}

fn rq(mut node: NodeSpec, req: Req) -> NodeSpec {
    node.req = Some(req);
    node
}

fn rd(mut node: NodeSpec, target: &'static str) -> NodeSpec {
    node.red = Some(target);
    node
}

fn md(mut node: NodeSpec, modifier: Mod) -> NodeSpec {
    node.modifier = modifier;
    node
}

fn fk(mut node: NodeSpec) -> NodeSpec {
    node.fork = true;
    node
}

fn sg(mut node: NodeSpec, provider: &'static str) -> NodeSpec {
    node.sug = Some(provider);
    node
}

// op helpers ----------------------------------------------------------------

fn parse(input: impl Into<String>) -> Op {
    Op::Parse {
        src: 0,
        input: input.into(),
    }
}

fn parse_s(src: i32, input: impl Into<String>) -> Op {
    Op::Parse {
        src,
        input: input.into(),
    }
}

fn exec(input: impl Into<String>) -> Op {
    Op::Exec {
        src: 0,
        input: input.into(),
    }
}

fn exec_s(src: i32, input: impl Into<String>) -> Op {
    Op::Exec {
        src,
        input: input.into(),
    }
}

fn sug(cursor: usize, input: impl Into<String>) -> Op {
    Op::Sug {
        src: 0,
        cursor,
        input: input.into(),
    }
}

fn sug_s(src: i32, cursor: usize, input: impl Into<String>) -> Op {
    Op::Sug {
        src,
        cursor,
        input: input.into(),
    }
}

fn usage(restricted: bool, node: &'static str) -> Op {
    Op::Usage {
        restricted,
        src: 0,
        node,
    }
}

fn usage_s(src: i32, restricted: bool, node: &'static str) -> Op {
    Op::Usage {
        restricted,
        src,
        node,
    }
}

fn smart(node: &'static str) -> Op {
    Op::Smart { src: 0, node }
}

fn smart_s(src: i32, node: &'static str) -> Op {
    Op::Smart { src, node }
}

fn path(node: &'static str) -> Op {
    Op::Path { node }
}

fn find(path: impl Into<String>) -> Op {
    Op::Find { path: path.into() }
}

fn nines(count: usize) -> String {
    "9".repeat(count)
}

fn scenario(id: &'static str, nodes: Vec<NodeSpec>, ops: Vec<Op>) -> Scenario {
    Scenario { id, nodes, ops }
}

// ---------------------------------------------------------------------------
// the scenario battery
// ---------------------------------------------------------------------------

fn scenarios() -> Vec<Scenario> {
    vec![
        // -- literals & execution ------------------------------------------------
        scenario(
            "literal_exec_basic",
            vec![ex(n("foo"), Exec::Const(5))],
            vec![
                exec("foo"),
                exec("bar"),
                parse("foo"),
                parse("foo bar"),
                path("foo"),
                find("foo"),
            ],
        ),
        scenario(
            "literal_child_exec",
            vec![n("foo"), ex(n("foo/bar"), Exec::Const(7))],
            vec![
                exec("foo"),
                exec("foo bar"),
                exec("foo baz"),
                exec("foo bar baz"),
                parse("foo"),
                parse("foo bar"),
                parse("foo bar baz"),
            ],
        ),
        scenario(
            "two_literals_and_empty_input",
            vec![ex(n("a"), Exec::Const(1)), ex(n("b"), Exec::Const(2))],
            vec![
                exec(""),
                exec("a"),
                exec("b"),
                parse(""),
                parse("a"),
                sug(0, ""),
                sug(1, "a"),
                sug(1, "z"),
            ],
        ),
        scenario(
            "merge_shared_prefix",
            vec![
                ex(n("foo"), Exec::Const(1)),
                ex(n("foo/bar"), Exec::Const(2)),
                ex(n("foo/baz"), Exec::Const(3)),
            ],
            vec![
                exec("foo"),
                exec("foo bar"),
                exec("foo baz"),
                exec("foo qux"),
                path("foo"),
                path("foo/bar"),
                find("foo,bar"),
                find("foo"),
                find("nope"),
                find("-"),
            ],
        ),
        scenario(
            "merge_discards_later_command",
            vec![ex(n("dup"), Exec::Const(1)), ex(n("dup"), Exec::Const(9))],
            vec![exec("dup"), usage(false, "."), smart(".")],
        ),
        scenario(
            "same_name_merge",
            vec![
                ex(n("same"), Exec::Const(1)),
                ex(n("same/@word:w"), Exec::Const(2)),
            ],
            vec![
                exec("same"),
                exec("same x"),
                exec("same x y"),
                usage(false, "."),
                smart("."),
                path("same"),
                path("same/@word:w"),
            ],
        ),
        scenario(
            "arg_at_root",
            vec![ex(n("@int:value"), Exec::Int("value"))],
            vec![
                exec("5"),
                exec("5 6"),
                parse("5"),
                parse("5 6"),
                path("@int:value"),
                find("value"),
            ],
        ),
        // -- integer arguments ---------------------------------------------------
        scenario(
            "int_unbounded_returns_value",
            vec![ex(n("num/@int:value"), Exec::Int("value"))],
            vec![
                exec("num 42"),
                exec("num -5"),
                exec("num 0"),
                exec("num 2147483648"),
                exec("num -2147483649"),
                exec("num "),
                exec("num abc"),
                exec("num"),
                parse("num 5"),
            ],
        ),
        scenario(
            "int_range_bounds",
            vec![ex(n("num/@int:value:0:10"), Exec::Int("value"))],
            vec![
                exec("num 0"),
                exec("num 10"),
                exec("num 5"),
                exec("num -1"),
                exec("num 11"),
                parse("num -1"),
                parse("num 11"),
                parse("num 5"),
            ],
        ),
        scenario(
            "int_min_only",
            vec![ex(n("num/@int:value:-100:-"), Exec::Int("value"))],
            vec![
                exec("num -100"),
                exec("num -101"),
                exec("num 999999"),
                parse("num -101"),
            ],
        ),
        scenario(
            "int_negative_parse_only",
            vec![ex(n("nn/@int:v:-100:100"), Exec::Const(1))],
            vec![
                parse("nn -100"),
                parse("nn -101"),
                parse("nn 100"),
                parse("nn 101"),
                exec("nn -100"),
                exec("nn 101"),
            ],
        ),
        // -- long ----------------------------------------------------------------
        scenario(
            "long_range_and_overflow",
            vec![ex(n("big/@long:v:-5:5"), Exec::Long("v"))],
            vec![
                exec("big 5"),
                exec("big -5"),
                exec("big 6"),
                exec("big 9223372036854775808"),
                exec("big"),
            ],
        ),
        scenario(
            "long_min_only_and_cast",
            vec![ex(n("b/@long:v:0:-"), Exec::Long("v"))],
            vec![
                exec("b 0"),
                exec("b -1"),
                // (int)(long) cast of i64::MAX: Java and Rust both truncate to -1
                exec("b 9223372036854775807"),
            ],
        ),
        // -- double / float ------------------------------------------------------
        scenario(
            "double_range",
            vec![ex(n("d/@double:x:-1.5:1.5"), Exec::Const(1))],
            vec![
                exec("d 0"),
                exec("d 1.5"),
                exec("d -.5"),
                exec("d 1.5000001"),
                exec("d -1.6"),
                exec("d abc"),
                exec("d 1.5.5"),
            ],
        ),
        scenario(
            "double_expected_on_empty",
            vec![ex(n("d/@double:x"), Exec::Const(1))],
            vec![exec("d"), exec("d "), exec("d 0.5")],
        ),
        scenario(
            "float_range",
            vec![ex(n("f/@float:x:0:100"), Exec::Const(1))],
            vec![
                exec("f 50.5"),
                exec("f 100.5"),
                exec("f -0.1"),
                // the scanner stops at 'e' (not an allowed number char), so
                // "1" parses and "e40" trips the separator check
                exec("f 1e40"),
                parse("f 1e40"),
            ],
        ),
        scenario(
            "float_infinity_over_max",
            vec![ex(n("f/@float:x"), Exec::Const(1))],
            vec![exec(format!("f {}", nines(44)))],
        ),
        scenario(
            "double_infinity_over_max",
            vec![ex(n("d/@double:x"), Exec::Const(1))],
            vec![exec(format!("d {}", nines(310)))],
        ),
        scenario(
            "float_max_bound_scientific",
            vec![ex(n("fm/@float:x:0:3.4028235E38"), Exec::Const(1))],
            vec![
                exec("fm 3.4028235"),
                exec("fm 3.4028235E38"),
                parse("fm 3.4028235E38"),
            ],
        ),
        // -- string family -------------------------------------------------------
        scenario(
            "word_arg",
            vec![ex(n("w/@word:s"), Exec::StrLen("s"))],
            vec![
                exec("w hello"),
                exec("w привет"),
                exec("w hello world"),
                exec("w a.b-c+d"),
                exec("w "),
                exec("w"),
            ],
        ),
        scenario(
            "string_quoted",
            vec![ex(n("q/@string:s"), Exec::StrLen("s"))],
            vec![
                exec(r#"q "hello world""#),
                exec("q hello"),
                exec("q \"\""),
                exec(r#"q "unclosed"#),
                exec(r#"q "a\"b""#),
                exec(r#"q "a\zb""#),
                exec(r#"q "a\nb""#),
                exec("q 'single'"),
                exec("q "),
            ],
        ),
        scenario(
            "greedy_string",
            vec![ex(n("g/@greedy:s"), Exec::StrLen("s"))],
            vec![
                exec("g one two  three"),
                exec("g привет"),
                exec("g "),
                exec("g"),
            ],
        ),
        // -- bool ----------------------------------------------------------------
        scenario(
            "bool_arg",
            vec![ex(n("flag/@bool:v"), Exec::Bool("v"))],
            vec![
                exec("flag true"),
                exec("flag false"),
                exec("flag tru"),
                exec("flag TRUE"),
                exec("flag"),
            ],
        ),
        // -- requirements --------------------------------------------------------
        scenario(
            "req_false_node",
            vec![rq(ex(n("secret"), Exec::Const(1)), Req::Never)],
            vec![
                exec("secret"),
                parse("secret"),
                usage(false, "."),
                usage(true, "."),
                smart("."),
            ],
        ),
        scenario(
            "req_eq_source",
            vec![rq(ex(n("admin"), Exec::Const(1)), Req::Eq(5))],
            vec![
                exec_s(5, "admin"),
                exec_s(7, "admin"),
                usage_s(5, true, "."),
                usage_s(7, true, "."),
                parse_s(7, "admin"),
                smart_s(5, "."),
                smart_s(7, "."),
            ],
        ),
        // -- redirects -----------------------------------------------------------
        scenario(
            "redirect_simple",
            vec![
                ex(n("real/@int:num"), Exec::Const(3)),
                rd(n("alias"), "real/@int:num"),
            ],
            vec![
                exec("alias 5"),
                exec("real 5"),
                // the documented Java quirk: a plain redirect only engages with
                // trailing input, so the bare alias reports unknown command
                exec("alias"),
                parse("alias 5"),
                parse("alias"),
                sug(6, "alias "),
            ],
        ),
        scenario(
            "redirect_chain_dead_end_leaf",
            vec![
                ex(n("one/@int:x"), Exec::Int("x")),
                rd(n("two"), "one/@int:x"),
                rd(n("three"), "two"),
            ],
            vec![
                exec("three 7"),
                exec("two 7"),
                exec("three"),
                usage(false, "."),
                usage(false, "one/@int:x"),
                path("two"),
                find("one"),
                find("three"),
            ],
        ),
        scenario(
            "redirect_to_root",
            vec![rd(n("r"), ".")],
            vec![
                exec("r foo"),
                exec("r r"),
                usage(false, "."),
                smart("."),
                parse("r foo"),
            ],
        ),
        // NOTE: every redirect here targets a leaf argument node — Java's
        // redirect parse re-enters the TARGET as a parse root (only the
        // target's CHILDREN are candidates), so the target's own exec never
        // engages and every op below dead-ends with the same error on both
        // sides. The real fork/modifier sums live in `fork_root_sums` and
        // the chain_fork_* scenarios (redirect-to-root shape).
        scenario(
            "redirect_leaf_arg_dead_end_forked",
            vec![
                ex(n("src/@int:x"), Exec::Int("x")),
                fk(md(rd(n("f"), "src/@int:x"), Mod::Dup(3))),
            ],
            vec![exec("f 2"), exec("f 5"), parse("f 2")],
        ),
        scenario(
            "redirect_leaf_arg_dead_end_nofork",
            vec![
                ex(n("src2/@int:y"), Exec::Int("y")),
                md(rd(n("f2"), "src2/@int:y"), Mod::Dup(3)),
            ],
            vec![exec("f2 5"), exec("f2 0")],
        ),
        scenario(
            "redirect_leaf_arg_dead_end_empty_fork",
            vec![
                ex(n("src3/@int:z"), Exec::Int("z")),
                fk(md(rd(n("e1"), "src3/@int:z"), Mod::Empty)),
            ],
            vec![exec("e1 1")],
        ),
        scenario(
            "redirect_leaf_arg_dead_end_empty_nofork",
            vec![
                ex(n("src3/@int:z"), Exec::Int("z")),
                md(rd(n("e2"), "src3/@int:z"), Mod::Empty),
            ],
            vec![exec("e2 1")],
        ),
        scenario(
            "redirect_leaf_arg_dead_end_identity",
            vec![
                ex(n("src4/@int:z"), Exec::Int("z")),
                md(rd(n("i1"), "src4/@int:z"), Mod::Identity),
            ],
            vec![exec("i1 4")],
        ),
        scenario(
            "redirect_leaf_chain_dead_end",
            vec![
                ex(n("deep/@int:q"), Exec::Int("q")),
                fk(md(rd(n("m1"), "deep/@int:q"), Mod::Dup(2))),
                md(rd(n("m2"), "m1"), Mod::Identity),
            ],
            vec![exec("m2 3"), exec("m2 9")],
        ),
        // -- real fork/modifier sums: redirects back at the root, the input
        // walking the redirect literals and finishing on the root-level
        // executable argument --
        scenario(
            "fork_root_sums",
            vec![
                ex(n("@int:v"), Exec::Int("v")),
                fk(md(rd(n("fr"), "."), Mod::Dup(3))),
                md(rd(n("fn"), "."), Mod::Dup(3)),
            ],
            vec![
                exec("fr 2"),
                exec("fr 5"),
                exec("fn 2"),
                exec("fn 0"),
                parse("fr 2"),
                parse("fn 5"),
                usage(false, "."),
                smart("."),
            ],
        ),
        // -- suggestions ---------------------------------------------------------
        scenario(
            "sug_literals_basic",
            vec![
                ex(n("foo"), Exec::Const(1)),
                ex(n("bar"), Exec::Const(1)),
                ex(n("baz"), Exec::Const(1)),
            ],
            vec![
                sug(0, ""),
                sug(1, "f"),
                sug(1, "b"),
                sug(2, "ba"),
                sug(4, "foo "),
                sug(1, "z"),
                sug(3, "FOO"),
            ],
        ),
        scenario(
            "sug_int_arg",
            vec![ex(n("num/@int:value:0:10"), Exec::Const(1))],
            vec![
                sug(4, "num "),
                sug(5, "num 1"),
                sug(5, "num 9"),
                sug(6, "num 12"),
                sug(6, "num 99"),
                sug(5, "num 5"),
            ],
        ),
        scenario(
            "sug_bool_arg",
            vec![ex(n("flag/@bool:v"), Exec::Const(1))],
            vec![
                sug(5, "flag "),
                sug(6, "flag t"),
                sug(6, "flag f"),
                sug(7, "flag fa"),
            ],
        ),
        scenario(
            "sug_custom_list_provider",
            vec![sg(ex(n("cmd/@word:w"), Exec::Const(1)), "alpha|beta|gamma")],
            vec![sug(4, "cmd "), sug(8, "cmd alph")],
        ),
        scenario(
            "sug_custom_tooltips",
            vec![sg(
                ex(n("cmd2/@word:w"), Exec::Const(1)),
                "alpha=tipA|beta=tipB",
            )],
            vec![sug(5, "cmd2 ")],
        ),
        scenario(
            "sug_merge_providers",
            vec![
                n("start"),
                sg(ex(n("start/@word:a"), Exec::Const(1)), "x|y|z"),
                sg(ex(n("start/@word:b"), Exec::Const(2)), "z|w"),
            ],
            vec![sug(6, "start "), sug(7, "start x")],
        ),
        scenario(
            "sug_moving_cursor",
            vec![ex(n("hello"), Exec::Const(1))],
            vec![
                sug(0, "hello"),
                sug(1, "hello"),
                sug(2, "hello"),
                sug(3, "hello"),
                sug(4, "hello"),
                sug(5, "hello"),
            ],
        ),
        scenario(
            "sug_word_arg_default_empty",
            vec![ex(n("w/@word:x"), Exec::Const(1))],
            vec![sug(2, "w ")],
        ),
        scenario(
            "sug_cyrillic_exact",
            vec![
                ex(n("привет"), Exec::Const(1)),
                ex(n("пока"), Exec::Const(1)),
                ex(n("Флаг"), Exec::Const(1)),
            ],
            vec![
                sug(1, "п"),
                sug(0, ""),
                sug(1, "ф"),
                sug(1, "П"),
                parse("флаг"),
                parse("Флаг"),
                exec("Флаг"),
            ],
        ),
        scenario(
            "sug_redirect_partial",
            vec![
                ex(n("real/@int:x"), Exec::Const(1)),
                rd(n("alias"), "real/@int:x"),
            ],
            vec![sug(6, "alias "), sug(5, "alias")],
        ),
        // -- usage ---------------------------------------------------------------
        scenario(
            "usage_full_tree",
            vec![
                ex(n("a"), Exec::Const(1)),
                ex(n("b/@int:i"), Exec::Int("i")),
                ex(n("c/@long:j/k"), Exec::Const(1)),
                rq(ex(n("d"), Exec::Const(1)), Req::Never),
                ex(n("e/@float:f/g/@double:h/i"), Exec::Const(1)),
                ex(n("f/1/ii"), Exec::Const(1)),
                ex(n("f/2/i"), Exec::Const(1)),
                ex(n("f/3/iii"), Exec::Const(1)),
                ex(n("g/h"), Exec::Const(1)),
                ex(n("g/i"), Exec::Const(1)),
                ex(n("g/@int:j"), Exec::Int("j")),
                ex(n("j0/@int:k/l"), Exec::Const(1)),
                ex(n("j0/@string:m"), Exec::StrLen("m")),
                rd(n("n/@int:o"), "."),
                ex(n("n"), Exec::Const(1)),
                ex(n("p/@string:q/r"), Exec::Const(1)),
                ex(n("s/@greedy:t"), Exec::StrLen("t")),
            ],
            vec![
                usage(false, "."),
                usage(true, "."),
                smart("."),
                smart("g"),
                smart("f"),
                usage(false, "f"),
                usage(false, "n"),
                usage(false, "c"),
                smart("j0"),
                path("e/@float:f/g/@double:h/i"),
                find("b,i"),
                find("n,o"),
                find("zzz"),
                find("b,zzz"),
                find("-"),
                path("."),
            ],
        ),
        scenario(
            "path_find_roundtrip",
            vec![
                ex(n("a/@int:x/y"), Exec::Const(1)),
                ex(n("b"), Exec::Const(1)),
                ex(n("b/@word:w"), Exec::Const(2)),
            ],
            vec![
                path("a/@int:x/y"),
                path("a/@int:x"),
                path("a"),
                path("b"),
                path("b/@word:w"),
                path("."),
                find("a,x,y"),
                find("b,w"),
                find("zzz"),
                find("b,zzz"),
                find("-"),
                find("a"),
            ],
        ),
        // -- parse error plumbing ------------------------------------------------
        scenario(
            "parse_multi_candidate_errors",
            vec![
                ex(n("m/@int:i"), Exec::Const(1)),
                ex(n("m/@bool:f"), Exec::Const(2)),
            ],
            vec![parse("m x"), exec("m x"), parse("m 5"), exec("m 5")],
        ),
        scenario(
            "expected_separator",
            vec![ex(n("num/@int:v"), Exec::Int("v"))],
            vec![
                parse("num 5x"),
                exec("num 5x"),
                parse("num 5 6"),
                exec("num 5 6"),
                parse("num 5"),
            ],
        ),
        scenario(
            "empty_tree",
            vec![],
            vec![
                exec(""),
                parse(""),
                sug(0, ""),
                usage(false, "."),
                smart("."),
                path("."),
                find("-"),
            ],
        ),
        // -- non-ASCII -----------------------------------------------------------
        scenario(
            "cyrillic_arg_word",
            vec![ex(n("имя/@word:значение"), Exec::StrLen("значение"))],
            vec![exec("имя привет"), exec("имя hello"), parse("имя привет")],
        ),
        scenario(
            "quoted_cyrillic",
            vec![ex(n("ц/@string:строка"), Exec::StrLen("строка"))],
            vec![exec("ц \"привет мир\""), exec("ц привет")],
        ),
        // -- astral input: semantic comparison only (Mode::Semantic) --------------
        scenario(
            "emoji_literal_semantic",
            vec![ex(n("🚀"), Exec::Const(1)), ex(n("🚀_да"), Exec::Const(2))],
            vec![
                exec("🚀"),
                parse("🚀"),
                parse("🚀x"),
                parse("x"),
                exec("🚀_да"),
            ],
        ),
        scenario(
            "emoji_quoted_semantic",
            vec![ex(n("q/@string:s"), Exec::Const(1))],
            vec![
                exec("q \"привет 🌍\""),
                parse("q \"привет 🌍\""),
                exec("q \"🌍\""),
            ],
        ),
        // -- task 21: requirements true/false through can_use ---------------------
        scenario(
            "req_true_node",
            vec![rq(ex(n("open"), Exec::Const(1)), Req::Always)],
            vec![
                exec("open"),
                parse("open"),
                usage(false, "."),
                usage(true, "."),
                smart("."),
            ],
        ),
        scenario(
            "req_true_false_siblings",
            vec![
                rq(ex(n("open"), Exec::Const(1)), Req::Always),
                rq(ex(n("closed"), Exec::Const(2)), Req::Never),
            ],
            vec![
                exec("open"),
                exec("closed"),
                usage(false, "."),
                usage(true, "."),
                smart("."),
                // empty input never visits a child, so the restricted literal
                // still shows up in suggestions at the root
                sug(0, ""),
                sug(1, "o"),
                sug(1, "c"),
                parse("closed"),
                parse("open"),
            ],
        ),
        scenario(
            "req_eq_on_intermediate_merge",
            vec![
                // the parent carries the requirement; the later registration
                // merges its child into the existing (restricted) parent
                rq(n("outer"), Req::Eq(5)),
                ex(n("outer/inner"), Exec::Const(3)),
            ],
            vec![
                exec_s(5, "outer inner"),
                exec_s(7, "outer inner"),
                parse_s(7, "outer inner"),
                usage_s(5, true, "."),
                usage_s(7, true, "."),
                smart_s(5, "."),
                smart_s(7, "."),
                usage_s(5, true, "outer"),
                find("outer,inner"),
            ],
        ),
        scenario(
            "req_on_argument_node",
            vec![
                n("gate"),
                sg(
                    rq(ex(n("gate/@int:v"), Exec::Int("v")), Req::Eq(2)),
                    "i:5|i:6",
                ),
            ],
            vec![
                exec_s(2, "gate 7"),
                exec_s(3, "gate 7"),
                parse_s(3, "gate 7"),
                // the requirement filters the candidate BEFORE its parse: for
                // source 3 the argument is skipped entirely (empty errors
                // map), for source 2 its parse error lands in the map
                parse_s(2, "gate "),
                parse_s(3, "gate "),
                usage_s(2, true, "."),
                usage_s(3, true, "."),
                smart_s(2, "gate"),
                smart_s(3, "gate"),
                // suggestions never re-check canUse: both sources see the
                // provider's list
                sug_s(2, 5, "gate "),
                sug_s(3, 5, "gate "),
            ],
        ),
        scenario(
            "req_nested_parent_and_child",
            vec![
                rq(n("hub"), Req::Always),
                rq(ex(n("hub/@int:v"), Exec::Int("v")), Req::Eq(9)),
                rq(ex(n("hub/@string:w"), Exec::StrLen("w")), Req::Never),
            ],
            vec![
                exec_s(9, "hub 5"),
                exec_s(8, "hub 5"),
                exec_s(9, "hub hello"),
                exec_s(9, "hub"),
                usage_s(9, true, "."),
                usage_s(8, true, "."),
                smart_s(9, "hub"),
            ],
        ),
        scenario(
            "req_restricted_redirect_target",
            vec![
                rq(ex(n("real/@int:v"), Exec::Int("v")), Req::Never),
                rd(n("alias"), "real/@int:v"),
            ],
            vec![
                exec("alias 5"),
                exec("real 5"),
                parse("alias 5"),
                sug(6, "alias "),
                usage(false, "."),
                smart("."),
            ],
        ),
        // -- task 21: nested redirects --------------------------------------------
        scenario(
            "redirect_chain_4_hops",
            vec![
                ex(n("one/@int:a"), Exec::Int("a")),
                rd(n("two"), "one/@int:a"),
                rd(n("three"), "two"),
                rd(n("four"), "three"),
            ],
            vec![
                exec("four 9"),
                exec("three 4"),
                exec("two 2"),
                exec("one 1"),
                exec("four"),
                parse("four 9"),
                parse("four"),
                usage(false, "."),
                usage(false, "one/@int:a"),
                usage(false, "two"),
                usage(false, "three"),
                usage(false, "four"),
                smart("."),
                smart("four"),
                path("four"),
                find("four"),
            ],
        ),
        scenario(
            "redirect_node_with_own_child",
            vec![
                ex(n("one/@int:a"), Exec::Int("a")),
                rd(n("two"), "one/@int:a"),
                // the later registration merges the string child into the
                // existing redirect node: `two` keeps its redirect AND gains
                // a child, so the parse must pick between the two shapes
                ex(n("two/@string:b"), Exec::StrLen("b")),
            ],
            vec![
                exec("two 5"),
                exec("two abc"),
                exec("two abc 5"),
                exec("two 5 abc"),
                parse("two abc"),
                parse("two 5"),
                parse("two abc 5"),
                usage(false, "."),
                smart("."),
                path("two/@string:b"),
                find("two,b"),
            ],
        ),
        scenario(
            "redirect_arg_node_with_redirect",
            vec![
                ex(n("one/@int:a"), Exec::Int("a")),
                rd(n("two"), "one/@int:a"),
                // an argument node that itself redirects: `three <w>` parses w
                // and then continues into the redirect chain
                rd(ex(n("three/@word:w"), Exec::StrLen("w")), "two"),
            ],
            vec![
                exec("three abc 5"),
                exec("three abc"),
                exec("three 5"),
                parse("three abc 5"),
                parse("three abc"),
                usage(false, "."),
                path("three/@word:w"),
                smart("."),
            ],
        ),
        scenario(
            "redirect_chain_suggestions_deep",
            vec![
                sg(ex(n("@int:a"), Exec::Int("a")), "i:1|i:2|i:3"),
                rd(n("four"), "@int:a"),
                rd(n("three"), "four"),
            ],
            vec![
                sug(6, "three "),
                sug(5, "three"),
                sug(7, "three 1"),
                sug(5, "four "),
                sug(2, "a "),
            ],
        ),
        // -- task 21: merge / add_child shapes -------------------------------------
        scenario(
            "merge_deep_shared_prefix",
            vec![
                ex(n("foo/bar/one"), Exec::Const(1)),
                ex(n("foo/bar/two"), Exec::Const(2)),
                ex(n("foo/baz"), Exec::Const(3)),
                ex(n("foo"), Exec::Const(4)),
            ],
            vec![
                exec("foo bar one"),
                exec("foo bar two"),
                exec("foo baz"),
                exec("foo"),
                exec("foo bar"),
                usage(false, "."),
                usage(false, "foo"),
                smart("."),
                smart("foo"),
                smart("foo/bar"),
                path("foo/bar/one"),
                find("foo,bar,one"),
                find("foo,bar"),
            ],
        ),
        scenario(
            "merge_arg_same_name_replaces_command",
            vec![
                // the same-named argument keeps the FIRST parser (int) and the
                // LATER command (addChild copies the command when non-null)
                n("x/@int:v"),
                ex(n("x/@string:v"), Exec::Const(9)),
            ],
            vec![
                exec("x 5"),
                exec("x abc"),
                parse("x 5"),
                parse("x abc"),
                usage(false, "."),
                smart("."),
                path("x/@int:v"),
                find("x,v"),
            ],
        ),
        scenario(
            "merge_literal_exec_then_child",
            vec![
                ex(n("m"), Exec::Const(1)),
                ex(n("m/@int:v"), Exec::Int("v")),
            ],
            vec![
                exec("m"),
                exec("m 5"),
                exec("m 5 6"),
                parse("m 5 6"),
                usage(false, "."),
                smart("."),
                path("m/@int:v"),
            ],
        ),
        scenario(
            "merge_insertion_order_preserved",
            vec![
                ex(n("a/one"), Exec::Const(1)),
                ex(n("a/two"), Exec::Const(2)),
                ex(n("a/three"), Exec::Const(3)),
                // merging a grandchild into an existing child must not move
                // that child's position in the insertion order
                ex(n("a/one/deep"), Exec::Const(4)),
                ex(n("a/two/extra"), Exec::Const(5)),
            ],
            vec![
                usage(false, "a"),
                smart("a"),
                usage(false, "."),
                exec("a one deep"),
                exec("a two extra"),
                exec("a three"),
                path("a/one/deep"),
                find("a,one,deep"),
            ],
        ),
        scenario(
            "merge_nested_command_replacement",
            vec![
                ex(n("p/q"), Exec::Const(1)),
                ex(n("p/q"), Exec::Const(9)),
                ex(n("p/q/r"), Exec::Const(2)),
            ],
            vec![
                exec("p q"),
                exec("p q r"),
                usage(false, "."),
                smart("p"),
                path("p/q/r"),
            ],
        ),
        scenario(
            "merge_same_arg_name_two_providers",
            vec![
                // the merged argument node keeps the first provider entirely
                // (addChild copies only the command and the grandchildren)
                sg(ex(n("start/@int:x"), Exec::Int("x")), "i:1|i:2"),
                sg(ex(n("start/@int:x"), Exec::Const(9)), "i:8|i:9"),
            ],
            vec![exec("start 3"), sug(6, "start "), parse("start 5")],
        ),
        // -- task 21: IntegerSuggestion mechanics (int providers on min/max args) --
        scenario(
            "sug_int_provider_bounded_arg",
            vec![sg(ex(n("num/@int:v:0:10"), Exec::Const(1)), "i:0|i:5|i:10")],
            vec![
                sug(4, "num "),
                sug(5, "num 1"),
                sug(5, "num 9"),
                sug(5, "num 0"),
                sug(6, "num 10"),
                sug(5, "num 99"),
            ],
        ),
        scenario(
            "sug_int_mixed_text_int_order",
            // the non-transitive compare-to-ignore-case comparator over a
            // mixed integer/text set: Java's order comes out of the HashSet
            // iteration + TimSort small-array path, byte for byte
            vec![sg(
                ex(n("mix/@word:w"), Exec::Const(1)),
                "i:3|i:8|3a|30|i:30",
            )],
            vec![sug(4, "mix "), sug(5, "mix 3")],
        ),
        scenario(
            "sug_int_negative_bounds",
            vec![sg(
                ex(n("num/@int:v:-10:-1"), Exec::Const(1)),
                "i:-10|i:-5|i:-1",
            )],
            vec![
                sug(4, "num "),
                sug(5, "num -"),
                sug(5, "num -5"),
                sug(6, "num -10"),
            ],
        ),
        scenario(
            "sug_int_single_value_range",
            vec![sg(ex(n("num/@int:v:5:5"), Exec::Const(1)), "i:5")],
            vec![sug(4, "num "), sug(5, "num 5"), sug(5, "num 6")],
        ),
        scenario(
            "sug_int_with_tooltip",
            vec![sg(
                ex(n("num/@int:v:0:100"), Exec::Const(1)),
                "i:7=seven|i:14=fortnight",
            )],
            vec![sug(4, "num "), sug(5, "num 7")],
        ),
        scenario(
            "sug_int_min_only_arg",
            vec![sg(
                ex(n("num/@int:v:100:-"), Exec::Const(1)),
                "i:100|i:1000",
            )],
            vec![sug(4, "num "), sug(6, "num 10")],
        ),
        // -- task 21: escape sequences in quoted strings ---------------------------
        scenario(
            "quoted_escape_matrix",
            vec![ex(n("q/@string:s"), Exec::StrLen("s"))],
            vec![
                exec(r#"q "a\\b""#),
                exec(r#"q "a\"b""#),
                exec(r#"q "a""#),
                exec("q \"a\\"),
                exec("q 'a\"b'"),
                exec("q 'a\\'b'"),
                exec("q 'a\\\"b'"),
                exec("q 'a\\zb'"),
                exec("q 'a\\"),
                exec(r#"q "a\"""#),
            ],
        ),
        scenario(
            "quoted_unquoted_stop_at_quote",
            vec![ex(n("q/@string:s"), Exec::StrLen("s"))],
            vec![
                // the unquoted word stops at the quote and the leftover makes
                // the parse fail with the separator error
                exec(r#"q a\"b"#),
                exec("q a'b"),
                parse(r#"q a\"b"#),
            ],
        ),
        scenario(
            "greedy_ignores_escapes",
            vec![ex(n("g/@greedy:s"), Exec::StrLen("s"))],
            vec![
                exec(r#"g a\"b"#),
                exec(r#"g "a\"b""#),
                exec(r#"g "a\\b""#),
                exec("g 'a\"b'"),
            ],
        ),
        // -- task 21: N-depth fork chains through execute_all ----------------------
        // every redirect points back at the root, so the redirect parse
        // re-enters the root and walks the remaining literals down to the
        // root-level executable argument — the shape where modifiers and
        // fork sums actually engage (see the dead-end note above)
        scenario(
            "chain_fork_3_stage",
            vec![
                ex(n("@int:v"), Exec::Int("v")),
                md(rd(n("f3"), "."), Mod::Dup(3)),
                md(rd(n("f2"), "."), Mod::Identity),
                fk(md(rd(n("f1"), "."), Mod::Dup(2))),
            ],
            vec![
                exec("f3 f2 f1 3"),
                exec("f3 f2 f1 0"),
                exec("f3 f1 3"),
                exec("f2 f1 3"),
                parse("f3 f2 f1 3"),
                usage(false, "."),
                smart("."),
            ],
        ),
        scenario(
            "chain_fork_4_stage",
            vec![
                ex(n("@int:v"), Exec::Int("v")),
                md(rd(n("g4"), "."), Mod::Dup(2)),
                md(rd(n("g3"), "."), Mod::Identity),
                fk(md(rd(n("g2"), "."), Mod::Identity)),
                fk(md(rd(n("g1"), "."), Mod::Dup(2))),
            ],
            vec![
                exec("g4 g3 g2 g1 7"),
                exec("g4 g2 g1 7"),
                exec("g4 g3 g2 g1 0"),
                exec("g2 g1 7"),
                parse("g4 g3 g2 g1 7"),
                usage(false, "."),
                smart("."),
            ],
        ),
        // -- task 21: misc parity pins ----------------------------------------------
        scenario(
            "int_boundary_parse_map",
            vec![ex(n("n/@int:v"), Exec::Const(1))],
            vec![
                parse("n 2147483647"),
                exec("n 2147483647"),
                parse("n 2147483648"),
                parse("n -2147483648"),
                parse("n -2147483649"),
                exec("n -2147483649"),
                parse("n +5"),
                exec("n +5"),
                parse("n 0005"),
                exec("n 0005"),
            ],
        ),
        scenario(
            "double_scientific_forms",
            vec![ex(n("d/@double:x"), Exec::Const(1))],
            vec![
                exec("d 1e2"),
                exec("d 1.5e2"),
                exec("d 1E2"),
                exec("d 0.5e-1"),
                exec("d -1.5e1"),
                parse("d 1e2"),
            ],
        ),
        scenario(
            "word_allowed_chars_matrix",
            vec![ex(n("w/@word:s"), Exec::StrLen("s"))],
            vec![
                exec("w a.b-c+d_e"),
                exec("w A1_b2.c3-d4+e5"),
                exec("w ."),
                exec("w -"),
                exec("w +"),
                exec("w _"),
            ],
        ),
        scenario(
            "bool_case_and_prefixes",
            vec![ex(n("flag/@bool:v"), Exec::Bool("v"))],
            vec![
                exec("flag t"),
                exec("flag T"),
                parse("flag True"),
                exec("flag f"),
                parse("flag FALSE"),
            ],
        ),
    ]
}

// ---------------------------------------------------------------------------
// wire protocol (request encoding + response parsing)
// ---------------------------------------------------------------------------

fn encode_exec(exec: &Exec) -> String {
    match exec {
        Exec::Const(value) => value.to_string(),
        Exec::Int(name) => format!("int:{name}"),
        Exec::Long(name) => format!("long:{name}"),
        Exec::StrLen(name) => format!("strlen:{name}"),
        Exec::Bool(name) => format!("bool:{name}"),
    }
}

fn encode_req(req: &Req) -> String {
    match req {
        Req::Always => "true".to_owned(),
        Req::Never => "false".to_owned(),
        Req::Eq(value) => format!("eq:{value}"),
    }
}

fn encode_modifier(modifier: Mod) -> String {
    match modifier {
        Mod::None => "none".to_owned(),
        Mod::Identity => "identity".to_owned(),
        Mod::Dup(count) => format!("dup:{count}"),
        Mod::Empty => "empty".to_owned(),
    }
}

fn encode_node(node: &NodeSpec) -> String {
    let mut line = format!("N {}", node.path);
    if let Some(exec) = &node.exec {
        line += &format!(" exec={}", encode_exec(exec));
    }
    if let Some(req) = &node.req {
        line += &format!(" req={}", encode_req(req));
    }
    if let Some(red) = node.red {
        line += &format!(" red={red}");
    }
    if node.modifier != Mod::None {
        line += &format!(" mod={}", encode_modifier(node.modifier));
    }
    if node.fork {
        line += " fork=1";
    }
    if let Some(provider) = node.sug {
        line += &format!(" sug=list:{provider}");
    }
    line.push('\n');
    line
}

fn encode_op(op: &Op) -> String {
    let body = match op {
        Op::Parse { src, input } => format!("parse {src} {input}"),
        Op::Exec { src, input } => format!("exec {src} {input}"),
        Op::Sug { src, cursor, input } => format!("sug {src} {cursor} {input}"),
        Op::Usage {
            restricted,
            src,
            node,
        } => format!("usage {} {src} {node}", if *restricted { "1" } else { "0" }),
        Op::Smart { src, node } => format!("smart {src} {node}"),
        Op::Path { node } => format!("path {node}"),
        Op::Find { path } => {
            format!(
                "find {}",
                if path.is_empty() {
                    "-".to_owned()
                } else {
                    path.clone()
                }
            )
        }
    };
    format!("O {body}\n")
}

fn encode_request(scenarios: &[Scenario]) -> String {
    // the handshake line is sent separately by run_java_oracle (it must hit
    // the pipe before the harness acks)
    let mut out = String::new();
    for sc in scenarios {
        out += &format!("S {}\n", sc.id);
        for node in &sc.nodes {
            out += &encode_node(node);
        }
        for op in &sc.ops {
            out += &encode_op(op);
        }
        out += "E\n";
    }
    out += "DONE\n";
    out
}

// ---------------------------------------------------------------------------
// structured result lines
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum ResultLine {
    Exceptions(usize),
    ErrEntry { node: String, cursor: Option<i64> },
    ErrCtx(Option<String>),
    ErrRaw(String),
    Reader { cursor: usize, can_read: bool },
    CtxRange { start: usize, end: usize },
    ExecOk(i32),
    ExecErrCursor(Option<i64>),
    ExecErrCtx(Option<String>),
    ExecErrRaw(String),
    SugRange { start: usize, end: usize },
    SugText(String),
    SugTooltip(Option<String>),
    UsageCount(usize),
    UsageLine(String),
    SmartCount(usize),
    SmartLine { child: String, usage: String },
    PathLine(Vec<String>),
    Found(bool),
}

impl ResultLine {
    fn render(&self) -> String {
        match self {
            ResultLine::Exceptions(count) => format!("exceptions = {count}"),
            ResultLine::ErrEntry { node, cursor } => {
                format!("error on node '{node}', cursor = {}", cursor.unwrap_or(-1))
            }
            ResultLine::ErrCtx(ctx) => {
                format!("error context = {}", ctx.as_deref().unwrap_or("<none>"))
            }
            ResultLine::ErrRaw(raw) => format!("error message = {raw}"),
            ResultLine::Reader { cursor, can_read } => {
                format!("reader cursor = {cursor}, can_read = {can_read}")
            }
            ResultLine::CtxRange { start, end } => format!("context range = [{start}, {end})"),
            ResultLine::ExecOk(value) => format!("execute = {value}"),
            ResultLine::ExecErrCursor(cursor) => {
                format!("execute error cursor = {}", cursor.unwrap_or(-1))
            }
            ResultLine::ExecErrCtx(ctx) => format!(
                "execute error context = {}",
                ctx.as_deref().unwrap_or("<none>")
            ),
            ResultLine::ExecErrRaw(raw) => format!("execute error message = {raw}"),
            ResultLine::SugRange { start, end } => {
                format!("suggestion range = [{start}, {end})")
            }
            ResultLine::SugText(text) => format!("suggestion = '{text}'"),
            ResultLine::SugTooltip(tooltip) => format!(
                "suggestion tooltip = {}",
                tooltip.as_deref().unwrap_or("<none>")
            ),
            ResultLine::UsageCount(count) => format!("usage count = {count}"),
            ResultLine::UsageLine(line) => format!("usage entry = '{line}'"),
            ResultLine::SmartCount(count) => format!("smart usage count = {count}"),
            ResultLine::SmartLine { child, usage } => {
                format!("smart usage entry ({child}) = '{usage}'")
            }
            ResultLine::PathLine(path) => format!("path = {path:?}"),
            ResultLine::Found(found) => format!("found = {found}"),
        }
    }
}

/// Split a response line into its kind tag and the rest of the line.
fn split_tag(line: &str) -> (&str, &str) {
    match line.split_once(' ') {
        Some((tag, rest)) => (tag, rest),
        None => (line, ""),
    }
}

fn parse_cursor_field(field: &str) -> Option<i64> {
    if field == "-1" {
        None
    } else {
        Some(field.parse().expect("java cursor must be an integer"))
    }
}

fn parse_ctx_field(field: &str) -> Option<String> {
    if field == "-" {
        None
    } else {
        Some(field.to_owned())
    }
}

fn expect_line(lines: &[String], at: usize, expected: &str) -> String {
    lines
        .get(at)
        .unwrap_or_else(|| panic!("java output ended early at line {at}, expected {expected}"))
        .clone()
}

fn expect_tag<'a>(line: &'a str, expected: &str) -> (&'a str, &'a str) {
    let (tag, rest) = split_tag(line);
    assert_eq!(
        tag, expected,
        "java output line mismatch: expected '{expected}', got '{line}'"
    );
    (tag, rest)
}

/// Consume the Java response lines for one op, mirroring the harness output
/// grammar (see tests/java_oracle/README.md). Returns the parsed lines and
/// the next read position.
fn parse_java_op(op: &Op, lines: &[String], at: usize) -> (Vec<ResultLine>, usize) {
    let mut cursor = at;
    match op {
        Op::Parse { .. } => {
            let count_line = expect_line(lines, cursor, "X <count>");
            cursor += 1;
            let (_, rest) = expect_tag(&count_line, "X");
            let count: usize = rest.parse().expect("X count");
            let mut out = vec![ResultLine::Exceptions(count)];
            for _ in 0..count {
                let entry = expect_line(lines, cursor, "Xe");
                cursor += 1;
                let (_, rest) = expect_tag(&entry, "Xe");
                let (node, cursor_field) = rest.split_once(' ').expect("Xe fields");
                out.push(ResultLine::ErrEntry {
                    node: node.to_owned(),
                    cursor: parse_cursor_field(cursor_field),
                });
                let ctx = expect_line(lines, cursor, "Xc");
                cursor += 1;
                let (_, rest) = expect_tag(&ctx, "Xc");
                out.push(ResultLine::ErrCtx(parse_ctx_field(rest)));
                let raw = expect_line(lines, cursor, "Xm");
                cursor += 1;
                let (_, rest) = expect_tag(&raw, "Xm");
                out.push(ResultLine::ErrRaw(rest.to_owned()));
            }
            let reader = expect_line(lines, cursor, "Xr");
            cursor += 1;
            let (_, rest) = expect_tag(&reader, "Xr");
            let (cursor_field, can_read_field) = rest.split_once(' ').expect("Xr fields");
            out.push(ResultLine::Reader {
                cursor: cursor_field.parse().expect("Xr cursor"),
                can_read: can_read_field == "1",
            });
            let range = expect_line(lines, cursor, "Xg");
            cursor += 1;
            let (_, rest) = expect_tag(&range, "Xg");
            let (start, end) = rest.split_once(' ').expect("Xg fields");
            out.push(ResultLine::CtxRange {
                start: start.parse().expect("Xg start"),
                end: end.parse().expect("Xg end"),
            });
            (out, cursor)
        }
        Op::Exec { .. } => {
            let first = expect_line(lines, cursor, "R|Rerr");
            cursor += 1;
            let (tag, rest) = split_tag(&first);
            match tag {
                "R" => (
                    vec![ResultLine::ExecOk(rest.parse().expect("R value"))],
                    cursor,
                ),
                "Rerr" => {
                    let mut out = vec![ResultLine::ExecErrCursor(parse_cursor_field(rest))];
                    let ctx = expect_line(lines, cursor, "Rc");
                    cursor += 1;
                    let (_, rest) = expect_tag(&ctx, "Rc");
                    out.push(ResultLine::ExecErrCtx(parse_ctx_field(rest)));
                    let raw = expect_line(lines, cursor, "Rm");
                    cursor += 1;
                    let (_, rest) = expect_tag(&raw, "Rm");
                    out.push(ResultLine::ExecErrRaw(rest.to_owned()));
                    (out, cursor)
                }
                other => panic!("expected 'R' or 'Rerr', got '{other}'"),
            }
        }
        Op::Sug { .. } => {
            let first = expect_line(lines, cursor, "Y");
            cursor += 1;
            let (_, rest) = expect_tag(&first, "Y");
            let (start, end) = rest.split_once(' ').expect("Y fields");
            let mut out = vec![ResultLine::SugRange {
                start: start.parse().expect("Y start"),
                end: end.parse().expect("Y end"),
            }];
            while let Some(line) = lines.get(cursor) {
                let (tag, rest) = split_tag(line);
                match tag {
                    "Ys" => {
                        cursor += 1;
                        out.push(ResultLine::SugText(rest.to_owned()));
                    }
                    "Yt" => {
                        cursor += 1;
                        out.push(ResultLine::SugTooltip(parse_ctx_field(rest)));
                    }
                    _ => break,
                }
            }
            (out, cursor)
        }
        Op::Usage { .. } => {
            let first = expect_line(lines, cursor, "U");
            cursor += 1;
            let (_, rest) = expect_tag(&first, "U");
            let count: usize = rest.parse().expect("U count");
            let mut out = vec![ResultLine::UsageCount(count)];
            for _ in 0..count {
                let line = expect_line(lines, cursor, "Us");
                cursor += 1;
                let (_, rest) = expect_tag(&line, "Us");
                out.push(ResultLine::UsageLine(rest.to_owned()));
            }
            (out, cursor)
        }
        Op::Smart { .. } => {
            let first = expect_line(lines, cursor, "M");
            cursor += 1;
            let (_, rest) = expect_tag(&first, "M");
            let count: usize = rest.parse().expect("M count");
            let mut out = vec![ResultLine::SmartCount(count)];
            for _ in 0..count {
                let line = expect_line(lines, cursor, "Ms");
                cursor += 1;
                let (_, rest) = expect_tag(&line, "Ms");
                let (child, usage) = rest.split_once(' ').expect("Ms fields");
                out.push(ResultLine::SmartLine {
                    child: child.to_owned(),
                    usage: usage.to_owned(),
                });
            }
            (out, cursor)
        }
        Op::Path { .. } => {
            let first = expect_line(lines, cursor, "P");
            cursor += 1;
            let (_, rest) = expect_tag(&first, "P");
            let path = if rest == "-" {
                vec![]
            } else {
                rest.split(',').map(ToOwned::to_owned).collect()
            };
            (vec![ResultLine::PathLine(path)], cursor)
        }
        Op::Find { .. } => {
            let first = expect_line(lines, cursor, "F");
            cursor += 1;
            let (_, rest) = expect_tag(&first, "F");
            (vec![ResultLine::Found(rest == "1")], cursor)
        }
    }
}

/// Split the harness response into per-scenario line lists keyed by id.
fn split_response(response: &str) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, Vec<String>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in response.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(id) = line.strip_prefix("S ") {
            current = Some(id.to_owned());
            result.entry(id.to_owned()).or_default();
        } else if line == "E" {
            current = None;
        } else if line == "DONE" {
            break;
        } else if let Some(id) = &current {
            result.entry(id.clone()).or_default().push(line.to_owned());
        }
    }
    result
}

// ---------------------------------------------------------------------------
// java process management
// ---------------------------------------------------------------------------

fn java_lib_sources() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../brigadier/src/main/java");
    dir.is_dir().then_some(dir)
}

fn harness_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/java_oracle/OracleHarness.java")
}

fn classes_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/java-oracle/classes")
}

fn collect_java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_java_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("java") {
            out.push(path);
        }
    }
}

fn probe_available(program: &str) -> bool {
    Command::new(program).arg("-version").output().is_ok()
}

/// Whether the compiled classes are newer than every java source involved.
fn classes_fresh(sources: &[PathBuf], stamp: &Path) -> bool {
    let Ok(stamp_meta) = std::fs::metadata(stamp) else {
        return false;
    };
    let Ok(stamp_time) = stamp_meta.modified() else {
        return false;
    };
    sources.iter().all(|source| {
        std::fs::metadata(source)
            .and_then(|meta| meta.modified())
            .map(|time| time <= stamp_time)
            .unwrap_or(false)
    })
}

/// Compile the Java library plus the harness into the cached classes dir.
/// The `Err` variant is the skip reason for the test.
fn ensure_java_classes() -> Result<PathBuf, &'static str> {
    let Some(lib_dir) = java_lib_sources() else {
        return Err(
            "the Java library sources were not found at ../brigadier/src/main/java \
             (set the tree up next to the crate to run the oracle)",
        );
    };
    let harness = harness_source();
    let mut sources = Vec::new();
    collect_java_files(&lib_dir, &mut sources);
    sources.push(harness);

    let classes = classes_dir();
    let stamp = classes.join(".compiled");
    if classes.join("OracleHarness.class").exists() && classes_fresh(&sources, &stamp) {
        return Ok(classes);
    }

    if !probe_available("javac") {
        return Err("no `javac` on PATH and no cached oracle classes under \
             target/java-oracle/classes; install a JDK or run \
             tests/java_oracle/build.cmd once to produce the classes");
    }

    let _ = std::fs::remove_dir_all(&classes);
    std::fs::create_dir_all(&classes).expect("create java classes dir");

    let mut compile = Command::new("javac");
    compile
        .arg("-encoding")
        .arg("UTF-8")
        .arg("-d")
        .arg(&classes);
    for source in &sources {
        compile.arg(source);
    }
    let output = compile
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .expect("spawn javac (probed as available)");
    if !output.status.success() {
        panic!(
            "javac failed with {}:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::File::create(&stamp).expect("write compile stamp");
    Ok(classes)
}

/// Read the harness handshake line through a buffered reader, returning the
/// reader for the stdout-consuming thread afterwards.
fn read_handshake(
    mut reader: std::io::BufReader<ChildStdout>,
) -> Result<std::io::BufReader<ChildStdout>, String> {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => Err("java harness closed stdout before the handshake".to_owned()),
        Ok(_) => {
            let line = line.trim_end_matches(['\r', '\n']).to_owned();
            if line == "OK BRIGAXSIR-ORACLE v1" {
                Ok(reader)
            } else {
                Err(format!("java harness handshake mismatch: '{line}'"))
            }
        }
        Err(err) => Err(format!("reading the java harness handshake failed: {err}")),
    }
}

/// Run the harness over the whole request with a hard timeout. A timeout or
/// a harness error is a test failure, not a skip: the harness is local and
/// deterministic, so hanging means a bug somewhere.
fn run_java_oracle(classes: &Path, request: &str) -> String {
    let mut child = Command::new("java")
        .arg("-cp")
        .arg(classes)
        .arg("OracleHarness")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn java (probed as available)");

    let mut stdin = child.stdin.take().expect("stdin piped");
    let stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");
    let request = request.to_owned();

    // send the handshake line first: the harness acks only after reading it
    let _ = stdin.write_all(b"BRIGAXSIR-ORACLE v1\n");
    let _ = stdin.flush();

    // handshake before feeding scenarios: a broken harness surfaces here
    // instead of as a silent hang
    let stdout = match read_handshake(std::io::BufReader::new(stdout)) {
        Ok(reader) => reader,
        Err(message) => {
            let _ = child.kill();
            let mut err_buffer = Vec::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_end(&mut err_buffer);
            }
            panic!(
                "{message}\n--- stderr ---\n{}",
                String::from_utf8_lossy(&err_buffer)
            );
        }
    };

    let reader_thread = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut stdout = stdout;
        let read_result = stdout.read_to_end(&mut buffer);
        (buffer, read_result.is_ok())
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let read_result = stderr.read_to_end(&mut buffer);
        (buffer, read_result.is_ok())
    });
    let writer_thread = std::thread::spawn(move || {
        let _ = stdin.write_all(request.as_bytes());
        let _ = stdin.flush();
        drop(stdin);
    });

    let deadline = Instant::now() + Duration::from_secs(180);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(err) => panic!("waiting on the java harness failed: {err}"),
        }
    };

    let (out_buffer, _) = reader_thread.join().expect("stdout reader thread");
    let (err_buffer, _) = stderr_thread.join().expect("stderr reader thread");
    writer_thread.join().expect("request writer thread");

    let response = String::from_utf8_lossy(&out_buffer).to_string();
    let stderr_text = String::from_utf8_lossy(&err_buffer).to_string();

    if timed_out {
        panic!("java oracle harness timed out after 180s\n--- stderr ---\n{stderr_text}");
    }
    let status = status.expect("non-timeout runs always have a status");
    if !status.success() {
        panic!(
            "java oracle harness exited with {status}\n--- stdout ---\n{response}\n--- stderr ---\n{stderr_text}"
        );
    }
    if response.contains("\nERR ") || response.starts_with("ERR ") {
        panic!("java oracle harness reported an error:\n{response}");
    }
    response
}

// ---------------------------------------------------------------------------
// the rust oracle: same scenarios, same wire grammar, real brigaxsir results
// ---------------------------------------------------------------------------

type NodeHandle = Arc<RwLock<CommandNode<i32, i32>>>;

/// One provider item: either a text suggestion (optionally with a tooltip) or
/// an integer suggestion (`i:<value>` in the spec, optionally with a tooltip)
/// — the int form exercises Java's `SuggestionsBuilder#suggest(int)` and the
/// `IntegerSuggestion` subtype.
struct ListProvider {
    items: Vec<ProviderItem>,
}

enum ProviderItem {
    Text(String, Option<String>),
    Int(i32, Option<String>),
}

impl SuggestionProvider<i32, i32> for ListProvider {
    fn get_suggestions(
        &self,
        _context: CommandContext<i32, i32>,
        mut builder: SuggestionsBuilder,
    ) -> SuggestionsFuture {
        for item in &self.items {
            builder = match item {
                ProviderItem::Text(word, Some(tooltip)) => builder.suggest_with_tooltip(
                    word.clone(),
                    Arc::new(LiteralMessage::new(tooltip.clone())) as Arc<dyn Message>,
                ),
                ProviderItem::Text(word, None) => builder.suggest(word.clone()),
                ProviderItem::Int(value, Some(tooltip)) => builder.suggest_integer_with_tooltip(
                    *value,
                    Arc::new(LiteralMessage::new(tooltip.clone())) as Arc<dyn Message>,
                ),
                ProviderItem::Int(value, None) => builder.suggest_integer(*value),
            };
        }
        builder.build_future()
    }
}

struct RustOracle {
    dispatcher: CommandDispatcher<i32, i32>,
    by_path: HashMap<String, NodeHandle>,
    reverse: Vec<(NodeHandle, String)>,
}

/// The command closure shape the scenario exec specs build (infallible —
/// Java's `Command<Integer>` with a constant result).
type CommandClosure = Box<dyn Fn(&CommandContext<i32, i32>) -> i32 + Send + Sync>;

/// The requirement closure shape (`Predicate<Integer>` in Java).
type RequirementClosure = Box<dyn Fn(&i32) -> bool + Send + Sync>;

fn make_command(exec: Exec) -> CommandClosure {
    match exec {
        Exec::Const(value) => Box::new(move |_| value),
        Exec::Int(name) => {
            Box::new(move |context: &CommandContext<i32, i32>| *context.get_argument::<i32>(name))
        }
        Exec::Long(name) => Box::new(move |context: &CommandContext<i32, i32>| {
            *context.get_argument::<i64>(name) as i32
        }),
        Exec::StrLen(name) => Box::new(move |context: &CommandContext<i32, i32>| {
            get_string(context, name).chars().count() as i32
        }),
        Exec::Bool(name) => Box::new(move |context: &CommandContext<i32, i32>| {
            if *context.get_argument::<bool>(name) {
                1
            } else {
                0
            }
        }),
    }
}

fn make_requirement(req: Req) -> RequirementClosure {
    match req {
        Req::Always => Box::new(|_source: &i32| true),
        Req::Never => Box::new(|_source: &i32| false),
        Req::Eq(value) => Box::new(move |source: &i32| *source == value),
    }
}

fn make_modifier(modifier: Mod) -> Option<Arc<brigaxsir::modifier::RedirectModifier<i32, i32>>> {
    match modifier {
        Mod::None => None,
        Mod::Identity => Some(Arc::new(|context| Ok(vec![context.get_source_arc()]))),
        Mod::Dup(count) => Some(Arc::new(move |context: &CommandContext<i32, i32>| {
            let source = *context.get_source_arc();
            Ok((0..count)
                .map(|offset| Arc::new(source + offset as i32))
                .collect::<Vec<_>>())
        })),
        Mod::Empty => Some(Arc::new(|_| Ok(vec![]))),
    }
}

/// A typed argument parser per the wire spec ("-" bound = absent), mirroring
/// the Java harness's `typeFor`.
fn arg_builder(
    spec: &ArgSpec,
    provider: Option<&'static str>,
) -> RequiredArgumentBuilder<i32, i32> {
    let builder = match (spec.ty, spec.min, spec.max) {
        ("int", "-", "-") => argument(spec.name, integer()),
        ("int", min, "-") => argument(
            spec.name,
            integer_min(min.parse::<i32>().expect("int bound")),
        ),
        ("int", min, max) => argument(
            spec.name,
            integer_range(
                min.parse::<i32>().expect("int bound"),
                max.parse::<i32>().expect("int bound"),
            ),
        ),
        ("long", "-", "-") => argument(spec.name, long()),
        ("long", min, "-") => {
            argument(spec.name, long_min(min.parse::<i64>().expect("long bound")))
        }
        ("long", min, max) => argument(
            spec.name,
            long_range(
                min.parse::<i64>().expect("long bound"),
                max.parse::<i64>().expect("long bound"),
            ),
        ),
        ("float", "-", "-") => argument(spec.name, float()),
        ("float", min, "-") => argument(
            spec.name,
            float_min(min.parse::<f32>().expect("float bound")),
        ),
        ("float", min, max) => argument(
            spec.name,
            float_range(
                min.parse::<f32>().expect("float bound"),
                max.parse::<f32>().expect("float bound"),
            ),
        ),
        ("double", "-", "-") => argument(spec.name, double()),
        ("double", min, "-") => argument(
            spec.name,
            double_min(min.parse::<f64>().expect("double bound")),
        ),
        ("double", min, max) => argument(
            spec.name,
            double_range(
                min.parse::<f64>().expect("double bound"),
                max.parse::<f64>().expect("double bound"),
            ),
        ),
        ("bool", "-", "-") => argument(spec.name, bool()),
        ("word", "-", "-") => argument(spec.name, word()),
        ("string", "-", "-") => argument(spec.name, string()),
        ("greedy", "-", "-") => argument(spec.name, greedy_string()),
        (other, ..) => panic!("unknown argument type in scenario: {other}"),
    };
    match provider {
        Some(provider) => builder.suggests(list_provider(provider)),
        None => builder,
    }
}

fn list_provider(spec: &'static str) -> ListProvider {
    ListProvider {
        items: spec
            .split('|')
            .map(|item| {
                if let Some(rest) = item.strip_prefix("i:") {
                    match rest.split_once('=') {
                        Some((value, tooltip)) => ProviderItem::Int(
                            value.parse::<i32>().expect("int suggestion value"),
                            Some(tooltip.to_owned()),
                        ),
                        None => ProviderItem::Int(
                            rest.parse::<i32>().expect("int suggestion value"),
                            None,
                        ),
                    }
                } else {
                    match item.split_once('=') {
                        Some((word, tooltip)) => {
                            ProviderItem::Text(word.to_owned(), Some(tooltip.to_owned()))
                        }
                        None => ProviderItem::Text(item.to_owned(), None),
                    }
                }
            })
            .collect(),
    }
}

fn parse_path(path: &'static str) -> Vec<Seg> {
    path.split('/')
        .map(|raw| {
            if let Some(rest) = raw.strip_prefix('@') {
                let parts: Vec<&str> = rest.split(':').collect();
                assert!(
                    parts.len() == 2 || parts.len() == 3 || parts.len() == 4,
                    "bad argument segment in scenario path: {raw}"
                );
                let min = if parts.len() >= 3 && parts[2] != "-" {
                    parts[2]
                } else {
                    "-"
                };
                let max = if parts.len() >= 4 && parts[3] != "-" {
                    parts[3]
                } else {
                    "-"
                };
                Seg::Arg(ArgSpec {
                    ty: parts[0],
                    name: parts[1],
                    min,
                    max,
                })
            } else {
                Seg::Lit(raw)
            }
        })
        .collect()
}

/// Apply exec/requirement/redirect flags to an owned builder (the Java
/// harness chains the same calls on its last builder).
fn apply_flags<K>(
    mut builder: ArgumentBuilder<i32, i32, K>,
    spec: &NodeSpec,
    oracle: &RustOracle,
) -> ArgumentBuilder<i32, i32, K> {
    if let Some(exec) = &spec.exec {
        builder = builder.executes(make_command(*exec));
    }
    if let Some(req) = &spec.req {
        builder = builder.requires(make_requirement(*req));
    }
    if let Some(red) = spec.red {
        let target = if red == "." {
            oracle.dispatcher.get_root()
        } else {
            oracle
                .by_path
                .get(red)
                .unwrap_or_else(|| panic!("redirect target not defined yet: {red}"))
                .clone()
        };
        let modifier = make_modifier(spec.modifier);
        builder = builder.forward(target, modifier, spec.fork);
    }
    builder
}

fn store_paths(oracle: &mut RustOracle, path: &'static str) {
    // record every node along the path (intermediates too: get_smart_usage
    // keys are the direct children, which are usually the top literals) —
    // the Java harness walk in attachTop does the same
    let segments = parse_path(path);
    let mut node = oracle.dispatcher.get_root();
    let mut prefix = String::new();
    for (raw, segment) in path.split('/').zip(segments.iter()) {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(raw);
        let child = node.read().get_child(segment.name());
        node = child.unwrap_or_else(|| panic!("node missing after registration: {raw}"));
        if !oracle
            .reverse
            .iter()
            .any(|(handle, _)| Arc::ptr_eq(handle, &node))
        {
            oracle.reverse.push((node.clone(), prefix.clone()));
        }
        oracle.by_path.insert(prefix.clone(), node.clone());
    }
}

fn build_node(oracle: &mut RustOracle, spec: &NodeSpec) {
    let segments = parse_path(spec.path);
    assert!(!segments.is_empty(), "empty node path in scenario");

    if segments.len() == 1 {
        match &segments[0] {
            Seg::Lit(name) => {
                let builder = apply_flags(literal::<i32, i32>(name), spec, oracle);
                let _handle = oracle.dispatcher.register(builder);
                store_paths(oracle, spec.path);
            }
            Seg::Arg(arg_spec) => {
                let builder = apply_flags(arg_builder(arg_spec, spec.sug), spec, oracle);
                let node: NodeHandle = Arc::new(RwLock::new(builder.build()));
                oracle.dispatcher.get_root().write().add_child(&node);
                store_paths(oracle, spec.path);
            }
        }
        return;
    }

    // deepest node carries the flags; parents chain with then_built (Java's
    // builder.then(CommandNode) builds eagerly the same way)
    let last = segments.last().unwrap();
    let mut node: NodeHandle = match last {
        Seg::Lit(name) => {
            let builder = apply_flags(literal::<i32, i32>(name), spec, oracle);
            Arc::new(RwLock::new(builder.build()))
        }
        Seg::Arg(arg_spec) => {
            let builder = apply_flags(arg_builder(arg_spec, spec.sug), spec, oracle);
            Arc::new(RwLock::new(builder.build()))
        }
    };
    for segment in segments[1..segments.len() - 1].iter().rev() {
        node = match segment {
            Seg::Lit(name) => Arc::new(RwLock::new(
                literal::<i32, i32>(name).then_built(node).build(),
            )),
            Seg::Arg(arg_spec) => Arc::new(RwLock::new(
                arg_builder(arg_spec, None).then_built(node).build(),
            )),
        };
    }
    match &segments[0] {
        Seg::Lit(name) => {
            let _handle = oracle
                .dispatcher
                .register(literal::<i32, i32>(name).then_built(node));
            store_paths(oracle, spec.path);
        }
        Seg::Arg(arg_spec) => {
            let built = Arc::new(RwLock::new(
                arg_builder(arg_spec, None).then_built(node).build(),
            ));
            oracle.dispatcher.get_root().write().add_child(&built);
            store_paths(oracle, spec.path);
        }
    }
}

impl RustOracle {
    fn new() -> Self {
        Self {
            dispatcher: CommandDispatcher::new(),
            by_path: HashMap::new(),
            reverse: Vec::new(),
        }
    }

    fn build_scenario(&mut self, scenario: &Scenario) {
        for node in &scenario.nodes {
            build_node(self, node);
        }
    }

    fn resolve_node(&self, spec: &str) -> NodeHandle {
        if spec == "." {
            return self.dispatcher.get_root();
        }
        self.by_path
            .get(spec)
            .cloned()
            .unwrap_or_else(|| panic!("op references unknown node path: {spec}"))
    }

    fn reverse_lookup(&self, node: &NodeHandle) -> String {
        self.reverse
            .iter()
            .find(|(handle, _)| Arc::ptr_eq(handle, node))
            .map(|(_, path)| path.clone())
            .unwrap_or_else(|| panic!("smart usage key has no spec path"))
    }

    fn run_op(&self, op: &Op) -> Vec<ResultLine> {
        match op {
            Op::Parse { src, input } => {
                let results = self.dispatcher.parse(input.clone(), *src);
                let mut lines = vec![ResultLine::Exceptions(results.exceptions.len())];
                for (name, exception) in results.exceptions.iter() {
                    lines.push(ResultLine::ErrEntry {
                        node: name.clone(),
                        cursor: exception.get_cursor().map(|value| value as i64),
                    });
                    lines.push(ResultLine::ErrCtx(exception.get_context()));
                    lines.push(ResultLine::ErrRaw(
                        exception.get_raw_message().get_string().to_owned(),
                    ));
                }
                lines.push(ResultLine::Reader {
                    cursor: results.reader.cursor(),
                    can_read: results.reader.can_read(),
                });
                let range = results.context.get_range();
                lines.push(ResultLine::CtxRange {
                    start: range.start(),
                    end: range.end(),
                });
                lines
            }
            Op::Exec { src, input } => match self.dispatcher.execute(input.clone(), *src) {
                Ok(value) => vec![ResultLine::ExecOk(value)],
                Err(exception) => vec![
                    ResultLine::ExecErrCursor(exception.get_cursor().map(|value| value as i64)),
                    ResultLine::ExecErrCtx(exception.get_context()),
                    ResultLine::ExecErrRaw(exception.get_raw_message().get_string().to_owned()),
                ],
            },
            Op::Sug { src, cursor, input } => {
                let results = self.dispatcher.parse(input.clone(), *src);
                let suggestions = block_on(
                    CommandDispatcher::get_completion_suggestions_with_cursor(results, *cursor),
                )
                .expect("suggestion future failed");
                let mut lines = vec![ResultLine::SugRange {
                    start: suggestions.range().start(),
                    end: suggestions.range().end(),
                }];
                for suggestion in suggestions.list() {
                    lines.push(ResultLine::SugText(suggestion.text().to_owned()));
                    lines.push(ResultLine::SugTooltip(
                        suggestion
                            .tooltip()
                            .map(|message| message.get_string().to_owned()),
                    ));
                }
                lines
            }
            Op::Usage {
                restricted,
                src,
                node,
            } => {
                let handle = self.resolve_node(node);
                let guard = handle.read();
                let usage = self.dispatcher.get_all_usage(&guard, src, *restricted);
                let mut lines = vec![ResultLine::UsageCount(usage.len())];
                for line in usage {
                    lines.push(ResultLine::UsageLine(line));
                }
                lines
            }
            Op::Smart { src, node } => {
                let handle = self.resolve_node(node);
                let guard = handle.read();
                let smart = self.dispatcher.get_smart_usage(&guard, src);
                let mut lines = vec![ResultLine::SmartCount(smart.len())];
                for (child, usage) in smart {
                    lines.push(ResultLine::SmartLine {
                        child: self.reverse_lookup(&child),
                        usage,
                    });
                }
                lines
            }
            Op::Path { node } => {
                let handle = self.resolve_node(node);
                vec![ResultLine::PathLine(self.dispatcher.get_path(&handle))]
            }
            Op::Find { path } => {
                // the wire encodes an empty path as "-"
                let names: Vec<&str> = if path.is_empty() || path == "-" {
                    vec![]
                } else {
                    path.split(',').collect()
                };
                vec![ResultLine::Found(
                    self.dispatcher.find_node(&names).is_some(),
                )]
            }
        }
    }
}

// ---------------------------------------------------------------------------
// comparison
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// BMP-only inputs: char cursors equal Java's UTF-16 cursors, compare
    /// everything including numeric positions.
    Exact,
    /// Astral input present: compare messages/texts/names/values only; skip
    /// cursors, ranges and context snippets (documented UTF-16 vs char
    /// divergence).
    Semantic,
}

fn is_bmp(input: &str) -> bool {
    input.chars().all(|code| (code as u32) < 0x10000)
}

fn scenario_mode(scenario: &Scenario) -> Mode {
    let all_bmp = scenario.ops.iter().all(|op| match op {
        Op::Parse { input, .. } | Op::Exec { input, .. } | Op::Sug { input, .. } => is_bmp(input),
        _ => true,
    });
    if all_bmp { Mode::Exact } else { Mode::Semantic }
}

fn op_desc(op: &Op) -> String {
    match op {
        Op::Parse { src, input } => format!("parse (source {src}) {input:?}"),
        Op::Exec { src, input } => format!("exec (source {src}) {input:?}"),
        Op::Sug { src, cursor, input } => format!("sug (source {src}, cursor {cursor}) {input:?}"),
        Op::Usage {
            restricted,
            src,
            node,
        } => format!("usage (restricted {restricted}, source {src}) {node}"),
        Op::Smart { src, node } => format!("smart (source {src}) {node}"),
        Op::Path { node } => format!("path {node}"),
        Op::Find { path } => format!("find {path:?}"),
    }
}

/// Compare one op's result lines under the scenario's mode. `position_free`
/// marks the lines whose numeric fields are cursor-derived and therefore
/// skipped in semantic mode.
fn compare_line(
    mode: Mode,
    index: usize,
    java: &ResultLine,
    rust: &ResultLine,
) -> Result<(), String> {
    let mismatch = |detail: String| -> Result<(), String> {
        Err(format!(
            "line {index}:\n    java: {}\n    rust: {}\n    {detail}",
            java.render(),
            rust.render()
        ))
    };
    let tag_mismatch = || mismatch("different line kinds".to_owned());

    match (java, rust) {
        (
            ResultLine::ErrEntry {
                node: jn,
                cursor: jc,
            },
            ResultLine::ErrEntry {
                node: rn,
                cursor: rc,
            },
        ) => {
            if jn != rn {
                return mismatch(format!("node names differ: java '{jn}' vs rust '{rn}'"));
            }
            if mode == Mode::Exact && jc != rc {
                return mismatch(format!("cursors differ: java {jc:?} vs rust {rc:?}"));
            }
            Ok(())
        }
        (ResultLine::ErrCtx(_), ResultLine::ErrCtx(_)) => {
            if mode == Mode::Exact && java != rust {
                return tag_mismatch();
            }
            Ok(())
        }
        (
            ResultLine::Reader {
                cursor: jc,
                can_read: jr,
            },
            ResultLine::Reader {
                cursor: rc,
                can_read: rr,
            },
        ) => {
            if jr != rr {
                return tag_mismatch();
            }
            if mode == Mode::Exact && jc != rc {
                return mismatch(format!("reader cursors differ: java {jc} vs rust {rc}"));
            }
            Ok(())
        }
        (ResultLine::ExecErrCursor(jc), ResultLine::ExecErrCursor(rc)) => {
            if mode == Mode::Exact && jc != rc {
                return mismatch(format!("cursors differ: java {jc:?} vs rust {rc:?}"));
            }
            Ok(())
        }
        (ResultLine::ExecErrCtx(_), ResultLine::ExecErrCtx(_)) => {
            if mode == Mode::Exact && java != rust {
                return tag_mismatch();
            }
            Ok(())
        }
        (
            ResultLine::SugRange { start: js, end: je },
            ResultLine::SugRange { start: rs, end: re },
        )
        | (
            ResultLine::CtxRange { start: js, end: je },
            ResultLine::CtxRange { start: rs, end: re },
        ) => {
            if mode == Mode::Exact && (js != rs || je != re) {
                return mismatch(format!(
                    "ranges differ: java [{js}, {je}) vs rust [{rs}, {re})"
                ));
            }
            Ok(())
        }
        _ => {
            if java != rust {
                tag_mismatch()
            } else {
                Ok(())
            }
        }
    }
}

fn compare_op(
    scenario_id: &str,
    op_index: usize,
    op: &Op,
    mode: Mode,
    java: &[ResultLine],
    rust: &[ResultLine],
) -> Result<(), String> {
    if java.len() != rust.len() {
        return Err(format!(
            "scenario '{scenario_id}' op #{op_index} ({}): line count differs: java {} vs rust {}\n  java: {java:?}\n  rust: {rust:?}",
            op_desc(op),
            java.len(),
            rust.len()
        ));
    }
    for (index, (jline, rline)) in java.iter().zip(rust.iter()).enumerate() {
        compare_line(mode, index, jline, rline).map_err(|detail| {
            format!(
                "scenario '{scenario_id}' op #{op_index} ({}) diverged, {detail}",
                op_desc(op)
            )
        })?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// the test
// ---------------------------------------------------------------------------

#[test]
fn differential_oracle_matches_java() {
    let battery = scenarios();
    assert!(
        battery.len() >= 75,
        "the scenario battery must hold 75+ scenarios"
    );
    // debug affordance: run only scenarios whose id contains this substring
    let filter = std::env::var("BRIGAXSIR_ORACLE_ONLY").ok();
    let all: Vec<Scenario> = battery
        .into_iter()
        .filter(|sc| filter.as_deref().is_none_or(|f| sc.id.contains(f)))
        .collect();

    // generation guard: suggestion ops need BMP inputs (cursor-unit mapping)
    for sc in &all {
        for op in &sc.ops {
            if let Op::Sug { input, .. } = op {
                assert!(
                    is_bmp(input),
                    "scenario '{}' uses a sug op on astral input: ranges are not comparable",
                    sc.id
                );
            }
        }
    }

    let request = encode_request(&all);

    let classes = match ensure_java_classes() {
        Ok(classes) => classes,
        Err(skip_reason) => {
            if !probe_available("java") {
                eprintln!(
                    "SKIP differential_oracle_matches_java: no `java` runtime on PATH; \
                     the differential oracle test needs Java 21 (java/javac) to run"
                );
                return;
            }
            eprintln!("SKIP differential_oracle_matches_java: {skip_reason}");
            return;
        }
    };

    let response = run_java_oracle(&classes, &request);
    let java_blocks = split_response(&response);

    let mut failures: Vec<String> = Vec::new();
    let mut compared_ops = 0usize;
    let mut compared_lines = 0usize;

    for sc in &all {
        let Some(lines) = java_blocks.get(sc.id) else {
            failures.push(format!(
                "scenario '{}': no java output block (harness skipped it?)",
                sc.id
            ));
            continue;
        };
        let mut rust_oracle = RustOracle::new();
        rust_oracle.build_scenario(sc);

        let mut cursor = 0usize;
        for (op_index, op) in sc.ops.iter().enumerate() {
            // a protocol mismatch here means a harness bug — panic straight up
            let (java_op_lines, next) = parse_java_op(op, lines, cursor);
            cursor = next;

            let rust_op_lines = rust_oracle.run_op(op);
            compared_ops += 1;
            compared_lines += rust_op_lines.len();

            let mode = scenario_mode(sc);
            if let Err(detail) =
                compare_op(sc.id, op_index, op, mode, &java_op_lines, &rust_op_lines)
            {
                failures.push(detail);
            }
        }
        if cursor != lines.len() {
            failures.push(format!(
                "scenario '{}': {} trailing java output lines were not consumed",
                sc.id,
                lines.len() - cursor
            ));
        }
    }

    if !failures.is_empty() {
        panic!(
            "differential oracle found {} divergence(s) across {} scenarios \
             ({compared_ops} ops, {compared_lines} result lines compared):\n\n{}",
            failures.len(),
            all.len(),
            failures.join("\n\n")
        );
    }
}
