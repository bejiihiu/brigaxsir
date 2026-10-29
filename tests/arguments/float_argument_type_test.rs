//! Port of Java's `FloatArgumentTypeTest` (1:1 in meaning; the
//! `EqualsTester` equality groups become cross-group `PartialEq` asserts,
//! and the raw-message asserts strengthen the Java ones to also pin the
//! exact bound-error wording, rendered Java's `Float.toString` way).

use brigaxsir::arguments::{
    ArgumentType,
    float_argument_type::{FloatArgumentType, float, float_min, float_range},
};
use brigaxsir::errors::{BuiltInExceptionProvider, built_in_exceptions};
use brigaxsir::string_reader::StringReader;

#[test]
fn parse() {
    let mut reader = StringReader::from("15");
    let result = float().parse(&mut reader).unwrap();
    assert_eq!(result.as_any().downcast_ref::<f32>(), Some(&15.0f32));
    assert!(!reader.can_read());
}

#[test]
fn parse_too_small() {
    let mut reader = StringReader::from("-5");
    let exception = float_range(0.0, 100.0).parse(&mut reader).unwrap_err();
    assert!(
        exception.is_type(built_in_exceptions().float_too_low().as_type()),
        "expected floatTooLow, got {:?}",
        exception
    );
    assert_eq!(exception.get_cursor(), Some(0));
    // the message embeds Java's Float.toString of both numbers
    assert_eq!(
        exception.get_raw_message().get_string(),
        "Float must not be less than 0.0, found -5.0"
    );
}

#[test]
fn parse_too_big() {
    let mut reader = StringReader::from("5");
    let exception = float_range(-100.0, 0.0).parse(&mut reader).unwrap_err();
    assert!(
        exception.is_type(built_in_exceptions().float_too_high().as_type()),
        "expected floatTooHigh, got {:?}",
        exception
    );
    assert_eq!(exception.get_cursor(), Some(0));
    assert_eq!(
        exception.get_raw_message().get_string(),
        "Float must not be more than 0.0, found 5.0"
    );
}

#[test]
fn equals() {
    // EqualsTester: within a group everything equals everything, across
    // groups nothing equals anything
    let groups: Vec<Vec<FloatArgumentType>> = vec![
        vec![float(), float()],
        vec![float_range(-100.0, 100.0), float_range(-100.0, 100.0)],
        vec![float_range(-100.0, 50.0), float_range(-100.0, 50.0)],
        vec![float_range(-50.0, 100.0), float_range(-50.0, 100.0)],
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
    assert_eq!(float().to_string(), "float()");
    assert_eq!(float_min(-100.0).to_string(), "float(-100.0)");
    assert_eq!(
        float_range(-100.0, 100.0).to_string(),
        "float(-100.0, 100.0)"
    );
    // Upstream's test asserts `float(-2.14748365E9, 100.0)` here, but that
    // string predates JDK 19's shortest-representation Float.toString: on the
    // project's reference JDK 21 (oracle run) Float.toString(-2147483648f) is
    // `-2.1474836E9`, and the port follows the live JDK — same decision the
    // error-model task made for the built-in messages.
    assert_eq!(
        float_range(i32::MIN as f32, 100.0).to_string(),
        "float(-2.1474836E9, 100.0)"
    );
}

#[test]
fn getters() {
    let type_ = float_range(-100.0, 100.0);
    assert_eq!(type_.get_minimum(), -100.0);
    assert_eq!(type_.get_maximum(), 100.0);
    // unbounded lower bound is Java's -Float.MAX_VALUE, NOT -Infinity
    assert_eq!(float().get_minimum(), -f32::MAX);
    assert_eq!(float().get_maximum(), f32::MAX);
    assert_eq!(float_min(-100.0).get_maximum(), f32::MAX);
}

#[test]
fn examples_match_java() {
    assert_eq!(
        float().examples(),
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
