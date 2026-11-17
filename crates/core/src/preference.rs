//! Choices a person makes once and expects to stick: a setting picked from a list,
//! the colour scheme, and where such choices are kept.

use std::collections::HashMap;

/// Somewhere to keep small strings between sessions. Adapters supply the real ones,
/// such as browser storage or a settings file.
pub trait Storage {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&mut self, key: &str, value: &str);
    fn remove(&mut self, key: &str);
}

/// Storage that lasts as long as the value does. Useful in tests and as a fallback.
#[derive(Clone, Debug, Default)]
pub struct MemoryStorage(HashMap<String, String>);

impl Storage for MemoryStorage {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get(key).cloned()
    }

    fn set(&mut self, key: &str, value: &str) {
        self.0.insert(key.to_string(), value.to_string());
    }

    fn remove(&mut self, key: &str) {
        self.0.remove(key);
    }
}

/// A choice from a fixed list of named options, kept in storage under `key`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preference {
    key: String,
    options: Vec<String>,
    value: String,
}

impl Preference {
    /// Starts from the stored choice if it is still one of `options`, else from
    /// `fallback`.
    pub fn load(key: &str, options: &[&str], fallback: &str, storage: &dyn Storage) -> Self {
        let options: Vec<String> = options.iter().map(|option| option.to_string()).collect();
        let stored = storage.get(key).filter(|value| options.contains(value));
        Preference {
            key: key.to_string(),
            options,
            value: stored.unwrap_or_else(|| fallback.to_string()),
        }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    /// Chooses an option and stores it. Returns whether the choice changed. Anything
    /// not in the list is ignored.
    pub fn choose(&mut self, value: &str, storage: &mut dyn Storage) -> bool {
        if value == self.value || !self.options.iter().any(|option| option == value) {
            return false;
        }
        self.value = value.to_string();
        storage.set(&self.key, value);
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorScheme {
    Light,
    Dark,
    /// Follow the operating system.
    System,
}

impl ColorScheme {
    pub const OPTIONS: [&'static str; 3] = ["light", "dark", "system"];

    pub fn parse(name: &str) -> Option<ColorScheme> {
        Some(match name {
            "light" => ColorScheme::Light,
            "dark" => ColorScheme::Dark,
            "system" => ColorScheme::System,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            ColorScheme::Light => "light",
            ColorScheme::Dark => "dark",
            ColorScheme::System => "system",
        }
    }

    /// Whether to render dark, given what the operating system currently prefers.
    pub fn is_dark(self, system_prefers_dark: bool) -> bool {
        match self {
            ColorScheme::Light => false,
            ColorScheme::Dark => true,
            ColorScheme::System => system_prefers_dark,
        }
    }
}

/// What the person is pointing with, which decides hit-target sizes and whether hover
/// exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Mouse,
    Touch,
    Pen,
}

impl PointerKind {
    pub fn hovers(self) -> bool {
        self == PointerKind::Mouse
    }

    /// The smallest comfortable target, in logical pixels.
    pub fn min_target(self) -> f64 {
        if self == PointerKind::Touch {
            44.0
        } else {
            24.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_is_stored_and_found_again() {
        let mut storage = MemoryStorage::default();
        let mut density = Preference::load("density", &["cozy", "compact"], "cozy", &storage);
        assert!(density.choose("compact", &mut storage));
        assert!(
            !density.choose("compact", &mut storage),
            "choosing it again changes nothing"
        );
        let reloaded = Preference::load("density", &["cozy", "compact"], "cozy", &storage);
        assert_eq!(reloaded.value(), "compact");
    }

    #[test]
    fn anything_outside_the_options_is_ignored() {
        let mut storage = MemoryStorage::default();
        storage.set("density", "gigantic");
        let mut density = Preference::load("density", &["cozy", "compact"], "cozy", &storage);
        assert_eq!(
            density.value(),
            "cozy",
            "a stored value that is no longer offered"
        );
        assert!(!density.choose("gigantic", &mut storage));
    }

    #[test]
    fn the_system_scheme_follows_the_operating_system() {
        assert!(ColorScheme::System.is_dark(true));
        assert!(!ColorScheme::System.is_dark(false));
        assert!(ColorScheme::Dark.is_dark(false));
        for name in ColorScheme::OPTIONS {
            assert_eq!(ColorScheme::parse(name).unwrap().name(), name);
        }
    }

    #[test]
    fn only_a_mouse_hovers_and_touch_needs_larger_targets() {
        assert!(PointerKind::Mouse.hovers());
        assert!(!PointerKind::Touch.hovers());
        assert!(PointerKind::Touch.min_target() > PointerKind::Pen.min_target());
    }
}
