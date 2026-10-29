//! Port of Java's `StringArgumentTypeTest` (1:1 in meaning; Java mocks
//! `StringReader.readUnquotedString`/`readString`, the real reader runs here
//! instead, so the parse behavior is exercised end to end).

use std::sync::Arc;

use brigaxsir::arguments::{
    ArgumentType,
    string_argument_type::{
        StringArgumentType, StringType, escape_if_required, greedy_string, string, word,
    },
};
use brigaxsir::string_reader::StringReader;

fn parse_string(type_: &StringArgumentType, input: &str) -> String {
    let mut reader = StringReader::from(input);
    let value = type_.parse(&mut reader).expect("parse should succeed");
    value
        .as_any()
        .downcast_ref::<String>()
        .expect("string argument value")
        .clone()
}

#[test]
fn test_parse_word() {
    // Java mocks readUnquotedString -> "hello"; a plain unquoted word here
    assert_eq!(parse_string(&word(), "hello"), "hello");
}

#[test]
fn test_parse_string() {
    // Java mocks readString -> "hello world"; the real reader needs the
    // quoted form to produce a value with spaces
    assert_eq!(parse_string(&string(), "\"hello world\""), "hello world");
}

#[test]
fn test_parse_greedy_string() {
    let mut reader = StringReader::from("Hello world! This is a test.");
    let result = greedy_string().parse(&mut reader).unwrap();
    assert_eq!(
        result.as_any().downcast_ref::<String>(),
        Some(&"Hello world! This is a test.".to_owned())
    );
    assert!(!reader.can_read());
}

#[test]
fn test_to_string() {
    assert_eq!(string().to_string(), "string()");
}

#[test]
fn test_escape_if_required_not_required() {
    assert_eq!(escape_if_required("hello"), "hello");
    assert_eq!(escape_if_required(""), "");
}

#[test]
fn test_escape_if_required_multiple_words() {
    assert_eq!(escape_if_required("hello world"), "\"hello world\"");
}

#[test]
fn test_escape_if_required_quote() {
    assert_eq!(
        escape_if_required("hello \"world\"!"),
        "\"hello \\\"world\\\"!\""
    );
}

#[test]
fn test_escape_if_required_escapes() {
    assert_eq!(escape_if_required("\\"), "\"\\\\\"");
}

#[test]
fn test_escape_if_required_single_quote() {
    assert_eq!(escape_if_required("\""), "\"\\\"\"");
}

#[test]
fn get_type_matches_the_factory() {
    // Java: getType() returns the StringType the factory picked
    assert_eq!(word().get_type(), StringType::SingleWord);
    assert_eq!(string().get_type(), StringType::QuotablePhrase);
    assert_eq!(greedy_string().get_type(), StringType::GreedyPhrase);
}

#[test]
fn string_type_examples_match_java() {
    assert_eq!(
        StringType::SingleWord.get_examples(),
        vec!["word".to_owned(), "words_with_underscores".to_owned()]
    );
    assert_eq!(
        StringType::QuotablePhrase.get_examples(),
        vec![
            "\"quoted phrase\"".to_owned(),
            "word".to_owned(),
            "\"\"".to_owned(),
        ]
    );
    assert_eq!(
        StringType::GreedyPhrase.get_examples(),
        vec![
            "word".to_owned(),
            "words with spaces".to_owned(),
            "\"and symbols\"".to_owned(),
        ]
    );
}

#[test]
fn examples_come_from_the_string_type() {
    // Java: getExamples() delegates to type.getExamples()
    assert_eq!(word().examples(), StringType::SingleWord.get_examples());
    assert_eq!(
        string().examples(),
        StringType::QuotablePhrase.get_examples()
    );
    assert_eq!(
        greedy_string().examples(),
        StringType::GreedyPhrase.get_examples()
    );
}

#[test]
fn parse_with_source_uses_the_default_overload() {
    // Java's contextual default ignores the source and calls parse(reader)
    let mut reader = StringReader::from("hello");
    let value = word().parse_with_source(&mut reader, &42i32).unwrap();
    assert_eq!(
        value.as_any().downcast_ref::<String>(),
        Some(&"hello".to_owned())
    );
}

#[test]
fn parse_returns_an_owned_string_value() {
    // sanity: the type-erased value really is a String downcast
    let value: Arc<dyn brigaxsir::context::ParsedValue> = {
        let mut reader = StringReader::from("hello world");
        string().parse(&mut reader).unwrap()
    };
    assert!(value.as_any().is::<String>());
}
