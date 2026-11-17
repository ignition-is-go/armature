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
pub fn leaves_arrows_alone(mods: Modifiers) -> bool {
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

pub fn nav_for_key(key: Key, orientation: Orientation, dir: Dir) -> Option<Nav> {
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
