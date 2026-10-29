use brigaxsir::{
    context::StringRange,
    suggestion::{Suggestion, SuggestionsBuilder},
};

#[test]
fn suggest_appends() {
    let builder = SuggestionsBuilder::new("Hello w", 6);
    let result = builder.suggest("world!").build();
    assert_eq!(
        result.list(),
        [Suggestion::new(StringRange::between(6, 7), "world!")].as_slice()
    );
    assert_eq!(result.range(), StringRange::between(6, 7));
    assert!(!result.is_empty());
}

#[test]
fn suggest_replaces() {
    let builder = SuggestionsBuilder::new("Hello w", 6);
    let result = builder.suggest("everybody").build();
    assert_eq!(
        result.list(),
        [Suggestion::new(StringRange::between(6, 7), "everybody")].as_slice()
    );
    assert_eq!(result.range(), StringRange::between(6, 7));
    assert!(!result.is_empty());
}

#[test]
fn suggest_noop() {
    let builder = SuggestionsBuilder::new("Hello w", 6);
    let result = builder.suggest("w").build();
    assert_eq!(result.list(), [].as_slice());
    assert!(result.is_empty());
}

#[test]
fn suggest_multiple() {
    let builder = SuggestionsBuilder::new("Hello w", 6);
    let result = builder
        .suggest("world!")
        .suggest("everybody")
        .suggest("weekend")
        .build();
    assert_eq!(
        result.list(),
        [
            Suggestion::new(StringRange::between(6, 7), "everybody"),
            Suggestion::new(StringRange::between(6, 7), "weekend"),
            Suggestion::new(StringRange::between(6, 7), "world!"),
        ]
        .as_slice()
    );
    assert_eq!(result.range(), StringRange::between(6, 7));
    assert!(!result.is_empty());
}

#[test]
fn restart() {
    let builder = SuggestionsBuilder::new("Hello w", 6);
    let builder = builder.suggest("won't be included in restart");
    let other = builder.restart();
    assert_ne!(other, builder);
    assert_eq!(other.input(), builder.input());
    assert_eq!(other.start(), builder.start());
    assert_eq!(other.remaining(), builder.remaining());
}

#[test]
fn sort_alphabetical() {
    let builder = SuggestionsBuilder::new("Hello w", 6);
    let result = builder
        .suggest("2")
        .suggest("4")
        .suggest("6")
        .suggest("8")
        .suggest("30")
        .suggest("32")
        .build();
    let actual = result.list().iter().map(|s| s.text()).collect::<Vec<_>>();
    assert_eq!(actual, ["2", "30", "32", "4", "6", "8"]);
}

#[test]
fn sort_numerical() {
    let builder = SuggestionsBuilder::new("Hello w", 6);
    let result = builder
        .suggest_integer(2)
        .suggest_integer(4)
        .suggest_integer(6)
        .suggest_integer(8)
        .suggest_integer(30)
        .suggest_integer(32)
        .build();
    let actual = result.list().iter().map(|s| s.text()).collect::<Vec<_>>();
    assert_eq!(actual, ["2", "4", "6", "8", "30", "32"]);
}

#[test]
fn sort_mixed() {
    let builder = SuggestionsBuilder::new("Hello w", 6);
    let result = builder
        .suggest("11")
        .suggest("22")
        .suggest("33")
        .suggest("a")
        .suggest("b")
        .suggest("c")
        .suggest_integer(2)
        .suggest_integer(4)
        .suggest_integer(6)
        .suggest_integer(8)
        .suggest_integer(30)
        .suggest_integer(32)
        .suggest("3a")
        .suggest("a3")
        .build();
    let actual = result.list().iter().map(|s| s.text()).collect::<Vec<_>>();
    // the exact Java order — mojang please: 30/32 land after 8 because
    // IntegerSuggestions compare numerically among themselves while the
    // comparator as a whole is not transitive; Java's TimSort small-array
    // path over its HashSet iteration order produces exactly this
    assert_eq!(
        actual,
        [
            "11", "2", "22", "33", "3a", "4", "6", "8", "30", "32", "a", "a3", "b", "c"
        ]
    );
}

#[test]
fn non_ascii_start_positions_by_characters() {
    // "привет " is 7 characters (14 bytes); start 7 must land on "мир", not
    // mid-character
    let builder = SuggestionsBuilder::new("привет мир", 7);
    assert_eq!(builder.remaining(), "мир");
    let result = builder.suggest("миру").build();
    assert_eq!(
        result.list(),
        [Suggestion::new(StringRange::between(7, 10), "миру")].as_slice()
    );
    assert_eq!(result.range(), StringRange::between(7, 10));
}
