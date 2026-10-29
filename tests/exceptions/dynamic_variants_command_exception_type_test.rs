//! Tests for the multi-argument `Dynamic2/3/4/N` exception types. Java has
//! no dedicated test files for these classes; the shapes below pin the
//! `create` / `createWithContext` contracts (type identity, message text,
//! input/cursor attachment) for each arity.

use std::sync::Arc;

use brigaxsir::{
    errors::{
        Dynamic2CommandExceptionType, Dynamic3CommandExceptionType, Dynamic4CommandExceptionType,
        DynamicNCommandExceptionType,
    },
    message::{LiteralMessage, Message},
    string_reader::StringReader,
};

fn message(string: impl Into<String>) -> Arc<dyn Message> {
    Arc::new(LiteralMessage::new(string))
}

#[test]
fn dynamic2_create() {
    let type_ = Dynamic2CommandExceptionType::new(|a: i32, b: i32| -> Arc<dyn Message> {
        message(format!("{a} vs {b}"))
    });
    let exception = type_.create(1, 2);
    assert!(exception.is_type(type_.as_type()));
    assert_eq!(exception.get_input(), None);
    assert_eq!(exception.get_cursor(), None);
    assert_eq!(exception.get_raw_message().get_string(), "1 vs 2");
    assert_eq!(exception.get_message(), "1 vs 2");
}

#[test]
fn dynamic2_create_with_context() {
    let type_ = Dynamic2CommandExceptionType::new(|a: i32, b: i32| -> Arc<dyn Message> {
        message(format!("{a} vs {b}"))
    });
    let mut reader = StringReader::from("Foo bar");
    reader.set_cursor(5);
    let exception = type_.create_with_context(&reader, 1, 2);
    assert!(exception.is_type(type_.as_type()));
    assert_eq!(exception.get_input(), Some("Foo bar"));
    assert_eq!(exception.get_cursor(), Some(5));
    // cursor 5 in "Foo bar": the window is the first five chars, "Foo b"
    assert_eq!(
        exception.get_message(),
        "1 vs 2 at position 5: Foo b<--[HERE]"
    );
}

#[test]
fn dynamic3_create_and_create_with_context() {
    let type_ = Dynamic3CommandExceptionType::new(|a: u8, b: u8, c: u8| -> Arc<dyn Message> {
        message(format!("{a}-{b}-{c}"))
    });

    let exception = type_.create(1, 2, 3);
    assert!(exception.is_type(type_.as_type()));
    assert_eq!(exception.get_message(), "1-2-3");

    let mut reader = StringReader::from("Foo bar");
    reader.set_cursor(5);
    let exception = type_.create_with_context(&reader, 1, 2, 3);
    assert!(exception.is_type(type_.as_type()));
    assert_eq!(exception.get_input(), Some("Foo bar"));
    assert_eq!(exception.get_cursor(), Some(5));
    assert_eq!(
        exception.get_message(),
        "1-2-3 at position 5: Foo b<--[HERE]"
    );
}

#[test]
fn dynamic4_create_and_create_with_context() {
    let type_ =
        Dynamic4CommandExceptionType::new(|a: u8, b: u8, c: u8, d: u8| -> Arc<dyn Message> {
            message(format!("{a}+{b}+{c}+{d}"))
        });

    let exception = type_.create(1, 2, 3, 4);
    assert!(exception.is_type(type_.as_type()));
    assert_eq!(exception.get_message(), "1+2+3+4");

    let mut reader = StringReader::from("Foo bar");
    reader.set_cursor(5);
    let exception = type_.create_with_context(&reader, 1, 2, 3, 4);
    assert!(exception.is_type(type_.as_type()));
    assert_eq!(exception.get_input(), Some("Foo bar"));
    assert_eq!(exception.get_cursor(), Some(5));
    assert_eq!(
        exception.get_message(),
        "1+2+3+4 at position 5: Foo b<--[HERE]"
    );
}

#[test]
fn dynamic_n_create() {
    let type_ = DynamicNCommandExceptionType::new(|args: &[String]| -> Arc<dyn Message> {
        message(args.join(", "))
    });

    let exception = type_.create(&["a".to_owned(), "b".to_owned(), "c".to_owned()]);
    assert!(exception.is_type(type_.as_type()));
    assert_eq!(exception.get_input(), None);
    assert_eq!(exception.get_cursor(), None);
    assert_eq!(exception.get_message(), "a, b, c");
}

#[test]
fn dynamic_n_create_with_context() {
    let type_ = DynamicNCommandExceptionType::new(|args: &[String]| -> Arc<dyn Message> {
        message(args.join(", "))
    });

    let mut reader = StringReader::from("Foo bar");
    reader.set_cursor(5);
    let exception = type_.create_with_context(&reader, &["a".to_owned()]);
    assert!(exception.is_type(type_.as_type()));
    assert_eq!(exception.get_input(), Some("Foo bar"));
    assert_eq!(exception.get_cursor(), Some(5));
    assert_eq!(exception.get_message(), "a at position 5: Foo b<--[HERE]");
}

#[test]
fn dynamic_n_create_empty() {
    // Java's varargs form always carries the leading `a`, but the function
    // contract here is just "the whole argument slice" — an empty slice is
    // the natural degenerate case and must not panic
    let type_ = DynamicNCommandExceptionType::new(|args: &[u8]| -> Arc<dyn Message> {
        message(format!("got {}", args.len()))
    });
    let exception = type_.create(&[]);
    assert_eq!(exception.get_message(), "got 0");
}

#[test]
fn instances_are_distinct_type_objects() {
    // two separately constructed types with equal functions are still
    // distinct identity-wise, like two Java objects
    let a = Dynamic2CommandExceptionType::new(|x: i32, y: i32| -> Arc<dyn Message> {
        message(format!("{x}{y}"))
    });
    let b = Dynamic2CommandExceptionType::new(|x: i32, y: i32| -> Arc<dyn Message> {
        message(format!("{x}{y}"))
    });
    assert!(!a.create(1, 2).is_type(b.as_type()));
}
