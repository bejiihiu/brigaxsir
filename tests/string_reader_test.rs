use brigaxsir::{
    errors::{BuiltInExceptionProvider, built_in_exceptions},
    string_reader::StringReader,
};

#[test]
fn can_read() {
    let mut reader = StringReader::from("abc".to_owned());
    assert!(reader.can_read());
    reader.skip(); // 'a'
    assert!(reader.can_read());
    reader.skip(); // 'b'
    assert!(reader.can_read());
    reader.skip(); // 'c'
    assert!(!reader.can_read());
}

#[test]
fn get_remaining_length() {
    let mut reader = StringReader::from("abc".to_owned());
    assert_eq!(reader.remaining_length(), 3);
    reader.set_cursor(1);
    assert_eq!(reader.remaining_length(), 2);
    reader.set_cursor(2);
    assert_eq!(reader.remaining_length(), 1);
    reader.set_cursor(3);
    assert_eq!(reader.remaining_length(), 0);
}

#[test]
fn can_read_length() {
    let reader = StringReader::from("abc".to_owned());
    assert!(reader.can_read_length(1));
    assert!(reader.can_read_length(2));
    assert!(reader.can_read_length(3));
    assert!(!reader.can_read_length(4));
    assert!(!reader.can_read_length(5));
}

#[test]
fn peek() {
    let mut reader = StringReader::from("abc".to_owned());
    assert_eq!(reader.peek(), 'a');
    assert_eq!(reader.cursor(), 0);
    reader.set_cursor(2);
    assert_eq!(reader.peek(), 'c');
    assert_eq!(reader.cursor(), 2);
}

#[test]
fn peek_length() {
    let mut reader = StringReader::from("abc".to_owned());
    assert_eq!(reader.peek_offset(0), 'a');
    assert_eq!(reader.peek_offset(2), 'c');
    assert_eq!(reader.cursor(), 0);
    reader.set_cursor(1);
    assert_eq!(reader.peek_offset(1), 'c');
    assert_eq!(reader.cursor(), 1);
}

#[test]
fn read() {
    let mut reader = StringReader::from("abc".to_owned());
    assert_eq!(reader.read(), 'a');
    assert_eq!(reader.read(), 'b');
    assert_eq!(reader.read(), 'c');
    assert_eq!(reader.cursor(), 3);
}

#[test]
fn skip() {
    let mut reader = StringReader::from("abc".to_owned());
    reader.skip();
    assert_eq!(reader.cursor(), 1);
}

#[test]
fn get_remaining() {
    let mut reader = StringReader::from("Hello!".to_owned());
    assert_eq!(reader.remaining(), "Hello!");
    reader.set_cursor(3);
    assert_eq!(reader.remaining(), "lo!");
    reader.set_cursor(6);
    assert_eq!(reader.remaining(), "");
}

#[test]
fn get_read() {
    let mut reader = StringReader::from("Hello!".to_owned());
    assert_eq!(reader.get_read(), "");
    reader.set_cursor(3);
    assert_eq!(reader.get_read(), "Hel");
    reader.set_cursor(6);
    assert_eq!(reader.get_read(), "Hello!");
}

#[test]
fn skip_whitespace_none() {
    let mut reader = StringReader::from("Hello!".to_owned());
    reader.skip_whitespace();
    assert_eq!(reader.cursor(), 0);
}

#[test]
fn skip_whitespace_mixed() {
    let mut reader = StringReader::from(" \t \t\nHello!".to_owned());
    reader.skip_whitespace();
    assert_eq!(reader.cursor(), 5);
}

#[test]
fn skip_whitespace_empty() {
    let mut reader = StringReader::from("".to_owned());
    reader.skip_whitespace();
    assert_eq!(reader.cursor(), 0);
}

#[test]
fn read_unquoted_string() {
    let mut reader = StringReader::from("hello world".to_owned());
    assert_eq!(reader.read_unquoted_string(), "hello");
    assert_eq!(reader.get_read(), "hello");
    assert_eq!(reader.remaining(), " world");
}

#[test]
fn read_unquoted_string_empty() {
    let mut reader = StringReader::from("".to_owned());
    assert_eq!(reader.read_unquoted_string(), "");
    assert_eq!(reader.get_read(), "");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_unquoted_string_empty_with_remaining() {
    let mut reader = StringReader::from(" hello world".to_owned());
    assert_eq!(reader.read_unquoted_string(), "");
    assert_eq!(reader.get_read(), "");
    assert_eq!(reader.remaining(), " hello world");
}

#[test]
fn read_quoted_string() {
    let mut reader = StringReader::from("\"hello world\"".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
    assert_eq!(reader.get_read(), "\"hello world\"");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_single_quoted_string() {
    let mut reader = StringReader::from("'hello world'".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
    assert_eq!(reader.get_read(), "'hello world'");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_mixed_quoted_string_double_inside_single() {
    let mut reader = StringReader::from("'hello \"world\"'".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "hello \"world\"");
    assert_eq!(reader.get_read(), "'hello \"world\"'");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_mixed_quoted_string_single_inside_double() {
    let mut reader = StringReader::from("\"hello 'world'\"".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "hello 'world'");
    assert_eq!(reader.get_read(), "\"hello 'world'\"");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_quoted_string_empty() {
    let mut reader = StringReader::from("".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "");
    assert_eq!(reader.get_read(), "");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_quoted_string_empty_quoted() {
    let mut reader = StringReader::from("\"\"".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "");
    assert_eq!(reader.get_read(), "\"\"");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_quoted_string_empty_quoted_with_remaining() {
    let mut reader = StringReader::from("\"\" hello world".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "");
    assert_eq!(reader.get_read(), "\"\"");
    assert_eq!(reader.remaining(), " hello world");
}

#[test]
fn read_quoted_string_with_escaped_quote() {
    let mut reader = StringReader::from("\"hello \\\"world\\\"\"".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "hello \"world\"");
    assert_eq!(reader.get_read(), "\"hello \\\"world\\\"\"");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_quoted_string_with_escaped_escapes() {
    let mut reader = StringReader::from("\"\\\\o/\"".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "\\o/");
    assert_eq!(reader.get_read(), "\"\\\\o/\"");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_quoted_string_with_remaining() {
    let mut reader = StringReader::from("\"hello world\" foo bar".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
    assert_eq!(reader.get_read(), "\"hello world\"");
    assert_eq!(reader.remaining(), " foo bar");
}

#[test]
fn read_quoted_string_with_immediate_remaining() {
    let mut reader = StringReader::from("\"hello world\"foo bar".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "hello world");
    assert_eq!(reader.get_read(), "\"hello world\"");
    assert_eq!(reader.remaining(), "foo bar");
}

#[test]
fn read_quoted_string_no_open() {
    let mut reader = StringReader::from("hello world\"".to_owned());
    let result = reader.read_quoted_string();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(
            e.is_type(
                built_in_exceptions()
                    .reader_expected_start_of_quote()
                    .as_type()
            )
        );
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_quoted_string_no_close() {
    let mut reader = StringReader::from("\"hello world".to_owned());
    let result = reader.read_quoted_string();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(
            e.is_type(
                built_in_exceptions()
                    .reader_expected_end_of_quote()
                    .as_type()
            )
        );
        assert_eq!(e.get_raw_message().get_string(), "Unclosed quoted string");
        assert_eq!(e.get_cursor(), Some(12));
    }
}

#[test]
fn read_quoted_string_invalid_escape() {
    let mut reader = StringReader::from("\"hello\\nworld\"".to_owned());
    let result = reader.read_quoted_string();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_invalid_escape().as_type()));
        assert_eq!(
            e.get_raw_message().get_string(),
            "Invalid escape sequence 'n' in quoted string"
        );
        assert_eq!(e.get_cursor(), Some(7));
    }
}

#[test]
fn read_quoted_string_invalid_quote_escape() {
    let mut reader = StringReader::from("'hello\\\"\'world".to_owned());
    let result = reader.read_quoted_string();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_invalid_escape().as_type()));
        assert_eq!(
            e.get_raw_message().get_string(),
            "Invalid escape sequence '\"' in quoted string"
        );
        assert_eq!(e.get_cursor(), Some(7));
    }
}

#[test]
fn read_string_no_quotes() {
    let mut reader = StringReader::from("hello world".to_owned());
    assert_eq!(reader.read_string().unwrap(), "hello");
    assert_eq!(reader.get_read(), "hello");
    assert_eq!(reader.remaining(), " world");
}

#[test]
fn read_string_single_quotes() {
    let mut reader = StringReader::from("'hello world'".to_owned());
    assert_eq!(reader.read_string().unwrap(), "hello world");
    assert_eq!(reader.get_read(), "'hello world'");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_string_double_quotes() {
    let mut reader = StringReader::from("\"hello world\"".to_owned());
    assert_eq!(reader.read_string().unwrap(), "hello world");
    assert_eq!(reader.get_read(), "\"hello world\"");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_int() {
    let mut reader = StringReader::from("1234567890".to_owned());
    assert_eq!(reader.read_int().unwrap(), 1234567890);
    assert_eq!(reader.get_read(), "1234567890");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_int_negative() {
    let mut reader = StringReader::from("-1234567890".to_owned());
    assert_eq!(reader.read_int().unwrap(), -1234567890);
    assert_eq!(reader.get_read(), "-1234567890");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_int_invalid() {
    let mut reader = StringReader::from("12.34".to_owned());
    let result = reader.read_int();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_invalid_int().as_type()));
        assert_eq!(e.get_raw_message().get_string(), "Invalid integer '12.34'");
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_int_none() {
    let mut reader = StringReader::from("".to_owned());
    let result = reader.read_int();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_expected_int().as_type()));
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_int_with_remaining() {
    let mut reader = StringReader::from("1234567890 foo bar".to_owned());
    assert_eq!(reader.read_int().unwrap(), 1234567890);
    assert_eq!(reader.get_read(), "1234567890");
    assert_eq!(reader.remaining(), " foo bar");
}

#[test]
fn read_int_rejects_leading_plus_like_java() {
    // Java's Integer.parseInt accepts a leading '+' ("+5" -> 5), but the
    // reader never feeds it one: isAllowedNumber does not include '+', so
    // the scan collects nothing and readInt reports the empty-token
    // expected-int error, exactly like Java's readInt("+5").
    let mut reader = StringReader::from("+5".to_owned());
    let result = reader.read_int();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_expected_int().as_type()));
        assert_eq!(e.get_cursor(), Some(0));
    }
    // '+' stops the scan mid-token too: "5+5" parses as 5 with '+' left over
    let mut reader = StringReader::from("5+5".to_owned());
    assert_eq!(reader.read_int().unwrap(), 5);
    assert_eq!(reader.remaining(), "+5");
}

#[test]
fn read_double_stops_at_exponent_like_java() {
    // Java's Double.parseDouble understands exponents, but the reader's
    // scan does not include 'e': "1e40" parses as the token "1" and leaves
    // "e40" for the dispatcher's separator check. The float reads behave
    // the same.
    let mut reader = StringReader::from("1e40".to_owned());
    assert_eq!(reader.read_double().unwrap(), 1.0);
    assert_eq!(reader.remaining(), "e40");

    let mut reader = StringReader::from("1e40".to_owned());
    assert_eq!(reader.read_float().unwrap(), 1.0);
    assert_eq!(reader.remaining(), "e40");

    // plain decimals and exponents-free shapes parse fully
    let mut reader = StringReader::from("-.5".to_owned());
    assert_eq!(reader.read_double().unwrap(), -0.5);
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_int_with_remaining_immediate() {
    let mut reader = StringReader::from("1234567890foo bar".to_owned());
    assert_eq!(reader.read_int().unwrap(), 1234567890);
    assert_eq!(reader.get_read(), "1234567890");
    assert_eq!(reader.remaining(), "foo bar");
}

#[test]
fn read_long() {
    let mut reader = StringReader::from("1234567890".to_owned());
    assert_eq!(reader.read_long().unwrap(), 1234567890);
    assert_eq!(reader.get_read(), "1234567890");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_long_negative() {
    let mut reader = StringReader::from("-1234567890".to_owned());
    assert_eq!(reader.read_long().unwrap(), -1234567890);
    assert_eq!(reader.get_read(), "-1234567890");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_long_invalid() {
    let mut reader = StringReader::from("12.34".to_owned());
    let result = reader.read_long();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_invalid_long().as_type()));
        assert_eq!(e.get_raw_message().get_string(), "Invalid long '12.34'");
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_long_none() {
    let mut reader = StringReader::from("".to_owned());
    let result = reader.read_long();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_expected_long().as_type()));
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_long_with_remaining() {
    let mut reader = StringReader::from("1234567890 foo bar".to_owned());
    assert_eq!(reader.read_long().unwrap(), 1234567890);
    assert_eq!(reader.get_read(), "1234567890");
    assert_eq!(reader.remaining(), " foo bar");
}

#[test]
fn read_long_with_remaining_immediate() {
    let mut reader = StringReader::from("1234567890foo bar".to_owned());
    assert_eq!(reader.read_long().unwrap(), 1234567890);
    assert_eq!(reader.get_read(), "1234567890");
    assert_eq!(reader.remaining(), "foo bar");
}

#[test]
fn read_double() {
    let mut reader = StringReader::from("123".to_owned());
    assert_eq!(reader.read_double().unwrap(), 123.0);
    assert_eq!(reader.get_read(), "123");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_double_with_decimal() {
    let mut reader = StringReader::from("12.34".to_owned());
    assert_eq!(reader.read_double().unwrap(), 12.34);
    assert_eq!(reader.get_read(), "12.34");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_double_negative() {
    let mut reader = StringReader::from("-123".to_owned());
    assert_eq!(reader.read_double().unwrap(), -123.0);
    assert_eq!(reader.get_read(), "-123");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_double_invalid() {
    let mut reader = StringReader::from("12.34.56".to_owned());
    let result = reader.read_double();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_invalid_double().as_type()));
        assert_eq!(
            e.get_raw_message().get_string(),
            "Invalid double '12.34.56'"
        );
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_double_none() {
    let mut reader = StringReader::from("".to_owned());
    let result = reader.read_double();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_expected_double().as_type()));
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_double_with_remaining() {
    let mut reader = StringReader::from("12.34 foo bar".to_owned());
    assert_eq!(reader.read_double().unwrap(), 12.34);
    assert_eq!(reader.get_read(), "12.34");
    assert_eq!(reader.remaining(), " foo bar");
}

#[test]
fn read_double_with_remaining_immediate() {
    let mut reader = StringReader::from("12.34foo bar".to_owned());
    assert_eq!(reader.read_double().unwrap(), 12.34);
    assert_eq!(reader.get_read(), "12.34");
    assert_eq!(reader.remaining(), "foo bar");
}

#[test]
fn read_float() {
    let mut reader = StringReader::from("123".to_owned());
    assert_eq!(reader.read_float().unwrap(), 123.0f32);
    assert_eq!(reader.get_read(), "123");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_float_with_decimal() {
    let mut reader = StringReader::from("12.34".to_owned());
    assert_eq!(reader.read_float().unwrap(), 12.34f32);
    assert_eq!(reader.get_read(), "12.34");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_float_negative() {
    let mut reader = StringReader::from("-123".to_owned());
    assert_eq!(reader.read_float().unwrap(), -123.0f32);
    assert_eq!(reader.get_read(), "-123");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_float_invalid() {
    let mut reader = StringReader::from("12.34.56".to_owned());
    let result = reader.read_float();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_invalid_float().as_type()));
        assert_eq!(e.get_raw_message().get_string(), "Invalid float '12.34.56'");
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_float_none() {
    let mut reader = StringReader::from("".to_owned());
    let result = reader.read_float();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_expected_float().as_type()));
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_float_with_remaining() {
    let mut reader = StringReader::from("12.34 foo bar".to_owned());
    assert_eq!(reader.read_float().unwrap(), 12.34f32);
    assert_eq!(reader.get_read(), "12.34");
    assert_eq!(reader.remaining(), " foo bar");
}

#[test]
fn read_float_with_remaining_immediate() {
    let mut reader = StringReader::from("12.34foo bar".to_owned());
    assert_eq!(reader.read_float().unwrap(), 12.34f32);
    assert_eq!(reader.get_read(), "12.34");
    assert_eq!(reader.remaining(), "foo bar");
}

#[test]
fn expect_correct() {
    let mut reader = StringReader::from("abc".to_owned());
    reader.expect('a').unwrap();
    assert_eq!(reader.cursor(), 1);
}

#[test]
fn expect_incorrect() {
    let mut reader = StringReader::from("bcd".to_owned());
    let result = reader.expect('a');
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_expected_symbol().as_type()));
        assert_eq!(e.get_raw_message().get_string(), "Expected 'a'");
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn expect_none() {
    let mut reader = StringReader::from("".to_owned());
    let result = reader.expect('a');
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_expected_symbol().as_type()));
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_boolean_correct() {
    let mut reader = StringReader::from("true".to_owned());
    assert!(reader.read_boolean().unwrap());
    assert_eq!(reader.get_read(), "true");
}

#[test]
fn read_boolean_incorrect() {
    let mut reader = StringReader::from("tuesday".to_owned());
    let result = reader.read_boolean();
    assert!(result.is_err());
    if let Err(e) = result {
        assert_eq!(
            e.get_raw_message().get_string(),
            "Invalid bool, expected true or false but found 'tuesday'"
        );
        assert!(e.is_type(built_in_exceptions().reader_invalid_bool().as_type()));
        assert_eq!(e.get_cursor(), Some(0));
    }
}

#[test]
fn read_boolean_none() {
    let mut reader = StringReader::from("".to_owned());
    let result = reader.read_boolean();
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(e.is_type(built_in_exceptions().reader_expected_bool().as_type()));
        assert_eq!(e.get_cursor(), Some(0));
    }
}

// -- non-ASCII behavior ------------------------------------------------------
// The cursor counts characters (Unicode scalar values), not bytes and not
// Java's UTF-16 units. These tests pin the char-based semantics the
// ASCII-only Java suite could never exercise.

#[test]
fn peek_read_count_characters_not_bytes() {
    let mut reader = StringReader::from("привет".to_owned());
    assert_eq!(reader.total_length(), 6);
    assert_eq!(reader.remaining_length(), 6);
    assert_eq!(reader.peek(), 'п');
    assert_eq!(reader.read(), 'п');
    assert_eq!(reader.read(), 'р');
    assert_eq!(reader.cursor(), 2);
    assert_eq!(reader.get_read(), "пр");
    assert_eq!(reader.remaining(), "ивет");
    assert_eq!(reader.remaining_length(), 4);
}

#[test]
fn emoji_is_one_character() {
    // 😀 is a 4-byte UTF-8 char (and a surrogate pair in Java)
    let mut reader = StringReader::from("😀x".to_owned());
    assert_eq!(reader.total_length(), 2);
    assert!(reader.can_read_length(2));
    assert!(!reader.can_read_length(3));
    assert_eq!(reader.peek(), '😀');
    assert_eq!(reader.peek_offset(1), 'x');
    assert_eq!(reader.read(), '😀');
    assert_eq!(reader.cursor(), 1);
    assert_eq!(reader.get_read(), "😀");
    assert_eq!(reader.remaining(), "x");
    assert_eq!(reader.peek(), 'x');
}

#[test]
fn set_cursor_positions_by_characters() {
    let mut reader = StringReader::from("привет".to_owned());
    reader.set_cursor(3);
    assert_eq!(reader.cursor(), 3);
    assert_eq!(reader.peek(), 'в');
    assert_eq!(reader.get_read(), "при");
    assert_eq!(reader.remaining(), "вет");
    assert_eq!(reader.remaining_length(), 3);
}

#[test]
fn read_unquoted_string_stops_at_non_ascii_like_java() {
    // isAllowedInUnquotedString is ASCII-only in Java too, so an unquoted
    // read never consumes Cyrillic — same empty result here
    let mut reader = StringReader::from("привет мир".to_owned());
    assert_eq!(reader.read_unquoted_string(), "");
    assert_eq!(reader.cursor(), 0);
    assert_eq!(reader.remaining(), "привет мир");
}

#[test]
fn read_quoted_string_with_cyrillic_and_escapes() {
    let mut reader = StringReader::from("\"привет \\\"мир\\\"\"".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "привет \"мир\"");
    assert_eq!(reader.get_read(), "\"привет \\\"мир\\\"\"");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn read_quoted_string_with_emoji() {
    let mut reader = StringReader::from("\"hi 😀 there\"".to_owned());
    assert_eq!(reader.read_quoted_string().unwrap(), "hi 😀 there");
    assert_eq!(reader.get_read(), "\"hi 😀 there\"");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn quoted_string_error_cursors_count_characters() {
    // unclosed quote: "привет мир is 11 characters, error lands at the end
    let mut reader = StringReader::from("\"привет мир".to_owned());
    let result = reader.read_quoted_string();
    let Err(e) = result else {
        panic!("expected an error");
    };
    assert!(
        e.is_type(
            built_in_exceptions()
                .reader_expected_end_of_quote()
                .as_type()
        )
    );
    assert_eq!(e.get_cursor(), Some(11));

    // invalid escape inside BMP text: "при\qвет — the bad char sits at
    // char index 5, same as Java's UTF-16 index would be here
    let mut reader = StringReader::from("\"при\\qвет\"".to_owned());
    let result = reader.read_quoted_string();
    let Err(e) = result else {
        panic!("expected an error");
    };
    assert!(e.is_type(built_in_exceptions().reader_invalid_escape().as_type()));
    assert_eq!(e.get_cursor(), Some(5));
}

#[test]
fn error_cursors_diverge_from_java_only_for_astral_input() {
    // 😀 is one char index here but two UTF-16 units in Java, so the escape
    // error lands at char 3 where Java would report 4
    let mut reader = StringReader::from("\"😀\\q\"".to_owned());
    let result = reader.read_quoted_string();
    let Err(e) = result else {
        panic!("expected an error");
    };
    assert!(e.is_type(built_in_exceptions().reader_invalid_escape().as_type()));
    assert_eq!(e.get_cursor(), Some(3));
}

#[test]
fn read_int_stops_at_non_ascii() {
    let mut reader = StringReader::from("42привета".to_owned());
    assert_eq!(reader.read_int().unwrap(), 42);
    assert_eq!(reader.cursor(), 2);
    assert_eq!(reader.get_read(), "42");
    assert_eq!(reader.remaining(), "привета");
}

#[test]
fn read_int_after_non_ascii_text() {
    let mut reader = StringReader::from("число 42".to_owned());
    reader.set_cursor(6);
    assert_eq!(reader.read_int().unwrap(), 42);
    assert_eq!(reader.get_read(), "число 42");
    assert_eq!(reader.remaining(), "");
}

#[test]
fn skip_whitespace_java_parity_nbsp_vs_ideographic() {
    // Java parity: NBSP (U+00A0) is NOT whitespace for
    // Character.isWhitespace and stays token material, while the
    // ideographic space (U+3000) is skipped.
    let mut reader = StringReader::from("\u{00A0}\u{3000}42".to_owned());
    reader.skip_whitespace();
    assert_eq!(reader.cursor(), 0);
    assert_eq!(reader.peek(), '\u{00A0}');
    reader.skip();
    reader.skip_whitespace();
    assert_eq!(reader.cursor(), 2);
    assert_eq!(reader.read_int().unwrap(), 42);
    assert_eq!(reader.get_read(), "\u{00A0}\u{3000}42");
}

#[test]
fn skip_whitespace_java_set_ascii_ranges() {
    // the control-character half of Java's set: tab/LF/VT/FF/CR plus the
    // C0 separators U+001C..U+001F — all skipped like Java, none of which
    // char::is_whitespace covers on the separator side
    for cp in [
        0x09u32, 0x0A, 0x0B, 0x0C, 0x0D, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
    ] {
        let c = char::from_u32(cp).unwrap();
        let mut reader = StringReader::from(format!("a{c}b"));
        reader.skip();
        reader.skip_whitespace();
        assert_eq!(reader.cursor(), 2, "cp {cp:#x} must be skipped");
        assert_eq!(reader.peek(), 'b');
    }
}

#[test]
fn skip_whitespace_keeps_java_non_spaces() {
    // the code points Rust's char::is_whitespace would eat but Java keeps:
    // NEL, no-break space, figure space, narrow no-break space
    for cp in [0x85u32, 0xA0, 0x2007, 0x202F] {
        let c = char::from_u32(cp).unwrap();
        let mut reader = StringReader::from(format!("a{c}b"));
        reader.skip();
        reader.skip_whitespace();
        assert_eq!(reader.cursor(), 1, "cp {cp:#x} must NOT be skipped");
        assert_eq!(reader.peek(), c);
    }
}

#[test]
fn skip_whitespace_java_set_exotic_spaces() {
    // the non-ASCII half of Java's set: Ogham, the en/peri/punctuation
    // space run around the excluded figure space, line/paragraph
    // separators, medium mathematical space and the ideographic space
    let mut skipped = Vec::new();
    for cp in 0x1680u32..=0x1680 {
        skipped.push(cp);
    }
    skipped.extend(0x2000..=0x2006);
    skipped.extend(0x2008..=0x200A);
    skipped.extend(0x2028..=0x2029);
    skipped.push(0x205F);
    skipped.push(0x3000);
    for cp in skipped {
        let c = char::from_u32(cp).unwrap();
        let mut reader = StringReader::from(format!("a{c}b"));
        reader.skip();
        reader.skip_whitespace();
        assert_eq!(reader.cursor(), 2, "cp {cp:#x} must be skipped");
        assert_eq!(reader.peek(), 'b');
    }
}

#[test]
fn skip_whitespace_matches_java_is_whitespace_over_the_whole_domain() {
    // exhaustive pin over every code point: the reader's whitespace set is
    // Java's Character.isWhitespace, which differs from
    // char::is_whitespace on exactly eight code points (U+001C..U+001F are
    // Java-only; U+0085, U+00A0, U+2007, U+202F are Rust-only). Dumped
    // from the live JDK 21; if either side's Unicode data drifts, this
    // fails and the set must be re-verified.
    const JAVA_ONLY: [u32; 4] = [0x1C, 0x1D, 0x1E, 0x1F];
    const RUST_ONLY: [u32; 4] = [0x85, 0xA0, 0x2007, 0x202F];
    for cp in 0u32..=0x10FFFF {
        let c = match char::from_u32(cp) {
            Some(c) => c,
            None => continue, // surrogates: no Rust char, Java says false
        };
        // Java set = char::is_whitespace plus the four Java-only code
        // points, minus the four Rust-only ones
        let is_java_ws = (c.is_whitespace() || JAVA_ONLY.contains(&cp)) && !RUST_ONLY.contains(&cp);
        let mut reader = StringReader::from(format!("a{c}"));
        reader.skip();
        reader.skip_whitespace();
        let skipped = reader.cursor() == 2;
        assert_eq!(skipped, is_java_ws, "cp {cp:#x}");
    }
}

#[test]
fn length_operations_saturate_past_the_end() {
    // Java throws StringIndexOutOfBounds once the cursor is past the end;
    // we saturate instead (deliberate divergence, see the struct docs)
    let mut reader = StringReader::from("аб".to_owned());
    reader.skip();
    reader.skip();
    reader.skip(); // one past the end
    assert_eq!(reader.cursor(), 3);
    assert_eq!(reader.remaining_length(), 0);
    assert_eq!(reader.remaining(), "");
    assert_eq!(reader.get_read(), "аб");
    assert!(!reader.can_read());
    assert!(!reader.can_read_length(0));
}

#[test]
#[should_panic(expected = "peeked past the end of the input")]
fn peek_past_end_panics_like_java() {
    // set_cursor(1) on a two-byte char lands exactly on the end — a byte
    // cursor would be sitting mid-character instead
    let mut reader = StringReader::from("п".to_owned());
    reader.set_cursor(1);
    let _ = reader.peek();
}

#[test]
fn error_context_slices_by_characters() {
    let mut reader = StringReader::from("абвгдеёжзийклмноп".to_owned());
    reader.set_cursor(12);
    let error = built_in_exceptions()
        .reader_expected_int()
        .create_with_context(&reader);
    // 12 > CONTEXT_AMOUNT(10): "..." plus the 10 chars before the cursor
    // (the ё in there is two bytes — byte slicing would have panicked)
    assert_eq!(error.get_context().unwrap(), "...вгдеёжзийк<--[HERE]");
}
