//! Scrolling behaviors computed from numbers the adapter measures: which rows to
//! render, when to follow new content, where a wheel picker rests, and how fast to
//! scroll while dragging near an edge.

use crate::convert::Cast;
use std::ops::Range;

use crate::geometry::{Point, Rect};
use crate::gesture::{DECELERATION, project_distance};

/// Works out which rows of a long list need rendering.
#[derive(Clone, Debug)]
pub struct Virtualizer {
    count: usize,
    /// Size assumed for a row until it is measured.
    estimate: f64,
    gap: f64,
    measured: Vec<Option<f64>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAlign {
    Start,
    Center,
    End,
}

impl Virtualizer {
    #[must_use]
    pub fn new(count: usize, estimate: f64, gap: f64) -> Self {
        Self {
            count,
            estimate,
            gap,
            measured: vec![None; count],
        }
    }

    /// Records the real size of a row once it has been rendered.
    pub fn measure(&mut self, index: usize, size: f64) {
        if let Some(slot) = self.measured.get_mut(index) {
            *slot = Some(size);
        }
    }

    fn size(&self, index: usize) -> f64 {
        self.measured
            .get(index)
            .copied()
            .unwrap_or_default()
            .unwrap_or(self.estimate)
    }

    /// Where row `index` starts.
    #[must_use]
    pub fn offset_of(&self, index: usize) -> f64 {
        (0..index.min(self.count))
            .map(|i| self.size(i) + self.gap)
            .sum()
    }

    #[must_use]
    pub fn total_size(&self) -> f64 {
        (self.offset_of(self.count) - self.gap).max(0.0)
    }

    /// The rows to render for a viewport scrolled to `offset`, with `overscan` extra
    /// rows on each side so scrolling does not show gaps.
    #[must_use]
    pub fn range(&self, offset: f64, viewport: f64, overscan: usize) -> Range<usize> {
        let mut start = 0.0;
        let mut first = None;
        let mut last = 0;
        for index in 0..self.count {
            let end = start + self.size(index);
            if end > offset && start < offset + viewport {
                first.get_or_insert(index);
                last = index.saturating_add(1);
            }
            start = end + self.gap;
        }
        first.map_or(0..0, |first| {
            first.saturating_sub(overscan)..(last.saturating_add(overscan)).min(self.count)
        })
    }

    /// The scroll offset that brings row `index` into view at `align`.
    #[must_use]
    pub fn scroll_to(&self, index: usize, align: ScrollAlign, viewport: f64) -> f64 {
        let index = index.min(self.count.saturating_sub(1));
        let (start, size) = (
            self.offset_of(index),
            if self.count == 0 {
                0.0
            } else {
                self.size(index)
            },
        );
        let offset = match align {
            ScrollAlign::Start => start,
            ScrollAlign::Center => start - (viewport - size) / 2.0,
            ScrollAlign::End => start - (viewport - size),
        };
        offset.clamp(0.0, (self.total_size() - viewport).max(0.0))
    }
}

/// Keeps a view pinned to the newest content, as a chat log does, unless the reader
/// has scrolled up to look at something.
#[derive(Clone, Copy, Debug)]
pub struct StickToBottom {
    following: bool,
    /// How close to the bottom still counts as at the bottom.
    slack: f64,
}

impl Default for StickToBottom {
    fn default() -> Self {
        Self {
            following: true,
            slack: 24.0,
        }
    }
}

impl StickToBottom {
    #[must_use]
    pub const fn is_following(&self) -> bool {
        self.following
    }

    /// The reader scrolled. Following stops when they leave the bottom and resumes
    /// when they return to it.
    pub fn scrolled(&mut self, offset: f64, viewport: f64, content: f64) {
        self.following = content - viewport - offset <= self.slack;
    }

    /// Content was added. Returns the offset to scroll to, if following.
    #[must_use]
    pub fn content_grew(&self, viewport: f64, content: f64) -> Option<f64> {
        self.following.then(|| (content - viewport).max(0.0))
    }

    /// Jumps back to the newest content and resumes following.
    pub const fn follow(&mut self) {
        self.following = true;
    }
}

/// The option centred in a wheel picker scrolled to `offset`.
#[must_use]
pub fn picker_index(offset: f64, item_size: f64, count: usize) -> usize {
    if count == 0 || item_size <= 0.0 {
        return 0;
    }
    ((offset / item_size).round().max(0.0).cast::<usize>()).min(count.saturating_sub(1))
}

/// The option a wheel picker comes to rest on when released moving at `velocity`
/// (units per second).
#[must_use]
pub fn picker_settle(offset: f64, velocity: f64, item_size: f64, count: usize) -> usize {
    picker_index(
        offset + project_distance(velocity, DECELERATION),
        item_size,
        count,
    )
}

/// How far in from an edge a drag starts scrolling.
pub const EDGE_ZONE: f64 = 48.0;
/// The fastest a drag at the edge scrolls, in units per second.
pub const EDGE_SPEED: f64 = 1080.0;

/// How fast to scroll a container, in units per second on each axis, while a drag is
/// at `at`. Scrolling starts within `zone` of an edge and speeds up towards it.
#[must_use]
pub fn edge_scroll(container: Rect, at: Point, zone: f64, max_speed: f64) -> Point {
    let axis = |position: f64, start: f64, end: f64| {
        // A small container gets a narrower band, never more than a third of it.
        let zone = zone.min((end - start) / 3.0).max(f64::MIN_POSITIVE);
        if position < start + zone {
            -max_speed * ((start + zone - position) / zone).min(1.0)
        } else if position > end - zone {
            max_speed * ((position - (end - zone)) / zone).min(1.0)
        } else {
            0.0
        }
    };
    Point::new(
        axis(at.x, container.x, container.right()),
        axis(at.y, container.y, container.bottom()),
    )
}

/// How far to lift content so a focused field clears an on-screen keyboard. `visible`
/// is the part of the window the keyboard has left showing. Zero when the field
/// already shows, with `margin` to spare.
#[must_use]
pub fn avoid_keyboard(field: Rect, visible: Rect, margin: f64) -> f64 {
    (field.bottom() + margin - visible.bottom()).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_rows_in_view_plus_overscan_are_rendered() {
        let list = Virtualizer::new(1000, 20.0, 0.0);
        assert_eq!(list.range(0.0, 100.0, 0), 0..5);
        assert_eq!(list.range(210.0, 100.0, 2), 8..18);
        assert_eq!(list.total_size(), 20_000.0);
    }

    #[test]
    fn measured_rows_shift_everything_after_them() {
        let mut list = Virtualizer::new(10, 20.0, 0.0);
        list.measure(0, 100.0);
        assert_eq!(list.offset_of(1), 100.0);
        assert_eq!(list.range(0.0, 100.0, 0), 0..1);
        assert_eq!(list.total_size(), 280.0);
    }

    #[test]
    fn gaps_sit_between_rows_and_not_after_the_last() {
        let list = Virtualizer::new(3, 10.0, 5.0);
        assert_eq!(list.offset_of(2), 30.0);
        assert_eq!(list.total_size(), 40.0);
    }

    #[test]
    fn scrolling_to_a_row_respects_alignment_and_the_ends() {
        let list = Virtualizer::new(100, 20.0, 0.0);
        assert_eq!(list.scroll_to(10, ScrollAlign::Start, 100.0), 200.0);
        assert_eq!(list.scroll_to(10, ScrollAlign::Center, 100.0), 160.0);
        assert_eq!(list.scroll_to(10, ScrollAlign::End, 100.0), 120.0);
        assert_eq!(list.scroll_to(0, ScrollAlign::End, 100.0), 0.0);
        assert_eq!(list.scroll_to(99, ScrollAlign::Start, 100.0), 1900.0);
    }

    #[test]
    fn an_empty_list_renders_nothing() {
        let list = Virtualizer::new(0, 20.0, 0.0);
        assert_eq!(list.range(0.0, 100.0, 3), 0..0);
        assert_eq!(list.total_size(), 0.0);
    }

    #[test]
    fn new_content_is_followed_until_the_reader_scrolls_up() {
        let mut log = StickToBottom::default();
        assert_eq!(log.content_grew(100.0, 500.0), Some(400.0));
        log.scrolled(200.0, 100.0, 500.0);
        assert_eq!(log.content_grew(100.0, 600.0), None);
        log.scrolled(480.0, 100.0, 600.0);
        assert!(log.is_following(), "back within reach of the bottom");
    }

    #[test]
    fn a_picker_rests_on_the_nearest_option() {
        assert_eq!(picker_index(74.0, 30.0, 10), 2);
        assert_eq!(picker_index(9000.0, 30.0, 10), 9);
        assert_eq!(picker_index(-40.0, 30.0, 10), 0);
        assert_eq!(picker_settle(0.0, 300.0, 30.0, 10), 5);
    }

    #[test]
    fn dragging_near_an_edge_scrolls_faster_the_closer_it_gets() {
        let container = Rect::new(0., 0., 200., 400.);
        let speed = |y| edge_scroll(container, Point::new(100., y), 40., 600.).y;
        assert_eq!(speed(200.), 0.0);
        assert_eq!(speed(380.), 300.0);
        assert_eq!(speed(400.), 600.0);
        assert_eq!(speed(-50.), -600.0);
    }

    #[test]
    fn a_field_under_the_keyboard_is_lifted_just_clear_of_it() {
        let visible = Rect::new(0., 0., 400., 500.);
        assert_eq!(
            avoid_keyboard(Rect::new(20., 480., 200., 40.), visible, 16.),
            36.0
        );
        assert_eq!(
            avoid_keyboard(Rect::new(20., 100., 200., 40.), visible, 16.),
            0.0
        );
    }
}
