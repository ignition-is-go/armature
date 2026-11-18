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
        let options: Vec<String> = options
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        let stored = storage.get(key).filter(|value| options.contains(value));
        Self {
            key: key.to_string(),
            options,
            value: stored.unwrap_or_else(|| fallback.to_string()),
        }
    }

    #[must_use]
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

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "light" => Self::Light,
            "dark" => Self::Dark,
            "system" => Self::System,
            _ => return None,
        })
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    /// Whether to render dark, given what the operating system currently prefers.
    #[must_use]
    pub const fn is_dark(self, system_prefers_dark: bool) -> bool {
        match self {
            Self::Light => false,
            Self::Dark => true,
            Self::System => system_prefers_dark,
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
    #[must_use]
    pub fn hovers(self) -> bool {
        self == Self::Mouse
    }

    /// The smallest comfortable target, in logical pixels.
    #[must_use]
    pub fn min_target(self) -> f64 {
        if self == Self::Touch { 44.0 } else { 24.0 }
    }
}

/// How the person is driving the interface at the moment. Focus is shown boldly
/// for the keyboard, and quietly or not at all for a pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Modality {
    #[default]
    Pointer,
    Keyboard,
}

/// A stored choice to apply to the page as it loads.
#[derive(Clone, Copy, Debug)]
pub struct StoredAttribute<'a> {
    /// The key the choice is stored under.
    pub key: &'a str,
    /// The attribute on the root element that carries it.
    pub attribute: &'a str,
    pub fallback: &'a str,
    /// The values allowed. Anything else stored is ignored.
    pub options: &'a [&'a str],
}

/// A script for a web page's `<head>` that puts stored choices on the root element
/// before anything is painted, so the page never flashes the default first. It reads
/// `localStorage`, then a cookie of the same name. A colour scheme stored as
/// `system` is resolved to what the operating system prefers.
#[must_use]
pub fn preferences_script(choices: &[StoredAttribute]) -> String {
    // Keys and values go through a JSON-style quote so nothing in them can end the string.
    let quote = |text: &str| -> String {
        let mut quoted = String::from("\"");
        for ch in text.chars() {
            match ch {
                '"' | '\\' => {
                    quoted.push('\\');
                    quoted.push(ch);
                }
                '<' => quoted.push_str("\\u003c"),
                '\n' => quoted.push_str("\\n"),
                other => quoted.push(other),
            }
        }
        quoted.push('"');
        quoted
    };
    let entries: Vec<String> = choices
        .iter()
        .map(|choice| {
            let options: Vec<String> = choice.options.iter().map(|option| quote(option)).collect();
            format!(
                "[{},{},{},[{}]]",
                quote(choice.key),
                quote(choice.attribute),
                quote(choice.fallback),
                options.join(",")
            )
        })
        .collect();
    format!(
        "(function(){{var r=document.documentElement;[{}].forEach(function(c){{var v=null;\
try{{v=localStorage.getItem(c[0])}}catch(e){{}}\
if(v===null){{var m=document.cookie.match(new RegExp(\"(?:^|; )\"+c[0]+\"=([^;]*)\"));if(m)v=decodeURIComponent(m[1])}}\
if(c[3].indexOf(v)<0)v=c[2];\
if(v===\"system\")v=matchMedia(\"(prefers-color-scheme: dark)\").matches?\"dark\":\"light\";\
r.setAttribute(c[1],v)}})}})();",
        entries.join(",")
    )
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

    #[test]
    fn the_script_names_each_choice_and_cannot_be_broken_out_of() {
        let script = preferences_script(&[StoredAttribute {
            key: "theme",
            attribute: "data-theme",
            fallback: "system",
            options: &ColorScheme::OPTIONS,
        }]);
        assert!(script.contains(r#"["theme","data-theme","system",["light","dark","system"]]"#));
        assert!(script.starts_with("(function(){") && script.ends_with("})();"));
        let hostile = preferences_script(&[StoredAttribute {
            key: "a\"</script>",
            attribute: "x",
            fallback: "y",
            options: &[],
        }]);
        assert!(!hostile.contains("</script>"));
        assert!(hostile.contains(r#""a\"\u003c/script>""#));
    }

    #[test]
    fn the_web_demo_carries_the_generated_script() {
        let script = preferences_script(&[StoredAttribute {
            key: "armature-theme",
            attribute: "data-theme",
            fallback: "system",
            options: &ColorScheme::OPTIONS,
        }]);
        let page = include_str!("../../../examples/leptos-demo/index.html");
        assert!(
            page.contains(&script),
            "index.html should contain:\n{script}"
        );
    }
}
