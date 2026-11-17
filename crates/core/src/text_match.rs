//! Matching typed text against labels, for filtering and for highlighting the match.

use std::ops::Range;

/// The byte ranges of `text` that `query` matches, ignoring case. A whole-substring
/// match is preferred, at a word start if there is one. Otherwise the query's
/// characters are matched in order with gaps. `None` means no match.
pub fn match_ranges(query: &str, text: &str) -> Option<Vec<Range<usize>>> {
    let query: Vec<char> = query.chars().collect();
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let matched = substring(&query, &chars).or_else(|| subsequence(&query, &chars))?;

    let mut ranges: Vec<Range<usize>> = Vec::new();
    for index in matched {
        let (start, ch) = chars[index];
        let end = start + ch.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    Some(ranges)
}

/// How well `text` matches `query`; higher is better and `None` is no match. Runs of
/// consecutive characters, word starts and a match at the very start all score up.
pub fn fuzzy_score(query: &str, text: &str) -> Option<u32> {
    let ranges = match_ranges(query, text)?;
    let mut score = 0u32;
    for range in &ranges {
        let length = text[range.clone()].chars().count() as u32;
        // A run is worth more than the same letters at separate word starts, so a
        // contiguous match always outranks an acronym-style one.
        score += length + 10 * (length - 1);
        let before = text[..range.start].chars().next_back();
        score += match before {
            None => 8,
            Some(ch) if !ch.is_alphanumeric() => 6,
            Some(_) => 0,
        };
    }
    let lead = ranges
        .first()
        .map_or(0, |first| text[..first.start].chars().count() as u32);
    Some(score.saturating_sub(lead.min(5)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment<'a> {
    pub text: &'a str,
    pub matched: bool,
}

/// Splits `text` into matched and unmatched pieces, in order, for rendering.
pub fn match_segments<'a>(text: &'a str, ranges: &[Range<usize>]) -> Vec<Segment<'a>> {
    let mut segments = Vec::new();
    let mut cursor = 0;
    for range in ranges {
        if range.start > cursor {
            segments.push(Segment {
                text: &text[cursor..range.start],
                matched: false,
            });
        }
        segments.push(Segment {
            text: &text[range.clone()],
            matched: true,
        });
        cursor = range.end;
    }
    if cursor < text.len() {
        segments.push(Segment {
            text: &text[cursor..],
            matched: false,
        });
    }
    segments
}

fn same(a: char, b: char) -> bool {
    a.to_lowercase().eq(b.to_lowercase())
}

fn substring(query: &[char], chars: &[(usize, char)]) -> Option<Vec<usize>> {
    if query.len() > chars.len() {
        return None;
    }
    let starts = 0..=chars.len() - query.len();
    let matches_at = |start: &usize| {
        query
            .iter()
            .zip(&chars[*start..])
            .all(|(q, (_, c))| same(*q, *c))
    };
    let at_word = |start: &usize| *start == 0 || !chars[start - 1].1.is_alphanumeric();
    let start = starts
        .clone()
        .filter(matches_at)
        .find(at_word)
        .or_else(|| starts.clone().find(matches_at))?;
    Some((start..start + query.len()).collect())
}

fn subsequence(query: &[char], chars: &[(usize, char)]) -> Option<Vec<usize>> {
    let mut matched = Vec::with_capacity(query.len());
    let mut from = 0;
    for q in query {
        let offset = chars[from..].iter().position(|(_, c)| same(*q, *c))?;
        matched.push(from + offset);
        from += offset + 1;
    }
    Some(matched)
}

#[cfg(test)]
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
        assert!(fuzzy_score("of", "open file") > fuzzy_score("of", "proof"));
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
