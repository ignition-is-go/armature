use armature_core::keys::Modifiers;
use armature_core::shortcut::Shortcut;
use leptos::prelude::*;

/// Runs `action` whenever the shortcut is pressed anywhere on the page.
///
/// It listens for as long as the calling component is mounted. `mod` is Cmd on Apple
/// platforms and Ctrl elsewhere. A shortcut that cannot be parsed never fires.
pub fn use_shortcut(shortcut: &str, action: impl Fn() + 'static) {
    let Some(shortcut) = Shortcut::parse(shortcut) else {
        return;
    };
    let listener = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.is_composing() {
            return;
        }
        let mods = Modifiers {
            shift: ev.shift_key(),
            ctrl: ev.ctrl_key() || ev.meta_key(),
            alt: ev.alt_key(),
        };
        // By what was typed, or by the physical key where the layout types
        // something else, as Option does on a Mac and other alphabets do.
        if shortcut.matches_typed(&ev.key(), &ev.code(), mods) {
            ev.prevent_default();
            action();
        }
    });
    on_cleanup(move || listener.remove());
}
