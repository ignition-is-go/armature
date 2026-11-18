//! Finding a trigger such as `@` or `/` in the text before the caret, and the query
//! typed after it. Offsets count characters.

/// Where a trigger may sit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Boundary {
    /// Only at the very start of the text.
    Start,
    /// At the start of any line.
    Line,
    /// After whitespace or an opening bracket or quote, so not in mid-word.
    #[default]
    Word,
    Anywhere,
}

/// Whether a query may hold spaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spaces {
    #[default]
    None,
    /// Single spaces between any number of words.
    Any,
    /// At most this many words.
    Words(usize),
}

/// One trigger and the queries it accepts.
#[derive(Clone, Debug)]
pub struct TriggerRule {
    /// The text that starts a match: `@`, `/`, `[[`.
    pub trigger: String,
    pub boundary: Boundary,
    pub spaces: Spaces,
    /// Which characters a query may hold besides spaces. Letters, digits, marks and
    /// `_-.` when not given.
    pub accepts: Option<fn(char) -> bool>,
    pub max_length: usize,
}

impl TriggerRule {
    #[must_use]
    pub fn new(trigger: &str) -> Self {
        Self {
            trigger: trigger.to_string(),
            boundary: Boundary::Word,
            spaces: Spaces::None,
            accepts: None,
            max_length: 50,
        }
    }

    fn takes(&self, ch: char) -> bool {
        self.accepts.map_or_else(
            || ch.is_alphanumeric() || is_mark(ch) || matches!(ch, '_' | '-' | '.'),
            |accepts| accepts(ch),
        )
    }
}

const fn is_mark(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{3099}'..='\u{309A}'
            | '\u{FE20}'..='\u{FE2F}'
    )
}

/// A trigger found before the caret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerMatch {
    /// Which rule matched, by its place in the list.
    pub rule: usize,
    pub trigger: String,
    /// Where the trigger starts and where the query ends, in characters.
    pub start: usize,
    pub end: usize,
    pub query: String,
}

/// Full-width triggers an input method commits, as their plain forms. One
/// character becomes one, so offsets stay put.
#[must_use]
pub fn normalize_trigger_text(text: &str) -> String {
    text.chars()
        .map(|ch| match ch {
            '＠' => '@',
            '＃' => '#',
            '／' => '/',
            '＋' => '+',
            '：' => ':',
            '［' => '[',
            other => other,
        })
        .collect()
}

/// Whether text typed after a trigger is still a query for `rule`.
pub fn is_valid_query(query: &str, rule: &TriggerRule) -> bool {
    if query.chars().count() > rule.max_length
        || query.starts_with(char::is_whitespace)
        || query.contains('\n')
    {
        return false;
    }
    let (mut words, mut previous) = (1_usize, None);
    for ch in query.chars() {
        if ch == ' ' {
            if rule.spaces == Spaces::None || previous == Some(' ') {
                return false;
            }
            words = words.saturating_add(1);
        } else if !rule.takes(ch) {
            return false;
        }
        previous = Some(ch);
    }
    match rule.spaces {
        Spaces::Words(most) => words.saturating_sub(usize::from(previous == Some(' '))) <= most,
        _ => true,
    }
}

/// Whether a trigger at `index` of `text` sits where `boundary` allows. `offset`
/// is how many characters of the whole text come before `text`; what the last of
/// them is, is not known, so it is no boundary.
#[must_use]
pub fn is_at_boundary(text: &[char], index: usize, offset: usize, boundary: Boundary) -> bool {
    if boundary == Boundary::Anywhere || offset.saturating_add(index) == 0 {
        return true;
    }
    if boundary == Boundary::Start || index == 0 {
        return false;
    }
    let before = text
        .get(index.saturating_sub(1))
        .copied()
        .unwrap_or_default();
    match boundary {
        Boundary::Line => before == '\n',
        _ => before.is_whitespace() || matches!(before, '(' | '[' | '{' | '"' | '\'' | '\u{FFFC}'),
    }
}

/// How much text before the caret can matter: the longest query and trigger, and
/// the character before them.
#[must_use]
pub fn look_behind(rules: &[TriggerRule]) -> usize {
    (rules
        .iter()
        .map(|rule| rule.max_length.saturating_add(rule.trigger.chars().count()))
        .max()
        .unwrap_or(0))
    .saturating_add(1)
}

/// The match that starts at `index`: a rule's trigger sitting there, where that rule
/// allows one, with a query it accepts running up to the caret.
fn match_starting_at(
    raw: &[char],
    source: &[char],
    index: usize,
    offset: usize,
    rules: &[TriggerRule],
) -> Option<TriggerMatch> {
    let from_here = source.get(index..).unwrap_or_default();
    rules.iter().enumerate().find_map(|(at, rule)| {
        let trigger: Vec<char> = rule.trigger.chars().collect();
        if trigger.is_empty() || !from_here.starts_with(&trigger) {
            return None;
        }
        let after_trigger = index.saturating_add(trigger.len());
        let query: String = raw
            .get(after_trigger..)
            .unwrap_or_default()
            .iter()
            .collect();
        let accepted = is_valid_query(&normalize_trigger_text(&query), rule)
            && is_at_boundary(source, index, offset, rule.boundary);
        accepted.then(|| TriggerMatch {
            rule: at,
            trigger: rule.trigger.clone(),
            start: offset.saturating_add(index),
            end: offset.saturating_add(source.len()),
            query,
        })
    })
}

/// Whether a character could be part of some rule's trigger or query. Once one
/// could not, nothing further back can reach the caret.
fn may_belong_to_a_match(rules: &[TriggerRule], ch: char) -> bool {
    rules.iter().any(|rule| {
        rule.trigger.contains(ch) || (ch == ' ' && rule.spaces != Spaces::None) || rule.takes(ch)
    })
}

/// The nearest trigger before the caret whose query runs up to it. `text` is the
/// text that ends at the caret, and `offset` how many characters come before it.
#[must_use]
pub fn find_trigger_match(
    text: &str,
    offset: usize,
    rules: &[TriggerRule],
) -> Option<TriggerMatch> {
    let raw: Vec<char> = text.chars().collect();
    let source: Vec<char> = normalize_trigger_text(text).chars().collect();
    // Looking back from the caret, as far as a match could start.
    for index in (0..source.len()).rev().take(look_behind(rules)) {
        if let Some(found) = match_starting_at(&raw, &source, index, offset, rules) {
            return Some(found);
        }
        let ch = source.get(index).copied().unwrap_or_default();
        if !may_belong_to_a_match(rules, ch) {
            break;
        }
    }
    None
}

/// A match the person dismissed, kept so the menu does not reopen on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dismissed {
    pub trigger: String,
    pub start: usize,
    pub end: usize,
}

/// Where a dismissed match is after an edit replaced `from..to` with `inserted`
/// characters. `None` once its trigger has been edited away.
#[must_use]
pub fn map_dismissed(
    dismissed: &Dismissed,
    from: usize,
    to: usize,
    inserted: usize,
) -> Option<Dismissed> {
    let removed = to.saturating_sub(from);
    let shift = |at: usize| at.saturating_add(inserted).saturating_sub(removed);
    let trigger_end = dismissed
        .start
        .saturating_add(dismissed.trigger.chars().count());
    if to <= dismissed.start {
        return Some(Dismissed {
            start: shift(dismissed.start),
            end: shift(dismissed.end),
            ..dismissed.clone()
        });
    }
    if from < trigger_end {
        return None;
    }
    if from <= dismissed.end {
        return Some(Dismissed {
            end: shift(dismissed.end).max(trigger_end),
            ..dismissed.clone()
        });
    }
    Some(dismissed.clone())
}
