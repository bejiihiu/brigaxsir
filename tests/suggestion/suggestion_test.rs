use std::sync::Arc;

use brigaxsir::{
    context::StringRange,
    message::LiteralMessage,
    suggestion::{IntegerSuggestion, Suggestion},
};

#[test]
fn apply_insertation_start() {
    let suggestion = Suggestion::new(StringRange::at(0), "And so I said: ");
    assert_eq!(
        suggestion.apply("Hello world!"),
        "And so I said: Hello world!"
    );
}

#[test]
fn apply_insertation_middle() {
    let suggestion = Suggestion::new(StringRange::at(6), "small ");
    assert_eq!(suggestion.apply("Hello world!"), "Hello small world!");
}

#[test]
fn apply_insertation_end() {
    let suggestion = Suggestion::new(StringRange::at(5), " world!");
    assert_eq!(suggestion.apply("Hello"), "Hello world!");
}

#[test]
fn apply_replacement_start() {
    let suggestion = Suggestion::new(StringRange::between(0, 5), "Goodbye");
    assert_eq!(suggestion.apply("Hello world!"), "Goodbye world!");
}

#[test]
fn apply_replacement_middle() {
    let suggestion = Suggestion::new(StringRange::between(6, 11), "Alex");
    assert_eq!(suggestion.apply("Hello world!"), "Hello Alex!");
}

#[test]
fn apply_replacement_end() {
    let suggestion = Suggestion::new(StringRange::between(6, 12), "Creeper!");
    assert_eq!(suggestion.apply("Hello world!"), "Hello Creeper!");
}

#[test]
fn apply_replacement_everything() {
    let suggestion = Suggestion::new(StringRange::between(0, 12), "Oh dear.");
    assert_eq!(suggestion.apply("Hello world!"), "Oh dear.");
}

#[test]
fn expand_unchanged() {
    let suggestion = Suggestion::new(StringRange::at(1), "oo");
    assert_eq!(suggestion.expand("f", StringRange::at(1)), suggestion);
}

#[test]
fn expand_left() {
    let suggestion = Suggestion::new(StringRange::at(1), "oo");
    assert_eq!(
        suggestion.expand("f", StringRange::between(0, 1)),
        Suggestion::new(StringRange::between(0, 1), "foo")
    );
}

#[test]
fn expand_right() {
    let suggestion = Suggestion::new(StringRange::at(0), "minecraft:");
    assert_eq!(
        suggestion.expand("fish", StringRange::between(0, 4)),
        Suggestion::new(StringRange::between(0, 4), "minecraft:fish")
    );
}

#[test]
fn expand_both() {
    let suggestion = Suggestion::new(StringRange::at(11), "minecraft:");
    assert_eq!(
        suggestion.expand("give Steve fish_block", StringRange::between(5, 21)),
        Suggestion::new(StringRange::between(5, 21), "Steve minecraft:fish_block")
    );
}

#[test]
fn expand_replacement() {
    let suggestion = Suggestion::new(StringRange::between(6, 11), "strangers");
    assert_eq!(
        suggestion.expand("Hello world!", StringRange::between(0, 12)),
        Suggestion::new(StringRange::between(0, 12), "Hello strangers!")
    );
}

// ranges are character units, so these splice multi-byte characters safely

#[test]
fn apply_replacement_with_cyrillic() {
    let suggestion = Suggestion::new(StringRange::between(7, 10), "миру");
    assert_eq!(suggestion.apply("привет мир!"), "привет миру!");
}

#[test]
fn expand_with_emoji() {
    // 😀 is one char index here (two UTF-16 units in Java)
    let suggestion = Suggestion::new(StringRange::at(2), "x");
    assert_eq!(
        suggestion.expand("😀", StringRange::between(0, 2)),
        Suggestion::new(StringRange::between(0, 2), "😀x")
    );
}

// tooltips are `Message`s now, like Java's

#[test]
fn tooltip_round_trips_through_the_message() {
    let suggestion = Suggestion::new_with_tooltip(
        StringRange::between(0, 3),
        "foo",
        Arc::new(LiteralMessage::new("a tooltip")),
    );
    assert_eq!(suggestion.tooltip().unwrap().get_string(), "a tooltip");
    assert!(suggestion.tooltip().is_some());
    assert!(
        Suggestion::new(StringRange::between(0, 3), "foo")
            .tooltip()
            .is_none()
    );
}

#[test]
fn tooltips_participate_in_equality() {
    let plain = Suggestion::new(StringRange::between(0, 3), "foo");
    let tipped = Suggestion::new_with_tooltip(
        StringRange::between(0, 3),
        "foo",
        Arc::new(LiteralMessage::new("tip")),
    );
    let same_tip = Suggestion::new_with_tooltip(
        StringRange::between(0, 3),
        "foo",
        Arc::new(LiteralMessage::new("tip")),
    );
    let other_tip = Suggestion::new_with_tooltip(
        StringRange::between(0, 3),
        "foo",
        Arc::new(LiteralMessage::new("other")),
    );
    assert_ne!(plain, tipped);
    assert_eq!(tipped, same_tip);
    assert_ne!(tipped, other_tip);
}

// IntegerSuggestion is a real type now, with numeric ordering

#[test]
fn integer_suggestion_keeps_its_value() {
    let suggestion = IntegerSuggestion::new(StringRange::between(0, 1), 42);
    assert_eq!(suggestion.value(), 42);
    let suggestion = Suggestion::from(suggestion);
    assert_eq!(suggestion.text(), "42");
    assert_eq!(suggestion.value(), Some(42));
    assert!(suggestion.is_integer());
    assert!(suggestion.as_integer().is_some());
    assert!(suggestion.as_text().is_none());
}

#[test]
fn integer_suggestion_apply() {
    let suggestion = IntegerSuggestion::new(StringRange::at(5), 13);
    assert_eq!(suggestion.apply("Hello"), "Hello13");
}

#[test]
fn compare_to_is_the_java_text_order() {
    let text = |s: &str| Suggestion::new(StringRange::at(0), s);
    assert_eq!(text("a").compare_to(&text("b")), std::cmp::Ordering::Less);
    assert_eq!(
        text("b").compare_to(&text("a")),
        std::cmp::Ordering::Greater
    );
    assert_eq!(text("a").compare_to(&text("a")), std::cmp::Ordering::Equal);
    // case-sensitive: uppercase sorts before lowercase
    assert_eq!(text("B").compare_to(&text("a")), std::cmp::Ordering::Less);
    // ignore-case folds, and equal-folded texts tie
    assert_eq!(
        text("bar").compare_to_ignore_case(&text("Bar")),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        text("Bar").compare_to_ignore_case(&text("bar")),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        text("a").compare_to_ignore_case(&text("A")),
        std::cmp::Ordering::Equal
    );
    assert_eq!(
        text("apple").compare_to_ignore_case(&text("banana")),
        std::cmp::Ordering::Less
    );
}

#[test]
fn integer_suggestions_compare_numerically() {
    let integer = |v: i32| Suggestion::from(IntegerSuggestion::new(StringRange::at(0), v));
    let text = |s: &str| Suggestion::new(StringRange::at(0), s);
    assert_eq!(
        integer(30).compare_to(&integer(4)),
        std::cmp::Ordering::Greater
    );
    assert_eq!(
        integer(4).compare_to(&integer(30)),
        std::cmp::Ordering::Less
    );
    // mixed: Java's IntegerSuggestion override compares the rendered text
    assert_eq!(integer(2).compare_to(&text("22")), std::cmp::Ordering::Less);
    // and its compareToIgnoreCase delegates to compareTo (case-sensitive),
    // while a text receiver folds case — Java's asymmetry, kept
    assert_eq!(
        integer(2).compare_to_ignore_case(&text("22")),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        text("22").compare_to_ignore_case(&integer(2)),
        std::cmp::Ordering::Greater
    );
}

#[test]
fn expand_of_integer_suggestion_becomes_text() {
    let suggestion = IntegerSuggestion::new(StringRange::at(1), 7);
    let widened = suggestion.expand("foo", StringRange::between(0, 1));
    assert!(!widened.is_integer());
    // "f" pulled in from the command + the integer's text
    assert_eq!(widened.text(), "f7");
    assert_eq!(widened.value(), None);
}
