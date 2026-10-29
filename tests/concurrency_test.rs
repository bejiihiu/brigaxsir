//! The concurrency suite: compile-time Send/Sync assertions for the public
//! surface, plus the multi-threaded patterns the crate supports — one
//! dispatcher shared between threads, registration into one tree from many
//! threads, a parse executed on a different thread than it was built on, and
//! a parse+execute stress loop checked through atomic counters.
//!
//! No async runtime: futures are polled with a tiny std-only `block_on`
//! (the same helper every other suggestion test uses); everything else is
//! plain `std::thread`, with scoped threads where a value borrows the
//! dispatcher.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    thread,
};

use brigaxsir::{
    arguments::integer_argument_type::integer,
    builder::{argument, literal},
    command_dispatcher::CommandDispatcher,
    context::{CommandContext, CommandContextBuilder, ContextChain, StringRange},
    errors::CommandSyntaxException,
    parse_results::ParseResults,
    result_consumer::ResultConsumer,
    string_reader::StringReader,
    suggestion::{Suggestion, SuggestionsFuture},
    tree::CommandNode,
};
use parking_lot::RwLock;

#[derive(Debug, PartialEq)]
struct Source {
    id: u32,
}

// Compile-time trait requirements: calling these generic functions with a
// concrete type only compiles when the type actually implements the trait —
// the std-only stand-in for `static_assertions::assert_impl_all`.
fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}

#[test]
fn send_sync_matrix() {
    // The dispatcher and the tree hold S/R only behind `Send + Sync`
    // closures, so they are Send+Sync for every source type; contexts are
    // Send+Sync when S is (the source lives in an `Arc<S>`).
    assert_send::<CommandDispatcher<Source>>();
    assert_sync::<CommandDispatcher<Source>>();
    assert_send::<Arc<RwLock<CommandNode<Source, i32>>>>();
    assert_sync::<Arc<RwLock<CommandNode<Source, i32>>>>();

    // Plain data types: unconditional.
    assert_send::<StringReader>();
    assert_sync::<StringReader>();
    assert_send::<CommandSyntaxException>();
    assert_sync::<CommandSyntaxException>();

    // The context family, all sharing the `Arc` child chain.
    assert_send::<CommandContext<Source>>();
    assert_sync::<CommandContext<Source>>();
    assert_send::<CommandContextBuilder<'static, Source, i32>>();
    assert_sync::<CommandContextBuilder<'static, Source, i32>>();
    assert_send::<ContextChain<Source, i32>>();
    assert_sync::<ContextChain<Source, i32>>();

    // Parse results: this is the line that makes "parse on one thread,
    // execute on another" work.
    assert_send::<ParseResults<'static, Source, i32>>();
    assert_sync::<ParseResults<'static, Source, i32>>();

    // Suggestion futures are `Send` (boxed `dyn Future + Send`); they are
    // deliberately not `Sync`, which is fine — a future is polled by one
    // thread at a time.
    assert_send::<SuggestionsFuture>();
}

/// The shared tree plus its two counters (command runs and argument sum).
type CountingTree = (
    Arc<RwLock<CommandNode<Source, i32>>>,
    Arc<AtomicU32>,
    Arc<AtomicU64>,
);

/// The tree every execute-heavy test shares: `count <value>` bumps a shared
/// counter and a shared sum, so cross-thread argument confusion would show
/// up in the totals, not just the call count. Returns the root, so a test
/// can hang several dispatchers off the same tree.
fn counting_tree() -> CountingTree {
    let counter = Arc::new(AtomicU32::new(0));
    let sum = Arc::new(AtomicU64::new(0));
    let mut dispatcher = CommandDispatcher::<Source, i32>::new();
    dispatcher.register(
        literal("count").then(argument("value", integer()).executes({
            let counter = Arc::clone(&counter);
            let sum = Arc::clone(&sum);
            move |context| {
                let value = *context.get_argument::<i32>("value");
                sum.fetch_add(value as u64, Ordering::SeqCst);
                counter.fetch_add(1, Ordering::SeqCst);
                1
            }
        })),
    );
    (dispatcher.get_root(), counter, sum)
}

/// ResultConsumer calls arrive from whichever thread ran the command, so a
/// shared consumer must be thread-safe — this one counts through an atomic.
struct CountingConsumer {
    calls: Arc<AtomicU32>,
}

impl ResultConsumer<Source, i32> for CountingConsumer {
    fn on_command_complete(
        &self,
        _context: Arc<CommandContext<Source, i32>>,
        _success: bool,
        _result: i32,
    ) {
        self.calls.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn registration_from_many_threads_into_one_tree() {
    const THREADS: usize = 8;
    let root: Arc<RwLock<CommandNode<Source, i32>>> = Arc::new(RwLock::new(CommandNode::root()));

    thread::scope(|scope| {
        for t in 0..THREADS {
            let root = Arc::clone(&root);
            scope.spawn(move || {
                // one dispatcher per thread, all over the same tree
                let mut dispatcher = CommandDispatcher::<Source, i32>::with_root(root);
                let name = format!("cmd{t}");
                dispatcher.register(literal(&name).executes(|_| 1));
            });
        }
    });

    // every thread's command landed in the one shared tree
    let dispatcher = CommandDispatcher::<Source, i32>::with_root(root);
    for t in 0..THREADS {
        let name = format!("cmd{t}");
        assert!(
            dispatcher.find_node(&[name.as_str()]).is_some(),
            "command {name} is missing from the shared tree"
        );
    }
}

#[test]
fn registration_merge_from_concurrent_threads() {
    const THREADS: usize = 4;
    let root: Arc<RwLock<CommandNode<Source, i32>>> = Arc::new(RwLock::new(CommandNode::root()));

    thread::scope(|scope| {
        for t in 0..THREADS {
            let root = Arc::clone(&root);
            scope.spawn(move || {
                let mut dispatcher = CommandDispatcher::<Source, i32>::with_root(root);
                // all threads register the SAME literal name with a distinct
                // subcommand — Java's addChild merge-on-existing semantics
                let sub = format!("sub{t}");
                dispatcher.register(literal("shared").then(literal(&sub).executes(|_| 1)));
            });
        }
    });

    // "shared" exists exactly once and carries all four subcommands
    let shared = root
        .read()
        .get_child("shared")
        .expect("the shared literal is missing from the tree");
    let mut names: Vec<String> = shared
        .read()
        .get_children()
        .iter()
        .map(|child| child.read().get_name().to_owned())
        .collect();
    names.sort();
    let expected: Vec<String> = (0..THREADS).map(|t| format!("sub{t}")).collect();
    assert_eq!(names, expected);
}

#[test]
fn parse_on_one_thread_execute_on_another() {
    let (root, counter, _sum) = counting_tree();
    let dispatcher = Arc::new(CommandDispatcher::<Source, i32>::with_root(root));

    thread::scope(|scope| {
        // parsed here...
        let parse = dispatcher.parse("count 5", Source { id: 1 });
        // ...executed over there; the parse result borrows the dispatcher,
        // which is why this needs a scoped thread rather than a detached one
        let dispatcher_for_child = Arc::clone(&dispatcher);
        scope.spawn(move || {
            assert_eq!(dispatcher_for_child.execute_parsed(parse).unwrap(), 1);
        });
    });

    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[test]
fn a_built_context_moves_to_another_thread() {
    let mut subject = CommandDispatcher::<Source, i32>::new();
    subject.register(literal("count").then(argument("value", integer()).executes(|_| 1)));

    let parse = subject.parse("count 7", Source { id: 1 });
    let context = parse.context.build("count 7");

    thread::scope(|scope| {
        scope.spawn(move || {
            assert_eq!(*context.get_argument::<i32>("value"), 7);
            assert_eq!(context.get_source().id, 1);
        });
    });
}

#[test]
fn the_same_parse_executes_concurrently_from_clones() {
    // Java permits re-executing one ParseResults (execute never mutates it);
    // ownership makes Rust take one clone per thread instead. Each clone is
    // an independent snapshot, so both runs must land.
    let (root, counter, _sum) = counting_tree();
    let dispatcher = Arc::new(CommandDispatcher::<Source, i32>::with_root(root));

    let parse = dispatcher.parse("count 5", Source { id: 1 });
    let parse_for_child = parse.clone();

    thread::scope(|scope| {
        let dispatcher_for_child = Arc::clone(&dispatcher);
        scope.spawn(move || {
            dispatcher_for_child
                .execute_parsed(parse_for_child)
                .unwrap();
        });
        dispatcher.execute_parsed(parse).unwrap();
    });

    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

#[test]
fn suggestions_polled_on_another_thread() {
    let mut subject = CommandDispatcher::<Source, i32>::new();
    subject.register(literal("bar").executes(|_| 1));
    subject.register(literal("baz").executes(|_| 1));

    thread::scope(|scope| {
        let parse = subject.parse("b", Source { id: 1 });
        let future = CommandDispatcher::get_completion_suggestions(parse);
        scope.spawn(move || {
            let suggestions = block_on(future).expect("suggestion future failed");
            assert_eq!(suggestions.range(), StringRange::between(0, 1));
            assert_eq!(
                suggestions.list(),
                [
                    Suggestion::new(StringRange::between(0, 1), "bar"),
                    Suggestion::new(StringRange::between(0, 1), "baz")
                ]
                .as_slice()
            );
        });
    });
}

#[test]
fn stress_parse_and_execute_from_many_threads() {
    const THREADS: usize = 8;
    const ITERS: usize = 250;

    let (root, counter, sum) = counting_tree();

    // both dispatchers hang off the same tree and share one consumer, so
    // every one of the executions is seen from whichever thread ran it
    let consumer_calls = Arc::new(AtomicU32::new(0));
    let mut primary = CommandDispatcher::<Source, i32>::with_root(Arc::clone(&root));
    primary.set_consumer(CountingConsumer {
        calls: Arc::clone(&consumer_calls),
    });
    let mut shared = CommandDispatcher::<Source, i32>::with_root(root);
    shared.set_consumer(CountingConsumer {
        calls: Arc::clone(&consumer_calls),
    });
    let primary = Arc::new(primary);
    let shared = Arc::new(shared);

    thread::scope(|scope| {
        for t in 0..THREADS {
            // the thread-local dispatcher and the shared one hang off the
            // same tree; both run concurrently
            let dispatcher = Arc::clone(&primary);
            let shared = Arc::clone(&shared);
            scope.spawn(move || {
                for i in 0..ITERS {
                    let value = (t * ITERS + i) as i32;
                    let input = format!("count {value}");
                    let through_shared = i % 2 == 0;
                    let result = if through_shared {
                        shared.execute(input, Source { id: t as u32 }).unwrap()
                    } else {
                        dispatcher.execute(input, Source { id: t as u32 }).unwrap()
                    };
                    assert_eq!(result, 1);
                }
            });
        }
    });

    assert_eq!(counter.load(Ordering::SeqCst), (THREADS * ITERS) as u32);
    // sum of 0..(THREADS*ITERS) — proves no argument value was lost or
    // duplicated across threads
    let expected_sum: u64 = (0..(THREADS * ITERS) as u64).sum();
    assert_eq!(sum.load(Ordering::SeqCst), expected_sum);
    assert_eq!(
        consumer_calls.load(Ordering::SeqCst),
        (THREADS * ITERS) as u32
    );
}

#[test]
fn stress_register_while_others_parse_and_execute() {
    const EXECUTORS: usize = 2;
    const REGISTRARS: usize = 2;
    const EXECUTE_ITERS: usize = 200;
    const REGISTRATIONS: usize = 100;

    let root: Arc<RwLock<CommandNode<Source, i32>>> = Arc::new(RwLock::new(CommandNode::root()));
    let counter = Arc::new(AtomicU32::new(0));
    {
        // seed the shared tree with the command the executors will hammer
        let mut setup = CommandDispatcher::<Source, i32>::with_root(Arc::clone(&root));
        let counted = Arc::clone(&counter);
        setup.register(literal("count").executes(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            1
        }));
    }

    let mut handles = Vec::new();
    for t in 0..EXECUTORS {
        let root = Arc::clone(&root);
        handles.push(thread::spawn(move || {
            let dispatcher = CommandDispatcher::<Source, i32>::with_root(root);
            for _ in 0..EXECUTE_ITERS {
                assert_eq!(
                    dispatcher
                        .execute("count", Source { id: t as u32 })
                        .unwrap(),
                    1
                );
            }
        }));
    }
    for t in 0..REGISTRARS {
        let root = Arc::clone(&root);
        handles.push(thread::spawn(move || {
            // fresh names every time: the add_child insert path (the merge
            // path is exercised by registration_merge_from_concurrent_threads)
            let mut dispatcher = CommandDispatcher::<Source, i32>::with_root(root);
            for i in 0..REGISTRATIONS {
                let name = format!("reg{t}_{i}");
                dispatcher.register(literal(&name).executes(|_| 1));
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    assert_eq!(
        counter.load(Ordering::SeqCst),
        (EXECUTORS * EXECUTE_ITERS) as u32
    );
    let dispatcher = CommandDispatcher::<Source, i32>::with_root(root);
    for t in 0..REGISTRARS {
        for i in 0..REGISTRATIONS {
            let name = format!("reg{t}_{i}");
            assert!(
                dispatcher.find_node(&[name.as_str()]).is_some(),
                "command {name} is missing from the shared tree"
            );
        }
    }
}

/// Minimal `block_on` for the tests, std-only: a channel-based waker. The
/// library's own futures complete on the first poll, so this never actually
/// blocks in practice.
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
