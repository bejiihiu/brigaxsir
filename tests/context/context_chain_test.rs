//! Port of Java's `ContextChainTest` — the whole `ContextChain` surface:
//! execution through a single command and a redirect modifier, the stage
//! walk, and the flatten failure. Mockito mocks became hand-rolled recorders.

use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use brigaxsir::{
    builder::literal_argument_builder::literal,
    command_dispatcher::CommandDispatcher,
    context::{
        CommandContext, CommandContextBuilder, ContextChain, ExecutableStage, ModifierStage, Stage,
    },
    errors::CommandSyntaxException,
    result_consumer::ResultConsumer,
};
use parking_lot::Mutex;

/// Recorded consumer call: `(context source, success, result)`.
type RecordedCall = (&'static str, bool, i32);

/// Java's `mock(ResultConsumer.class)` + `verify`/`verifyNoMoreInteractions`:
/// every `onCommandComplete` call is recorded as
/// `(context source, success, result)`, so a list comparison covers both
/// verifications at once.
#[derive(Default, Clone)]
struct RecordingConsumer {
    calls: Arc<Mutex<Vec<RecordedCall>>>,
}

impl ResultConsumer<&'static str, i32> for RecordingConsumer {
    fn on_command_complete(
        &self,
        context: Arc<CommandContext<&'static str, i32>>,
        success: bool,
        result: i32,
    ) {
        self.calls
            .lock()
            .push((*context.get_source(), success, result));
    }
}

/// Java's `mock(Command.class)` with `when(command.run(any()))
/// .thenReturn(4)` plus `verify(command).run(argThat(
/// contextSourceMatches(source)))`: the recorder proves both the invocation
/// and which source it ran for.
fn counting_command(
    ran: Arc<Mutex<Vec<&'static str>>>,
    counter: Arc<AtomicU32>,
) -> impl Fn(&CommandContext<&'static str, i32>) -> i32 {
    move |context: &CommandContext<&'static str, i32>| {
        ran.lock().push(*context.get_source());
        counter.fetch_add(1, Ordering::SeqCst);
        4
    }
}

#[test]
fn execute_all_for_single_command() {
    let consumer = RecordingConsumer::default();
    let ran = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let counter = Arc::new(AtomicU32::new(0));

    let mut subject = CommandDispatcher::<&'static str, i32>::new();
    subject.register(literal("foo").executes(counting_command(ran.clone(), counter.clone())));
    let source = "compile_source";

    let parse = subject.parse("foo", source);
    let top_context = Arc::new(parse.context.build("foo"));
    let chain = ContextChain::try_flatten(top_context).unwrap();

    let runtime_source = "runtime_source";
    assert_eq!(
        chain
            .execute_all(Arc::new(runtime_source), &consumer)
            .unwrap(),
        4
    );

    // Java: verify(command).run(argThat(contextSourceMatches(runtimeSource)))
    assert_eq!(*ran.lock(), vec!["runtime_source"]);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    // Java: verify(consumer).onCommandComplete(argThat(...), eq(true), eq(4))
    // + verifyNoMoreInteractions(consumer)
    assert_eq!(*consumer.calls.lock(), vec![("runtime_source", true, 4)]);
}

#[test]
fn execute_all_for_redirected_command() {
    let consumer = RecordingConsumer::default();
    let ran = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let counter = Arc::new(AtomicU32::new(0));
    let redirected_source = "redirected_source";

    let mut subject = CommandDispatcher::<&'static str, i32>::new();
    subject.register(literal("foo").executes(counting_command(ran.clone(), counter.clone())));
    // Java: `literal("bar").redirect(dispatcher.getRoot(), context -> redirectedSource)`
    subject.register(
        literal("bar").redirect_with(
            subject.get_root(),
            Arc::new(
                move |_: &CommandContext<&'static str, i32>| -> Result<Arc<&'static str>, CommandSyntaxException> {
                    Ok(Arc::new(redirected_source))
                },
            ),
        ),
    );
    let source = "compile_source";

    let parse = subject.parse("bar foo", source);
    let top_context = Arc::new(parse.context.build("bar foo"));
    let chain = ContextChain::try_flatten(top_context).unwrap();

    let runtime_source = "runtime_source";
    assert_eq!(
        chain
            .execute_all(Arc::new(runtime_source), &consumer)
            .unwrap(),
        4
    );

    // the command saw the redirected source, not the runtime one
    assert_eq!(*ran.lock(), vec!["redirected_source"]);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(*consumer.calls.lock(), vec![("redirected_source", true, 4)]);
}

#[test]
fn single_stage_execution() {
    let mut subject = CommandDispatcher::<&'static str, i32>::new();
    subject.register(literal("foo").executes(|_| 1));
    let source = "source";

    let parse = subject.parse("foo", source);
    let top_context = Arc::new(parse.context.build("foo"));
    let chain = ContextChain::try_flatten(top_context.clone()).unwrap();

    assert!(matches!(chain.get_stage(), Stage::Execute));
    // Java: `assertThat(chain.getTopContext(), is(topContext))` — identity
    assert!(Arc::ptr_eq(&chain.get_top_context(), &top_context));
    assert!(chain.next_stage().is_none());
}

#[test]
fn multi_stage_execution() {
    let mut subject = CommandDispatcher::<&'static str, i32>::new();
    subject.register(literal("foo").executes(|_| 1));
    subject.register(literal("bar").redirect(subject.get_root()));
    let source = "source";

    let parse = subject.parse("bar bar foo", source);
    let top_context = Arc::new(parse.context.build("bar bar foo"));
    let stage0 = ContextChain::try_flatten(top_context.clone()).unwrap();

    assert!(matches!(stage0.get_stage(), Stage::Modify));
    assert!(Arc::ptr_eq(&stage0.get_top_context(), &top_context));

    let stage1 = stage0.next_stage().unwrap();
    assert!(matches!(stage1.get_stage(), Stage::Modify));
    let child1 = top_context.get_child().unwrap();
    assert!(Arc::ptr_eq(&stage1.get_top_context(), &child1));

    let stage2 = stage1.next_stage().unwrap();
    assert!(matches!(stage2.get_stage(), Stage::Execute));
    let child2 = child1.get_child().unwrap();
    assert!(Arc::ptr_eq(&stage2.get_top_context(), &child2));

    assert!(stage2.next_stage().is_none());
}

#[test]
fn missing_execute() {
    let mut subject = CommandDispatcher::<&'static str, i32>::new();
    subject.register(literal("foo").executes(|_| 1));
    subject.register(literal("bar").redirect(subject.get_root()));

    let parse = subject.parse("bar bar", "source");
    let top_context = Arc::new(parse.context.build("bar bar"));
    // Java: `assertThat(ContextChain.tryFlatten(topContext), is(Optional.empty()))`
    assert!(ContextChain::try_flatten(top_context).is_none());
}

/// A 3+ stage chain: each redirect literal points back at the root and
/// carries a `SingleRedirectModifier` rewriting the source to its own marker,
/// so the stage order is observable in the source the executable finally saw.
/// Java's `ContextChainTest` walks two MODIFY stages; these walk three and
/// four and check the `execute_all` sum plus every consumer event.
#[test]
fn three_stage_chain_execute_all() {
    let consumer = RecordingConsumer::default();
    let ran = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let counter = Arc::new(AtomicU32::new(0));

    let mut subject = CommandDispatcher::<&'static str, i32>::new();
    subject.register(literal("foo").executes(counting_command(ran.clone(), counter.clone())));
    // three redirect stages: s3 (innermost) ← s2 ← s1, walking "r1 r2 r3 foo"
    for (name, marker) in [("r1", "s1"), ("r2", "s2"), ("r3", "s3")] {
        let captured = marker;
        subject.register(
            literal(name).redirect_with(
                subject.get_root(),
                Arc::new(
                    move |_: &CommandContext<&'static str, i32>| -> Result<
                        Arc<&'static str>,
                        CommandSyntaxException,
                    > { Ok(Arc::new(captured)) },
                ),
            ),
        );
    }

    let parse = subject.parse("r1 r2 r3 foo", "compile");
    let top_context = Arc::new(parse.context.build("r1 r2 r3 foo"));
    let stage0: Arc<ContextChain<&'static str, i32>> =
        Arc::new(ContextChain::try_flatten(top_context.clone()).unwrap());

    // the stage walk: MODIFY → MODIFY → MODIFY → EXECUTE, each top context
    // identical to the matching child chain entry
    let mut expected = top_context.clone();
    let mut stage = stage0.clone();
    loop {
        assert!(matches!(stage.get_stage(), Stage::Modify));
        assert!(Arc::ptr_eq(&stage.get_top_context(), &expected));
        let next = stage.next_stage().unwrap();
        expected = expected.get_child().unwrap();
        stage = next;
        if !matches!(stage.get_stage(), Stage::Modify) {
            break;
        }
    }
    assert!(matches!(stage.get_stage(), Stage::Execute));
    assert!(Arc::ptr_eq(&stage.get_top_context(), &expected));
    assert!(stage.next_stage().is_none());

    // execute_all: the source flows r1 → s1, r2 → s2, r3 → s3, and the
    // executable runs once for s3; the sum is the single command result 4
    assert_eq!(
        stage0.execute_all(Arc::new("runtime"), &consumer).unwrap(),
        4
    );
    assert_eq!(*ran.lock(), vec!["s3"]);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(*consumer.calls.lock(), vec![("s3", true, 4)]);
}

#[test]
fn four_stage_chain_execute_all_forked() {
    let consumer = RecordingConsumer::default();
    let ran = Arc::new(Mutex::new(Vec::<&'static str>::new()));
    let counter = Arc::new(AtomicU32::new(0));

    let mut subject = CommandDispatcher::<&'static str, i32>::new();
    subject.register(literal("foo").executes(counting_command(ran.clone(), counter.clone())));
    // four stages; the LAST two are forked (fork mode is sticky from the
    // stage that sets it onward): each stage still hands down one source,
    // so one executable run happens — but the forked sum counts 1 per run
    for (name, forked) in [("q1", false), ("q2", false), ("q3", true), ("q4", true)] {
        let captured = format!("{name}-src");
        let captured: &'static str = Box::leak(captured.into_boxed_str());
        subject.register(
            literal(name).forward(
                subject.get_root(),
                Some(Arc::new(
                    move |_: &CommandContext<&'static str, i32>| -> Result<
                        Vec<Arc<&'static str>>,
                        CommandSyntaxException,
                    > { Ok(vec![Arc::new(captured)]) },
                ) as Arc<brigaxsir::modifier::RedirectModifier<&'static str, i32>>),
                forked,
            ),
        );
    }

    let parse = subject.parse("q1 q2 q3 q4 foo", "compile");
    let top_context = Arc::new(parse.context.build("q1 q2 q3 q4 foo"));
    let chain = ContextChain::try_flatten(top_context).unwrap();

    // forked mode: the executable runs once (one source end to end), the
    // consumer sees its real result 4, but the stage sum reports 1
    assert_eq!(
        chain.execute_all(Arc::new("runtime"), &consumer).unwrap(),
        1
    );
    assert_eq!(*ran.lock(), vec!["q4-src"]);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(*consumer.calls.lock(), vec![("q4-src", true, 4)]);
}

/// Java's ContextChain TODO, closed type-safely: the executable stage can
/// only exist for a command-bearing context. Java's constructor threw
/// IllegalArgumentException("Last command in chain must be executable") at
/// runtime; the port makes the failure unrepresentable instead.
#[test]
fn executable_stage_rejects_commandless_context() {
    let subject = CommandDispatcher::<&'static str, i32>::new();
    let source = Arc::new("source");
    let builder = CommandContextBuilder::new(&subject, source, subject.get_root(), 0);
    let context = Arc::new(builder.build(""));

    assert!(ExecutableStage::new(context).is_none());
}

#[test]
fn executable_stage_wraps_a_command_bearing_context() {
    let mut subject = CommandDispatcher::<&'static str, i32>::new();
    subject.register(literal("foo").executes(|_| 1));
    let parse = subject.parse("foo", "source");
    let context = Arc::new(parse.context.build("foo"));

    let stage = ExecutableStage::new(context.clone()).unwrap();
    // same context object, and the command rides inside the stage
    assert!(Arc::ptr_eq(stage.context(), &context));

    let consumer = RecordingConsumer::default();
    assert_eq!(
        ContextChain::run_executable(&stage, Arc::new("runtime"), &consumer, false).unwrap(),
        1
    );
    assert_eq!(*consumer.calls.lock(), vec![("runtime", true, 1)]);
}

#[test]
fn modifier_stage_wraps_any_context() {
    let mut subject = CommandDispatcher::<&'static str, i32>::new();
    subject.register(literal("foo").executes(|_| 1));
    let parse = subject.parse("foo", "source");
    let context = Arc::new(parse.context.build("foo"));

    let stage = ModifierStage::new(context.clone());
    assert!(Arc::ptr_eq(stage.context(), &context));
    // the wrapped context keeps its full surface - the "full context" side
    // of Java's TODO
    assert_eq!(*stage.context().get_source(), "source");
}
