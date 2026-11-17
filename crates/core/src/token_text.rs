//! Text with tokens in it, such as a prompt that mentions people and files. A token
//! counts as one character, so a caret offset means the same thing whether it
//! passes text or tokens. The whole value can be written as plain text and read
//! back.

use std::sync::atomic::{AtomicU64, Ordering};

/// Something set into text as one unit: a mention, a file, a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// What sort of thing it is, such as `mention` or `file`.
    pub kind: String,
    /// What it stands for, such as a user's handle or a file's path.
    pub value: String,
    /// What is shown for it.
    pub label: String,
    /// Tells this token from another with the same content. Empty until
    /// `normalize` gives it one.
    pub id: String,
}

impl Token {
    pub fn new(kind: &str, value: &str, label: &str) -> Self {
        Token {
            kind: kind.into(),
            value: value.into(),
            label: label.into(),
            id: String::new(),
        }
    }

    pub fn with_id(mut self, id: &str) -> Self {
        self.id = id.into();
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Token(Token),
}

impl From<&str> for Segment {
    fn from(text: &str) -> Self {
        Segment::Text(text.to_string())
    }
}

impl From<Token> for Segment {
    fn from(token: Token) -> Self {
        Segment::Token(token)
    }
}

/// The character a token reads as in `offset_text`.
pub const TOKEN_CHAR: char = '\u{FFFC}';

static LAST_ID: AtomicU64 = AtomicU64::new(0);

fn next_id() -> String {
    format!("t{}", LAST_ID.fetch_add(1, Ordering::Relaxed) + 1)
}

/// Keeps generated ids clear of one a token came with.
fn reserve(id: &str) {
    let digits = id.strip_prefix('t').filter(|digits| {
        !digits.is_empty() && digits.len() <= 15 && digits.bytes().all(|b| b.is_ascii_digit())
    });
    if let Some(number) = digits.and_then(|digits| digits.parse::<u64>().ok()) {
        LAST_ID.fetch_max(number, Ordering::Relaxed);
    }
}

/// The value in its one canonical form: neighbouring text joined, empty text
/// dropped, and every token with an id of its own. A repeated id stays with the
/// first token that has it.
pub fn normalize(value: Vec<Segment>) -> Vec<Segment> {
    for segment in &value {
        if let Segment::Token(token) = segment {
            reserve(&token.id);
        }
    }
    let mut ids: Vec<String> = Vec::new();
    let mut result: Vec<Segment> = Vec::new();
    for segment in value {
        match segment {
            Segment::Token(mut token) => {
                if token.id.is_empty() || ids.contains(&token.id) {
                    token.id = next_id();
                }
                ids.push(token.id.clone());
                result.push(Segment::Token(token));
            }
            Segment::Text(text) if text.is_empty() => {}
            Segment::Text(text) => match result.last_mut() {
                Some(Segment::Text(last)) => last.push_str(&text),
                _ => result.push(Segment::Text(text)),
            },
        }
    }
    result
}

fn length_of(segment: &Segment) -> usize {
    match segment {
        Segment::Text(text) => text.chars().count(),
        Segment::Token(_) => 1,
    }
}

/// How many offsets the value spans: one per character and one per token.
pub fn value_length(value: &[Segment]) -> usize {
    value.iter().map(length_of).sum()
}

/// The value as text with each token as one placeholder character, so text
/// functions can work on offsets.
pub fn offset_text(value: &[Segment]) -> String {
    value
        .iter()
        .map(|segment| match segment {
            Segment::Text(text) => text.clone(),
            Segment::Token(_) => TOKEN_CHAR.to_string(),
        })
        .collect()
}

/// The part of the value between two offsets. Tokens are kept whole.
pub fn slice_value(value: &[Segment], start: usize, end: usize) -> Vec<Segment> {
    let mut result = Vec::new();
    let mut at = 0;
    for segment in value {
        let length = length_of(segment);
        let from = start.saturating_sub(at);
        let to = end.saturating_sub(at).min(length);
        if from < to {
            result.push(match segment {
                Segment::Token(token) => Segment::Token(token.clone()),
                Segment::Text(text) => {
                    Segment::Text(text.chars().skip(from).take(to - from).collect())
                }
            });
        }
        at += length;
        if at >= end {
            break;
        }
    }
    result
}

/// The value with `start..end` replaced by `content`. A token inside the range
/// goes entirely.
pub fn replace_range(
    value: &[Segment],
    start: usize,
    end: usize,
    content: Vec<Segment>,
) -> Vec<Segment> {
    let mut next = slice_value(value, 0, start);
    next.extend(content);
    next.extend(slice_value(value, end, usize::MAX));
    normalize(next)
}

/// The token at an offset, if a token sits there.
pub fn token_at(value: &[Segment], offset: usize) -> Option<&Token> {
    let mut at = 0;
    for segment in value {
        if let (Segment::Token(token), true) = (segment, at == offset) {
            return Some(token);
        }
        at += length_of(segment);
    }
    None
}

pub fn offset_of_token(value: &[Segment], id: &str) -> Option<usize> {
    let mut at = 0;
    for segment in value {
        if matches!(segment, Segment::Token(token) if token.id == id) {
            return Some(at);
        }
        at += length_of(segment);
    }
    None
}

/// The tokens an edit took out, for telling the owner.
pub fn removed_tokens(before: &[Segment], after: &[Segment]) -> Vec<Token> {
    let tokens = |value: &[Segment]| -> Vec<Token> {
        value
            .iter()
            .filter_map(|segment| {
                if let Segment::Token(token) = segment {
                    Some(token.clone())
                } else {
                    None
                }
            })
            .collect()
    };
    let kept = tokens(after);
    tokens(before)
        .into_iter()
        .filter(|token| !kept.iter().any(|other| other.id == token.id))
        .collect()
}

/// Where deleting or moving by a word from `offset` ends. It stops at a token,
/// and takes just the token when one is next.
pub fn word_boundary(value: &[Segment], offset: usize, forward: bool) -> usize {
    let text: Vec<char> = offset_text(value).chars().collect();
    let char_at = |index: usize| {
        if forward {
            text.get(index).copied()
        } else {
            index.checked_sub(1).and_then(|at| text.get(at).copied())
        }
    };
    let inside = |index: usize| {
        if forward {
            index < text.len()
        } else {
            index > 0
        }
    };
    let step = |index: usize| if forward { index + 1 } else { index - 1 };
    let mut at = offset;
    if char_at(at) == Some(TOKEN_CHAR) {
        return step(at);
    }
    while inside(at) && char_at(at).is_some_and(|ch| ch.is_whitespace() && ch != '\n') {
        at = step(at);
    }
    if char_at(at) == Some(TOKEN_CHAR) && at != offset {
        return at;
    }
    while inside(at) && char_at(at).is_some_and(|ch| !ch.is_whitespace() && ch != TOKEN_CHAR) {
        at = step(at);
    }
    if at == offset && inside(at) {
        step(at)
    } else {
        at
    }
}

/// Where deleting or moving to the end of the line from `offset` ends. At the
/// edge of a line already, it takes the line break.
pub fn line_boundary(value: &[Segment], offset: usize, forward: bool) -> usize {
    let text: Vec<char> = offset_text(value).chars().collect();
    if !forward {
        let start = text[..offset.min(text.len())]
            .iter()
            .rposition(|ch| *ch == '\n')
            .map_or(0, |at| at + 1);
        return if start == offset && offset > 0 {
            offset - 1
        } else {
            start
        };
    }
    match text
        .iter()
        .skip(offset)
        .position(|ch| *ch == '\n')
        .map(|found| found + offset)
    {
        None => text.len(),
        Some(end) if end == offset => offset + 1,
        Some(end) => end,
    }
}

fn escaped(text: &str, special: &[char]) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if special.contains(&ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn unescaped(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => out.extend(chars.next()),
            other => out.push(other),
        }
    }
    out
}

fn trailing_slashes(text: &str) -> &str {
    &text[text.trim_end_matches('\\').len()..]
}

/// The value as plain text: text as it is, and each token as a link,
/// `[@label](kind:value)`. Text that would read as a link is escaped, so
/// `from_text` gives the same value back.
pub fn to_text(value: &[Segment]) -> String {
    let mut out = String::new();
    for (index, segment) in value.iter().enumerate() {
        match segment {
            Segment::Token(token) => {
                out.push_str(&format!(
                    "[@{}]({}:{})",
                    escaped(&token.label, &['\\', '[', ']']),
                    token.kind,
                    escaped(&token.value, &['\\', '(', ')'])
                ));
            }
            Segment::Text(text) => {
                let mut parts: Vec<&str> = text.split("[@").collect();
                let last = parts.pop().unwrap_or_default();
                for part in parts {
                    out.push_str(part);
                    out.push_str(trailing_slashes(part));
                    out.push_str("\\[@");
                }
                out.push_str(last);
                if matches!(value.get(index + 1), Some(Segment::Token(_))) {
                    out.push_str(trailing_slashes(last));
                }
            }
        }
    }
    out
}

/// The value as a person reads it: text, with each token as its label.
pub fn to_label(value: &[Segment]) -> String {
    value
        .iter()
        .map(|segment| match segment {
            Segment::Text(text) => text.as_str(),
            Segment::Token(token) => token.label.as_str(),
        })
        .collect()
}

/// Reads a link at `at`, which is at `[@`. Returns the token and where the link
/// ends.
fn link_at(text: &[char], at: usize) -> Option<(Token, usize)> {
    // Runs to an unescaped `stop`, refusing any unescaped character in `refused`.
    let run = |from: usize, stop: char, refused: &[char]| -> Option<(String, usize)> {
        let mut raw = String::new();
        let mut index = from;
        loop {
            match *text.get(index)? {
                '\\' => {
                    raw.push('\\');
                    raw.push(*text.get(index + 1)?);
                    index += 2;
                }
                ch if ch == stop => return Some((raw, index)),
                ch if refused.contains(&ch) => return None,
                ch => {
                    raw.push(ch);
                    index += 1;
                }
            }
        }
    };
    let (label, close) = run(at + 2, ']', &[])?;
    if text.get(close + 1) != Some(&'(') {
        return None;
    }
    let mut index = close + 2;
    let mut kind = String::new();
    while let Some(ch) = text
        .get(index)
        .filter(|ch| ch.is_alphanumeric() || **ch == '_' || **ch == '-')
    {
        kind.push(*ch);
        index += 1;
    }
    if kind.is_empty() || text.get(index) != Some(&':') {
        return None;
    }
    let (value, end) = run(index + 1, ')', &['('])?;
    Some((
        Token::new(&kind, &unescaped(&value), &unescaped(&label)),
        end + 1,
    ))
}

/// Reads a value written by `to_text`. Anything that is not a well-formed link
/// stays text.
pub fn from_text(text: &str) -> Vec<Segment> {
    let chars: Vec<char> = text.chars().collect();
    let mut value: Vec<Segment> = Vec::new();
    let mut plain = String::new();
    let mut index = 0;
    while index < chars.len() {
        let Some(bracket) = (index..chars.len().saturating_sub(1))
            .find(|at| chars[*at] == '[' && chars[*at + 1] == '@')
        else {
            break;
        };
        let slashes = chars[index..bracket]
            .iter()
            .rev()
            .take_while(|ch| **ch == '\\')
            .count();
        plain.extend(&chars[index..bracket - slashes]);
        plain.push_str(&"\\".repeat(slashes / 2));
        let link = if slashes % 2 == 1 {
            None
        } else {
            link_at(&chars, bracket)
        };
        match link {
            Some((token, end)) => {
                value.push(Segment::Text(std::mem::take(&mut plain)));
                value.push(Segment::Token(token));
                index = end;
            }
            None => {
                plain.push_str("[@");
                index = bracket + 2;
            }
        }
    }
    plain.extend(&chars[index.min(chars.len())..]);
    value.push(Segment::Text(plain));
    normalize(value)
}
