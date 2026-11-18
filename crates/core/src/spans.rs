//! Ranges on a track, such as events on a timeline.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: f64,
    pub end: f64,
}

impl Span {
    /// Spans that only touch at an edge do not overlap.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Whether `candidate` fits on a track without overlapping any of `others`.
#[must_use]
pub fn is_span_free(candidate: Span, others: &[Span]) -> bool {
    !others.iter().any(|other| candidate.overlaps(other))
}

/// The lane for each span so that overlapping spans sit in separate lanes. Each span
/// takes the lowest lane that is free, in order of start.
#[must_use]
pub fn stack_spans(spans: &[Span]) -> Vec<usize> {
    let mut in_start_order: Vec<(usize, Span)> = spans.iter().copied().enumerate().collect();
    in_start_order.sort_by(|(_, a), (_, b)| a.start.total_cmp(&b.start));

    let mut lane_ends: Vec<f64> = Vec::new();
    let mut lanes = vec![0; spans.len()];
    for (index, span) in in_start_order {
        let lane = lane_ends
            .iter()
            .position(|end| *end <= span.start)
            .unwrap_or(lane_ends.len());
        match lane_ends.get_mut(lane) {
            Some(end) => *end = span.end,
            None => lane_ends.push(span.end),
        }
        if let Some(slot) = lanes.get_mut(index) {
            *slot = lane;
        }
    }
    lanes
}

fn snap(value: f64, grid: Option<f64>) -> f64 {
    match grid {
        Some(grid) if grid > 0.0 => (value / grid).round() * grid,
        _ => value,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanEdge {
    Start,
    End,
}

/// The span drawn by pressing at `from` and dragging to `to`, in either direction. It
/// is never shorter than `min_length`.
#[must_use]
pub fn draw_span(from: f64, to: f64, min_length: f64, grid: Option<f64>) -> Span {
    let (from, to) = (snap(from, grid), snap(to, grid));
    if to >= from {
        Span {
            start: from,
            end: to.max(from + min_length),
        }
    } else {
        Span {
            start: to.min(from - min_length),
            end: from,
        }
    }
}

/// `span` slid by `delta`, keeping its length and staying on `track`.
#[must_use]
pub fn move_span(span: Span, delta: f64, track: Span, grid: Option<f64>) -> Span {
    let length = span.end - span.start;
    let start = snap(span.start + delta, grid)
        .min(track.end - length)
        .max(track.start);
    Span {
        start,
        end: start + length,
    }
}

/// `span` with one edge dragged to `to`. The other edge stays put and the span keeps
/// at least `min_length`.
#[must_use]
pub fn resize_span(
    span: Span,
    edge: SpanEdge,
    to: f64,
    min_length: f64,
    grid: Option<f64>,
) -> Span {
    let to = snap(to, grid);
    match edge {
        SpanEdge::Start => Span {
            start: to.min(span.end - min_length),
            ..span
        },
        SpanEdge::End => Span {
            end: to.max(span.start + min_length),
            ..span
        },
    }
}

/// What to tell a screen reader after a span changes.
#[must_use]
pub fn span_message(label: &str, span: Span) -> String {
    format!("{label} now runs from {} to {}.", span.start, span.end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: f64, end: f64) -> Span {
        Span { start, end }
    }

    #[test]
    fn overlapping_spans_go_to_separate_lanes() {
        let lanes = stack_spans(&[span(0., 10.), span(5., 15.), span(8., 12.)]);
        assert_eq!(lanes, [0, 1, 2]);
    }

    #[test]
    fn a_lane_is_reused_once_its_span_has_ended() {
        let lanes = stack_spans(&[span(0., 10.), span(5., 15.), span(10., 20.)]);
        assert_eq!(lanes, [0, 1, 0]);
    }

    #[test]
    fn lanes_are_reported_in_the_order_the_spans_were_given() {
        let lanes = stack_spans(&[span(5., 15.), span(0., 10.)]);
        assert_eq!(lanes, [1, 0]);
    }

    #[test]
    fn touching_edges_do_not_count_as_overlap() {
        assert!(is_span_free(span(10., 20.), &[span(0., 10.)]));
        assert!(!is_span_free(span(9., 20.), &[span(0., 10.)]));
    }

    #[test]
    fn a_span_can_be_drawn_in_either_direction() {
        assert_eq!(draw_span(10., 42., 5., Some(10.)), span(10., 40.));
        assert_eq!(draw_span(40., 12., 5., Some(10.)), span(10., 40.));
        assert_eq!(draw_span(10., 11., 5., None), span(10., 15.));
    }

    #[test]
    fn a_moved_span_keeps_its_length_and_stays_on_the_track() {
        let track = span(0., 100.);
        assert_eq!(
            move_span(span(10., 30.), 500., track, None),
            span(80., 100.)
        );
        assert_eq!(move_span(span(10., 30.), -500., track, None), span(0., 20.));
        assert_eq!(
            move_span(span(10., 30.), 14., track, Some(10.)),
            span(20., 40.)
        );
    }

    #[test]
    fn resizing_moves_one_edge_and_keeps_a_minimum_length() {
        assert_eq!(
            resize_span(span(10., 30.), SpanEdge::End, 50., 5., None),
            span(10., 50.)
        );
        assert_eq!(
            resize_span(span(10., 30.), SpanEdge::End, 0., 5., None),
            span(10., 15.)
        );
        assert_eq!(
            resize_span(span(10., 30.), SpanEdge::Start, 99., 5., None),
            span(25., 30.)
        );
    }
}
