const RESET_MS: u64 = 500;

/// A letter without its case or accent, so "Éditer" is found by typing "e".
fn fold(ch: char) -> char {
    let lower = ch.to_lowercase().next().unwrap_or(ch);
    match lower {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'è'..='ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ì'..='ï' | 'ī' | 'į' | 'ı' => 'i',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò'..='ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ù'..='ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'š' | 'ś' => 's',
        'ž' | 'ź' | 'ż' => 'z',
        other => other,
    }
}

fn folded(text: &str) -> String {
    text.trim().chars().map(fold).collect()
}

#[derive(Clone, Debug, Default)]
pub struct Typeahead {
    buffer: String,
    last_ms: u64,
}

impl Typeahead {
    /// Feeds one typed character and returns the index of the item to jump to.
    /// `items` is `(label, enabled)` in display order.
    pub fn push(
        &mut self,
        ch: char,
        now_ms: u64,
        items: &[(&str, bool)],
        current: Option<usize>,
    ) -> Option<usize> {
        if now_ms.saturating_sub(self.last_ms) > RESET_MS {
            self.buffer.clear();
        }
        self.last_ms = now_ms;
        self.buffer.push(fold(ch));

        let first = self.buffer.chars().next()?;
        let repeated = self.buffer.chars().all(|c| c == first);
        // Repeating one letter cycles through its matches; a longer prefix keeps the
        // current item if it still matches.
        let (needle, start) = if repeated {
            (first.to_string(), current.map_or(0, |c| c + 1))
        } else {
            (self.buffer.clone(), current.unwrap_or(0))
        };

        let n = items.len();
        (0..n).map(|offset| (start + offset) % n).find(|i| {
            let (label, enabled) = items[*i];
            enabled && folded(label).starts_with(&needle)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEMS: [(&str, bool); 4] = [
        ("Apple", true),
        ("Banana", true),
        ("Blueberry", true),
        ("Cherry", true),
    ];

    #[test]
    fn a_prefix_narrows_the_match() {
        let mut t = Typeahead::default();
        assert_eq!(t.push('b', 0, &ITEMS, None), Some(1));
        assert_eq!(t.push('l', 100, &ITEMS, Some(1)), Some(2));
    }

    #[test]
    fn repeating_a_letter_cycles() {
        let mut t = Typeahead::default();
        assert_eq!(t.push('b', 0, &ITEMS, None), Some(1));
        assert_eq!(t.push('b', 100, &ITEMS, Some(1)), Some(2));
        assert_eq!(t.push('b', 200, &ITEMS, Some(2)), Some(1));
    }

    #[test]
    fn the_buffer_resets_after_a_pause() {
        let mut t = Typeahead::default();
        t.push('b', 0, &ITEMS, None);
        assert_eq!(t.push('c', 1000, &ITEMS, Some(1)), Some(3));
    }

    #[test]
    fn accents_and_case_do_not_get_in_the_way() {
        let items = [("Open", true), ("  Éditer", true), ("Ødegaard", true)];
        assert_eq!(Typeahead::default().push('e', 0, &items, None), Some(1));
        assert_eq!(Typeahead::default().push('É', 0, &items, None), Some(1));
        assert_eq!(Typeahead::default().push('o', 0, &items, Some(0)), Some(2));
    }

    #[test]
    fn disabled_items_never_match() {
        let items = [("Apple", false), ("Avocado", true)];
        assert_eq!(Typeahead::default().push('a', 0, &items, None), Some(1));
    }
}
