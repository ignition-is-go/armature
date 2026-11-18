#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Enter,
    Space,
    Escape,
    Tab,
    Backspace,
    /// Conventionally starts editing the focused item.
    F2,
    /// Conventionally moves between the panes or regions of a window.
    F6,
    Char(char),
}

/// What a key means on Apple platforms where it differs:
///
/// Cmd with the up or down arrow goes to the first or last item, and Ctrl+P and Ctrl+N
/// step up and down a list, as they do in every text field there. `None` leaves the key
/// as it is. An adapter on those platforms passes keys through this first.
#[must_use]
pub const fn apple_key(key: Key, ctrl: bool, cmd: bool, others: bool) -> Option<Key> {
    match (key, ctrl, cmd, others) {
        (Key::ArrowDown, false, true, false) => Some(Key::End),
        (Key::ArrowUp, false, true, false) => Some(Key::Home),
        (Key::Char('n' | 'N'), true, false, false) => Some(Key::ArrowDown),
        (Key::Char('p' | 'P'), true, false, false) => Some(Key::ArrowUp),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    /// Ctrl, or Cmd on macOS.
    pub ctrl: bool,
    pub alt: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Orientation {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Dir {
    #[default]
    Ltr,
    Rtl,
}
