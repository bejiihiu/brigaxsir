use std::{future::Future, sync::Arc};

use brigaxsir::{
    context::{CommandContext, StringRange},
    errors::SimpleCommandExceptionType,
    message::LiteralMessage,
    prelude::*,
    string_reader::StringReader,
    suggestion::{
        Suggestion, SuggestionProvider, Suggestions, SuggestionsBuilder, SuggestionsFuture,
    },
};

/// Minimal `block_on` for the tests, std-only: a channel-based waker. The
/// library's own futures complete on the first poll, so this never actually
/// blocks in practice.
fn block_on<F: Future>(future: F) -> F::Output {
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

fn block_on_suggestions(future: SuggestionsFuture) -> Suggestions {
    block_on(future).expect("suggestion future failed")
}

fn test_suggestions(
    subject: &CommandDispatcher<()>,
    contents: &str,
    cursor: usize,
    range: StringRange,
    suggestions: Vec<&str>,
) {
    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions_with_cursor(
        subject.parse(contents, ()),
        cursor,
    ));
    assert_eq!(result.range(), range);

    let mut expected = Vec::new();
    for suggestion in suggestions {
        expected.push(Suggestion::new(range, suggestion));
    }

    assert_eq!(result.list(), expected.as_slice());
}

fn input_with_offset(input: &str, offset: usize) -> StringReader {
    let mut result = StringReader::from(input);
    result.set_cursor(offset);
    result
}

#[test]
fn get_completion_suggestions_non_ascii() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(literal("привет"));

    // "прив" is 4 characters; suggestion splicing must not trip over bytes
    test_suggestions(
        &subject,
        "прив",
        4,
        StringRange::between(0, 4),
        vec!["привет"],
    );
}

// Non-ASCII completion behaviour. The expected values below were produced by
// the real Java 1.3 classes (compiled and run locally as the oracle). Where
// the two index models differ the cursor numbers are translated: a Rust
// cursor counts characters, Java's counts UTF-16 units, and 🚀 is one
// character but two units — so a Rust cursor c (c >= 1) into "🚀..."
// corresponds to Java cursor c + 1. See the StringReader docs for the
// documented project-wide divergence.

#[test]
fn get_completion_suggestions_non_ascii_emoji_literal() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(literal("🚀_да"));
    subject.register(literal("🌍_нет"));

    // Java oracle at cursor 2 (the whole emoji): range 0..2, ["🚀_да"] —
    // here the emoji is one character, so the range is 0..1
    test_suggestions(&subject, "🚀", 1, StringRange::between(0, 1), vec!["🚀_да"]);

    // end-of-input anchor over the same tree, same answer
    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(
        subject.parse("🚀", ()),
    ));
    assert_eq!(result.range(), StringRange::between(0, 1));
    assert_eq!(
        result.list(),
        [Suggestion::new(StringRange::between(0, 1), "🚀_да")].as_slice()
    );
}

#[test]
fn get_completion_suggestions_non_ascii_cyrillic_case_insensitive() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(literal("привет"));
    subject.register(literal("прощай"));

    // an uppercase Cyrillic prefix never parses (literal matching is case
    // sensitive, Java included) but still completes case-insensitively —
    // this rides the lowercase prefix-match path end to end
    test_suggestions(
        &subject,
        "ПРИВ",
        4,
        StringRange::between(0, 4),
        vec!["привет"],
    );

    // both Cyrillic literals share a prefix; the pinned order is the
    // Java-observed one
    test_suggestions(
        &subject,
        "пр",
        2,
        StringRange::between(0, 2),
        vec!["привет", "прощай"],
    );
}

#[test]
fn get_completion_suggestions_non_ascii_moving_cursor() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(literal("🚀_да").then(literal("флаг")).then(literal("стоп")));
    subject.register(literal("🌍_нет"));

    // "🚀_да ф" is six characters; Java sees seven UTF-16 units, so the
    // oracle values below were taken at cursor c + 1 on the Java side
    test_suggestions(
        &subject,
        "🚀_да ф",
        0,
        StringRange::at(0),
        vec!["🌍_нет", "🚀_да"],
    );
    test_suggestions(
        &subject,
        "🚀_да ф",
        1,
        StringRange::between(0, 1),
        vec!["🚀_да"],
    );
    test_suggestions(
        &subject,
        "🚀_да ф",
        2,
        StringRange::between(0, 2),
        vec!["🚀_да"],
    );
    test_suggestions(
        &subject,
        "🚀_да ф",
        3,
        StringRange::between(0, 3),
        vec!["🚀_да"],
    );
    test_suggestions(&subject, "🚀_да ф", 4, StringRange::at(0), vec![]);
    test_suggestions(
        &subject,
        "🚀_да ф",
        5,
        StringRange::at(5),
        vec!["стоп", "флаг"],
    );
    test_suggestions(
        &subject,
        "🚀_да ф",
        6,
        StringRange::between(5, 6),
        vec!["флаг"],
    );

    // end-of-input anchor agrees with cursor 6 (Java's total length 7)
    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(
        subject.parse("🚀_да ф", ()),
    ));
    assert_eq!(result.range(), StringRange::between(5, 6));
    assert_eq!(
        result.list(),
        [Suggestion::new(StringRange::between(5, 6), "флаг")].as_slice()
    );
}

#[test]
fn get_completion_suggestions_root_commands() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(literal("foo"));
    subject.register(literal("bar"));
    subject.register(literal("baz"));

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(
        subject.parse("", ()),
    ));

    assert_eq!(result.range(), StringRange::at(0));
    assert_eq!(
        result.list(),
        [
            Suggestion::new(StringRange::at(0), "bar"),
            Suggestion::new(StringRange::at(0), "baz"),
            Suggestion::new(StringRange::at(0), "foo")
        ]
        .as_slice()
    );
}

#[test]
fn get_completion_suggestions_root_commands_with_input_offset() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(literal("foo"));
    subject.register(literal("bar"));
    subject.register(literal("baz"));

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(
        subject.parse(input_with_offset("OOO", 3), ()),
    ));

    assert_eq!(result.range(), StringRange::at(3));
    assert_eq!(
        result.list(),
        [
            Suggestion::new(StringRange::at(3), "bar"),
            Suggestion::new(StringRange::at(3), "baz"),
            Suggestion::new(StringRange::at(3), "foo")
        ]
        .as_slice()
    );
}

#[test]
fn get_completion_suggestions_root_commands_partial() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(literal("foo"));
    subject.register(literal("bar"));
    subject.register(literal("baz"));

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(
        subject.parse("b", ()),
    ));

    assert_eq!(result.range(), StringRange::between(0, 1));
    assert_eq!(
        result.list(),
        [
            Suggestion::new(StringRange::between(0, 1), "bar"),
            Suggestion::new(StringRange::between(0, 1), "baz")
        ]
        .as_slice()
    );
}

#[test]
fn get_completion_suggestions_root_commands_partial_with_input_offset() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(literal("foo"));
    subject.register(literal("bar"));
    subject.register(literal("baz"));

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(
        subject.parse(input_with_offset("Zb", 1), ()),
    ));

    assert_eq!(result.range(), StringRange::between(1, 2));
    assert_eq!(
        result.list(),
        [
            Suggestion::new(StringRange::between(1, 2), "bar"),
            Suggestion::new(StringRange::between(1, 2), "baz")
        ]
        .as_slice()
    );
}

#[test]
fn get_completion_suggestions_sub_commands() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(
        literal("parent")
            .then(literal("foo"))
            .then(literal("bar"))
            .then(literal("baz")),
    );

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(
        subject.parse("parent ", ()),
    ));

    assert_eq!(result.range(), StringRange::at(7));
    assert_eq!(
        result.list(),
        [
            Suggestion::new(StringRange::at(7), "bar"),
            Suggestion::new(StringRange::at(7), "baz"),
            Suggestion::new(StringRange::at(7), "foo")
        ]
        .as_slice()
    );
}

#[test]
fn get_completion_suggestions_moving_cursor_sub_commands() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(
        literal("parent_one")
            .then(literal("faz"))
            .then(literal("fbz"))
            .then(literal("gaz")),
    );

    subject.register(literal("parent_two"));

    test_suggestions(
        &subject,
        "parent_one faz ",
        0,
        StringRange::at(0),
        vec!["parent_one", "parent_two"],
    );
    test_suggestions(
        &subject,
        "parent_one faz ",
        1,
        StringRange::between(0, 1),
        vec!["parent_one", "parent_two"],
    );
    test_suggestions(
        &subject,
        "parent_one faz ",
        7,
        StringRange::between(0, 7),
        vec!["parent_one", "parent_two"],
    );
    test_suggestions(
        &subject,
        "parent_one faz ",
        8,
        StringRange::between(0, 8),
        vec!["parent_one"],
    );
    test_suggestions(&subject, "parent_one faz ", 10, StringRange::at(0), vec![]);
    test_suggestions(
        &subject,
        "parent_one faz ",
        11,
        StringRange::at(11),
        vec!["faz", "fbz", "gaz"],
    );
    test_suggestions(
        &subject,
        "parent_one faz ",
        12,
        StringRange::between(11, 12),
        vec!["faz", "fbz"],
    );
    test_suggestions(
        &subject,
        "parent_one faz ",
        13,
        StringRange::between(11, 13),
        vec!["faz"],
    );
    test_suggestions(&subject, "parent_one faz ", 14, StringRange::at(0), vec![]);
    test_suggestions(&subject, "parent_one faz ", 15, StringRange::at(0), vec![]);
}

#[test]
fn get_completion_suggestions_sub_commands_partial() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(
        literal("parent")
            .then(literal("foo"))
            .then(literal("bar"))
            .then(literal("baz")),
    );

    let parse = subject.parse("parent b", ());

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(parse));

    assert_eq!(result.range(), StringRange::between(7, 8));
    assert_eq!(
        result.list(),
        [
            Suggestion::new(StringRange::between(7, 8), "bar"),
            Suggestion::new(StringRange::between(7, 8), "baz")
        ]
        .as_slice()
    );
}

#[test]
fn get_completion_suggestions_sub_commands_partial_with_input_offset() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(
        literal("parent")
            .then(literal("foo"))
            .then(literal("bar"))
            .then(literal("baz")),
    );

    let parse = subject.parse(input_with_offset("junk parent b", 5), ());

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(parse));

    assert_eq!(result.range(), StringRange::between(12, 13));
    assert_eq!(
        result.list(),
        [
            Suggestion::new(StringRange::between(12, 13), "bar"),
            Suggestion::new(StringRange::between(12, 13), "baz")
        ]
        .as_slice()
    );
}

#[test]
fn get_completion_suggestions_redirect() {
    let mut subject = CommandDispatcher::<()>::new();
    let actual = subject.register(literal("actual").then(literal("sub")));
    subject.register(literal("redirect").redirect(actual));

    let parse = subject.parse("redirect ", ());

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(parse));

    assert_eq!(result.range(), StringRange::at(9));
    assert_eq!(
        result.list(),
        [Suggestion::new(StringRange::at(9), "sub")].as_slice()
    );
}

#[test]
fn get_completion_suggestions_redirect_partial() {
    let mut subject = CommandDispatcher::<()>::new();
    let actual = subject.register(literal("actual").then(literal("sub")));
    subject.register(literal("redirect").redirect(actual));

    let parse = subject.parse("redirect s", ());

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(parse));

    assert_eq!(result.range(), StringRange::between(9, 10));
    assert_eq!(
        result.list(),
        [Suggestion::new(StringRange::between(9, 10), "sub")].as_slice()
    );
}

#[test]
fn get_completion_suggestions_moving_cursor_redirect() {
    let mut subject = CommandDispatcher::<()>::new();
    let actual_one = subject.register(
        literal("actual_one")
            .then(literal("faz"))
            .then(literal("fbz"))
            .then(literal("gaz")),
    );

    subject.register(literal("actual_two"));

    subject.register(literal("redirect_one").redirect(actual_one.clone()));
    subject.register(literal("redirect_two").redirect(actual_one));

    test_suggestions(
        &subject,
        "redirect_one faz ",
        0,
        StringRange::at(0),
        vec!["actual_one", "actual_two", "redirect_one", "redirect_two"],
    );
    test_suggestions(
        &subject,
        "redirect_one faz ",
        9,
        StringRange::between(0, 9),
        vec!["redirect_one", "redirect_two"],
    );
    test_suggestions(
        &subject,
        "redirect_one faz ",
        10,
        StringRange::between(0, 10),
        vec!["redirect_one"],
    );
    test_suggestions(
        &subject,
        "redirect_one faz ",
        12,
        StringRange::at(0),
        vec![],
    );
    test_suggestions(
        &subject,
        "redirect_one faz ",
        13,
        StringRange::at(13),
        vec!["faz", "fbz", "gaz"],
    );
    test_suggestions(
        &subject,
        "redirect_one faz ",
        14,
        StringRange::between(13, 14),
        vec!["faz", "fbz"],
    );
    test_suggestions(
        &subject,
        "redirect_one faz ",
        15,
        StringRange::between(13, 15),
        vec!["faz"],
    );
    test_suggestions(
        &subject,
        "redirect_one faz ",
        16,
        StringRange::at(0),
        vec![],
    );
    test_suggestions(
        &subject,
        "redirect_one faz ",
        17,
        StringRange::at(0),
        vec![],
    );
}

#[test]
fn get_completion_suggestions_redirect_partial_with_input_offset() {
    let mut subject = CommandDispatcher::<()>::new();
    let actual = subject.register(literal("actual").then(literal("sub")));
    subject.register(literal("redirect").redirect(actual));

    let parse = subject.parse(input_with_offset("/redirect s", 1), ());

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(parse));

    assert_eq!(result.range(), StringRange::between(10, 11));
    assert_eq!(
        result.list(),
        [Suggestion::new(StringRange::between(10, 11), "sub")].as_slice()
    );
}

#[test]
fn get_completion_suggestions_redirect_lots() {
    let mut subject = CommandDispatcher::<()>::new();
    let loop_ = subject.register(literal("redirect"));
    subject.register(
        literal("redirect").then(literal("loop").then(argument("loop", integer()).redirect(loop_))),
    );

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(
        subject.parse("redirect loop 1 loop 02 loop 003 ", ()),
    ));

    assert_eq!(result.range(), StringRange::at(33));
    assert_eq!(
        result.list(),
        [Suggestion::new(StringRange::at(33), "loop")].as_slice()
    );
}

// Java: getCompletionSuggestions_redirect_contextualArgument — a custom
// provider reading a previously parsed argument

struct ContextualSuggestions;

impl SuggestionProvider<(), i32> for ContextualSuggestions {
    fn get_suggestions(
        &self,
        context: CommandContext<(), i32>,
        mut builder: SuggestionsBuilder,
    ) -> SuggestionsFuture {
        let arg_one = get_string(&context, "arg_one");
        builder = builder.suggest(format!("contextual_{arg_one}"));
        builder.build_future()
    }
}

#[test]
fn get_completion_suggestions_redirect_contextual_argument() {
    let mut subject = CommandDispatcher::<()>::new();
    let actual = subject.register(
        literal("actual").then(
            argument("arg_one", word())
                .then(argument("arg_two", word()).suggests(ContextualSuggestions)),
        ),
    );
    subject.register(literal("redirect").redirect(actual));

    let parse = subject.parse("redirect first ", ());

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(parse));

    assert_eq!(result.range(), StringRange::at(15));
    assert_eq!(
        result.list(),
        [Suggestion::new(StringRange::at(15), "contextual_first")].as_slice()
    );
}

#[test]
fn get_completion_suggestions_execute_simulation() {
    let mut subject = CommandDispatcher::<()>::new();
    let execute = subject.register(literal("execute"));
    subject.register(
        literal("execute")
            .then(literal("as").then(argument("name", word()).redirect(execute.clone())))
            .then(literal("store").then(argument("name", word()).redirect(execute)))
            .then(literal("run").executes(|_| 0)),
    );

    let parse = subject.parse("execute as Dinnerbone as", ());

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(parse));

    assert!(result.is_empty());
}

#[test]
fn get_completion_suggestions_execute_simulation_partial() {
    let mut subject = CommandDispatcher::<()>::new();
    let execute = subject.register(literal("execute"));
    subject.register(
        literal("execute")
            .then(
                literal("as")
                    .then(literal("bar").redirect(execute.clone()))
                    .then(literal("baz").redirect(execute.clone())),
            )
            .then(literal("store").then(argument("name", word()).redirect(execute)))
            .then(literal("run").executes(|_| 0)),
    );

    let parse = subject.parse("execute as bar as ", ());

    let result = block_on_suggestions(CommandDispatcher::get_completion_suggestions(parse));

    assert_eq!(result.range(), StringRange::at(18));
    assert_eq!(
        result.list(),
        [
            Suggestion::new(StringRange::at(18), "bar"),
            Suggestion::new(StringRange::at(18), "baz")
        ]
        .as_slice()
    );
}

// A provider that fails inside its future. Java has no equivalent test: its
// merged future would never complete (allOf fails, the thenRun merge is
// skipped, nobody observes the failed stage), so this pins the documented
// Rust decision instead — see SuggestionsFuture.

struct FailingSuggestions(&'static str);

impl SuggestionProvider<(), i32> for FailingSuggestions {
    fn get_suggestions(
        &self,
        _context: CommandContext<(), i32>,
        _builder: SuggestionsBuilder,
    ) -> SuggestionsFuture {
        let type_ = SimpleCommandExceptionType::new(Arc::new(LiteralMessage::new(self.0)));
        Suggestions::failed_future(type_.create())
    }
}

#[test]
fn get_completion_suggestions_failing_provider_propagates_the_error() {
    let mut subject = CommandDispatcher::<()>::new();
    subject.register(
        literal("cmd")
            .then(argument("a", word()).suggests(FailingSuggestions("first boom")))
            .then(argument("b", word()).suggests(FailingSuggestions("second boom"))),
    );

    let parse = subject.parse("cmd ", ());
    let error = block_on(CommandDispatcher::get_completion_suggestions(parse))
        .expect_err("the merged future should carry the provider error");

    // every provider ran, and the first error in list order wins
    assert_eq!(error.get_message(), "first boom");
}
