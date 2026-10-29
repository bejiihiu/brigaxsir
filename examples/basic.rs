//! The README quickstart, plus the parse/execute split and usage strings.
//!
//! Run with `cargo run --example basic`.

use std::sync::Arc;

use brigaxsir::prelude::*;

#[derive(Debug, PartialEq)]
struct CommandSource {}

fn main() {
    let mut dispatcher = CommandDispatcher::new();

    // Registering a command: `foo` with a `foo <bar>` child, each with its
    // own `executes` closure. Registration is append-only, so anything else
    // in the program can keep extending this tree.
    dispatcher.register(
        literal("foo")
            .then(argument("bar", integer()).executes(|ctx| {
                println!("Bar is {}", get_integer(ctx, "bar"));
                1
            }))
            .executes(|_| {
                println!("Called foo with no arguments");
                1
            }),
    );

    // `foo` and `foo <bar>` are both valid commands now.
    assert_eq!(
        dispatcher
            .execute("foo", Arc::new(CommandSource {}))
            .unwrap(),
        1
    );
    assert_eq!(
        dispatcher
            .execute("foo 123", Arc::new(CommandSource {}))
            .unwrap(),
        1
    );

    // A failed parse surfaces as an error instead of an exception; the
    // message carries the input snippet, Java-style.
    let error = dispatcher
        .execute("foo abc", Arc::new(CommandSource {}))
        .unwrap_err();
    println!("error: {}", error.get_message());

    // Parse and execute in two steps, so the (expensive) parse can be cached
    // and executed more than once.
    let parse = dispatcher.parse("foo 123", Arc::new(CommandSource {}));
    assert!(parse.get_exceptions().is_empty());
    assert_eq!(dispatcher.execute_parsed(parse.clone()).unwrap(), 1);
    assert_eq!(dispatcher.execute_parsed(parse).unwrap(), 1);

    // Usage strings for help output: every executable path, plus the compact
    // "smart" rendering.
    let source = Arc::new(CommandSource {});
    let root = dispatcher.get_root();
    let all_usage = dispatcher.get_all_usage(&root.read(), &source, true);
    println!("all usage: {all_usage:?}");
    assert_eq!(all_usage, ["foo", "foo <bar>"]);

    for (node, usage) in dispatcher.get_smart_usage(&root.read(), &source) {
        println!("smart usage of {}: {usage}", node.read().get_name());
    }
}
