use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use brigaxsir::{
    arguments::{
        integer_argument_type::{get_integer, integer},
        string_argument_type::{get_string, string},
    },
    builder::{literal_argument_builder::literal, required_argument_builder::argument},
    command_dispatcher::CommandDispatcher,
    context::CommandContext,
    errors::{BuiltInExceptionProvider, CommandSyntaxException, built_in_exceptions},
    result_consumer::ResultConsumer,
    string_reader::StringReader,
    tree::CommandNode,
};
use parking_lot::{Mutex, RwLock};

#[derive(Debug, PartialEq)]
struct CommandSource {}

/// Hand-rolled Mockito stand-in: records every `onCommandComplete` call as
/// `(success, result)` pairs. Java's test uses `verify(consumer)` +
/// `verifyNoMoreInteractions`; comparing the recorded list does both.
#[derive(Default, Clone)]
struct RecordingConsumer {
    calls: Arc<Mutex<Vec<(bool, i32)>>>,
}

impl ResultConsumer<CommandSource, i32> for RecordingConsumer {
    fn on_command_complete(
        &self,
        _context: Arc<CommandContext<CommandSource, i32>>,
        success: bool,
        result: i32,
    ) {
        self.calls.lock().push((success, result));
    }
}

/// Same recorder for numeric sources — the forked-mode tests verify
/// per-source outcomes (Java's `contextSourceMatches` matcher), so the
/// source value is recorded too.
#[derive(Default, Clone)]
struct SourceRecordingConsumer {
    calls: Arc<Mutex<Vec<(i64, bool, i32)>>>,
}

impl ResultConsumer<i64, i32> for SourceRecordingConsumer {
    fn on_command_complete(
        &self,
        context: Arc<CommandContext<i64, i32>>,
        success: bool,
        result: i32,
    ) {
        self.calls
            .lock()
            .push((*context.get_source(), success, result));
    }
}

/// Java asserts `assertThat(ex, is(exception))` — reference identity. The
/// exception travels by value here, so the strongest equivalent is full
/// field equality over the shared Arcs (same type object, same message
/// object, same input and cursor).
fn assert_same_exception(actual: &CommandSyntaxException, expected: &CommandSyntaxException) {
    assert!(Arc::ptr_eq(actual.get_type(), expected.get_type()));
    assert!(Arc::ptr_eq(
        actual.get_raw_message(),
        expected.get_raw_message()
    ));
    assert_eq!(actual.get_input(), expected.get_input());
    assert_eq!(actual.get_cursor(), expected.get_cursor());
}

fn input_with_offset(input: &str, offset: usize) -> StringReader {
    let mut result: StringReader = input.into();
    result.set_cursor(offset);
    result
}

#[test]
fn with_root_uses_the_given_tree() {
    // Java: `new CommandDispatcher<>(root)` — the root-injection constructor:
    // register into an externally built tree and run it
    let root = Arc::new(RwLock::new(CommandNode::root()));
    root.write().add_child(&Arc::new(RwLock::new(
        literal("foo").executes(|_| 7).build(),
    )));
    let subject = CommandDispatcher::<CommandSource>::with_root(root.clone());

    assert_eq!(subject.execute("foo", CommandSource {}).unwrap(), 7);
    // the dispatcher runs against exactly that tree, by identity
    assert!(Arc::ptr_eq(&subject.get_root(), &root));
}

#[test]
#[should_panic(expected = "expects a RootCommandNode")]
fn with_root_rejects_non_root() {
    // Java's parameter type RootCommandNode<S> makes this unrepresentable;
    // the port turns it into a documented programmer-error panic
    let node = Arc::new(RwLock::new(literal("foo").executes(|_| 1).build()));
    let _ = CommandDispatcher::<()>::with_root(node);
}

#[test]
fn create_and_execute_command() {
    let mut subject = CommandDispatcher::new();
    // `verify(command).run(any(CommandContext.class))` — the counter is that
    // verification
    let calls = Arc::new(AtomicU32::new(0));
    let counted = calls.clone();
    subject.register(literal("foo").executes(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        42
    }));

    assert_eq!(subject.execute("foo", &CommandSource {}).unwrap(), 42);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn create_and_execute_offset_command() {
    let mut subject = CommandDispatcher::new();
    subject.register(literal("foo").executes(|_| 42));

    assert_eq!(
        subject
            .execute(input_with_offset("/foo", 1), &CommandSource {})
            .unwrap(),
        42
    );
}

#[test]
fn create_and_merge_commands() {
    let mut subject = CommandDispatcher::new();
    // Java: one shared mock command, `verify(command, times(2)).run(...)`
    let calls = Arc::new(AtomicU32::new(0));
    let counted = calls.clone();
    subject.register(literal("base").then(literal("foo").executes(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        42
    })));
    let counted = calls.clone();
    subject.register(literal("base").then(literal("bar").executes(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        42
    })));

    assert_eq!(subject.execute("base foo", &CommandSource {}).unwrap(), 42);
    assert_eq!(subject.execute("base bar", &CommandSource {}).unwrap(), 42);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn execute_unknown_command() {
    let mut subject = CommandDispatcher::<_, i32>::new();
    subject.register(literal("bar"));
    subject.register(literal("baz"));

    let execute_result = subject.execute("foo", &CommandSource {});

    let err = execute_result.err().unwrap();
    assert!(err.is_type(built_in_exceptions().dispatcher_unknown_command().as_type()));
    assert_eq!(err.get_cursor(), Some(0));
}

#[test]
fn execute_impermissible_command() {
    let mut subject = CommandDispatcher::<_, i32>::new();
    subject.register(literal("foo").requires(|_| false));

    let execute_result = subject.execute("foo", &CommandSource {});

    let err = execute_result.err().unwrap();
    assert!(err.is_type(built_in_exceptions().dispatcher_unknown_command().as_type()));
    assert_eq!(err.get_cursor(), Some(0));
}

#[test]
fn execute_empty_command() {
    let mut subject = CommandDispatcher::<_, i32>::new();
    subject.register(literal(""));

    let execute_result = subject.execute("", &CommandSource {});

    let err = execute_result.err().unwrap();
    assert!(err.is_type(built_in_exceptions().dispatcher_unknown_command().as_type()));
    assert_eq!(err.get_cursor(), Some(0));
}

#[test]
fn execute_unknown_subcommand() {
    let mut subject = CommandDispatcher::new();
    subject.register(literal("foo").executes(|_| 42));

    let execute_result = subject.execute("foo bar", &CommandSource {});

    let err = execute_result.err().unwrap();
    assert!(
        err.is_type(
            built_in_exceptions()
                .dispatcher_unknown_argument()
                .as_type()
        )
    );
    assert_eq!(err.get_cursor(), Some(4));
}

#[test]
fn execute_incorrect_literal() {
    let mut subject = CommandDispatcher::new();
    subject.register(literal("foo").executes(|_| 42).then(literal("bar")));

    let execute_result = subject.execute("foo baz", &CommandSource {});

    let err = execute_result.err().unwrap();
    assert!(
        err.is_type(
            built_in_exceptions()
                .dispatcher_unknown_argument()
                .as_type()
        )
    );
    assert_eq!(err.get_cursor(), Some(4));
}

#[test]
fn execute_ambiguous_incorrect_argument() {
    let mut subject = CommandDispatcher::new();
    subject.register(
        literal("foo")
            .executes(|_| 42)
            .then(literal("bar"))
            .then(literal("baz")),
    );

    let execute_result = subject.execute("foo unknown", &CommandSource {});

    let err = execute_result.err().unwrap();
    assert!(
        err.is_type(
            built_in_exceptions()
                .dispatcher_unknown_argument()
                .as_type()
        )
    );
    assert_eq!(err.get_cursor(), Some(4));
}

#[test]
fn execute_subcommand() {
    let mut subject = CommandDispatcher::new();

    subject.register(
        literal("foo")
            .then(literal("a"))
            .then(literal("=").executes(|_| 100))
            .then(literal("c"))
            .executes(|_| 42),
    );

    assert_eq!(subject.execute("foo =", &CommandSource {}).unwrap(), 100);
}

#[test]
fn parse_incomplete_literal() {
    let mut subject = CommandDispatcher::new();
    subject.register(literal("foo").then(literal("bar").executes(|_| 42)));

    let parse = subject.parse("foo ", &CommandSource {});
    assert_eq!(parse.reader.remaining(), " ");
    assert_eq!(parse.context.get_nodes().len(), 1);
}

#[test]
fn parse_incomplete_argument() {
    let mut subject = CommandDispatcher::new();
    subject.register(literal("foo").then(argument("bar", integer()).executes(|_| 42)));

    let parse = subject.parse("foo ", &CommandSource {});
    assert_eq!(parse.reader.remaining(), " ");
    assert_eq!(parse.context.get_nodes().len(), 1);
}

#[test]
fn execute_ambiguous_parent_subcommand() {
    let mut subject = CommandDispatcher::new();

    subject.register(
        literal("test")
            .then(argument("incorrect", integer()).executes(|_| 42))
            .then(argument("right", integer()).then(argument("sub", integer()).executes(|_| 100))),
    );

    assert_eq!(subject.execute("test 1 2", &CommandSource {}).unwrap(), 100);
}

#[test]
fn execute_ambiguous_parent_subcommand_via_redirect() {
    let mut subject = CommandDispatcher::new();

    let real = subject.register(
        literal("test")
            .then(argument("incorrect", integer()).executes(|_| 42))
            .then(argument("right", integer()).then(argument("sub", integer()).executes(|_| 100))),
    );

    subject.register(literal("redirect").redirect(real));

    assert_eq!(
        subject.execute("redirect 1 2", &CommandSource {}).unwrap(),
        100
    );
}

#[test]
fn execute_redirected_multiple_times() {
    let mut subject = CommandDispatcher::new();

    let concrete_node = subject.register(literal("actual").executes(|_| 42));
    let root = subject.get_root();
    let redirect_node = subject.register(literal("redirected").redirect(root.clone()));

    let input = "redirected redirected actual";

    let parse = subject.parse(input, &CommandSource {});
    assert_eq!(parse.context.get_range().get(input), "redirected");
    assert_eq!(parse.context.get_nodes().len(), 1);
    // Java: `assertThat(parse.getContext().getRootNode(), is(subject.getRoot()))`
    assert!(Arc::ptr_eq(parse.context.get_root_node(), &root));
    assert_eq!(
        parse.context.get_nodes()[0].get_range(),
        parse.context.get_range()
    );
    assert!(Arc::ptr_eq(
        &parse.context.get_nodes()[0].get_node(),
        &redirect_node
    ));

    let child1 = parse.context.get_child();
    assert!(child1.is_some());
    let child1 = child1.unwrap();
    assert_eq!(child1.get_range().get(input), "redirected");
    assert_eq!(child1.get_nodes().len(), 1);
    assert!(Arc::ptr_eq(child1.get_root_node(), &root));
    assert_eq!(child1.get_nodes()[0].get_range(), child1.get_range());
    assert!(Arc::ptr_eq(
        &child1.get_nodes()[0].get_node(),
        &redirect_node
    ));

    let child2 = child1.get_child();
    assert!(child2.is_some());
    let child2 = child2.unwrap();
    assert_eq!(child2.get_range().get(input), "actual");
    assert_eq!(child2.get_nodes().len(), 1);
    assert!(Arc::ptr_eq(child2.get_root_node(), &root));
    assert_eq!(child2.get_nodes()[0].get_range(), child2.get_range());
    assert!(Arc::ptr_eq(
        &child2.get_nodes()[0].get_node(),
        &concrete_node
    ));

    assert_eq!(subject.execute_parsed(parse).unwrap(), 42);
}

#[test]
fn execute_redirected() {
    let mut subject = CommandDispatcher::<i64, i32>::new();

    // Java: two identity-distinct source Objects; the values stand in
    let source1 = Arc::new(1i64);
    let source2 = Arc::new(2i64);

    let modifier =
        move |_: &CommandContext<i64, i32>| -> Result<Vec<Arc<i64>>, CommandSyntaxException> {
            Ok(vec![source1.clone(), source2.clone()])
        };

    // Java: verify(command).run(argThat(hasProperty("source", is(source1))))
    // + the same for source2 — the recorder pins which sources ran
    let ran = Arc::new(Mutex::new(Vec::<i64>::new()));
    let recorder = ran.clone();
    let concrete_node = subject.register(literal("actual").executes(move |c| {
        recorder.lock().push(*c.get_source());
        42
    }));
    let redirect_node =
        subject.register(literal("redirected").fork(subject.get_root(), Arc::new(modifier)));

    let input = "redirected actual";
    let parse = subject.parse(input, 0);
    assert_eq!(parse.context.get_range().get(input), "redirected");
    assert_eq!(parse.context.get_nodes().len(), 1);
    let root = subject.get_root();
    assert!(Arc::ptr_eq(parse.context.get_root_node(), &root));
    assert_eq!(
        parse.context.get_nodes()[0].get_range(),
        parse.context.get_range()
    );
    assert!(Arc::ptr_eq(
        &parse.context.get_nodes()[0].get_node(),
        &redirect_node
    ));
    // Java: `assertThat(parse.getContext().getSource(), is(source))`
    assert_eq!(*parse.context.get_source(), 0);

    let parent = parse.context.get_child();
    assert!(parent.is_some());
    let parent = parent.unwrap();
    assert_eq!(parent.get_range().get(input), "actual");
    assert_eq!(parent.get_nodes().len(), 1);
    // Java asserts the parse context's root again at this spot (its own
    // copy-paste quirk) — reproduced as-is
    assert!(Arc::ptr_eq(parse.context.get_root_node(), &root));
    assert_eq!(parent.get_nodes()[0].get_range(), parent.get_range());
    assert!(Arc::ptr_eq(
        &parent.get_nodes()[0].get_node(),
        &concrete_node
    ));
    assert_eq!(*parent.get_source(), 0);

    assert_eq!(subject.execute_parsed(parse).unwrap(), 2);
    assert_eq!(*ran.lock(), vec![1, 2]);
}

#[test]
fn execute_orphaned_subcommand() {
    let mut subject = CommandDispatcher::new();

    subject.register(
        literal("foo")
            .then(argument("bar", integer()))
            .executes(|_| 42),
    );

    let result = subject.execute("foo 5", &CommandSource {});
    assert!(result.is_err());
    let result = result.unwrap_err();
    assert!(result.is_type(built_in_exceptions().dispatcher_unknown_command().as_type()));
    assert_eq!(result.get_cursor(), Some(5));
}

#[test]
fn execute_invalid_other() {
    let mut subject = CommandDispatcher::new();

    subject.register(literal("w").executes(|_| panic!("This should not run")));
    subject.register(literal("world").executes(|_| 42));

    assert_eq!(subject.execute("world", &CommandSource {}).unwrap(), 42);
}

#[test]
fn parse_no_space_separator() {
    let mut subject = CommandDispatcher::new();

    subject.register(
        literal("foo")
            .then(argument("bar", integer()))
            .executes(|_| 42),
    );

    let result = subject.execute("foo$", &CommandSource {});
    assert!(result.is_err());
    let result = result.unwrap_err();
    assert!(result.is_type(built_in_exceptions().dispatcher_unknown_command().as_type()));
    assert_eq!(result.get_cursor(), Some(0));
}

#[test]
fn execute_invalid_subcommand() {
    let mut subject = CommandDispatcher::new();

    subject.register(
        literal("foo")
            .then(argument("bar", integer()))
            .executes(|_| 42),
    );

    let result = subject.execute("foo bar", &CommandSource {});
    assert!(result.is_err());
    let result = result.unwrap_err();
    // the original argument error surfaces as-is now (Java stores the
    // CommandSyntaxException in the errors map unwrapped)
    assert!(result.is_type(built_in_exceptions().reader_expected_int().as_type()));
    assert_eq!(
        result.get_message(),
        "Expected integer at position 4: foo <--[HERE]"
    );
    assert_eq!(result.get_cursor(), Some(4));
}

#[test]
fn get_path() {
    let mut subject = CommandDispatcher::<()>::new();

    // the handle passed to get_path must be the tree's own node (Java
    // compares by identity), which then_built preserves
    let bar = Arc::new(RwLock::new(literal("bar").build()));
    subject.register(literal("foo").then_built(bar.clone()));

    assert_eq!(
        subject.get_path(&bar),
        vec!["foo".to_owned(), "bar".to_owned()]
    );
}

#[test]
fn find_node_doesnt_exist() {
    let subject = CommandDispatcher::<()>::new();

    assert!(subject.find_node(&["foo", "bar"]).is_none())
}

#[test]
fn find_node_exists() {
    let mut subject = CommandDispatcher::<()>::new();

    // the handle passed to find_node must be the tree's own node (Java
    // compares by identity), which then_built preserves
    let bar = Arc::new(RwLock::new(literal("bar").build()));
    subject.register(literal("foo").then_built(bar.clone()));

    assert!(Arc::ptr_eq(
        &subject.find_node(&["foo", "bar"]).unwrap(),
        &bar
    ));
}

#[test]
fn correct_execute_context_after_redirect() {
    let mut subject = CommandDispatcher::<i32>::new();

    let root = subject.get_root();
    // Java: `c -> c.getSource() + getInteger(c, "value")`
    let add_modifier = Arc::new(
        |c: &CommandContext<i32>| -> Result<Arc<i32>, CommandSyntaxException> {
            Ok(Arc::new(*c.get_source() + get_integer(c, "value")))
        },
    );
    subject.register(
        literal("add").then(argument("value", integer()).redirect_with(root.clone(), add_modifier)),
    );
    subject.register(literal("blank").redirect(root.clone()));
    // Java: `CommandContext::getSource`
    subject.register(literal("run").executes(|c| *c.get_source()));

    assert_eq!(subject.execute("run", 0).unwrap(), 0);
    assert_eq!(subject.execute("run", 1).unwrap(), 1);

    assert_eq!(subject.execute("add 5 run", 1).unwrap(), 1 + 5);
    assert_eq!(subject.execute("add 5 add 6 run", 2).unwrap(), 2 + 5 + 6);
    assert_eq!(subject.execute("add 5 blank run", 1).unwrap(), 1 + 5);
    assert_eq!(subject.execute("blank add 5 run", 1).unwrap(), 1 + 5);
    assert_eq!(
        subject.execute("add 5 blank add 6 run", 2).unwrap(),
        2 + 5 + 6
    );
    assert_eq!(
        subject.execute("add 5 blank blank add 6 run", 2).unwrap(),
        2 + 5 + 6
    );
}

#[test]
fn shared_redirect_and_execute_nodes() {
    let mut subject = CommandDispatcher::<i32>::new();

    let root = subject.get_root();
    let add_modifier = Arc::new(
        |c: &CommandContext<i32>| -> Result<Arc<i32>, CommandSyntaxException> {
            Ok(Arc::new(*c.get_source() + get_integer(c, "value")))
        },
    );
    // the same argument node redirects AND executes
    subject.register(
        literal("add").then(
            argument("value", integer())
                .redirect_with(root, add_modifier)
                .executes(|c| *c.get_source()),
        ),
    );

    assert_eq!(subject.execute("add 5", 1).unwrap(), 1);
    assert_eq!(subject.execute("add 5 add 6", 1).unwrap(), 1 + 5);
}

#[test]
fn incomplete_redirect_should_throw() {
    let mut subject = CommandDispatcher::new();

    let foo = subject.register(
        literal("foo")
            .then(
                literal("bar")
                    .then(argument("value", integer()).executes(|c| get_integer(c, "value"))),
            )
            .then(literal("awa").executes(|_| 2)),
    );
    subject.register(literal("baz").redirect(foo));

    let err = subject.execute("baz bar", &CommandSource {}).unwrap_err();
    assert!(
        err.is_type(built_in_exceptions().dispatcher_unknown_command().as_type()),
        "unexpected error: {err}"
    );
}

#[test]
fn redirect_modifier_empty_result() {
    let mut subject = CommandDispatcher::new();

    let foo = subject.register(
        literal("foo")
            .then(
                literal("bar")
                    .then(argument("value", integer()).executes(|c| get_integer(c, "value"))),
            )
            .then(literal("awa").executes(|_| 2)),
    );
    // Java: `context -> Collections.emptyList()`
    let empty_modifier = Arc::new(
        |_: &CommandContext<CommandSource>| -> Result<Vec<Arc<CommandSource>>, CommandSyntaxException> {
            Ok(vec![])
        },
    );
    subject.register(literal("baz").fork(foo, empty_modifier));

    // no commands executed, so the result is 0
    assert_eq!(subject.execute("baz bar 100", CommandSource {}).unwrap(), 0);
}

#[test]
fn result_consumer_in_non_error_run() {
    let mut subject = CommandDispatcher::new();
    let consumer = RecordingConsumer::default();
    subject.set_consumer(consumer.clone());

    subject.register(literal("foo").executes(|_| 5));

    assert_eq!(subject.execute("foo", CommandSource {}).unwrap(), 5);
    // exactly one call: (success, 5)
    assert_eq!(*consumer.calls.lock(), vec![(true, 5)]);
}

#[test]
fn result_consumer_in_forked_non_error_run() {
    let mut subject = CommandDispatcher::<i64, i32>::new();
    let consumer = SourceRecordingConsumer::default();
    subject.set_consumer(consumer.clone());

    // Java: `c -> (Integer)(c.getSource())`
    subject.register(literal("foo").executes(|c| *c.get_source() as i32));

    // Java: `literal("repeat").fork(subject.getRoot(), context -> Arrays.asList(contexts))`
    let fork_sources = vec![Arc::new(9i64), Arc::new(10i64), Arc::new(11i64)];
    let fork_modifier = Arc::new(
        move |_: &CommandContext<i64, i32>| -> Result<Vec<Arc<i64>>, CommandSyntaxException> {
            Ok(fork_sources.clone())
        },
    );
    subject.register(literal("repeat").fork(subject.get_root(), fork_modifier));

    // the three forked sources each ran the command once
    assert_eq!(subject.execute("repeat foo", 0).unwrap(), 3);
    assert_eq!(
        *consumer.calls.lock(),
        vec![(9, true, 9), (10, true, 10), (11, true, 11)]
    );
}

#[test]
fn exception_in_non_forked_command() {
    let mut subject = CommandDispatcher::new();
    let consumer = RecordingConsumer::default();
    subject.set_consumer(consumer.clone());

    let exception = built_in_exceptions().reader_expected_bool().create();
    let thrown = exception.clone();
    subject.register(literal("crash").executes_result(move |_| Err(thrown.clone())));

    let err = subject.execute("crash", CommandSource {}).unwrap_err();
    assert_same_exception(&err, &exception);

    assert_eq!(*consumer.calls.lock(), vec![(false, 0)]);
}

#[test]
fn exception_in_non_forked_redirected_command() {
    let mut subject = CommandDispatcher::new();
    let consumer = RecordingConsumer::default();
    subject.set_consumer(consumer.clone());
    subject.register(literal("redirect").redirect(subject.get_root()));

    let exception = built_in_exceptions().reader_expected_bool().create();
    let thrown = exception.clone();
    subject.register(literal("crash").executes_result(move |_| Err(thrown.clone())));

    let err = subject
        .execute("redirect crash", CommandSource {})
        .unwrap_err();
    assert_same_exception(&err, &exception);

    assert_eq!(*consumer.calls.lock(), vec![(false, 0)]);
}

#[test]
fn exception_in_forked_redirected_command() {
    let mut subject = CommandDispatcher::new();
    let consumer = RecordingConsumer::default();
    subject.set_consumer(consumer.clone());
    // Java: `Collections::singleton` — the modifier passes the source through
    subject.register(
        literal("redirect").fork(
            subject.get_root(),
            Arc::new(
                |c: &CommandContext<CommandSource>| -> Result<Vec<Arc<CommandSource>>, CommandSyntaxException> {
                    Ok(vec![c.get_source_arc()])
                },
            ),
        ),
    );

    let exception = built_in_exceptions().reader_expected_bool().create();
    let thrown = exception.clone();
    subject.register(literal("crash").executes_result(move |_| Err(thrown.clone())));

    // forked mode swallows the failure: the result is 0, the consumer still
    // sees the failed command
    assert_eq!(
        subject.execute("redirect crash", CommandSource {}).unwrap(),
        0
    );
    assert_eq!(*consumer.calls.lock(), vec![(false, 0)]);
}

#[test]
fn exception_in_non_forked_redirect() {
    let mut subject = CommandDispatcher::new();
    let consumer = RecordingConsumer::default();
    subject.set_consumer(consumer.clone());
    let calls = Arc::new(AtomicU32::new(0));
    let counted = calls.clone();
    subject.register(literal("noop").executes(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        3
    }));

    let exception = built_in_exceptions().reader_expected_bool().create();
    let thrown = exception.clone();
    subject.register(
        literal("redirect").redirect_with(
            subject.get_root(),
            Arc::new(
                move |_: &CommandContext<CommandSource>| -> Result<Arc<CommandSource>, CommandSyntaxException> {
                    Err(thrown.clone())
                },
            ),
        ),
    );

    let err = subject
        .execute("redirect noop", CommandSource {})
        .unwrap_err();
    assert_same_exception(&err, &exception);

    // Java: `verifyZeroInteractions(command)`
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(*consumer.calls.lock(), vec![(false, 0)]);
}

#[test]
fn exception_in_forked_redirect() {
    let mut subject = CommandDispatcher::new();
    let consumer = RecordingConsumer::default();
    subject.set_consumer(consumer.clone());
    let calls = Arc::new(AtomicU32::new(0));
    let counted = calls.clone();
    subject.register(literal("noop").executes(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        3
    }));

    let exception = built_in_exceptions().reader_expected_bool().create();
    let thrown = exception.clone();
    subject.register(
        literal("redirect").fork(
            subject.get_root(),
            Arc::new(
                move |_: &CommandContext<CommandSource>| -> Result<Vec<Arc<CommandSource>>, CommandSyntaxException> {
                    Err(thrown.clone())
                },
            ),
        ),
    );

    // forked mode swallows the failing modifier and stops there
    assert_eq!(
        subject.execute("redirect noop", CommandSource {}).unwrap(),
        0
    );

    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(*consumer.calls.lock(), vec![(false, 0)]);
}

#[test]
fn partial_exception_in_forked_redirect() {
    let mut subject = CommandDispatcher::<i64, i32>::new();
    let consumer = SourceRecordingConsumer::default();
    subject.set_consumer(consumer.clone());

    let calls = Arc::new(AtomicU32::new(0));
    let counted = calls.clone();
    subject.register(literal("run").executes(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        3
    }));

    // 10 = the plain source, 20 = the rejected one, 30 = the other one —
    // Java uses three identity-distinct Objects, the values stand in
    let split_sources = vec![Arc::new(10i64), Arc::new(20i64), Arc::new(30i64)];
    let split = Arc::new(
        move |_: &CommandContext<i64, i32>| -> Result<Vec<Arc<i64>>, CommandSyntaxException> {
            Ok(split_sources.clone())
        },
    );
    subject.register(literal("split").fork(subject.get_root(), split));

    let exception = built_in_exceptions().reader_expected_bool().create();
    let thrown = exception.clone();
    let filter = Arc::new(
        move |c: &CommandContext<i64, i32>| -> Result<Vec<Arc<i64>>, CommandSyntaxException> {
            if *c.get_source() == 20 {
                Err(thrown.clone())
            } else {
                Ok(vec![c.get_source_arc()])
            }
        },
    );
    subject.register(literal("filter").fork(subject.get_root(), filter));

    assert_eq!(subject.execute("split filter run", 0).unwrap(), 2);

    // the rejected source never ran the command
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        *consumer.calls.lock(),
        vec![(20, false, 0), (10, true, 3), (30, true, 3)]
    );
}

// positions/cursors are character units, so non-ASCII literals and quoted
// arguments parse end to end just like ASCII ones

#[test]
fn execute_non_ascii_command() {
    let mut subject = CommandDispatcher::new();
    subject.register(
        literal("привет").then(argument("имя", string()).executes(|context| {
            assert_eq!(get_string(context, "имя"), "мир");
            42
        })),
    );

    // the argument must be quoted: unquoted strings only allow ASCII, same
    // as Java's isAllowedInUnquotedString
    assert_eq!(
        subject
            .execute("привет \"мир\"", &CommandSource {})
            .unwrap(),
        42
    );
}

#[test]
fn execute_non_ascii_integer_argument() {
    let mut subject = CommandDispatcher::new();
    subject.register(
        literal("число").then(argument("n", integer()).executes(|context| {
            assert_eq!(get_integer(context, "n"), 42);
            42
        })),
    );

    assert_eq!(subject.execute("число 42", &CommandSource {}).unwrap(), 42);
}
