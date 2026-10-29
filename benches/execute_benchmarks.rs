//! Port of Java's `com.mojang.brigadier.benchmarks.ExecuteBenchmarks`
//! (JMH, `@BenchmarkMode(AverageTime)`, `@OutputTimeUnit(NANOSECONDS)`).
//!
//! The tree mirrors the Java `@Setup` line for line: `command` executes,
//! `redirect` redirects to the root, `fork` forks to the root with a modifier
//! returning three fresh sources. The three `ParseResults` are parsed once in
//! setup and only executed in the benchmark, exactly like JMH's `@State`.
//!
//! One Rust-side artifact to know about when reading the numbers: Java's
//! `execute(ParseResults)` takes the same object every iteration. The Rust
//! `execute_parsed` consumes the value (it never mutates it, but takes it by
//! value), so each iteration pays a `ParseResults::clone` — a handful of
//! `Arc` bumps plus one small `Vec` — on top of the execute itself. That
//! clone is included in the reported numbers; it is the honest price of
//! re-executing the same parse results through the public API today.
//!
//! The dispatcher is leaked (`Box::leak`): `ParseResults` borrows the
//! dispatcher it was parsed against, and a state struct owning both would be
//! self-referential. A leaked `&'static` is plain safe code with exactly the
//! JMH `@State` lifetime.

use std::{hint::black_box, sync::Arc};

use brigaxsir::{command_dispatcher::CommandDispatcher, parse_results::ParseResults, prelude::*};
use criterion::{Criterion, criterion_group, criterion_main};

/// Java: the `ExecuteBenchmarks.setup()` tree; the fork modifier returns
/// three fresh sources per call, like Java's `Lists.newArrayList(...)`.
fn build_dispatcher() -> CommandDispatcher<(), i32> {
    let mut dispatcher = CommandDispatcher::new();
    dispatcher.register(literal("command").executes(|_| 0));
    let root = dispatcher.get_root();
    dispatcher.register(literal("redirect").redirect(root.clone()));
    dispatcher.register(literal("fork").fork(
        root,
        Arc::new(|_| Ok(vec![Arc::new(()), Arc::new(()), Arc::new(())])),
    ));
    dispatcher
}

struct ExecuteState {
    dispatcher: &'static CommandDispatcher<(), i32>,
    simple: ParseResults<'static, (), i32>,
    single_redirect: ParseResults<'static, (), i32>,
    forked_redirect: ParseResults<'static, (), i32>,
}

fn setup() -> ExecuteState {
    let dispatcher: &'static CommandDispatcher<(), i32> = Box::leak(Box::new(build_dispatcher()));
    let simple = dispatcher.parse("command", ());
    let single_redirect = dispatcher.parse("redirect command", ());
    let forked_redirect = dispatcher.parse("fork command", ());
    ExecuteState {
        dispatcher,
        simple,
        single_redirect,
        forked_redirect,
    }
}

fn bench_execute(c: &mut Criterion) {
    let state = setup();
    let mut group = c.benchmark_group("execute");

    // Java: execute_simple
    group.bench_function("execute_simple", |b| {
        b.iter(|| black_box(state.dispatcher).execute_parsed(black_box(&state.simple).clone()))
    });
    // Java: execute_single_redirect
    group.bench_function("execute_single_redirect", |b| {
        b.iter(|| {
            black_box(state.dispatcher).execute_parsed(black_box(&state.single_redirect).clone())
        })
    });
    // Java: execute_forked_redirect
    group.bench_function("execute_forked_redirect", |b| {
        b.iter(|| {
            black_box(state.dispatcher).execute_parsed(black_box(&state.forked_redirect).clone())
        })
    });

    group.finish();
}

criterion_group!(benches, bench_execute);
criterion_main!(benches);
