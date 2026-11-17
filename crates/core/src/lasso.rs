//! Selecting whatever a pointer-drawn rectangle touches.

use crate::geometry::{Point, Rect};
use crate::keys::Modifiers;

/// The keys of the items the rectangle from `start` to `at` touches, in the order
/// given.
pub fn lasso_hits<K: Clone>(start: Point, at: Point, items: &[(K, Rect)]) -> Vec<K> {
    let lasso = Rect::from_corners(start, at);
    items
        .iter()
        .filter(|(_, rect)| lasso.intersects(rect))
        .map(|(key, _)| key.clone())
        .collect()
}

/// The selection while a lasso is drawn. A plain lasso replaces what was selected
/// before it began, Shift adds to it, and Ctrl flips the items it touches.
pub fn lasso_selection<K: Clone + PartialEq>(before: &[K], hits: &[K], mods: Modifiers) -> Vec<K> {
    if mods.ctrl {
        let kept = before.iter().filter(|key| !hits.contains(key));
        let added = hits.iter().filter(|key| !before.contains(key));
        kept.chain(added).cloned().collect()
    } else if mods.shift {
        let added = hits.iter().filter(|key| !before.contains(key));
        before.iter().chain(added).cloned().collect()
    } else {
        hits.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<(&'static str, Rect)> {
        vec![
            ("a", Rect::new(0., 0., 10., 10.)),
            ("b", Rect::new(20., 0., 10., 10.)),
            ("c", Rect::new(40., 0., 10., 10.)),
        ]
    }

    #[test]
    fn the_lasso_takes_what_it_touches_whichever_way_it_is_drawn() {
        let forwards = lasso_hits(Point::new(5., 5.), Point::new(25., 8.), &items());
        let backwards = lasso_hits(Point::new(25., 8.), Point::new(5., 5.), &items());
        assert_eq!(forwards, ["a", "b"]);
        assert_eq!(backwards, forwards);
    }

    #[test]
    fn a_lasso_in_empty_space_selects_nothing() {
        assert!(lasso_hits(Point::new(11., 0.), Point::new(19., 10.), &items()).is_empty());
    }

    #[test]
    fn modifiers_add_to_or_flip_the_earlier_selection() {
        let before = ["a", "c"];
        let hits = ["a", "b"];
        let shift = Modifiers {
            shift: true,
            ctrl: false,
            alt: false,
        };
        let ctrl = Modifiers {
            shift: false,
            ctrl: true,
            alt: false,
        };
        assert_eq!(
            lasso_selection(&before, &hits, Modifiers::default()),
            ["a", "b"]
        );
        assert_eq!(lasso_selection(&before, &hits, shift), ["a", "c", "b"]);
        assert_eq!(lasso_selection(&before, &hits, ctrl), ["c", "b"]);
    }
}
