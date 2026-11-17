use crate::keys::Modifiers;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Single,
    Multiple,
}

/// The selection that pressing `key` would produce, file-manager style: a plain press
/// replaces, Ctrl toggles, Shift selects the range from `anchor`. `order` is the
/// selectable keys in display order.
pub fn propose(
    mode: Mode,
    selected: &[String],
    anchor: Option<&str>,
    key: &str,
    mods: Modifiers,
    order: &[&str],
) -> Vec<String> {
    if mode == Mode::Single {
        return vec![key.to_string()];
    }
    let position = |k: &str| order.iter().position(|o| *o == k);

    if mods.shift
        && let (Some(a), Some(b)) = (anchor.and_then(position), position(key))
    {
        let (lo, hi) = (a.min(b), a.max(b));
        return order[lo..=hi].iter().map(|k| k.to_string()).collect();
    }
    if mods.ctrl {
        let mut next: Vec<String> = selected.to_vec();
        match next.iter().position(|k| k == key) {
            Some(i) => {
                next.remove(i);
            }
            None => next.push(key.to_string()),
        }
        next.sort_by_key(|k| position(k).unwrap_or(usize::MAX));
        return next;
    }
    vec![key.to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER: [&str; 4] = ["a", "b", "c", "d"];
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ctrl: false,
    };
    const CTRL: Modifiers = Modifiers {
        shift: false,
        ctrl: true,
    };

    fn strings(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    #[test]
    fn single_mode_always_replaces() {
        let next = propose(
            Mode::Single,
            &strings(&["a"]),
            Some("a"),
            "c",
            SHIFT,
            &ORDER,
        );
        assert_eq!(next, strings(&["c"]));
    }

    #[test]
    fn shift_selects_the_range_from_the_anchor_in_either_direction() {
        let next = propose(
            Mode::Multiple,
            &strings(&["c"]),
            Some("c"),
            "a",
            SHIFT,
            &ORDER,
        );
        assert_eq!(next, strings(&["a", "b", "c"]));
    }

    #[test]
    fn ctrl_toggles_and_keeps_display_order() {
        let selected = strings(&["c"]);
        let added = propose(Mode::Multiple, &selected, Some("c"), "a", CTRL, &ORDER);
        assert_eq!(added, strings(&["a", "c"]));
        let removed = propose(Mode::Multiple, &added, Some("a"), "c", CTRL, &ORDER);
        assert_eq!(removed, strings(&["a"]));
    }

    #[test]
    fn a_plain_press_replaces_a_multiple_selection() {
        let next = propose(
            Mode::Multiple,
            &strings(&["a", "b"]),
            Some("a"),
            "d",
            Modifiers::default(),
            &ORDER,
        );
        assert_eq!(next, strings(&["d"]));
    }
}
