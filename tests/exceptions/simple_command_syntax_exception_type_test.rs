//! Port of Java's `SimpleCommandSyntaxExceptionTypeTest` (1:1 in meaning;
//! Java's Mockito mock of `CommandExceptionType` is a unit struct here).

use std::sync::Arc;

use brigaxsir::{
    errors::{CommandExceptionType, CommandSyntaxException, SimpleCommandExceptionType},
    message::LiteralMessage,
    string_reader::StringReader,
};

#[derive(Debug)]
struct MockExceptionType;
impl CommandExceptionType for MockExceptionType {}

#[test]
fn create_with_context() {
    let type_ = SimpleCommandExceptionType::new(Arc::new(LiteralMessage::new("error")));
    let mut reader = StringReader::from("Foo bar");
    reader.set_cursor(5);
    let exception = type_.create_with_context(&reader);
    assert!(exception.is_type(type_.as_type()));
    assert_eq!(exception.get_input(), Some("Foo bar"));
    assert_eq!(exception.get_cursor(), Some(5));
}

#[test]
fn get_context_none() {
    let exception = CommandSyntaxException::new(
        Arc::new(MockExceptionType),
        Arc::new(LiteralMessage::new("error")),
    );
    assert_eq!(exception.get_context(), None);
}

#[test]
fn get_context_short() {
    let exception = CommandSyntaxException::new_with_context(
        Arc::new(MockExceptionType),
        Arc::new(LiteralMessage::new("error")),
        "Hello world!",
        5,
    );
    assert_eq!(exception.get_context().as_deref(), Some("Hello<--[HERE]"));
}

#[test]
fn get_context_long() {
    let exception = CommandSyntaxException::new_with_context(
        Arc::new(MockExceptionType),
        Arc::new(LiteralMessage::new("error")),
        "Hello world! This has an error in it. Oh dear!",
        20,
    );
    assert_eq!(
        exception.get_context().as_deref(),
        Some("...d! This ha<--[HERE]")
    );
}
