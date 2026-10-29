//! A custom `ArgumentType` end to end: parsing with a Java-style dynamic
//! error, completion suggestions, usage examples, and typed reads of the
//! parsed value.
//!
//! Run with `cargo run --example custom_argument`.

use std::{
    any::Any,
    hash::{Hash, Hasher},
    sync::Arc,
};

use brigaxsir::{
    arguments::ArgumentType,
    context::ParsedValue,
    errors::{CommandSyntaxException, DynamicCommandExceptionType},
    message::LiteralMessage,
    prelude::*,
    string_reader::StringReader,
    suggestion::{SuggestionsBuilder, SuggestionsFuture},
};

/// The words this argument accepts.
const COLORS: [&str; 5] = ["red", "green", "blue", "black", "white"];

/// The value `ColorArgumentType` parses into.
#[derive(Debug, PartialEq, Eq, Hash)]
struct Color {
    name: String,
}

impl ParsedValue for Color {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn type_name(&self) -> &'static str {
        "Color"
    }

    fn value_eq(&self, other: &dyn ParsedValue) -> bool {
        other.as_any().downcast_ref::<Color>() == Some(self)
    }

    fn value_hash(&self, state: &mut dyn Hasher) {
        state.write(self.name.as_bytes())
    }
}

/// One shared error type for every "unknown color" failure, with the bad
/// name baked into the message — Java's `DynamicCommandExceptionType`.
static INVALID_COLOR: std::sync::LazyLock<DynamicCommandExceptionType<String>> =
    std::sync::LazyLock::new(|| {
        DynamicCommandExceptionType::new(|name| {
            Arc::new(LiteralMessage::new(format!("Unknown color '{name}'")))
        })
    });

/// Parses a color name out of the input. Argument types are plain data — the
/// unit struct here — so they can live anywhere a built-in type can.
struct ColorArgumentType;

impl ArgumentType for ColorArgumentType {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
        // copy the word out first: the borrowed slice shares the reader
        let word = reader.read_unquoted_string().to_owned();
        if COLORS.contains(&word.as_str()) {
            Ok(Arc::new(Color { name: word }))
        } else {
            // the error carries the reader's input and cursor, so it renders
            // with the `<--[HERE]` snippet like every built-in error
            Err(INVALID_COLOR.create_with_context(reader, word))
        }
    }

    fn list_suggestions(&self, builder: SuggestionsBuilder) -> SuggestionsFuture {
        let mut builder = builder;
        for color in COLORS {
            builder = builder.suggest(color);
        }
        builder.build_future()
    }

    fn examples(&self) -> Vec<String> {
        COLORS.iter().map(|color| (*color).to_owned()).collect()
    }
}

fn main() {
    let mut dispatcher = CommandDispatcher::new();

    dispatcher.register(
        literal("paint").then(argument("color", ColorArgumentType).executes(|ctx| {
            // the parsed value comes back typed, like any built-in
            let color: &Color = ctx.get_argument("color");
            println!("painting with {}", color.name);
            1
        })),
    );

    // A valid input parses and runs.
    let _ = dispatcher.execute("paint red", ()).unwrap();

    // An invalid one fails with the custom error, snippet included.
    let error = dispatcher.execute("paint mauve", ()).unwrap_err();
    println!("error: {}", error.get_message());
    println!("context: {}", error.get_context().unwrap());

    // ...and the argument completes like any built-in one.
    let parse = dispatcher.parse("paint ", ());
    let suggestions = block_on(CommandDispatcher::get_completion_suggestions(parse)).unwrap();
    let texts: Vec<&str> = suggestions.list().iter().map(|s| s.text()).collect();
    println!("completions for `paint `: {texts:?}");
    assert_eq!(texts, ["black", "blue", "green", "red", "white"]);
}

/// Minimal `block_on`, std-only: a channel-based waker. The library's own
/// futures complete on the first poll, so this never actually blocks. In a
/// real application you would await the future inside your own runtime
/// instead (`futures::executor::block_on`, tokio, smol, ...).
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
