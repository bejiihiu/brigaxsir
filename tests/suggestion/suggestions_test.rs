use brigaxsir::{
    context::StringRange,
    suggestion::{Suggestion, Suggestions},
};

#[test]
fn merge_empty() {
    let merged = Suggestions::merge("foo b", &[]);
    assert!(merged.is_empty());
}

#[test]
fn merge_single() {
    let suggestions = Suggestions::new(
        StringRange::at(5),
        vec![Suggestion::new(StringRange::at(5), "ar")],
    );
    let merged = Suggestions::merge("foo b", std::slice::from_ref(&suggestions));
    assert_eq!(merged, suggestions);
}

#[test]
fn merge_multiple() {
    let a = Suggestions::new(
        StringRange::at(5),
        vec![
            Suggestion::new(StringRange::at(5), "ar"),
            Suggestion::new(StringRange::at(5), "az"),
            Suggestion::new(StringRange::at(5), "Az"),
        ],
    );
    let b = Suggestions::new(
        StringRange::between(4, 5),
        vec![
            Suggestion::new(StringRange::between(4, 5), "foo"),
            Suggestion::new(StringRange::between(4, 5), "qux"),
            Suggestion::new(StringRange::between(4, 5), "apple"),
            Suggestion::new(StringRange::between(4, 5), "Bar"),
        ],
    );
    let merged = Suggestions::merge("foo b", &[a, b]);

    // exact Java order, ties included: "bar" before "Bar" and "baz" before
    // "bAz" fall out of Java's HashSet iteration order feeding a stable sort
    assert_eq!(
        merged.list(),
        [
            Suggestion::new(StringRange::between(4, 5), "apple"),
            Suggestion::new(StringRange::between(4, 5), "bar"),
            Suggestion::new(StringRange::between(4, 5), "Bar"),
            Suggestion::new(StringRange::between(4, 5), "baz"),
            Suggestion::new(StringRange::between(4, 5), "bAz"),
            Suggestion::new(StringRange::between(4, 5), "foo"),
            Suggestion::new(StringRange::between(4, 5), "qux"),
        ]
        .as_slice()
    );
    assert_eq!(merged.range(), StringRange::between(4, 5));
}
