//! Port of Java's `BoolArgumentTypeTest` (1:1 in meaning; Java mocks
//! `StringReader.readBoolean`, the real reader runs here instead, so the
//! parse behavior is exercised end to end).

use brigaxsir::arguments::{
    ArgumentType,
    bool_argument_type::{BoolArgumentType, bool},
};
use brigaxsir::string_reader::StringReader;

fn parse_bool(type_: &BoolArgumentType, input: &str) -> bool {
    let mut reader = StringReader::from(input);
    let value = type_.parse(&mut reader).expect("parse should succeed");
    *value
        .as_any()
        .downcast_ref::<bool>()
        .expect("bool argument value")
}

#[test]
fn parse() {
    assert!(parse_bool(&bool(), "true"));
}

#[test]
fn parse_false() {
    // same reader path as Java's readBoolean mock, just the other literal
    assert!(!parse_bool(&bool(), "false"));
}

#[test]
fn examples_match_java() {
    assert_eq!(
        bool().examples(),
        vec!["true".to_owned(), "false".to_owned()]
    );
}

#[test]
fn parse_with_source_uses_the_default_overload() {
    // Java's contextual default ignores the source and calls parse(reader)
    let mut reader = StringReader::from("true");
    let value = bool().parse_with_source(&mut reader, &42i32).unwrap();
    assert_eq!(value.as_any().downcast_ref::<bool>(), Some(&true));
}
