//! Port of Java's `DynamicCommandSyntaxExceptionTypeTest` (1:1 in meaning).

use std::sync::Arc;

use brigaxsir::{
    errors::DynamicCommandExceptionType,
    message::{LiteralMessage, Message},
    string_reader::StringReader,
};

#[test]
fn create_with_context() {
    let type_ = DynamicCommandExceptionType::new(|name: String| -> Arc<dyn Message> {
        Arc::new(LiteralMessage::new(format!("Hello, {name}!")))
    });
    let mut reader = StringReader::from("Foo bar");
    reader.set_cursor(5);
    let exception = type_.create_with_context(&reader, "World".to_owned());
    assert!(exception.is_type(type_.as_type()));
    assert_eq!(exception.get_input(), Some("Foo bar"));
    assert_eq!(exception.get_cursor(), Some(5));
}
