//! Ranges on a track, such as events on a timeline.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: f64,
    pub end: f64,
}

impl Span {
    /// Spans that only touch at an edge do not overlap.
    pub fn overlaps(&self, other: &Span) -> bool {
        self.start < other.end && other.start < self.end
    }
}

/// Whether `candidate` fits on a track without overlapping any of `others`.
pub fn is_span_free(candidate: Span, others: &[Span]) -> bool {
    !others.iter().any(|other| candidate.overlaps(other))
}

/// The lane for each span so that overlapping spans sit in separate lanes. Each span
/// takes the lowest lane that is free, in order of start.
pub fn stack_spans(spans: &[Span]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by(|a, b| spans[*a].start.total_cmp(&spans[*b].start));

    let mut lane_ends: Vec<f64> = Vec::new();
    let mut lanes = vec![0; spans.len()];
    for index in order {
        let span = spans[index];
        let lane = lane_ends
            .iter()
            .position(|end| *end <= span.start)
            .unwrap_or(lane_ends.len());
        if lane == lane_ends.len() {
            lane_ends.push(span.end);
        } else {
            lane_ends[lane] = span.end;
        }
        lanes[index] = lane;
    }
    lanes
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
}
