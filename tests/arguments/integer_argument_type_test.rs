//! Port of Java's `IntegerArgumentTypeTest` (1:1 in meaning; the
//! `EqualsTester` equality groups become cross-group `PartialEq` asserts,
//! and the raw-message asserts strengthen the Java ones to also pin the
//! exact bound-error wording).

use brigaxsir::arguments::{
    ArgumentType,
    integer_argument_type::{IntegerArgumentType, integer, integer_min, integer_range},
};
use brigaxsir::errors::{BuiltInExceptionProvider, built_in_exceptions};
use brigaxsir::string_reader::StringReader;

#[test]
fn parse() {
    let mut reader = StringReader::from("15");
    let result = integer().parse(&mut reader).unwrap();
    assert_eq!(result.as_any().downcast_ref::<i32>(), Some(&15));
    assert!(!reader.can_read());
}

#[test]
fn parse_too_small() {
    let mut reader = StringReader::from("-5");
    let exception = integer_range(0, 100).parse(&mut reader).unwrap_err();
    assert!(
        exception.is_type(built_in_exceptions().integer_too_low().as_type()),
        "expected integerTooLow, got {:?}",
        exception
    );
    assert_eq!(exception.get_cursor(), Some(0));
    assert_eq!(
        exception.get_raw_message().get_string(),
        "Integer must not be less than 0, found -5"
    );
}

#[test]
fn parse_too_big() {
    let mut reader = StringReader::from("5");
    let exception = integer_range(-100, 0).parse(&mut reader).unwrap_err();
    assert!(
        exception.is_type(built_in_exceptions().integer_too_high().as_type()),
        "expected integerTooHigh, got {:?}",
        exception
    );
    assert_eq!(exception.get_cursor(), Some(0));
    assert_eq!(
        exception.get_raw_message().get_string(),
        "Integer must not be more than 0, found 5"
    );
}

#[test]
fn equals() {
    // EqualsTester: within a group everything equals everything, across
    // groups nothing equals anything
    let groups: Vec<Vec<IntegerArgumentType>> = vec![
        vec![integer(), integer()],
        vec![integer_range(-100, 100), integer_range(-100, 100)],
        vec![integer_range(-100, 50), integer_range(-100, 50)],
        vec![integer_range(-50, 100), integer_range(-50, 100)],
    ];
    for group in &groups {
        for a in group {
            for b in group {
                assert_eq!(a, b);
            }
        }
    }
    for (i, group_a) in groups.iter().enumerate() {
        for (j, group_b) in groups.iter().enumerate() {
            if i != j {
                assert_ne!(group_a[0], group_b[0]);
            }
        }
    }
}

#[test]
fn to_string() {
    assert_eq!(integer().to_string(), "integer()");
    assert_eq!(integer_min(-100).to_string(), "integer(-100)");
    assert_eq!(integer_range(-100, 100).to_string(), "integer(-100, 100)");
    assert_eq!(
        integer_range(i32::MIN, 100).to_string(),
        "integer(-2147483648, 100)"
    );
}

#[test]
fn getters() {
    let type_ = integer_range(-100, 100);
    assert_eq!(type_.get_minimum(), -100);
    assert_eq!(type_.get_maximum(), 100);
    // unbounded is Java's MIN/MAX sentinel pair
    assert_eq!(integer().get_minimum(), i32::MIN);
    assert_eq!(integer().get_maximum(), i32::MAX);
    assert_eq!(integer_min(-100).get_maximum(), i32::MAX);
}

#[test]
fn examples_match_java() {
    assert_eq!(
        integer().examples(),
        vec!["0".to_owned(), "123".to_owned(), "-123".to_owned()]
    );
}
