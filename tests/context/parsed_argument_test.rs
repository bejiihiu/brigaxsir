//! The port of Java's `context/ParsedArgumentTest`.

use brigaxsir::{context::ParsedArgument, string_reader::StringReader};

// Java's EqualsTester groups, flattened into the crate's within-group-equal /
// cross-group-unequal loops.
#[test]
fn test_equals() {
    let group1 = [
        ParsedArgument::new(0, 3, "bar".to_owned()),
        ParsedArgument::new(0, 3, "bar".to_owned()),
    ];
    let group2 = [
        ParsedArgument::new(3, 6, "baz".to_owned()),
        ParsedArgument::new(3, 6, "baz".to_owned()),
    ];
    let group3 = [
        ParsedArgument::new(6, 9, "baz".to_owned()),
        ParsedArgument::new(6, 9, "baz".to_owned()),
    ];

    let groups = [&group1, &group2, &group3];
    for group in groups {
        assert_eq!(group[0], group[1]);
    }
    for (i, a) in groups.iter().enumerate() {
        for (j, b) in groups.iter().enumerate() {
            if i != j {
                assert_ne!(a[0], b[0]);
            }
        }
    }
}

#[test]
fn get_raw() {
    // Java: `new StringReader("0123456789")` and the reader overload of
    // `StringRange.get`
    let reader = StringReader::from("0123456789");
    // Java's `new ParsedArgument<>(2, 5, "")` — the string literal is the
    // String value type
    let argument = ParsedArgument::new(2, 5, String::new());
    assert_eq!(argument.get_range().get_from_reader(&reader), "234");
}
