//! Matching typed text against labels, for filtering and for highlighting the match.

use crate::convert::Cast;
use std::ops::Range;

/// The byte ranges of `text` that `query` matches, ignoring case.
///
/// A whole-substring match is preferred, at a word start if there is one. Otherwise the
/// query's characters are matched in order with gaps. `None` means no match.
#[must_use]
pub fn match_ranges(query: &str, text: &str) -> Option<Vec<Range<usize>>> {
    let query: Vec<char> = query.chars().collect();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let matched = substring(&query, &chars).or_else(|| subsequence(&query, &chars))?;

    let mut ranges: Vec<Range<usize>> = Vec::new();
    for index in matched {
        let (start, ch) = chars.get(index).copied().unwrap_or_default();
        let end = start.saturating_add(ch.len_utf8());
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    Some(ranges)
}

/// How well `text` matches `query`, from 1 to 1000; `None` is no match.
///
/// Case and accents are ignored. In order of preference: the whole text, a prefix, a
/// word that starts with the query, any substring, then the query's letters in order,
/// which score more at the start of a word or right after one another, so `gp` finds
/// `Git push`. Among matches of one kind, covering more of the text ranks higher, so
/// `set` puts `Set` above `Settings`.
#[must_use]
pub fn fuzzy_score(query: &str, text: &str) -> Option<u32> {
    let needle: Vec<char> = crate::fold::fold_text(query, None).chars().collect();
    if needle.is_empty() {
        return Some(1000);
    }
    let haystack: Vec<char> = crate::fold::fold_text(text, None).chars().collect();
    if haystack == needle {
        return Some(1000);
    }
    let starts_word = |index: usize| {
        index == 0
            || !haystack
                .get(index.saturating_sub(1))
                .copied()
                .unwrap_or_default()
                .is_alphanumeric()
    };
    let found_at = |from: usize| {
        (from..haystack
            .len()
            .saturating_sub(needle.len().saturating_sub(1)))
            .find(|at| haystack.get(*at..).unwrap_or_default().starts_with(&needle))
    };
    // Up to 90 more for a query that covers the whole text.
    let coverage: u32 = needle
        .len()
        .saturating_mul(90)
        .checked_div(haystack.len().max(1))
        .unwrap_or_default()
        .cast();
    match found_at(0) {
        Some(0) => return Some(coverage.saturating_add(900)),
        Some(first) => {
            let mut at = Some(first);
            while let Some(index) = at.filter(|index| !starts_word(*index)) {
                at = found_at(index.saturating_add(1));
            }
            return Some(coverage.saturating_add(if at.is_some() { 750 } else { 600 }));
        }
        None => {}
    }
    // The letters in order, anywhere. Spaces in the query only separate words.
    let (mut position, mut previous, mut bonus, mut letters) = (0, None, 0u32, 0u32);
    for ch in needle.iter().filter(|ch| **ch != ' ') {
        let found = (position..haystack.len())
            .find(|at| haystack.get(*at).copied().unwrap_or_default() == *ch)?;
        letters = letters.saturating_add(1);
        bonus = bonus.saturating_add((u32::from(starts_word(found))).saturating_add(u32::from(
            previous.is_some_and(|previous: usize| found == previous.saturating_add(1)),
        )));
        previous = Some(found);
        position = found.saturating_add(1);
    }
    (letters > 0).then(|| {
        ((bonus.saturating_mul(400))
            .checked_div(letters.saturating_mul(2))
            .unwrap_or_default())
        .saturating_add(100)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment<'a> {
    pub text: &'a str,
    pub matched: bool,
}

/// Splits `text` into matched and unmatched pieces, in order, for rendering.
#[must_use]
pub fn match_segments<'a>(text: &'a str, ranges: &[Range<usize>]) -> Vec<Segment<'a>> {
    let mut segments = Vec::new();
    let mut cursor = 0;
    for range in ranges {
        if range.start > cursor {
            segments.push(Segment {
                text: text.get(cursor..range.start).unwrap_or_default(),
                matched: false,
            });
        }
        segments.push(Segment {
            text: text.get(range.clone()).unwrap_or_default(),
            matched: true,
        });
        cursor = range.end;
    }
    if cursor < text.len() {
        segments.push(Segment {
            text: text.get(cursor..).unwrap_or_default(),
            matched: false,
        });
    }
    segments
}

fn same(a: char, b: char) -> bool {
    // Case and accents are no difference: `brule` finds `brûlé`.
    crate::fold::fold_char(a, None) == crate::fold::fold_char(b, None)
}

fn substring(query: &[char], chars: &[(usize, char)]) -> Option<Vec<usize>> {
    if query.len() > chars.len() {
        return None;
    }
    let starts = 0..=chars.len().saturating_sub(query.len());
    let matches_at = |start: &usize| {
        query
            .iter()
            .zip(chars.get(*start..).unwrap_or_default())
            .all(|(q, (_, c))| same(*q, *c))
    };
    let at_word = |start: &usize| {
        *start == 0
            || !chars
                .get(start.saturating_sub(1))
                .copied()
                .unwrap_or_default()
                .1
                .is_alphanumeric()
    };
    let start = starts
        .clone()
        .filter(matches_at)
        .find(at_word)
        .or_else(|| starts.clone().find(matches_at))?;
    Some((start..start.saturating_add(query.len())).collect())
}

fn subsequence(query: &[char], chars: &[(usize, char)]) -> Option<Vec<usize>> {
    let mut matched = Vec::with_capacity(query.len());
    let mut from = 0;
    for q in query {
        let offset = chars
            .get(from..)
            .unwrap_or_default()
            .iter()
            .position(|(_, c)| same(*q, *c))?;
        matched.push(from.saturating_add(offset));
        from = from.saturating_add(offset.saturating_add(1));
    }
    Some(matched)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "a test fails by panicking, and its arithmetic is on values it wrote itself"
)]
mod tests {
    use super::*;

    /// Each matched run as its byte offset and text.
    fn matched<'a>(query: &str, text: &'a str) -> Vec<(usize, &'a str)> {
        let ranges = match_ranges(query, text).unwrap();
        ranges.into_iter().map(|r| (r.start, &text[r])).collect()
    }

    #[test]
    fn a_whole_substring_beats_scattered_letters() {
        assert!(fuzzy_score("abc", "abcdef") > fuzzy_score("abc", "a_b_c"));
    }

    #[test]
    fn word_starts_beat_the_same_letters_mid_word() {
        assert!(fuzzy_score("of", "open file") > fuzzy_score("of", "oxford"));
        assert!(fuzzy_score("of", "open file") > fuzzy_score("of", "oxen safe"));
    }

    #[test]
    fn a_match_at_the_start_beats_one_in_the_middle() {
        assert!(fuzzy_score("set", "settings") > fuzzy_score("set", "reset"));
    }

    #[test]
    fn a_word_start_is_preferred_over_an_earlier_mid_word_match() {
        assert_eq!(matched("in", "main input"), [(5, "in")]);
    }

    #[test]
    fn letters_out_of_order_do_not_match() {
        assert_eq!(fuzzy_score("ba", "abc"), None);
        assert_eq!(match_ranges("xyz", "abc"), None);
    }

    #[test]
    fn matching_ignores_case_and_handles_multibyte_text() {
        assert_eq!(matched("É", "café"), [(3, "é")]);
    }

    #[test]
    fn scattered_matches_merge_into_runs() {
        assert_eq!(
            match_ranges("opfl", "open file"),
            Some(vec![0..2, 5..6, 7..8])
        );
    }

    #[test]
    fn segments_cover_the_whole_text_in_order() {
        let text = "open file";
        let segments = match_segments(text, &match_ranges("fi", text).unwrap());
        let rendered: Vec<_> = segments.iter().map(|s| (s.text, s.matched)).collect();
        assert_eq!(rendered, [("open ", false), ("fi", true), ("le", false)]);
    }

    #[test]
    fn an_empty_query_matches_everything_with_nothing_highlighted() {
        assert_eq!(match_ranges("", "abc"), Some(vec![]));
        assert_eq!(
            match_segments("abc", &[]),
            [Segment {
                text: "abc",
                matched: false
            }]
        );
    }
}
