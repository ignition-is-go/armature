//! Moving items within or between lists, such as the two sides of a transfer list or
//! the columns of a board.

/// Moves `keys` into list `to` at `index` (or its end), taking them out of whichever
/// lists hold them. They keep the order they are given in. Returns how many moved.
pub fn transfer<T: PartialEq + Clone>(
    lists: &mut [Vec<T>],
    keys: &[T],
    to: usize,
    index: Option<usize>,
) -> usize {
    let Some(destination) = lists.get(to) else {
        return 0;
    };
    let present: Vec<T> = keys
        .iter()
        .filter(|key| lists.iter().any(|list| list.contains(key)))
        .cloned()
        .collect();
    // Removing items that sit before the target position shifts it left.
    let target = index.unwrap_or(destination.len()).min(destination.len());
    let removed_before = destination
        .iter()
        .take(target)
        .filter(|item| present.contains(item))
        .count();
    for list in lists.iter_mut() {
        list.retain(|item| !present.contains(item));
    }
    let at = target.saturating_sub(removed_before);
    if let Some(destination) = lists.get_mut(to) {
        destination.splice(at..at, present.iter().cloned());
    }
    present.len()
}

/// What to tell a screen reader after a transfer.
#[must_use]
pub fn transfer_message(count: usize, destination: &str) -> String {
    match count {
        0 => "Nothing moved.".to_string(),
        1 => format!("1 item moved to {destination}."),
        many => format!("{many} items moved to {destination}."),
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::string_slice,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::as_conversions,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::too_many_lines,
    reason = "a test fails by panicking, and its arithmetic is on values it wrote itself"
)]
mod tests {
    use super::*;

    #[test]
    fn items_move_between_lists_in_the_order_given() {
        let mut lists = vec![vec!["a", "b", "c"], vec!["x"]];
        assert_eq!(transfer(&mut lists, &["c", "a"], 1, None), 2);
        assert_eq!(lists, [vec!["b"], vec!["x", "c", "a"]]);
    }

    #[test]
    fn an_index_places_them_inside_the_destination() {
        let mut lists = vec![vec!["a"], vec!["x", "y"]];
        transfer(&mut lists, &["a"], 1, Some(1));
        assert_eq!(lists[1], ["x", "a", "y"]);
    }

    #[test]
    fn moving_within_one_list_accounts_for_the_gap_left_behind() {
        let mut lists = vec![vec!["a", "b", "c", "d"]];
        transfer(&mut lists, &["a"], 0, Some(3));
        assert_eq!(lists[0], ["b", "c", "a", "d"]);
    }

    #[test]
    fn unknown_keys_and_lists_are_ignored() {
        let mut lists = vec![vec!["a"], vec![]];
        assert_eq!(transfer(&mut lists, &["zzz"], 1, None), 0);
        assert_eq!(transfer(&mut lists, &["a"], 9, None), 0);
        assert_eq!(lists[0], ["a"]);
        assert_eq!(transfer_message(1, "Done"), "1 item moved to Done.");
    }
}
