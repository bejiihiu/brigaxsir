//! `Message`/`LiteralMessage` and the `ImmutableStringReader` read-only view:
//! trait-object usage, external implementations, and feeding a custom
//! read-only reader to the `createWithContext` constructors (the Java
//! `ImmutableStringReader` parameter contract).

use std::sync::Arc;

use brigaxsir::{
    errors::{CommandSyntaxException, SimpleCommandExceptionType},
    message::{LiteralMessage, Message},
    string_reader::{ImmutableStringReader, StringReader},
};

#[test]
fn literal_message_returns_its_string() {
    let message = LiteralMessage::new("Hello, world!");
    assert_eq!(message.get_string(), "Hello, world!");
    // Java: toString() returns the wrapped string
    assert_eq!(message.to_string(), "Hello, world!");
}

#[test]
fn message_works_as_a_trait_object() {
    // Java stores `Message` references (any implementation); same here
    let message: Arc<dyn Message> = Arc::new(LiteralMessage::new("styled"));
    assert_eq!(message.get_string(), "styled");
}

#[test]
fn custom_message_implementation() {
    // the point of the trait: user messages richer than a plain string
    #[derive(Debug)]
    struct PrefixedMessage {
        prefix: String,
        body: String,
    }
    impl Message for PrefixedMessage {
        fn get_string(&self) -> &str {
            &self.body
        }
    }

    let message = Arc::new(PrefixedMessage {
        prefix: "[err] ".to_owned(),
        body: "something failed".to_owned(),
    });
    let exception = CommandSyntaxException::new(Arc::new(NotAThing), message.clone());
    assert_eq!(exception.get_raw_message().get_string(), "something failed");
    // the exception only renders get_string(), like Java's getMessage()
    assert_eq!(exception.get_message(), "something failed");
    let _ = message.prefix; // richer payloads stay reachable through the Arc
}

#[derive(Debug)]
struct NotAThing;
impl brigaxsir::errors::CommandExceptionType for NotAThing {}

/// A read-only view over a string, independent of `StringReader` — proves
/// the `createWithContext(reader)` constructors accept anything implementing
/// `ImmutableStringReader`, like Java's interface parameter.
struct View {
    string: String,
    cursor: usize,
}

impl ImmutableStringReader for View {
    fn string(&self) -> &str {
        &self.string
    }
    fn remaining_length(&self) -> usize {
        self.string.chars().count().saturating_sub(self.cursor)
    }
    fn total_length(&self) -> usize {
        self.string.chars().count()
    }
    fn cursor(&self) -> usize {
        self.cursor
    }
    fn get_read(&self) -> &str {
        let end = self
            .string
            .char_indices()
            .nth(self.cursor)
            .map_or(self.string.len(), |(byte, _)| byte);
        &self.string[..end]
    }
    fn remaining(&self) -> &str {
        let start = self
            .string
            .char_indices()
            .nth(self.cursor)
            .map_or(self.string.len(), |(byte, _)| byte);
        &self.string[start..]
    }
    fn can_read_length(&self, length: usize) -> bool {
        self.cursor + length <= self.string.chars().count()
    }
    fn can_read(&self) -> bool {
        self.can_read_length(1)
    }
    fn peek(&self) -> char {
        self.remaining()
            .chars()
            .next()
            .expect("peeked past the end")
    }
    fn peek_offset(&self, offset: usize) -> char {
        self.remaining()
            .chars()
            .nth(offset)
            .expect("peeked past the end")
    }
}

#[test]
fn create_with_context_accepts_any_immutable_reader() {
    let type_ = SimpleCommandExceptionType::new(Arc::new(LiteralMessage::new("error")));

    let view = View {
        string: "Foo bar".to_owned(),
        cursor: 5,
    };
    let exception = type_.create_with_context(&view);
    assert_eq!(exception.get_input(), Some("Foo bar"));
    assert_eq!(exception.get_cursor(), Some(5));
    assert_eq!(
        exception.get_message(),
        "error at position 5: Foo b<--[HERE]"
    );

    // StringReader itself implements the same trait
    let mut reader = StringReader::from("Foo bar");
    reader.set_cursor(5);
    let exception = type_.create_with_context(&reader);
    assert_eq!(exception.get_input(), Some("Foo bar"));
    assert_eq!(exception.get_cursor(), Some(5));
}
