//! Byte-exact reproduction of the Java collection machinery behind
//! `Suggestions::create`.
//!
//! Java sorts the result of `new ArrayList<>(new HashSet<>())` — the list
//! order is the HashSet's **iteration order**, and for fewer than
//! `TimSort.MIN_MERGE` (32) elements Java's sort is its small-array path
//! (initial run detection + binary insertion sort). Both steps affect the
//! final order, and the mixed text/integer comparator
//! (`Suggestion#compareToIgnoreCase`) is not even transitive, so "just sort
//! it" does not reproduce Java's output for `SuggestionsBuilderTest#sort_mixed`.
//!
//! The two helpers here were verified against the Java 1.3 sources running on
//! a local JVM: the emulated set order matches Java's `HashSet` iteration and
//! the emulated sort matches `List.sort` output for every Java test case.

use std::{cmp::Ordering, collections::HashSet};

use super::Suggestion;

/// Java's `String.hashCode`: 31-based folding over UTF-16 code units
/// (wrapping `int` arithmetic).
pub(super) fn java_string_hash(s: &str) -> i32 {
    let mut hash: i32 = 0;
    for unit in s.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash
}

/// Java's `Objects.hash(a, b, ...)`: `Arrays.hashCode` over the boxed parts.
pub(super) fn java_objects_hash(parts: &[i32]) -> i32 {
    let mut hash: i32 = 1;
    for part in parts {
        hash = hash.wrapping_mul(31).wrapping_add(*part);
    }
    hash
}

/// Reproduce `new ArrayList<>(new HashSet<>())` for a collection of
/// suggestions: deduplicate (first occurrence wins, like `HashSet.add`)
/// keeping the Java table's iteration order.
///
/// The order model: a Java `HashMap` bucket index is
/// `spread(hashCode) & (capacity - 1)` with `spread(h) = h ^ (h >>> 16)`,
/// collisions keep insertion order, and resizes preserve within-bucket order
/// — which all adds up to "stable sort by final bucket index, ties by
/// insertion sequence". The final capacity grows from 16, doubling while the
/// element count exceeds 3/4 of it.
pub(super) fn java_set_order(items: impl IntoIterator<Item = Suggestion>) -> Vec<Suggestion> {
    let mut seen = HashSet::new();
    let mut unique: Vec<Suggestion> = Vec::new();
    for item in items {
        if seen.insert(item.clone()) {
            unique.push(item);
        }
    }
    let count = unique.len();
    if count < 2 {
        return unique;
    }

    let mut capacity = 16usize;
    while count > capacity / 4 * 3 {
        capacity *= 2;
    }
    let mut order: Vec<usize> = (0..count).collect();
    let buckets: Vec<usize> = unique
        .iter()
        .map(|suggestion| {
            let hash = suggestion.java_hash_code() as u32;
            // Java's spread function; the shift is a logical one on u32
            let spread = hash ^ (hash >> 16);
            (spread as usize) & (capacity - 1)
        })
        .collect();
    order.sort_by_key(|&index| buckets[index]);
    order
        .into_iter()
        .map(|index| unique[index].clone())
        .collect()
}

/// Java's `List.sort((a, b) -> a.compareToIgnoreCase(b))` for the input
/// sequence the HashSet emulation produced.
///
/// Below `TimSort.MIN_MERGE` (32) this is a byte-exact port of Java's
/// small-array path (`countRunAndMakeAscending` + `binarySort`), which is
/// what decides the final order for the non-transitive mixed comparator.
/// From 32 elements up Java switches to full run-merging TimSort; a stable
/// sort is used instead, which produces identical output for consistent
/// comparators and may differ only for pathological mixed input larger than
/// the Java tests ever exercise.
pub(super) fn java_sort_ignore_case(list: &mut [Suggestion]) {
    let count = list.len();
    if count < 2 {
        return;
    }
    if count >= 32 {
        list.sort_by(|a, b| a.compare_to_ignore_case(b));
        return;
    }

    // TimSort.countRunAndMakeAscending
    let mut run_hi = 1;
    if list[1].compare_to_ignore_case(&list[0]) == Ordering::Less {
        while run_hi < count
            && list[run_hi].compare_to_ignore_case(&list[run_hi - 1]) == Ordering::Less
        {
            run_hi += 1;
        }
        list[..run_hi].reverse();
    } else {
        while run_hi < count
            && list[run_hi].compare_to_ignore_case(&list[run_hi - 1]) != Ordering::Less
        {
            run_hi += 1;
        }
    }

    // TimSort.binarySort over the remaining elements
    for start in run_hi..count {
        let (mut left, mut right) = (0usize, start);
        while left < right {
            let mid = (left + right) / 2;
            if list[start].compare_to_ignore_case(&list[mid]) == Ordering::Less {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        // rotate the pivot (currently the last slot) into place — the same
        // result as Java's arraycopy shift, without needing Copy
        list[left..=start].rotate_right(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{context::StringRange, suggestion::IntegerSuggestion};

    /// The `SuggestionsTest#merge_multiple` pipeline, computed step by step
    /// the way Java does, with the orders observed on a real JVM.
    #[test]
    fn set_order_matches_java_for_merge_multiple() {
        let originals = [
            Suggestion::new(StringRange::at(5), "ar"),
            Suggestion::new(StringRange::at(5), "az"),
            Suggestion::new(StringRange::at(5), "Az"),
            Suggestion::new(StringRange::between(4, 5), "foo"),
            Suggestion::new(StringRange::between(4, 5), "qux"),
            Suggestion::new(StringRange::between(4, 5), "apple"),
            Suggestion::new(StringRange::between(4, 5), "Bar"),
        ];
        let set1 = java_set_order(originals);
        let texts: Vec<&str> = set1.iter().map(|s| s.text()).collect();
        // observed on Java 21 with the 1.3 sources
        assert_eq!(texts, ["Bar", "apple", "az", "Az", "qux", "foo", "ar"]);
    }

    #[test]
    fn sort_matches_java_for_sort_mixed() {
        let mut list: Vec<Suggestion> = ["11", "22", "33", "a", "b", "c"]
            .iter()
            .map(|s| Suggestion::new(StringRange::between(6, 7), *s))
            .collect();
        for value in [2, 4, 6, 8, 30, 32] {
            list.push(Suggestion::Integer(IntegerSuggestion::new(
                StringRange::between(6, 7),
                value,
            )));
        }
        list.push(Suggestion::new(StringRange::between(6, 7), "3a"));
        list.push(Suggestion::new(StringRange::between(6, 7), "a3"));
        let ordered = java_set_order(list);
        let texts_before: Vec<&str> = ordered.iter().map(|s| s.text()).collect();
        // observed on Java 21 with the 1.3 sources
        assert_eq!(
            texts_before,
            [
                "3a", "4", "6", "30", "32", "b", "c", "2", "a", "11", "22", "33", "8", "a3"
            ]
        );
        let mut sorted = ordered;
        java_sort_ignore_case(&mut sorted);
        let texts: Vec<&str> = sorted.iter().map(|s| s.text()).collect();
        assert_eq!(
            texts,
            [
                "11", "2", "22", "33", "3a", "4", "6", "8", "30", "32", "a", "a3", "b", "c"
            ]
        );
    }
}
