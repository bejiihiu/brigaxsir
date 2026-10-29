//! Every built-in error message, byte-compared against the strings in Java's
//! `BuiltInExceptions.java`, plus the identity guarantees of the shared
//! provider instances.

use std::sync::Arc;

use brigaxsir::errors::{BuiltInExceptionProvider, built_in_exceptions};

/// The bare message of an exception created by `type_`, no context attached.
fn text<F, T>(
    get: F,
    create: impl FnOnce(&T) -> brigaxsir::errors::CommandSyntaxException,
) -> String
where
    F: Fn(&brigaxsir::errors::BuiltInExceptions) -> &T,
{
    create(get(built_in_exceptions()))
        .get_raw_message()
        .get_string()
        .to_owned()
}

#[test]
fn numeric_range_messages_match_java_byte_for_byte() {
    let built_in = built_in_exceptions();
    assert_eq!(
        built_in
            .double_too_low()
            .create(1.0, 2.0)
            .get_raw_message()
            .get_string(),
        "Double must not be less than 2.0, found 1.0"
    );
    assert_eq!(
        built_in
            .double_too_high()
            .create(3.0, 2.0)
            .get_raw_message()
            .get_string(),
        "Double must not be more than 2.0, found 3.0"
    );
    assert_eq!(
        built_in
            .float_too_low()
            .create(1.0, 2.0)
            .get_raw_message()
            .get_string(),
        "Float must not be less than 2.0, found 1.0"
    );
    assert_eq!(
        built_in
            .float_too_high()
            .create(3.0, 2.0)
            .get_raw_message()
            .get_string(),
        "Float must not be more than 2.0, found 3.0"
    );
    assert_eq!(
        built_in
            .integer_too_low()
            .create(1, 2)
            .get_raw_message()
            .get_string(),
        "Integer must not be less than 2, found 1"
    );
    assert_eq!(
        built_in
            .integer_too_high()
            .create(3, 2)
            .get_raw_message()
            .get_string(),
        "Integer must not be more than 2, found 3"
    );
    assert_eq!(
        built_in
            .long_too_low()
            .create(1, 2)
            .get_raw_message()
            .get_string(),
        "Long must not be less than 2, found 1"
    );
    assert_eq!(
        built_in
            .long_too_high()
            .create(3, 2)
            .get_raw_message()
            .get_string(),
        "Long must not be more than 2, found 3"
    );
}

#[test]
fn numeric_range_messages_use_java_number_formatting() {
    // Rust Display would print "1" and expand 1e20; Java writes 1.0/1.0E20.
    // These pin the Double.toString/Float.toString shape inside the messages.
    let built_in = built_in_exceptions();
    assert_eq!(
        built_in
            .double_too_low()
            .create(1e20, 1.0)
            .get_raw_message()
            .get_string(),
        "Double must not be less than 1.0, found 1.0E20"
    );
    assert_eq!(
        built_in
            .double_too_low()
            .create(0.5, 0.25)
            .get_raw_message()
            .get_string(),
        "Double must not be less than 0.25, found 0.5"
    );
    assert_eq!(
        built_in
            .float_too_low()
            .create(0.5, 0.25)
            .get_raw_message()
            .get_string(),
        "Float must not be less than 0.25, found 0.5"
    );
    assert_eq!(
        built_in
            .float_too_high()
            .create(3.0, 2.5e-3)
            .get_raw_message()
            .get_string(),
        "Float must not be more than 0.0025, found 3.0"
    );
}

#[test]
fn reader_messages_match_java_byte_for_byte() {
    assert_eq!(
        text(|b| b.literal_incorrect(), |t| t.create("foo".to_owned())),
        "Expected literal foo"
    );
    assert_eq!(
        text(|b| b.reader_expected_start_of_quote(), |t| t.create()),
        "Expected quote to start a string"
    );
    assert_eq!(
        text(|b| b.reader_expected_end_of_quote(), |t| t.create()),
        "Unclosed quoted string"
    );
    assert_eq!(
        text(|b| b.reader_invalid_escape(), |t| t.create('\\')),
        "Invalid escape sequence '\\' in quoted string"
    );
    assert_eq!(
        text(
            |b| b.reader_invalid_bool(),
            |t| t.create("maybe".to_owned())
        ),
        "Invalid bool, expected true or false but found 'maybe'"
    );
    assert_eq!(
        text(|b| b.reader_invalid_int(), |t| t.create("12.34".to_owned())),
        "Invalid integer '12.34'"
    );
    assert_eq!(
        text(|b| b.reader_expected_int(), |t| t.create()),
        "Expected integer"
    );
    assert_eq!(
        text(
            |b| b.reader_invalid_long(),
            |t| t.create("12.34".to_owned())
        ),
        "Invalid long '12.34'"
    );
    assert_eq!(
        text(|b| b.reader_expected_long(), |t| t.create()),
        "Expected long"
    );
    assert_eq!(
        text(
            |b| b.reader_invalid_double(),
            |t| t.create("foo".to_owned())
        ),
        "Invalid double 'foo'"
    );
    assert_eq!(
        text(|b| b.reader_expected_double(), |t| t.create()),
        "Expected double"
    );
    assert_eq!(
        text(|b| b.reader_invalid_float(), |t| t.create("foo".to_owned())),
        "Invalid float 'foo'"
    );
    assert_eq!(
        text(|b| b.reader_expected_float(), |t| t.create()),
        "Expected float"
    );
    assert_eq!(
        text(|b| b.reader_expected_bool(), |t| t.create()),
        "Expected bool"
    );
    assert_eq!(
        text(|b| b.reader_expected_symbol(), |t| t.create('a')),
        "Expected 'a'"
    );
}

#[test]
fn dispatcher_messages_match_java_byte_for_byte() {
    assert_eq!(
        text(|b| b.dispatcher_unknown_command(), |t| t.create()),
        "Unknown command"
    );
    assert_eq!(
        text(|b| b.dispatcher_unknown_argument(), |t| t.create()),
        "Incorrect argument for command"
    );
    assert_eq!(
        text(
            |b| b.dispatcher_expected_argument_separator(),
            |t| t.create()
        ),
        "Expected whitespace to end one argument, but found trailing data"
    );
    assert_eq!(
        text(
            |b| b.dispatcher_parse_exception(),
            |t| t.create("blah".to_owned())
        ),
        "Could not parse command: blah"
    );
}

#[test]
fn provider_instances_are_shared_singletons() {
    // same static instance on every call, so exceptions raised by the same
    // provider method always carry the same type identity — Java's
    // static-singleton semantics
    let first = built_in_exceptions().reader_expected_int().create();
    let second = built_in_exceptions().reader_expected_int().create();
    assert!(first.is_type(second.get_type()));

    let again = built_in_exceptions().reader_expected_int();
    assert!(Arc::ptr_eq(
        built_in_exceptions().reader_expected_int().as_type(),
        again.as_type()
    ));

    // ...while distinct provider methods are distinct types
    assert!(!first.is_type(built_in_exceptions().reader_expected_long().as_type()));
}

#[test]
fn context_amount_constant_matches_java() {
    assert_eq!(
        brigaxsir::errors::CommandSyntaxException::CONTEXT_AMOUNT,
        10
    );
}
