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
#[must_use]
pub fn propose(
    mode: Mode,
    selected: &[String],
    anchor: Option<&str>,
    key: &str,
    mods: Modifiers,
    order: &[&str],
) -> Vec<String> {
    if mode == Mode::Single {
        // Toggling the one selected item clears it.
        let sole = mods.ctrl && selected.len() == 1 && selected[0] == key;
        return if sole {
            Vec::new()
        } else {
            vec![key.to_string()]
        };
    }
    let position = |k: &str| order.iter().position(|o| *o == k);

    if mods.shift
        && let (Some(a), Some(b)) = (anchor.and_then(position), position(key))
    {
        let (lo, hi) = (a.min(b), a.max(b));
        return order
            .get(lo..=hi)
            .unwrap_or_default()
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
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

/// What Ctrl+A leaves selected: everything selectable, or nothing once everything
/// already is. Only a multiple selection has a select-all.
#[must_use]
pub fn toggle_all(mode: Mode, selected: &[String], order: &[&str]) -> Vec<String> {
    let all = order
        .iter()
        .all(|key| selected.iter().any(|chosen| chosen == key));
    if mode == Mode::Single {
        selected.to_vec()
    } else if all {
        Vec::new()
    } else {
        order.iter().map(std::string::ToString::to_string).collect()
    }
}

/// The chosen labels written out for a control that shows what is selected. Past
/// `most`, the rest are counted: `Small, Medium and 2 more`.
#[must_use]
pub fn format_labels(labels: &[&str], most: usize) -> String {
    let shown = labels
        .get(..labels.len().min(most.max(1)))
        .unwrap_or_default();
    let rest = labels.len().saturating_sub(shown.len());
    match (shown, rest) {
        ([], _) => String::new(),
        (shown, 0) if shown.len() == 1 => shown.first().copied().unwrap_or_default().to_string(),
        (shown, 0) => format!(
            "{} and {}",
            shown
                .get(..shown.len().saturating_sub(1))
                .unwrap_or_default()
                .join(", "),
            shown
                .get(shown.len().saturating_sub(1))
                .copied()
                .unwrap_or_default()
        ),
        (shown, rest) => format!("{} and {rest} more", shown.join(", ")),
    }
}

/// The keys from one key to another in `order`, both included, whichever comes
/// first. Empty if either is not in the list.
#[must_use]
pub fn keys_between(order: &[&str], from: &str, to: &str) -> Vec<String> {
    let at = |key: &str| order.iter().position(|entry| *entry == key);
    match (at(from), at(to)) {
        (Some(a), Some(b)) => order
            .get(a.min(b)..=a.max(b))
            .unwrap_or_default()
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// The selection after extending from an anchor to `to`. `base` is what was
/// selected when the anchor was set: an additive range keeps it, first and in its
/// own order, and extending again replaces only the range, so a range can shrink.
/// Keys that cannot be chosen are skipped.
pub fn range_selection(
    order: &[&str],
    anchor: &str,
    base: &[String],
    to: &str,
    additive: bool,
    selectable: impl Fn(&str) -> bool,
) -> Vec<String> {
    let range: Vec<String> = keys_between(order, anchor, to)
        .into_iter()
        .filter(|key| selectable(key))
        .collect();
    if !additive {
        return range;
    }
    let mut next = base.to_vec();
    next.extend(range.into_iter().filter(|key| !base.contains(key)));
    next
}

/// The selection with a key added at the end, or taken out.
#[must_use]
pub fn toggled_keys(selected: &[String], key: &str) -> Vec<String> {
    if selected.iter().any(|chosen| chosen == key) {
        selected
            .iter()
            .filter(|chosen| *chosen != key)
            .cloned()
            .collect()
    } else {
        selected.iter().cloned().chain([key.to_string()]).collect()
    }
}

/// Select-all as a toggle: every key that can be chosen is added, or, once they
/// all are, taken out. Keys that cannot be chosen are left as they were.
#[must_use]
pub fn all_toggled(selected: &[String], selectable: &[&str]) -> Vec<String> {
    if selectable.is_empty() {
        return selected.to_vec();
    }
    if selectable
        .iter()
        .all(|key| selected.iter().any(|chosen| chosen == key))
    {
        return selected
            .iter()
            .filter(|chosen| !selectable.contains(&chosen.as_str()))
            .cloned()
            .collect();
    }
    let mut next = selected.to_vec();
    next.extend(
        selectable
            .iter()
            .filter(|key| !selected.iter().any(|chosen| chosen == *key))
            .map(std::string::ToString::to_string),
    );
    next
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORDER: [&str; 4] = ["a", "b", "c", "d"];
    const SHIFT: Modifiers = Modifiers {
        shift: true,
        ctrl: false,
        alt: false,
    };
    const CTRL: Modifiers = Modifiers {
        shift: false,
        ctrl: true,
        alt: false,
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

    #[test]
    fn toggling_the_only_selected_item_in_single_mode_clears_it() {
        assert!(propose(Mode::Single, &strings(&["a"]), Some("a"), "a", CTRL, &ORDER).is_empty());
        assert_eq!(
            propose(Mode::Single, &strings(&["a"]), Some("a"), "b", CTRL, &ORDER),
            strings(&["b"])
        );
    }

    #[test]
    fn select_all_fills_then_empties_and_does_nothing_for_a_single_selection() {
        let all = toggle_all(Mode::Multiple, &strings(&["b"]), &ORDER);
        assert_eq!(all, strings(&ORDER));
        assert!(toggle_all(Mode::Multiple, &all, &ORDER).is_empty());
        assert_eq!(
            toggle_all(Mode::Single, &strings(&["b"]), &ORDER),
            strings(&["b"])
        );
    }

    #[test]
    fn chosen_labels_are_listed_and_the_overflow_counted() {
        assert_eq!(format_labels(&[], 2), "");
        assert_eq!(format_labels(&["Small"], 2), "Small");
        assert_eq!(format_labels(&["Small", "Large"], 2), "Small and Large");
        assert_eq!(
            format_labels(&["Small", "Medium", "Large", "Huge"], 2),
            "Small, Medium and 2 more"
        );
    }
}
