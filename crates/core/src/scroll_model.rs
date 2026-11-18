//! The arithmetic of a scroll area with its own scrollbars: how long the thumb is
//! and where it sits, what a drag or a press on the track scrolls to, how near the
//! ends the content is, and when a scrollbar shows.

use crate::keys::Key;

/// One axis of a scrolling area, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisMetrics {
    /// How far it is scrolled. A rubber band can take it past either end.
    pub position: f64,
    /// The length of the view.
    pub client: f64,
    /// The length of the content.
    pub scroll: f64,
}

/// Content within this many pixels of an edge counts as at it.
pub const EDGE_TOLERANCE: f64 = 1.0;
/// The shortest a thumb gets, so it can always be grabbed.
pub const MIN_THUMB_SIZE: f64 = 16.0;
const PAGE_FRACTION: f64 = 0.875;

pub fn max_scroll(metrics: AxisMetrics) -> f64 {
    (metrics.scroll - metrics.client).max(0.0)
}

pub fn overflows(metrics: AxisMetrics) -> bool {
    max_scroll(metrics) >= EDGE_TOLERANCE
}

fn clamped_position(metrics: AxisMetrics) -> f64 {
    metrics.position.clamp(0.0, max_scroll(metrics))
}

/// How far the content is from each end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    pub before: f64,
    pub after: f64,
    pub at_start: bool,
    pub at_end: bool,
    /// From 0 at the start to 1 at the end.
    pub progress: f64,
}

pub fn edges_of(metrics: AxisMetrics) -> Edges {
    let max = max_scroll(metrics);
    let position = clamped_position(metrics);
    let fits = max < EDGE_TOLERANCE;
    let (before, after) = if fits {
        (0.0, 0.0)
    } else {
        (position, max - position)
    };
    Edges {
        before,
        after,
        at_start: before < EDGE_TOLERANCE,
        at_end: after < EDGE_TOLERANCE,
        progress: if fits { 0.0 } else { position / max },
    }
}

/// The thumb's length and its offset along a track of length `track`. It is as
/// long as the visible share of the content, never shorter than the minimum, and
/// stays in the track through a rubber band.
pub fn thumb_geometry(metrics: AxisMetrics, track: f64) -> (f64, f64) {
    if track <= 0.0 || metrics.scroll <= 0.0 {
        return (0.0, 0.0);
    }
    let size = track.min(MIN_THUMB_SIZE.max(track * metrics.client / metrics.scroll));
    let max = max_scroll(metrics);
    let offset = if max <= 0.0 {
        0.0
    } else {
        (track - size) * clamped_position(metrics) / max
    };
    (size, offset)
}

/// Where the content scrolls to when the thumb is dragged by `delta` from a
/// scroll position of `start`: as far along the content as along the track.
pub fn dragged_position(start: f64, delta: f64, metrics: AxisMetrics, track: f64) -> f64 {
    let travel = track - thumb_geometry(metrics, track).0;
    if travel <= 0.0 {
        return start;
    }
    let max = max_scroll(metrics);
    (start + delta / travel * max).clamp(0.0, max)
}

/// The scroll position that centers the thumb on a point of the track.
pub fn position_at(point: f64, metrics: AxisMetrics, track: f64) -> f64 {
    let size = thumb_geometry(metrics, track).0;
    let travel = track - size;
    if travel <= 0.0 {
        return 0.0;
    }
    ((point - size / 2.0) / travel).clamp(0.0, 1.0) * max_scroll(metrics)
}

/// Which way a press on the track pages: -1 before the thumb, 1 after it, and 0
/// on it.
pub fn page_direction(point: f64, thumb_size: f64, thumb_offset: f64) -> i8 {
    if point < thumb_offset {
        -1
    } else if point > thumb_offset + thumb_size {
        1
    } else {
        0
    }
}

/// How far a page scrolls: most of the view, so a little stays for context.
pub fn page_length(client: f64) -> f64 {
    (client * PAGE_FRACTION).round().max(1.0)
}

/// A wheel's movement in pixels. `mode` is 0 for pixels, 1 for lines and 2 for
/// pages, as wheel events report it.
pub fn wheel_pixels(delta: f64, mode: u8, line_height: f64, page: f64) -> f64 {
    match mode {
        1 => delta * line_height,
        2 => delta * page,
        _ => delta,
    }
}

/// When a scrollbar shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollbarType {
    /// Whenever there is overflow.
    #[default]
    Auto,
    Always,
    /// Only while scrolling or dragging.
    Scroll,
    /// While scrolling, dragging or hovering.
    Hover,
}

pub fn scrollbar_shown(
    kind: ScrollbarType,
    overflow: bool,
    hovering: bool,
    scrolling: bool,
    dragging: bool,
) -> bool {
    match kind {
        ScrollbarType::Always => true,
        _ if !overflow => false,
        ScrollbarType::Auto => true,
        _ if dragging || scrolling => true,
        kind => kind == ScrollbarType::Hover && hovering,
    }
}

/// Whether an area that sticks to its end keeps following after a scroll of
/// `delta` leaves `after` pixels to the end. Moving away lets go, whatever moved
/// it; reaching the end while moving toward it follows again.
pub fn follows_end(following: bool, delta: f64, after: f64) -> bool {
    let at_end = after < EDGE_TOLERANCE;
    if following {
        at_end || delta >= 0.0
    } else {
        at_end && delta > 0.0
    }
}

/// Whether a key scrolls toward the start, which lets go of the end. Not while
/// typing, where the keys are text.
pub fn scrolls_toward_start(key: Key, shift: bool, editing: bool) -> bool {
    !editing
        && (matches!(key, Key::ArrowUp | Key::PageUp | Key::Home) || (key == Key::Space && shift))
}
