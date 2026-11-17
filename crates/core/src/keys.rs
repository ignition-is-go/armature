#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    Enter,
    Space,
    Escape,
    Tab,
    Char(char),
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
