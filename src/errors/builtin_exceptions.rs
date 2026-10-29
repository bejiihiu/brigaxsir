//! The built-in exception provider. Java: `BuiltInExceptionProvider` +
//! `BuiltInExceptions`. Every message string here is byte-identical to the
//! Java source, including how Java renders embedded `double`/`float` values.

use std::{
    fmt::Write as _,
    sync::{Arc, LazyLock},
};

use super::{
    dynamic_command_exception_type::DynamicCommandExceptionType,
    dynamic2_command_exception_type::Dynamic2CommandExceptionType,
    simple_command_exception_type::SimpleCommandExceptionType,
};
use crate::message::{LiteralMessage, Message};

/// Access to the library's own exception types. Java:
/// `BuiltInExceptionProvider` (interface). The methods return the shared
/// instances behind [`built_in_exceptions`], so the type identity of every
/// exception built from them is stable across calls — Java's static-singleton
/// semantics.
pub trait BuiltInExceptionProvider {
    /// Java: `doubleTooLow()`.
    fn double_too_low(&self) -> &Dynamic2CommandExceptionType<f64, f64>;
    /// Java: `doubleTooHigh()`.
    fn double_too_high(&self) -> &Dynamic2CommandExceptionType<f64, f64>;
    /// Java: `floatTooLow()`.
    fn float_too_low(&self) -> &Dynamic2CommandExceptionType<f32, f32>;
    /// Java: `floatTooHigh()`.
    fn float_too_high(&self) -> &Dynamic2CommandExceptionType<f32, f32>;
    /// Java: `integerTooLow()`.
    fn integer_too_low(&self) -> &Dynamic2CommandExceptionType<i32, i32>;
    /// Java: `integerTooHigh()`.
    fn integer_too_high(&self) -> &Dynamic2CommandExceptionType<i32, i32>;
    /// Java: `longTooLow()`.
    fn long_too_low(&self) -> &Dynamic2CommandExceptionType<i64, i64>;
    /// Java: `longTooHigh()`.
    fn long_too_high(&self) -> &Dynamic2CommandExceptionType<i64, i64>;
    /// Java: `literalIncorrect()`.
    fn literal_incorrect(&self) -> &DynamicCommandExceptionType<String>;
    /// Java: `readerExpectedStartOfQuote()`.
    fn reader_expected_start_of_quote(&self) -> &SimpleCommandExceptionType;
    /// Java: `readerExpectedEndOfQuote()`.
    fn reader_expected_end_of_quote(&self) -> &SimpleCommandExceptionType;
    /// Java: `readerInvalidEscape()`.
    fn reader_invalid_escape(&self) -> &DynamicCommandExceptionType<char>;
    /// Java: `readerInvalidBool()`.
    fn reader_invalid_bool(&self) -> &DynamicCommandExceptionType<String>;
    /// Java: `readerInvalidInt()`.
    fn reader_invalid_int(&self) -> &DynamicCommandExceptionType<String>;
    /// Java: `readerExpectedInt()`.
    fn reader_expected_int(&self) -> &SimpleCommandExceptionType;
    /// Java: `readerInvalidLong()`.
    fn reader_invalid_long(&self) -> &DynamicCommandExceptionType<String>;
    /// Java: `readerExpectedLong()`.
    fn reader_expected_long(&self) -> &SimpleCommandExceptionType;
    /// Java: `readerInvalidDouble()`.
    fn reader_invalid_double(&self) -> &DynamicCommandExceptionType<String>;
    /// Java: `readerExpectedDouble()`.
    fn reader_expected_double(&self) -> &SimpleCommandExceptionType;
    /// Java: `readerInvalidFloat()`.
    fn reader_invalid_float(&self) -> &DynamicCommandExceptionType<String>;
    /// Java: `readerExpectedFloat()`.
    fn reader_expected_float(&self) -> &SimpleCommandExceptionType;
    /// Java: `readerExpectedBool()`.
    fn reader_expected_bool(&self) -> &SimpleCommandExceptionType;
    /// Java: `readerExpectedSymbol()`.
    fn reader_expected_symbol(&self) -> &DynamicCommandExceptionType<char>;
    /// Java: `dispatcherUnknownCommand()`.
    fn dispatcher_unknown_command(&self) -> &SimpleCommandExceptionType;
    /// Java: `dispatcherUnknownArgument()`.
    fn dispatcher_unknown_argument(&self) -> &SimpleCommandExceptionType;
    /// Java: `dispatcherExpectedArgumentSeparator()`.
    fn dispatcher_expected_argument_separator(&self) -> &SimpleCommandExceptionType;
    /// Java: `dispatcherParseException()`. The library itself never wraps
    /// argument-type errors (Java wraps `RuntimeException`, which has no
    /// Rust analogue — argument errors travel as `Result`), so this type is
    /// provided for API parity and user code.
    fn dispatcher_parse_exception(&self) -> &DynamicCommandExceptionType<String>;
}

/// The library's built-in exception types, one instance per
/// [`BuiltInExceptionProvider`] method, holding the Java message strings.
pub struct BuiltInExceptions {
    double_too_low: Dynamic2CommandExceptionType<f64, f64>,
    double_too_high: Dynamic2CommandExceptionType<f64, f64>,
    float_too_low: Dynamic2CommandExceptionType<f32, f32>,
    float_too_high: Dynamic2CommandExceptionType<f32, f32>,
    integer_too_low: Dynamic2CommandExceptionType<i32, i32>,
    integer_too_high: Dynamic2CommandExceptionType<i32, i32>,
    long_too_low: Dynamic2CommandExceptionType<i64, i64>,
    long_too_high: Dynamic2CommandExceptionType<i64, i64>,
    literal_incorrect: DynamicCommandExceptionType<String>,
    reader_expected_start_of_quote: SimpleCommandExceptionType,
    reader_expected_end_of_quote: SimpleCommandExceptionType,
    reader_invalid_escape: DynamicCommandExceptionType<char>,
    reader_invalid_bool: DynamicCommandExceptionType<String>,
    reader_invalid_int: DynamicCommandExceptionType<String>,
    reader_expected_int: SimpleCommandExceptionType,
    reader_invalid_long: DynamicCommandExceptionType<String>,
    reader_expected_long: SimpleCommandExceptionType,
    reader_invalid_double: DynamicCommandExceptionType<String>,
    reader_expected_double: SimpleCommandExceptionType,
    reader_invalid_float: DynamicCommandExceptionType<String>,
    reader_expected_float: SimpleCommandExceptionType,
    reader_expected_bool: SimpleCommandExceptionType,
    reader_expected_symbol: DynamicCommandExceptionType<char>,
    dispatcher_unknown_command: SimpleCommandExceptionType,
    dispatcher_unknown_argument: SimpleCommandExceptionType,
    dispatcher_expected_argument_separator: SimpleCommandExceptionType,
    dispatcher_parse_exception: DynamicCommandExceptionType<String>,
}

impl BuiltInExceptions {
    // Not public on purpose: hand-rolled instances would be distinct type
    // objects from the shared ones, which only breeds identity confusion.
    // The provider exists as the global behind built_in_exceptions().
    fn new() -> Self {
        Self {
            double_too_low: Dynamic2CommandExceptionType::new(|found, min| {
                literal(format!(
                    "Double must not be less than {}, found {}",
                    java_double_to_string(min),
                    java_double_to_string(found)
                ))
            }),
            double_too_high: Dynamic2CommandExceptionType::new(|found, max| {
                literal(format!(
                    "Double must not be more than {}, found {}",
                    java_double_to_string(max),
                    java_double_to_string(found)
                ))
            }),
            float_too_low: Dynamic2CommandExceptionType::new(|found, min| {
                literal(format!(
                    "Float must not be less than {}, found {}",
                    java_float_to_string(min),
                    java_float_to_string(found)
                ))
            }),
            float_too_high: Dynamic2CommandExceptionType::new(|found, max| {
                literal(format!(
                    "Float must not be more than {}, found {}",
                    java_float_to_string(max),
                    java_float_to_string(found)
                ))
            }),
            integer_too_low: Dynamic2CommandExceptionType::new(|found, min| {
                literal(format!(
                    "Integer must not be less than {min}, found {found}"
                ))
            }),
            integer_too_high: Dynamic2CommandExceptionType::new(|found, max| {
                literal(format!(
                    "Integer must not be more than {max}, found {found}"
                ))
            }),
            long_too_low: Dynamic2CommandExceptionType::new(|found, min| {
                literal(format!("Long must not be less than {min}, found {found}"))
            }),
            long_too_high: Dynamic2CommandExceptionType::new(|found, max| {
                literal(format!("Long must not be more than {max}, found {found}"))
            }),
            literal_incorrect: DynamicCommandExceptionType::new(|expected| {
                literal(format!("Expected literal {expected}"))
            }),
            reader_expected_start_of_quote: SimpleCommandExceptionType::new(literal(
                "Expected quote to start a string".to_owned(),
            )),
            reader_expected_end_of_quote: SimpleCommandExceptionType::new(literal(
                "Unclosed quoted string".to_owned(),
            )),
            reader_invalid_escape: DynamicCommandExceptionType::new(|character| {
                literal(format!(
                    "Invalid escape sequence '{character}' in quoted string"
                ))
            }),
            reader_invalid_bool: DynamicCommandExceptionType::new(|value| {
                literal(format!(
                    "Invalid bool, expected true or false but found '{value}'"
                ))
            }),
            reader_invalid_int: DynamicCommandExceptionType::new(|value| {
                literal(format!("Invalid integer '{value}'"))
            }),
            reader_expected_int: SimpleCommandExceptionType::new(literal(
                "Expected integer".to_owned(),
            )),
            reader_invalid_long: DynamicCommandExceptionType::new(|value| {
                literal(format!("Invalid long '{value}'"))
            }),
            reader_expected_long: SimpleCommandExceptionType::new(literal(
                "Expected long".to_owned(),
            )),
            reader_invalid_double: DynamicCommandExceptionType::new(|value| {
                literal(format!("Invalid double '{value}'"))
            }),
            reader_expected_double: SimpleCommandExceptionType::new(literal(
                "Expected double".to_owned(),
            )),
            reader_invalid_float: DynamicCommandExceptionType::new(|value| {
                literal(format!("Invalid float '{value}'"))
            }),
            reader_expected_float: SimpleCommandExceptionType::new(literal(
                "Expected float".to_owned(),
            )),
            reader_expected_bool: SimpleCommandExceptionType::new(literal(
                "Expected bool".to_owned(),
            )),
            reader_expected_symbol: DynamicCommandExceptionType::new(|symbol| {
                literal(format!("Expected '{symbol}'"))
            }),
            dispatcher_unknown_command: SimpleCommandExceptionType::new(literal(
                "Unknown command".to_owned(),
            )),
            dispatcher_unknown_argument: SimpleCommandExceptionType::new(literal(
                "Incorrect argument for command".to_owned(),
            )),
            dispatcher_expected_argument_separator: SimpleCommandExceptionType::new(literal(
                "Expected whitespace to end one argument, but found trailing data".to_owned(),
            )),
            dispatcher_parse_exception: DynamicCommandExceptionType::new(|message| {
                literal(format!("Could not parse command: {message}"))
            }),
        }
    }
}

impl BuiltInExceptionProvider for BuiltInExceptions {
    fn double_too_low(&self) -> &Dynamic2CommandExceptionType<f64, f64> {
        &self.double_too_low
    }
    fn double_too_high(&self) -> &Dynamic2CommandExceptionType<f64, f64> {
        &self.double_too_high
    }
    fn float_too_low(&self) -> &Dynamic2CommandExceptionType<f32, f32> {
        &self.float_too_low
    }
    fn float_too_high(&self) -> &Dynamic2CommandExceptionType<f32, f32> {
        &self.float_too_high
    }
    fn integer_too_low(&self) -> &Dynamic2CommandExceptionType<i32, i32> {
        &self.integer_too_low
    }
    fn integer_too_high(&self) -> &Dynamic2CommandExceptionType<i32, i32> {
        &self.integer_too_high
    }
    fn long_too_low(&self) -> &Dynamic2CommandExceptionType<i64, i64> {
        &self.long_too_low
    }
    fn long_too_high(&self) -> &Dynamic2CommandExceptionType<i64, i64> {
        &self.long_too_high
    }
    fn literal_incorrect(&self) -> &DynamicCommandExceptionType<String> {
        &self.literal_incorrect
    }
    fn reader_expected_start_of_quote(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_start_of_quote
    }
    fn reader_expected_end_of_quote(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_end_of_quote
    }
    fn reader_invalid_escape(&self) -> &DynamicCommandExceptionType<char> {
        &self.reader_invalid_escape
    }
    fn reader_invalid_bool(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_bool
    }
    fn reader_invalid_int(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_int
    }
    fn reader_expected_int(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_int
    }
    fn reader_invalid_long(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_long
    }
    fn reader_expected_long(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_long
    }
    fn reader_invalid_double(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_double
    }
    fn reader_expected_double(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_double
    }
    fn reader_invalid_float(&self) -> &DynamicCommandExceptionType<String> {
        &self.reader_invalid_float
    }
    fn reader_expected_float(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_float
    }
    fn reader_expected_bool(&self) -> &SimpleCommandExceptionType {
        &self.reader_expected_bool
    }
    fn reader_expected_symbol(&self) -> &DynamicCommandExceptionType<char> {
        &self.reader_expected_symbol
    }
    fn dispatcher_unknown_command(&self) -> &SimpleCommandExceptionType {
        &self.dispatcher_unknown_command
    }
    fn dispatcher_unknown_argument(&self) -> &SimpleCommandExceptionType {
        &self.dispatcher_unknown_argument
    }
    fn dispatcher_expected_argument_separator(&self) -> &SimpleCommandExceptionType {
        &self.dispatcher_expected_argument_separator
    }
    fn dispatcher_parse_exception(&self) -> &DynamicCommandExceptionType<String> {
        &self.dispatcher_parse_exception
    }
}

static BUILT_IN_EXCEPTIONS: LazyLock<BuiltInExceptions> = LazyLock::new(BuiltInExceptions::new);

/// The global built-in exception provider, the one the library's reader,
/// tree, dispatcher and argument types raise through. Java:
/// `CommandSyntaxException.BUILT_IN_EXCEPTIONS`.
///
/// Divergence from Java: that field is a mutable, replaceable static; here
/// the provider is fixed. Swapping it globally in Rust would need interior
/// mutability on a `static` (a lock on every error raised anywhere) for a
/// hook nothing in the Java tests exercises — if a use case shows up, a
/// setter can be added later without breaking anything.
pub fn built_in_exceptions() -> &'static BuiltInExceptions {
    &BUILT_IN_EXCEPTIONS
}

/// `Arc<dyn Message>` shorthand for the built-in message closures.
fn literal(string: String) -> Arc<dyn Message> {
    Arc::new(LiteralMessage::new(string))
}

/// Formats `value` the way Java's `Double.toString` does, so built-in
/// messages embed numbers byte-identically. Rust's plain `Display` differs:
/// it prints `1` where Java prints `1.0`, and expands `1e20` where Java
/// writes `1.0E20`.
///
/// Java's shape (JDK 19+, which also matches what the Java test oracle runs):
/// shortest round-trip digits, always at least one digit after the decimal
/// point, plain decimal within `1e-3 ≤ |value| < 1e7` and `d.dddE±n`
/// scientific notation outside.
///
/// Shared with the arguments module, whose `Display` impls must render the
/// same way (`doubleArg`'s `toString` embeds the bounds as Java strings).
pub(crate) fn java_double_to_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-Infinity" } else { "Infinity" }.to_owned();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    java_real_from_scientific(value.is_sign_negative(), &format!("{:e}", value.abs()))
}

/// Same as [`java_double_to_string`] for `f32` / Java's `Float.toString`.
pub(crate) fn java_float_to_string(value: f32) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-Infinity" } else { "Infinity" }.to_owned();
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0"
        } else {
            "0.0"
        }
        .to_owned();
    }
    java_real_from_scientific(value.is_sign_negative(), &format!("{:e}", value.abs()))
}

/// Renders `scientific` — Rust's shortest `{:e}` form of a positive finite
/// magnitude, e.g. `"1.5e-3"` — in Java's `Double.toString`/`Float.toString`
/// shape, signed by `negative`.
///
/// `{:e}` yields `d.dddEe` (no plus sign, no zero padding, no trailing
/// zeros), which maps straight onto Java's rules: digits are the mantissa
/// without the dot, `exponent` the decimal exponent.
fn java_real_from_scientific(negative: bool, scientific: &str) -> String {
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("Rust's {:e} always contains an exponent");
    let exponent: i32 = exponent
        .parse()
        .expect("Rust's {:e} exponent is a plain integer");

    let mut digits = String::with_capacity(mantissa.len());
    for c in mantissa.chars() {
        if c != '.' {
            digits.push(c);
        }
    }

    let mut result = String::new();
    if negative {
        result.push('-');
    }

    if (-3..7).contains(&exponent) {
        if exponent >= 0 {
            // decimal point sits after (exponent + 1) digits
            let integer_len = exponent as usize + 1;
            if digits.len() > integer_len {
                result.push_str(&digits[..integer_len]);
                result.push('.');
                result.push_str(&digits[integer_len..]);
            } else {
                result.push_str(&digits);
                for _ in digits.len()..integer_len {
                    result.push('0');
                }
                result.push_str(".0");
            }
        } else {
            result.push_str("0.");
            for _ in 0..(-exponent - 1) {
                result.push('0');
            }
            result.push_str(&digits);
        }
    } else {
        result.push_str(&digits[..1]);
        result.push('.');
        if digits.len() > 1 {
            result.push_str(&digits[1..]);
        } else {
            result.push('0');
        }
        result.push('E');
        let _ = write!(result, "{exponent}");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_double_matches_java_to_string() {
        assert_eq!(java_double_to_string(1.0), "1.0");
        assert_eq!(java_double_to_string(-1.0), "-1.0");
        assert_eq!(java_double_to_string(0.0), "0.0");
        assert_eq!(java_double_to_string(-0.0), "-0.0");
        assert_eq!(java_double_to_string(0.5), "0.5");
        assert_eq!(java_double_to_string(2.5), "2.5");
        assert_eq!(java_double_to_string(123.456), "123.456");
        assert_eq!(java_double_to_string(0.001), "0.001");
        assert_eq!(java_double_to_string(0.0001), "1.0E-4");
        assert_eq!(java_double_to_string(9999999.0), "9999999.0");
        assert_eq!(java_double_to_string(10_000_000.0), "1.0E7");
        assert_eq!(java_double_to_string(1e20), "1.0E20");
        assert_eq!(java_double_to_string(123_456_789.0), "1.23456789E8");
        assert_eq!(java_double_to_string(f64::MAX), "1.7976931348623157E308");
        assert_eq!(
            java_double_to_string(f64::MIN_POSITIVE),
            "2.2250738585072014E-308"
        );
        assert_eq!(java_double_to_string(f64::NAN), "NaN");
        assert_eq!(java_double_to_string(f64::INFINITY), "Infinity");
        assert_eq!(java_double_to_string(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn java_float_matches_java_to_string() {
        assert_eq!(java_float_to_string(1.0), "1.0");
        assert_eq!(java_float_to_string(0.1), "0.1");
        assert_eq!(java_float_to_string(0.0015), "0.0015");
        assert_eq!(java_float_to_string(1e-4), "1.0E-4");
        assert_eq!(java_float_to_string(1e7), "1.0E7");
        assert_eq!(java_float_to_string(f32::MAX), "3.4028235E38");
        assert_eq!(java_float_to_string(f32::NAN), "NaN");
        assert_eq!(java_float_to_string(-0.0), "-0.0");
    }
}
