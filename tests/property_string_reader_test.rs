//! Property-based tests for the `StringReader` — the fuzz-style coverage a
//! hand-written test suite can only hint at. Every property holds for
//! arbitrary UTF-8 input: the generator mixes full-Unicode strings with a
//! dense pool of Cyrillic, emoji (astral plane), both quote kinds, the
//! escape character, whitespace and number characters, so the interesting
//! classes get real hit rates.
//!
//! The invariants checked here:
//!
//! 1. `get_read() + remaining()` reconstructs the whole input for any
//!    cursor — including cursors pushed past the end, where both slice to
//!    the input bounds (the documented saturation divergence from Java).
//! 2. `remaining_length() + cursor() == total_length()` for in-bounds
//!    cursors; past the end the remaining length is 0 (Java would go
//!    negative and only fail later on use).
//! 3. `can_read()` ⇔ `cursor() < total_length()`, unconditionally.
//! 4. `set_cursor(0)` restores the full remaining input.
//! 5. Random operation walks keep every invariant after every step.
//! 6. Number reads follow the token contract (`Ok` exactly when the scanned
//!    token parses; the cursor advances by the token on success and is
//!    restored on failure; `expected-*` vs `invalid-*` kinds) — huge values
//!    surface as `invalid-*` errors, never panics.
//! 7. Quoted strings roundtrip: escaping `\` and the terminator makes
//!    `read_string` return the original body byte-for-byte.
//! 8. Garbage inputs only ever produce `CommandSyntaxException`s. The
//!    documented panic spots (`peek`/`read` past the end, Java's
//!    `StringIndexOutOfBoundsException` parity) are gated programmer errors
//!    and stay outside these properties.
//!
//! proptest's defaults are kept deliberately: 256 cases per property with
//! the deterministic built-in RNG (no custom seed configuration), so runs
//! are reproducible and the whole file stays well inside the debug-mode
//! time budget.

use std::str::FromStr;

use proptest::{prelude::*, test_runner::TestCaseError};

use brigaxsir::{
    errors::{
        BuiltInExceptionProvider, CommandSyntaxException, DynamicCommandExceptionType,
        SimpleCommandExceptionType, built_in_exceptions,
    },
    string_reader::StringReader,
};

/// The dense character pool: every class the parser cares about shows up
/// with a real hit rate — Cyrillic, emoji (astral), both quote kinds, the
/// escape character, whitespace, number characters, and the case-folding
/// oddity characters (`ß`, U+00A0).
const POOL: [char; 20] = [
    'а', 'Я', 'ё', '🎉', '🚀', '😀', '"', '\'', '\\', ' ', '\t', '\n', '0', '7', '.', '-', '+',
    'x', '\u{00A0}', 'ß',
];

/// Half the cases draw full-Unicode strings (the generator can produce any
/// scalar value), the other half dense pool strings — both feed every
/// property.
fn any_input() -> BoxedStrategy<String> {
    prop_oneof![
        1 => any::<String>(),
        2 => proptest::collection::vec(0usize..POOL.len(), 0..=16)
            .prop_map(|idxs| idxs.into_iter().map(|i| POOL[i]).collect()),
    ]
    .boxed()
}

/// A string plus a cursor: every in-bounds position plus three
/// past-the-end positions (the saturation branch).
fn reader_and_cursor() -> BoxedStrategy<(String, usize)> {
    any_input()
        .prop_flat_map(|s| {
            let len = s.chars().count();
            (Just(s), 0usize..=len + 3)
        })
        .boxed()
}

/// The maximal run of number-allowed characters from the start of `s` —
/// the same token the reader's number scans collect from cursor 0.
fn number_token(s: &str) -> String {
    s.chars()
        .take_while(|c| StringReader::is_allowed_number(*c))
        .collect()
}

/// The shared invariants after every reader mutation.
fn assert_reader_invariants(reader: &StringReader, s: &str) -> Result<(), TestCaseError> {
    let total = reader.total_length();
    prop_assert_eq!(total, s.chars().count());

    // get_read + remaining always reconstruct the whole input; on saturated
    // cursors both clamp to the input bounds and the sum still holds
    let joined: String = reader.get_read().to_owned() + reader.remaining();
    prop_assert_eq!(&joined, s);
    prop_assert_eq!(
        reader.get_read().chars().count(),
        reader.cursor().min(total)
    );

    if reader.cursor() <= total {
        prop_assert_eq!(reader.remaining_length(), total - reader.cursor());
    } else {
        // saturation: past the end there is nothing left to read (Java's
        // `getRemainingLength` would have gone negative instead)
        prop_assert_eq!(reader.remaining_length(), 0);
    }
    prop_assert_eq!(reader.can_read(), reader.cursor() < total);
    Ok(())
}

/// The token contract all four number readers share: `Ok` exactly when the
/// scanned token parses to the type, the cursor advances by the token's
/// character count on success and is restored on failure, and the error
/// kind separates expected (empty token) from invalid (unparseable token).
fn assert_number_contract<T>(
    s: &str,
    read: impl FnOnce(&mut StringReader) -> Result<T, CommandSyntaxException>,
    parse: impl Fn(&str) -> Option<T>,
    expected: &SimpleCommandExceptionType,
    invalid: &DynamicCommandExceptionType<String>,
    type_name: &str,
) -> Result<(), TestCaseError>
where
    T: PartialEq + std::fmt::Debug,
{
    let token = number_token(s);
    let token_chars = token.chars().count();
    let mut reader = StringReader::from(s);
    let start = reader.cursor();

    match read(&mut reader) {
        Ok(value) => {
            prop_assert!(
                parse(&token).is_some(),
                "{type_name}: read returned Ok where the token {token:?} does not parse"
            );
            prop_assert_eq!(value, parse(&token).unwrap());
            prop_assert_eq!(reader.cursor(), start + token_chars);
        }
        Err(err) => {
            prop_assert!(
                parse(&token).is_none(),
                "{type_name}: read failed where the token {token:?} parses"
            );
            if token.is_empty() {
                prop_assert!(
                    err.is_type(expected.as_type()),
                    "{type_name}: empty token must give the expected-* error, got {err}"
                );
            } else {
                prop_assert!(
                    err.is_type(invalid.as_type()),
                    "{type_name}: unparseable token must give the invalid-* error, got {err}"
                );
            }
            prop_assert_eq!(reader.cursor(), start);
        }
    }
    Ok(())
}

/// Run the token contract for all four number types over the same input.
fn assert_all_number_contracts(s: &str) -> Result<(), TestCaseError> {
    let built_in = built_in_exceptions();
    assert_number_contract(
        s,
        |r| r.read_int(),
        |t| i32::from_str(t).ok(),
        built_in.reader_expected_int(),
        built_in.reader_invalid_int(),
        "read_int",
    )?;
    assert_number_contract(
        s,
        |r| r.read_long(),
        |t| i64::from_str(t).ok(),
        built_in.reader_expected_long(),
        built_in.reader_invalid_long(),
        "read_long",
    )?;
    assert_number_contract(
        s,
        |r| r.read_double(),
        |t| f64::from_str(t).ok(),
        built_in.reader_expected_double(),
        built_in.reader_invalid_double(),
        "read_double",
    )?;
    assert_number_contract(
        s,
        |r| r.read_float(),
        |t| f32::from_str(t).ok(),
        built_in.reader_expected_float(),
        built_in.reader_invalid_float(),
        "read_float",
    )
}

proptest! {
    #[test]
    fn slice_invariants_hold_at_every_cursor((s, cursor) in reader_and_cursor()) {
        let mut reader = StringReader::from(s.clone());
        reader.set_cursor(cursor);
        assert_reader_invariants(&reader, &s)?;

        // set_cursor(0) restores the full remaining input
        reader.set_cursor(0);
        prop_assert_eq!(reader.remaining(), s.as_str());
        prop_assert_eq!(reader.get_read(), "");
        prop_assert_eq!(reader.remaining_length(), reader.total_length());
        prop_assert_eq!(reader.can_read(), !s.is_empty());
    }

    #[test]
    fn random_operation_walks_keep_the_invariants(
        (s, ops) in (any_input(), proptest::collection::vec(reader_op(), 1..=8)),
    ) {
        let mut reader = StringReader::from(s.clone());
        for op in &ops {
            let before = reader.cursor();
            match op {
                ReaderOp::Skips(n) => {
                    for _ in 0..*n {
                        reader.skip();
                    }
                }
                ReaderOp::SetCursor(x) => {
                    reader.set_cursor(*x as usize % (reader.total_length() + 4));
                }
                ReaderOp::SkipWhitespace => reader.skip_whitespace(),
                ReaderOp::ReadUnquoted => {
                    let got = reader.read_unquoted_string().to_owned();
                    prop_assert!(got.chars().all(StringReader::is_allowed_in_unquoted_string));
                    prop_assert_eq!(reader.cursor() - before, got.chars().count());
                }
                ReaderOp::ReadInt => {
                    let expected_token: String = s
                        .chars()
                        .skip(before)
                        .take_while(|c| StringReader::is_allowed_number(*c))
                        .collect();
                    match reader.read_int() {
                        Ok(v) => {
                            prop_assert_eq!(Some(v), expected_token.parse::<i32>().ok());
                            prop_assert_eq!(reader.cursor() - before, expected_token.chars().count());
                        }
                        Err(_) => {
                            prop_assert!(expected_token.parse::<i32>().is_err());
                            // a failed read restores the cursor exactly
                            prop_assert_eq!(reader.cursor(), before);
                        }
                    }
                }
                ReaderOp::ReadQuoted => {
                    // mid-string quoted reads exercise the escape machinery
                    // at arbitrary positions; a failure's cursor follows
                    // Java's error-pointing rules (the invalid-escape error
                    // points at the offending character) rather than
                    // restoring, so only the absence of panics and the
                    // standing invariants are checked here
                    let _ = reader.read_quoted_string();
                }
                ReaderOp::Expect(c) => {
                    let should_match = reader.can_read() && reader.peek() == *c;
                    match reader.expect(*c) {
                        Ok(()) => prop_assert!(should_match),
                        Err(_) => prop_assert!(!should_match),
                    }
                    if should_match {
                        prop_assert_eq!(reader.cursor(), before + 1);
                    } else {
                        prop_assert_eq!(reader.cursor(), before);
                    }
                }
                ReaderOp::Peek => {
                    if reader.can_read() {
                        // peek never disagrees with the first remaining char
                        prop_assert_eq!(reader.peek(), reader.remaining().chars().next().unwrap());
                        prop_assert_eq!(reader.peek_offset(0), reader.peek());
                    }
                }
            }
            assert_reader_invariants(&reader, &s)?;
        }
    }

    #[test]
    fn number_reads_follow_the_token_contract_on_any_input(s in any_input()) {
        assert_all_number_contracts(&s)?;
    }

    #[test]
    fn number_reads_follow_the_token_contract_on_java_number_shapes(
        (token, tail_idx) in (java_number_token(), 0usize..4),
    ) {
        // tails are all non-number characters, so the token scan stops at
        // the boundary and the shapes above get exercised densely
        let tails = ["", " ", "x", "привет"];
        let s = format!("{token}{}", tails[tail_idx]);
        assert_all_number_contracts(&s)?;
    }

    #[test]
    fn unquoted_reads_take_the_maximal_allowed_run(s in any_input()) {
        let mut reader = StringReader::from(s.clone());
        let got = reader.read_unquoted_string();
        let expected: String = s
            .chars()
            .take_while(|c| StringReader::is_allowed_in_unquoted_string(*c))
            .collect();
        prop_assert_eq!(got, expected.as_str());
        prop_assert_eq!(reader.cursor(), expected.chars().count());
    }

    #[test]
    fn quoted_strings_roundtrip(body in any_input(), use_single in any::<bool>()) {
        let (open, close) = if use_single { ('\'', '\'') } else { ('"', '"') };
        let input = format!("{open}{}{close}", escape(&body, open));

        // the escape scheme makes the input well-formed by construction, so
        // the reads cannot fail
        let mut reader = StringReader::from(input.clone());
        let from_quoted = reader.read_quoted_string().expect("well-formed quoted input");
        prop_assert_eq!(&from_quoted, &body);
        // the closing terminator is consumed: the reader sits at the end
        prop_assert_eq!(reader.cursor(), reader.total_length());

        let mut reader = StringReader::from(input);
        let from_read_string = reader.read_string().expect("well-formed quoted input");
        prop_assert_eq!(&from_read_string, &body);
    }

    #[test]
    fn garbage_input_only_produces_errors((input, pick) in (any_input(), 0usize..POOL.len())) {
        // every call must be either Ok or a CommandSyntaxException — never a
        // panic; results are deliberately discarded, the property is the
        // absence of panics
        let _ = StringReader::from(input.clone()).read_string();
        let _ = StringReader::from(input.clone()).read_quoted_string();
        let _ = StringReader::from(input.clone()).read_boolean();
        let _ = StringReader::from(input.clone()).expect(POOL[pick]);
        let mut reader = StringReader::from(input);
        let _ = reader.read_unquoted_string();
    }

    #[test]
    fn unterminated_or_mangled_quotes_only_produce_errors(junk in any_input()) {
        // a quote start followed by arbitrary junk: unterminated quotes,
        // dangling escapes and invalid escape sequences are all errors
        let _ = StringReader::from(format!("\"{junk}")).read_quoted_string();
        let _ = StringReader::from(format!("'{junk}")).read_quoted_string();
    }
}

/// One reader operation, drawn at random for the walk property.
#[derive(Debug, Clone)]
enum ReaderOp {
    Skips(u32),
    SetCursor(u32),
    SkipWhitespace,
    ReadUnquoted,
    ReadInt,
    ReadQuoted,
    Expect(char),
    Peek,
}

fn reader_op() -> BoxedStrategy<ReaderOp> {
    prop_oneof![
        2 => (1u32..=3).prop_map(ReaderOp::Skips),
        2 => any::<u32>().prop_map(ReaderOp::SetCursor),
        1 => Just(ReaderOp::SkipWhitespace),
        1 => Just(ReaderOp::ReadUnquoted),
        1 => Just(ReaderOp::ReadInt),
        1 => Just(ReaderOp::ReadQuoted),
        2 => (0usize..POOL.len()).prop_map(|i| ReaderOp::Expect(POOL[i])),
        1 => Just(ReaderOp::Peek),
    ]
    .boxed()
}

/// Escape a body so it is a well-formed quoted-string payload for the given
/// terminator: only `\` and the terminator need escaping (Java's
/// `readStringUntil` treats everything else after a backslash as an error).
fn escape(body: &str, quote: char) -> String {
    let mut out = String::new();
    for c in body.chars() {
        if c == '\\' || c == quote {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Number tokens in the shapes Java's grammar allows — plain and signed
/// integers with leading zeros, fraction shapes (`.5`, `1.`, `-.5`), huge
/// values (oversized for `int`, still fine for the float types) and the
/// malformed shapes that must stay errors (bare dots/dashes, double signs).
fn java_number_token() -> BoxedStrategy<String> {
    fn digits(size: std::ops::RangeInclusive<usize>) -> BoxedStrategy<String> {
        proptest::collection::vec(0u8..10, size)
            .prop_map(|v| v.into_iter().map(|d| char::from(b'0' + d)).collect())
            .boxed()
    }

    prop_oneof![
        3 => digits(1..=12),
        2 => digits(1..=12).prop_map(|d| format!("-{d}")),
        1 => digits(1..=4).prop_map(|d| format!("00{d}")),
        1 => digits(1..=4).prop_map(|d| format!("-000{d}")),
        2 => (digits(1..=6), digits(1..=6)).prop_map(|(a, b)| format!("{a}.{b}")),
        1 => digits(1..=6).prop_map(|a| format!("{a}.")),
        1 => digits(1..=6).prop_map(|a| format!(".{a}")),
        1 => digits(1..=6).prop_map(|a| format!("-{a}.")),
        1 => digits(1..=6).prop_map(|a| format!("-.{a}")),
        2 => (1usize..=40).prop_map(|n| "9".repeat(n)),
        2 => (1usize..=40).prop_map(|n| format!("-{}", "1".repeat(n))),
        1 => Just(".".to_owned()),
        1 => Just("-".to_owned()),
        1 => Just("-.".to_owned()),
        1 => Just("--1".to_owned()),
        1 => Just("1-2".to_owned()),
        1 => Just("1..2".to_owned()),
    ]
    .boxed()
}
