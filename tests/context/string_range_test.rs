use brigaxsir::{context::StringRange, string_reader::StringReader};

// Java's StringRangeTest has no dedicated file in 1.3 (the type is covered
// indirectly); these pin the char-unit contract from notes.md §3 on top of
// the ASCII behavior.

#[test]
fn get_ascii_matches_java() {
    assert_eq!(StringRange::between(1, 4).get("hello"), "ell");
    assert_eq!(StringRange::at(0).get("hello"), "");
}

#[test]
fn get_counts_characters_not_bytes() {
    // привет is six two-byte chars: byte slicing would panic or misplace
    assert_eq!(StringRange::between(1, 4).get("привет"), "рив");
    assert_eq!(StringRange::between(0, 2).get("привет"), "пр");
    // one char = 4 bytes; a byte-based slice [0..1] would panic here
    assert_eq!(StringRange::between(0, 1).get("😀x"), "😀");
}

#[test]
fn get_saturates_out_of_range() {
    // Java's substring throws here; the port clamps to the input bounds so
    // no range can panic
    assert_eq!(StringRange::between(0, 100).get("hi"), "hi");
    assert_eq!(StringRange::between(5, 2).get("hi"), "");
}

#[test]
fn length_is_in_characters() {
    assert_eq!(StringRange::between(0, 2).length(), 2);
    assert!(StringRange::at(3).is_empty());
}

// Java: `get(ImmutableStringReader)` — the reader overload, ParsedArgumentTest's
// `getRaw` uses it
#[test]
fn get_from_reader_slices_the_reader_input() {
    let reader = StringReader::from("0123456789");
    assert_eq!(StringRange::between(2, 5).get_from_reader(&reader), "234");
    // char-indexed like every other range consumer in the crate
    let reader = StringReader::from("привет мир");
    assert_eq!(StringRange::between(7, 10).get_from_reader(&reader), "мир");
}
