//! Port of Java's `LongArgumentTypeTest` (1:1 in meaning; the
//! `EqualsTester` equality groups become cross-group `PartialEq` asserts,
//! and the raw-message asserts strengthen the Java ones to also pin the
//! exact bound-error wording).

use brigaxsir::arguments::{
    ArgumentType,
    long_argument_type::{LongArgumentType, long, long_min, long_range},
};
use brigaxsir::errors::{BuiltInExceptionProvider, built_in_exceptions};
use brigaxsir::string_reader::StringReader;

#[test]
fn parse() {
    let mut reader = StringReader::from("15");
    let result = long().parse(&mut reader).unwrap();
    assert_eq!(result.as_any().downcast_ref::<i64>(), Some(&15));
    assert!(!reader.can_read());
}

#[test]
fn parse_too_small() {
    let mut reader = StringReader::from("-5");
    let exception = long_range(0, 100).parse(&mut reader).unwrap_err();
    assert!(
        exception.is_type(built_in_exceptions().long_too_low().as_type()),
        "expected longTooLow, got {:?}",
        exception
    );
    assert_eq!(exception.get_cursor(), Some(0));
    assert_eq!(
        exception.get_raw_message().get_string(),
        "Long must not be less than 0, found -5"
    );
}

#[test]
fn parse_too_big() {
    let mut reader = StringReader::from("5");
    let exception = long_range(-100, 0).parse(&mut reader).unwrap_err();
    assert!(
        exception.is_type(built_in_exceptions().long_too_high().as_type()),
        "expected longTooHigh, got {:?}",
        exception
    );
    assert_eq!(exception.get_cursor(), Some(0));
    assert_eq!(
        exception.get_raw_message().get_string(),
        "Long must not be more than 0, found 5"
    );
}

#[test]
fn equals() {
    // EqualsTester: within a group everything equals everything, across
    // groups nothing equals anything
    let groups: Vec<Vec<LongArgumentType>> = vec![
        vec![long(), long()],
        vec![long_range(-100, 100), long_range(-100, 100)],
        vec![long_range(-100, 50), long_range(-100, 50)],
        vec![long_range(-50, 100), long_range(-50, 100)],
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
    // Java spells the toString prefix `longArg` even though this port names
    // the factories long/long_min/long_range
    assert_eq!(long().to_string(), "longArg()");
    assert_eq!(long_min(-100).to_string(), "longArg(-100)");
    assert_eq!(long_range(-100, 100).to_string(), "longArg(-100, 100)");
    assert_eq!(
        long_range(i64::MIN, 100).to_string(),
        "longArg(-9223372036854775808, 100)"
    );
}

#[test]
fn getters() {
    let type_ = long_range(-100, 1_000_000_000_000);
    assert_eq!(type_.get_minimum(), -100);
    assert_eq!(type_.get_maximum(), 1_000_000_000_000);
    // unbounded is Java's MIN/MAX sentinel pair
    assert_eq!(long().get_minimum(), i64::MIN);
    assert_eq!(long().get_maximum(), i64::MAX);
    assert_eq!(long_min(-100).get_maximum(), i64::MAX);
}

#[test]
fn examples_match_java() {
    assert_eq!(
        long().examples(),
        vec!["0".to_owned(), "123".to_owned(), "-123".to_owned()]
    );
}
