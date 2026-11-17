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
    pub fn parse(text: &str) -> Option<Shortcut> {
        let mut mods = Modifiers::default();
        let mut key = None;
        for part in text.split('+').map(|part| part.trim().to_lowercase()) {
            match part.as_str() {
                "mod" | "ctrl" | "cmd" => mods.ctrl = true,
                "shift" => mods.shift = true,
                "alt" | "option" => mods.alt = true,
                name if key.is_none() => key = Some(key_named(name)?),
                _ => return None,
            }
        }
        Some(Shortcut { key: key?, mods })
    }

    /// Whether a key press is this shortcut. Letters match in either case.
    pub fn matches(&self, key: Key, mods: Modifiers) -> bool {
        let same_key = match (self.key, key) {
            (Key::Char(a), Key::Char(b)) => a.to_lowercase().eq(b.to_lowercase()),
            (a, b) => a == b,
        };
        same_key && self.mods == mods
    }

    /// How to show the shortcut to a person on `platform`.
    pub fn format(&self, platform: Platform) -> String {
        let key = key_label(self.key);
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
    pub fn aria(&self, platform: Platform) -> String {
        let main = if platform == Platform::Mac {
            "Meta"
        } else {
            "Control"
        };
        self.joined(main, &key_label(self.key))
    }

    fn joined(&self, main: &str, key: &str) -> String {
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

fn key_named(name: &str) -> Option<Key> {
    Some(match name {
        "enter" | "return" => Key::Enter,
        "escape" | "esc" => Key::Escape,
        "space" => Key::Space,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "up" => Key::ArrowUp,
        "down" => Key::ArrowDown,
        "left" => Key::ArrowLeft,
        "right" => Key::ArrowRight,
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
