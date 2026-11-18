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
        match self.accepts {
            Some(accepts) => accepts(ch),
            None => ch.is_alphanumeric() || is_mark(ch) || matches!(ch, '_' | '-' | '.'),
        }
    }
}

const fn is_mark(ch: char) -> bool {
    matches!(ch as u32, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0x3099..=0x309A | 0xFE20..=0xFE2F)
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
    let (mut words, mut previous) = (1, None);
    for ch in query.chars() {
        if ch == ' ' {
            if rule.spaces == Spaces::None || previous == Some(' ') {
                return false;
            }
            words += 1;
        } else if !rule.takes(ch) {
            return false;
        }
        previous = Some(ch);
    }
    match rule.spaces {
        Spaces::Words(most) => words - usize::from(previous == Some(' ')) <= most,
        _ => true,
    }
}

/// Whether a trigger at `index` of `text` sits where `boundary` allows. `offset`
/// is how many characters of the whole text come before `text`; what the last of
/// them is, is not known, so it is no boundary.
#[must_use]
pub fn is_at_boundary(text: &[char], index: usize, offset: usize, boundary: Boundary) -> bool {
    if boundary == Boundary::Anywhere || offset + index == 0 {
        return true;
    }
    if boundary == Boundary::Start || index == 0 {
        return false;
    }
    let before = text[index - 1];
    match boundary {
        Boundary::Line => before == '\n',
        _ => before.is_whitespace() || matches!(before, '(' | '[' | '{' | '"' | '\'' | '\u{FFFC}'),
    }
}

/// How much text before the caret can matter: the longest query and trigger, and
/// the character before them.
#[must_use]
pub fn look_behind(rules: &[TriggerRule]) -> usize {
    rules
        .iter()
        .map(|rule| rule.max_length + rule.trigger.chars().count())
        .max()
        .unwrap_or(0)
        + 1
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
    let caret = source.len();
    let limit = look_behind(rules);
    for index in (0..caret).rev().take_while(|index| caret - index <= limit) {
        for (at, rule) in rules.iter().enumerate() {
            let trigger: Vec<char> = rule.trigger.chars().collect();
            if trigger.is_empty() || !source[index..].starts_with(&trigger) {
                continue;
            }
            let query: String = raw[index + trigger.len()..].iter().collect();
            if is_valid_query(&normalize_trigger_text(&query), rule)
                && is_at_boundary(&source, index, offset, rule.boundary)
            {
                return Some(TriggerMatch {
                    rule: at,
                    trigger: rule.trigger.clone(),
                    start: offset + index,
                    end: offset + caret,
                    query,
                });
            }
        }
        let ch = source[index];
        let continues = rules.iter().any(|rule| {
            rule.trigger.contains(ch)
                || (ch == ' ' && rule.spaces != Spaces::None)
                || rule.takes(ch)
        });
        if !continues {
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
    let shift = |at: usize| (at + inserted).saturating_sub(to - from);
    let trigger_end = dismissed.start + dismissed.trigger.chars().count();
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
