//! Keyboard shortcuts written as text, such as `mod+shift+b`. `mod` is Ctrl, or Cmd on
//! macOS.

use crate::keys::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub key: Key,
    pub mods: Modifiers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Other,
}

impl Shortcut {
    /// Parses `mod+shift+b`, `alt+enter`, `escape` and the like. `None` if a part is
    /// not a known modifier or key.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut mods = Modifiers::default();
        let mut key = None;
        for part in text.split('+').map(|part| part.trim().to_lowercase()) {
            match part.as_str() {
                "mod" | "ctrl" | "control" | "cmd" | "command" | "meta" => mods.ctrl = true,
                "shift" => mods.shift = true,
                "alt" | "option" | "opt" => mods.alt = true,
                name if key.is_none() => key = Some(key_named(name)?),
                _ => return None,
            }
        }
        Some(Self { key: key?, mods })
    }

    /// Whether a key press is this shortcut. Letters match in either case.
    #[must_use]
    pub fn matches(&self, key: Key, mods: Modifiers) -> bool {
        let same_key = match (self.key, key) {
            (Key::Char(a), Key::Char(b)) => a.to_lowercase().eq(b.to_lowercase()),
            (a, b) => a == b,
        };
        same_key && self.mods == mods
    }

    /// Whether a key event is this shortcut, given the character it typed and the
    /// physical key's code (`KeyN`, `Digit1`). Letters and digits match by the
    /// physical key when the layout types something that is not a Latin letter or
    /// digit there: Option+N on a Mac types `˜`, Shift+1 types `!`, and other
    /// alphabets type their own letters. A layout that types a different Latin
    /// letter keeps its letters, so AZERTY's A is not QWERTY's.
    #[must_use]
    pub fn matches_typed(&self, typed: &str, code: &str, mods: Modifiers) -> bool {
        if self.mods != mods {
            return false;
        }
        let Key::Char(wanted) = self.key else {
            return key_from_name(typed).is_some_and(|key| key == self.key);
        };
        let wanted = wanted.to_ascii_lowercase();
        let mut chars = typed.chars();
        let typed = match (chars.next(), chars.next()) {
            (Some(one), None) => one.to_lowercase().next(),
            _ => None,
        };
        if typed == Some(wanted) {
            return true;
        }
        if !wanted.is_ascii_alphanumeric()
            || typed.is_some_and(|typed| typed.is_ascii_alphanumeric())
        {
            return false;
        }
        let physical = if wanted.is_ascii_digit() {
            format!("Digit{wanted}")
        } else {
            format!("Key{}", wanted.to_ascii_uppercase())
        };
        code == physical
    }

    /// `format`, wrapped so that a right-to-left layout never reorders it.
    #[must_use]
    pub fn display(&self, platform: Platform) -> String {
        format!("\u{2066}{}\u{2069}", self.format(platform))
    }

    /// How to show the shortcut to a person on `platform`.
    pub fn format(&self, platform: Platform) -> String {
        let key = match (platform, self.key) {
            (Platform::Mac, key) => mac_glyph(key).map_or_else(|| key_label(key), String::from),
            (Platform::Other, Key::Escape) => "Esc".to_string(),
            (Platform::Other, Key::ArrowUp) => "Up".to_string(),
            (Platform::Other, Key::ArrowDown) => "Down".to_string(),
            (Platform::Other, Key::ArrowLeft) => "Left".to_string(),
            (Platform::Other, Key::ArrowRight) => "Right".to_string(),
            (Platform::Other, key) => key_label(key),
        };
        match platform {
            Platform::Mac => {
                let mut out = String::new();
                for (held, symbol) in [
                    (self.mods.alt, '⌥'),
                    (self.mods.shift, '⇧'),
                    (self.mods.ctrl, '⌘'),
                ] {
                    if held {
                        out.push(symbol);
                    }
                }
                out + &key
            }
            Platform::Other => self.joined("Ctrl", &key),
        }
    }

    /// The value for an `aria-keyshortcuts` attribute.
    #[must_use]
    pub fn aria(&self, platform: Platform) -> String {
        let main = if platform == Platform::Mac {
            "Meta"
        } else {
            "Control"
        };
        self.joined(main, &key_label(self.key))
    }

    fn joined(self, main: &str, key: &str) -> String {
        let held = [
            (self.mods.ctrl, main),
            (self.mods.alt, "Alt"),
            (self.mods.shift, "Shift"),
        ];
        let mut parts: Vec<&str> = held
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| *name)
            .collect();
        parts.push(key);
        parts.join("+")
    }
}

/// Whether a shortcut may run.
///
/// `order` is the layer it belongs to: 0 for the page, and `None` for a popup that is
/// closed. `blocking` is the lowest layer that is modal, if any: only shortcuts at or
/// above it run.
#[must_use]
pub fn shortcut_allowed(order: Option<u32>, blocking: Option<u32>) -> bool {
    order.is_some_and(|order| blocking.is_none_or(|blocking| order >= blocking))
}

const fn mac_glyph(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::Backspace => "⌫",
        Key::Enter => "↩",
        Key::Escape => "⎋",
        Key::Tab => "⇥",
        Key::Space => "␣",
        Key::ArrowUp => "↑",
        Key::ArrowDown => "↓",
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        Key::PageUp => "⇞",
        Key::PageDown => "⇟",
        Key::Home => "↖",
        Key::End => "↘",
        _ => return None,
    })
}

/// A key from the name a platform gives it, such as `ArrowUp`, `Escape` or ` `.
fn key_from_name(name: &str) -> Option<Key> {
    Some(match name {
        " " => Key::Space,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        other => {
            let named = key_named(&other.to_lowercase())?;
            // A single character that is not a named key is typed text.
            match named {
                Key::Char(ch) => Key::Char(ch),
                key => key,
            }
        }
    })
}

fn key_named(name: &str) -> Option<Key> {
    Some(match name {
        "enter" | "return" => Key::Enter,
        "escape" | "esc" => Key::Escape,
        "space" => Key::Space,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "f2" => Key::F2,
        "f6" => Key::F6,
        "up" | "arrowup" => Key::ArrowUp,
        "down" | "arrowdown" => Key::ArrowDown,
        "left" | "arrowleft" => Key::ArrowLeft,
        "right" | "arrowright" => Key::ArrowRight,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(ch), None) => Key::Char(ch),
                _ => return None,
            }
        }
    })
}

fn key_label(key: Key) -> String {
    match key {
        Key::Enter => "Enter".into(),
        Key::Escape => "Escape".into(),
        Key::Space => "Space".into(),
        Key::Tab => "Tab".into(),
        Key::Backspace => "Backspace".into(),
        Key::F2 => "F2".into(),
        Key::F6 => "F6".into(),
        Key::ArrowUp => "ArrowUp".into(),
        Key::ArrowDown => "ArrowDown".into(),
        Key::ArrowLeft => "ArrowLeft".into(),
        Key::ArrowRight => "ArrowRight".into(),
        Key::Home => "Home".into(),
        Key::End => "End".into(),
        Key::PageUp => "PageUp".into(),
        Key::PageDown => "PageDown".into(),
        Key::Char(ch) => ch.to_uppercase().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOD_SHIFT: Modifiers = Modifiers {
        shift: true,
        ctrl: true,
        alt: false,
    };

    #[test]
    fn a_shortcut_matches_only_its_exact_modifiers() {
        let bold = Shortcut::parse("mod+shift+b").unwrap();
        assert!(bold.matches(Key::Char('B'), MOD_SHIFT));
        assert!(!bold.matches(
            Key::Char('b'),
            Modifiers {
                ctrl: true,
                ..Default::default()
            }
        ));
        assert!(!bold.matches(Key::Char('n'), MOD_SHIFT));
    }

    #[test]
    fn named_keys_and_any_part_order_parse() {
        let submit = Shortcut::parse("Enter + Alt").unwrap();
        assert!(submit.matches(
            Key::Enter,
            Modifiers {
                alt: true,
                ..Default::default()
            }
        ));
        assert!(
            Shortcut::parse("escape")
                .unwrap()
                .matches(Key::Escape, Modifiers::default())
        );
    }

    #[test]
    fn unknown_or_incomplete_shortcuts_do_not_parse() {
        assert_eq!(Shortcut::parse("mod+shift"), None);
        assert_eq!(Shortcut::parse("mod+banana"), None);
        assert_eq!(Shortcut::parse("a+b"), None);
    }

    #[test]
    fn a_shortcut_is_shown_in_the_platform_style() {
        let bold = Shortcut::parse("mod+shift+b").unwrap();
        assert_eq!(bold.format(Platform::Mac), "⇧⌘B");
        assert_eq!(bold.format(Platform::Other), "Ctrl+Shift+B");
        assert_eq!(bold.aria(Platform::Mac), "Meta+Shift+B");
        assert_eq!(bold.aria(Platform::Other), "Control+Shift+B");
    }
}
