use crate::keys::{Dir, Key, Modifiers, Orientation};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Next,
    Prev,
    First,
    Last,
}

/// Whether the held modifiers leave arrow keys to the platform: Ctrl, Cmd and Alt with
/// an arrow belong to the browser or the operating system.
#[must_use]
pub const fn leaves_arrows_alone(mods: Modifiers) -> bool {
    mods.ctrl || mods.alt
}

/// Whether a key press is an arrow that a list-like control should not act on.
pub fn is_platform_arrow<Part>(input: &crate::primitive::Input<Part>) -> bool {
    let arrow = |key: &Key| {
        matches!(
            key,
            Key::ArrowUp | Key::ArrowDown | Key::ArrowLeft | Key::ArrowRight
        )
    };
    matches!(input, crate::primitive::Input::Key { key, mods, .. } if arrow(key) && leaves_arrows_alone(*mods))
}

#[must_use]
pub const fn nav_for_key(key: Key, orientation: Orientation, dir: Dir) -> Option<Nav> {
    use Orientation::{Horizontal, Vertical};
    Some(match (key, orientation, dir) {
        (Key::Home, _, _) => Nav::First,
        (Key::End, _, _) => Nav::Last,
        (Key::ArrowDown, Vertical, _) => Nav::Next,
        (Key::ArrowUp, Vertical, _) => Nav::Prev,
        (Key::ArrowRight, Horizontal, Dir::Ltr) | (Key::ArrowLeft, Horizontal, Dir::Rtl) => {
            Nav::Next
        }
        (Key::ArrowLeft, Horizontal, Dir::Ltr) | (Key::ArrowRight, Horizontal, Dir::Rtl) => {
            Nav::Prev
        }
        _ => return None,
    })
}

/// Index to move to, skipping disabled items. `None` means there is nowhere to go.
#[must_use]
pub fn step(enabled: &[bool], current: Option<usize>, nav: Nav, wrap: bool) -> Option<usize> {
    let n = enabled.len();
    let first = || enabled.iter().position(|e| *e);
    let last = || enabled.iter().rposition(|e| *e);
    match (nav, current) {
        (Nav::First, _) | (Nav::Next, None) => first(),
        (Nav::Last, _) | (Nav::Prev, None) => last(),
        (Nav::Next, Some(cur)) => (cur + 1..n).find(|i| enabled[*i]).or_else(|| {
            if wrap {
                first().filter(|i| *i != cur)
            } else {
                None
            }
        }),
        (Nav::Prev, Some(cur)) => (0..cur.min(n)).rev().find(|i| enabled[*i]).or_else(|| {
            if wrap {
                last().filter(|i| *i != cur)
            } else {
                None
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_disabled_items() {
        let enabled = [true, false, true];
        assert_eq!(step(&enabled, Some(0), Nav::Next, false), Some(2));
        assert_eq!(step(&enabled, Some(2), Nav::Prev, false), Some(0));
    }

    #[test]
    fn stops_at_the_end_unless_wrapping() {
        let enabled = [true, true, false];
        assert_eq!(step(&enabled, Some(1), Nav::Next, false), None);
        assert_eq!(step(&enabled, Some(1), Nav::Next, true), Some(0));
        assert_eq!(step(&enabled, Some(0), Nav::Prev, true), Some(1));
    }

    #[test]
    fn enters_from_nothing_at_the_matching_edge() {
        let enabled = [false, true, true];
        assert_eq!(step(&enabled, None, Nav::Next, false), Some(1));
        assert_eq!(step(&enabled, None, Nav::Prev, false), Some(2));
    }

    #[test]
    fn horizontal_arrows_flip_in_rtl() {
        let nav = |k, d| nav_for_key(k, Orientation::Horizontal, d);
        assert_eq!(nav(Key::ArrowRight, Dir::Ltr), Some(Nav::Next));
        assert_eq!(nav(Key::ArrowRight, Dir::Rtl), Some(Nav::Prev));
        assert_eq!(nav(Key::ArrowDown, Dir::Ltr), None);
    }

    #[test]
    fn arrows_with_ctrl_or_alt_belong_to_the_platform() {
        use crate::primitive::Input;
        let key = |key, mods| Input::<()>::Key {
            key,
            mods,
            now_ms: 0,
        };
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        let shift = Modifiers {
            shift: true,
            ..Default::default()
        };
        assert!(is_platform_arrow(&key(Key::ArrowDown, ctrl)));
        assert!(
            !is_platform_arrow(&key(Key::ArrowDown, shift)),
            "Shift extends a selection"
        );
        assert!(!is_platform_arrow(&key(Key::Home, ctrl)));
    }
}

/// One item of a group that shares a single tab stop.
#[derive(Clone, Copy, Debug)]
pub struct Stop<'a> {
    pub key: &'a str,
    pub disabled: bool,
    /// Whether it can take focus at all. A disabled item often still can, so it
    /// can be read.
    pub focusable: bool,
    /// Whether it is the one to start on, such as the selected tab.
    pub preferred: bool,
}

/// Which item of a group Tab lands on. The one focus was last on keeps it while it
/// can still take focus. Otherwise it is the first preferred item that is enabled,
/// then the first enabled one, then any that can take focus, so a group whose
/// items are all disabled can still be reached.
#[must_use]
pub fn roving_stop<'a>(items: &[Stop<'a>], remembered: Option<&str>) -> Option<&'a str> {
    let remembered =
        remembered.and_then(|key| items.iter().find(|item| item.key == key && item.focusable));
    remembered
        .or_else(|| {
            items
                .iter()
                .find(|item| item.preferred && !item.disabled && item.focusable)
        })
        .or_else(|| items.iter().find(|item| !item.disabled && item.focusable))
        .or_else(|| items.iter().find(|item| item.focusable))
        .map(|item| item.key)
}

/// Where the caret is in a text field that sits in a group moved through with
/// arrows, such as a toolbar's search box.
#[derive(Clone, Copy, Debug)]
pub struct FieldCaret {
    /// The selection's ends, or `None` for a field with no caret, such as a
    /// checkbox.
    pub selection: Option<(usize, usize)>,
    pub length: usize,
    pub rtl: bool,
    /// Whether Up and Down mean something to the field, as they do in a number
    /// field or a text area.
    pub uses_up_down: bool,
}

/// Whether a key pressed in such a field is the field's own, or moves on to the
/// next item of the group. Left and Right leave only from the end they point at,
/// with nothing selected. Home and End always stay.
#[must_use]
pub fn field_keeps_key(key: Key, field: FieldCaret) -> bool {
    let Some((start, end)) = field.selection else {
        return false;
    };
    let (toward_start, toward_end) = if field.rtl {
        (Key::ArrowRight, Key::ArrowLeft)
    } else {
        (Key::ArrowLeft, Key::ArrowRight)
    };
    match key {
        Key::Home | Key::End => true,
        Key::ArrowUp | Key::ArrowDown => field.uses_up_down,
        key if key == toward_end => !(start == end && end == field.length),
        key if key == toward_start => !(start == end && start == 0),
        _ => true,
    }
}
