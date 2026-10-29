//! Port of Java's `com.mojang.brigadier.benchmarks.ParsingBenchmarks`
//! (JMH, `@BenchmarkMode(AverageTime)`, `@OutputTimeUnit(NANOSECONDS)`).
//!
//! The tree mirrors the Java `@Setup` line for line: the `a` ladder with
//! `1`-`i`,`ii` and `2`-`i`,`ii`, `b`/`1`, `c`, the always-restricted `d`,
//! `e` with a command of its own plus children, `f`/`1`-`2` with mixed
//! `requires`, `g`, `h` (the redirect target of `k`), `i`, `j` redirecting
//! to the root and `k` redirecting to `h`. Java's `Object` source becomes
//! `()` and `c -> 0` becomes `.executes(|_| 0)`.
//!
//! The four Java benchmark methods are ported 1:1, including the duplicate:
//! Java's `parse_` benchmark parses `"c"` exactly like `parse_c` — kept for
//! fidelity. Two extra benchmarks the `notes.md` baseline table asks for
//! (literal-miss and argument parsing) are marked as Rust-side additions —
//! the Java file has no benchmark for either.
//!
//! The dispatcher is leaked (`Box::leak`) so the suggestion benchmarks can
//! hold pre-parsed `ParseResults` in the state: `ParseResults` borrows the
//! dispatcher it was parsed against, and criterion's state closure cannot
//! express that self-reference. A leaked `&'static` is plain safe code and
//! lives for the whole bench run, which is exactly the JMH `@State` lifetime.

use std::{
    hint::black_box,
    task::{Context, Poll, Waker},
};

use brigaxsir::{
    command_dispatcher::CommandDispatcher,
    prelude::*,
    suggestion::{Suggestions, SuggestionsFuture},
};
use criterion::{Criterion, criterion_group, criterion_main};

/// Java: the `ParsingBenchmarks.setup()` tree.
fn build_subject() -> CommandDispatcher<(), i32> {
    let mut subject = CommandDispatcher::new();
    subject.register(
        literal("a")
            .then(
                literal("1")
                    .then(literal("i").executes(|_| 0))
                    .then(literal("ii").executes(|_| 0)),
            )
            .then(
                literal("2")
                    .then(literal("i").executes(|_| 0))
                    .then(literal("ii").executes(|_| 0)),
            ),
    );
    subject.register(literal("b").then(literal("1").executes(|_| 0)));
    subject.register(literal("c").executes(|_| 0));
    subject.register(literal("d").requires(|_| false).executes(|_| 0));
    subject.register(
        literal("e").executes(|_| 0).then(
            literal("1")
                .executes(|_| 0)
                .then(literal("i").executes(|_| 0))
                .then(literal("ii").executes(|_| 0)),
        ),
    );
    subject.register(
        literal("f")
            .then(
                literal("1")
                    .then(literal("i").executes(|_| 0))
                    .then(literal("ii").executes(|_| 0).requires(|_| false)),
            )
            .then(
                literal("2")
                    .then(literal("i").executes(|_| 0).requires(|_| false))
                    .then(literal("ii").executes(|_| 0)),
            ),
    );
    subject.register(
        literal("g")
            .executes(|_| 0)
            .then(literal("1").then(literal("i").executes(|_| 0))),
    );
    let h = subject.register(
        literal("h")
            .executes(|_| 0)
            .then(literal("1").then(literal("i").executes(|_| 0)))
            .then(literal("2").then(literal("i").then(literal("ii").executes(|_| 0))))
            .then(literal("3").executes(|_| 0)),
    );
    subject.register(
        literal("i")
            .executes(|_| 0)
            .then(literal("1").executes(|_| 0))
            .then(literal("2").executes(|_| 0)),
    );
    subject.register(literal("j").redirect(subject.get_root()));
    subject.register(literal("k").redirect(h));
    subject
}

/// Rust-side addition: a one-integer-argument dispatcher for the `parse_args`
/// baseline row — the Java benchmark tree has no argument nodes at all, and
/// argument parsing is the other half of the parse cost story.
fn build_args_subject() -> CommandDispatcher<(), i32> {
    let mut subject = CommandDispatcher::new();
    subject.register(literal("num").then(argument("value", integer()).executes(|_| 0)));
    subject
}

/// Poll one suggestion future to completion. Every provider on this tree is
/// a literal (no async providers), so the merged future completes on the
/// first poll — `Pending`/`Err` here would mean the benchmark tree changed.
/// Java's JMH equivalent would call `.join()` on the `CompletableFuture`.
fn block_on_suggestions(mut future: SuggestionsFuture) -> Suggestions {
    let mut cx = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(Ok(suggestions)) => suggestions,
        _ => panic!("suggestion future must complete on the first poll for the benchmark tree"),
    }
}

fn bench_parsing(c: &mut Criterion) {
    // `Box::leak` keeps the trees alive for the whole bench run and lets the
    // pre-parsed suggestion inputs borrow them as `&'static`.
    let subject: &'static CommandDispatcher<(), i32> = Box::leak(Box::new(build_subject()));
    let args_subject: &'static CommandDispatcher<(), i32> =
        Box::leak(Box::new(build_args_subject()));

    let mut group = c.benchmark_group("parsing");

    // Java: parse_a1i
    group.bench_function("parse_a1i", |b| {
        b.iter(|| black_box(subject).parse(black_box("a 1 i"), ()))
    });
    // Java: parse_c
    group.bench_function("parse_c", |b| {
        b.iter(|| black_box(subject).parse(black_box("c"), ()))
    });
    // Java: parse_k1i
    group.bench_function("parse_k1i", |b| {
        b.iter(|| black_box(subject).parse(black_box("k 1 i"), ()))
    });
    // Java: parse_ (a verbatim duplicate of parse_c in the Java file)
    group.bench_function("parse_", |b| {
        b.iter(|| black_box(subject).parse(black_box("c"), ()))
    });
    // Rust-side addition: no literal matches at the root, so the whole
    // relevant-nodes scan runs and finds nothing.
    group.bench_function("parse_literal_miss", |b| {
        b.iter(|| black_box(subject).parse(black_box("x"), ()))
    });
    // Rust-side addition: one integer argument after a literal.
    group.bench_function("parse_args", |b| {
        b.iter(|| black_box(args_subject).parse(black_box("num 42"), ()))
    });

    // Rust-side addition (the Java file has no suggestion benchmark): the
    // `notes.md` baseline table needs suggestion numbers. The pre-parsed
    // inputs mirror Java's `@Setup` style — parse once, measure the
    // completion computation. `get_completion_suggestions` consumes the
    // results, so each iteration clones them (a handful of `Arc` bumps plus
    // one small `Vec`); that clone is included in the numbers, same as in
    // the execute benchmarks.
    let empty_parse = subject.parse("", ());
    let a_space_parse = subject.parse("a ", ());
    group.bench_function("suggestions_root", |b| {
        b.iter(|| {
            black_box(block_on_suggestions(
                CommandDispatcher::get_completion_suggestions(black_box(empty_parse.clone())),
            ))
        })
    });
    group.bench_function("suggestions_subtree", |b| {
        b.iter(|| {
            black_box(block_on_suggestions(
                CommandDispatcher::get_completion_suggestions(black_box(a_space_parse.clone())),
            ))
        })
    });

    group.finish();
}

criterion_group!(benches, bench_parsing);
criterion_main!(benches);
