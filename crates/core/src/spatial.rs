//! Moving focus by where things are on screen, for layouts that are not a list or a
//! grid: a dashboard, a canvas, a TV-style menu.

use crate::geometry::Rect;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// The item nearest to `from` in a direction. Only items that lie that way are
/// considered. One within 45 degrees of straight ahead always beats one outside that
/// cone; otherwise distance decides, with sideways distance counted double. On a tie
/// the earlier item wins.
pub fn nearest_in_direction<K: Clone>(
    items: &[(K, Rect)],
    from: Rect,
    direction: Direction,
) -> Option<K> {
    let origin = from.center();
    let mut best: Option<(bool, f64, &K)> = None;
    for (key, rect) in items {
        let center = rect.center();
        let (ahead, aside) = match direction {
            Direction::Up => (origin.y - center.y, center.x - origin.x),
            Direction::Down => (center.y - origin.y, center.x - origin.x),
            Direction::Left => (origin.x - center.x, center.y - origin.y),
            Direction::Right => (center.x - origin.x, center.y - origin.y),
        };
        if ahead <= 0.0 {
            continue;
        }
        let in_cone = aside.abs() <= ahead;
        let score = 2.0f64.mul_add(aside.abs(), ahead);
        let better = match best {
            None => true,
            Some((best_cone, best_score, _)) => {
                (in_cone && !best_cone) || (in_cone == best_cone && score < best_score)
            }
        };
        if better {
            best = Some((in_cone, score, key));
        }
    }
    best.map(|(_, _, key)| key.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(x: f64, y: f64) -> Rect {
        Rect::new(x, y, 50.0, 50.0)
    }

    fn tiles() -> Vec<(&'static str, Rect)> {
        vec![
            ("home", tile(0., 0.)),
            ("mail", tile(100., 0.)),
            ("far", tile(400., 0.)),
            ("below", tile(0., 100.)),
            ("diagonal", tile(120., 110.)),
        ]
    }

    #[test]
    fn the_nearest_item_in_the_direction_wins() {
        assert_eq!(
            nearest_in_direction(&tiles(), tile(0., 0.), Direction::Right),
            Some("mail")
        );
        assert_eq!(
            nearest_in_direction(&tiles(), tile(0., 0.), Direction::Down),
            Some("below")
        );
    }

    #[test]
    fn straight_ahead_beats_closer_but_off_to_the_side() {
        let items = vec![("aligned", tile(0., 300.)), ("offset", tile(140., 100.))];
        assert_eq!(
            nearest_in_direction(&items, tile(0., 0.), Direction::Down),
            Some("aligned")
        );
    }

    #[test]
    fn nothing_that_way_means_nowhere_to_go() {
        assert_eq!(
            nearest_in_direction(&tiles(), tile(0., 0.), Direction::Left),
            None
        );
        assert_eq!(
            nearest_in_direction(&tiles(), tile(0., 0.), Direction::Up),
            None
        );
    }

    #[test]
    fn an_item_inside_the_cone_beats_a_nearer_one_outside_it() {
        // "wide" scores lower (20 + 2*30 = 80) but sits outside 45 degrees.
        let items = vec![
            ("wide", Rect::new(30., 20., 50., 50.)),
            ("ahead", Rect::new(10., 100., 50., 50.)),
        ];
        assert_eq!(
            nearest_in_direction(&items, tile(0., 0.), Direction::Down),
            Some("ahead")
        );
    }

    #[test]
    fn the_earlier_item_wins_a_tie() {
        let items = vec![("first", tile(-60., 100.)), ("second", tile(60., 100.))];
        assert_eq!(
            nearest_in_direction(&items, tile(0., 0.), Direction::Down),
            Some("first")
        );
    }
}
