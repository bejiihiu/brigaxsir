//! A custom `CommandExceptionType` working end to end: a user argument type
//! raises its own error during parsing, the dispatcher keeps the *original*
//! exception in `ParseResults::get_exceptions` (Java stores the caught
//! `CommandSyntaxException` unwrapped in `parseNodes`), and `execute_parsed`
//! surfaces it unchanged when it is the only candidate error.

use std::{any::Any, sync::Arc};

use brigaxsir::{
    arguments::ArgumentType,
    builder::{literal_argument_builder::literal, required_argument_builder::argument},
    command_dispatcher::CommandDispatcher,
    context::ParsedValue,
    errors::{CommandExceptionType, CommandSyntaxException},
    message::{LiteralMessage, Message},
    string_reader::StringReader,
};

/// A bespoke exception type, the way user crates define them in Java
/// (`class NotAGoodWord implements CommandExceptionType`).
#[derive(Debug)]
struct NotAGoodWord;
impl CommandExceptionType for NotAGoodWord {}

/// An argument type that only accepts the word "good" and throws a
/// custom-typed exception otherwise.
struct GoodWordArgument {
    type_: Arc<dyn CommandExceptionType>,
}

impl ArgumentType for GoodWordArgument {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn parse(
        &self,
        reader: &mut StringReader,
    ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
        let word = reader.read_unquoted_string().to_owned();
        if word != "good" {
            let message: Arc<dyn Message> =
                Arc::new(LiteralMessage::new(format!("Not a good word: {word}")));
            return Err(CommandSyntaxException::new_with_context(
                self.type_.clone(),
                message,
                reader.string(),
                reader.cursor(),
            ));
        }
        Ok(Arc::new(word))
    }

    fn examples(&self) -> Vec<String> {
        vec!["good".to_owned()]
    }
}

#[test]
fn custom_type_flows_through_parse_and_execute() {
    let type_: Arc<dyn CommandExceptionType> = Arc::new(NotAGoodWord);

    let mut subject = CommandDispatcher::<(), i32>::new();
    subject.register(
        literal("check")
            .then(argument(
                "word",
                GoodWordArgument {
                    type_: type_.clone(),
                },
            ))
            .executes(|_| 42),
    );

    let parse = subject.parse("check bad", ());
    #[allow(clippy::mutable_key_type)] // keys are never mutated; same shape as Java's map
    let exceptions = parse.get_exceptions();
    assert_eq!(exceptions.len(), 1);
    let exception = exceptions.values().next().unwrap();
    assert!(exception.is_type(&type_));
    // cursor 9 = end of the consumed "bad"; window chars 0..9 = "check bad"
    assert_eq!(
        exception.get_message(),
        "Not a good word: bad at position 9: check bad<--[HERE]"
    );

    // executing a failed parse surfaces the single collected exception
    // unchanged — same type, same message
    let err = subject.execute_parsed(parse).unwrap_err();
    assert!(err.is_type(&type_));
    assert_eq!(
        err.get_message(),
        "Not a good word: bad at position 9: check bad<--[HERE]"
    );
}

#[test]
fn custom_type_via_simple_struct_also_flows_through() {
    // the other custom-error route: reuse SimpleCommandExceptionType instead
    // of implementing the marker trait directly
    let type_ = Arc::new(brigaxsir::errors::SimpleCommandExceptionType::new(
        Arc::new(LiteralMessage::new("no good words here")),
    ));

    struct BadAlways {
        type_: Arc<brigaxsir::errors::SimpleCommandExceptionType>,
    }

    impl ArgumentType for BadAlways {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn parse(
            &self,
            _reader: &mut StringReader,
        ) -> Result<Arc<dyn ParsedValue>, CommandSyntaxException> {
            Err(self.type_.create())
        }

        fn examples(&self) -> Vec<String> {
            vec![]
        }
    }

    let mut subject = CommandDispatcher::<(), i32>::new();
    subject.register(
        literal("check")
            .then(argument(
                "word",
                BadAlways {
                    type_: type_.clone(),
                },
            ))
            .executes(|_| 42),
    );

    let err = subject.execute("check anything", ()).unwrap_err();
    assert!(err.is_type(type_.as_type()));
    assert_eq!(err.get_raw_message().get_string(), "no good words here");
}
