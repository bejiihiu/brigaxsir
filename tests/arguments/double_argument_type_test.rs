//! Port of Java's `DoubleArgumentTypeTest` (1:1 in meaning; the
//! `EqualsTester` equality groups become cross-group `PartialEq` asserts,
//! and the raw-message asserts strengthen the Java ones to also pin the
//! exact bound-error wording, rendered Java's `Double.toString` way).

use brigaxsir::arguments::{
    ArgumentType,
    double_argument_type::{DoubleArgumentType, double, double_min, double_range},
};
use brigaxsir::errors::{BuiltInExceptionProvider, built_in_exceptions};
use brigaxsir::string_reader::StringReader;

#[test]
fn parse() {
    let mut reader = StringReader::from("15");
    let result = double().parse(&mut reader).unwrap();
    assert_eq!(result.as_any().downcast_ref::<f64>(), Some(&15.0));
    assert!(!reader.can_read());
}

#[test]
fn parse_too_small() {
    let mut reader = StringReader::from("-5");
    let exception = double_range(0.0, 100.0).parse(&mut reader).unwrap_err();
    assert!(
        exception.is_type(built_in_exceptions().double_too_low().as_type()),
        "expected doubleTooLow, got {:?}",
        exception
    );
    assert_eq!(exception.get_cursor(), Some(0));
    // the message embeds Java's Double.toString of both numbers
    assert_eq!(
        exception.get_raw_message().get_string(),
        "Double must not be less than 0.0, found -5.0"
    );
}

#[test]
fn parse_too_big() {
    let mut reader = StringReader::from("5");
    let exception = double_range(-100.0, 0.0).parse(&mut reader).unwrap_err();
    assert!(
        exception.is_type(built_in_exceptions().double_too_high().as_type()),
        "expected doubleTooHigh, got {:?}",
        exception
    );
    assert_eq!(exception.get_cursor(), Some(0));
    assert_eq!(
        exception.get_raw_message().get_string(),
        "Double must not be more than 0.0, found 5.0"
    );
}

#[test]
fn equals() {
    // EqualsTester: within a group everything equals everything, across
    // groups nothing equals anything
    let groups: Vec<Vec<DoubleArgumentType>> = vec![
        vec![double(), double()],
        vec![double_range(-100.0, 100.0), double_range(-100.0, 100.0)],
        vec![double_range(-100.0, 50.0), double_range(-100.0, 50.0)],
        vec![double_range(-50.0, 100.0), double_range(-50.0, 100.0)],
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
fn equals_matches_java_double_comparison() {
    // Java's `==` on the double fields: -0.0 == 0.0, NaN never equals
    // anything (Java equals is also false for NaN on both sides)
    assert_eq!(double_range(0.0, 1.0), double_range(-0.0, 1.0));
    assert_ne!(double_range(f64::NAN, 1.0), double_range(f64::NAN, 1.0));
}

#[test]
fn to_string() {
    assert_eq!(double().to_string(), "double()");
    assert_eq!(double_min(-100.0).to_string(), "double(-100.0)");
    assert_eq!(
        double_range(-100.0, 100.0).to_string(),
        "double(-100.0, 100.0)"
    );
    // Java's Double.toString(-2147483648.0) — oracle-verified on JDK 21
    assert_eq!(
        double_range(i32::MIN as f64, 100.0).to_string(),
        "double(-2.147483648E9, 100.0)"
    );
}

#[test]
fn getters() {
    let type_ = double_range(-100.0, 100.0);
    assert_eq!(type_.get_minimum(), -100.0);
    assert_eq!(type_.get_maximum(), 100.0);
    // unbounded lower bound is Java's -Double.MAX_VALUE, NOT -Infinity
    assert_eq!(double().get_minimum(), -f64::MAX);
    assert_eq!(double().get_maximum(), f64::MAX);
    assert_eq!(double_min(-100.0).get_maximum(), f64::MAX);
}

#[test]
fn examples_match_java() {
    assert_eq!(
        double().examples(),
        vec![
            "0".to_owned(),
            "1.2".to_owned(),
            ".5".to_owned(),
            "-1".to_owned(),
            "-.5".to_owned(),
            "-1234.56".to_owned(),
        ]
    );
}
