//! Where a carousel's pages are, worked out from its slides as measured: slides of
//! any width, grouped into pages, lined up at the start, middle or end of the
//! view. Lengths are along the scrolling axis.

use crate::convert::Cast;

/// A slide's place along the track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlideBox {
    pub start: f64,
    pub size: f64,
}

/// Where in the view a page lines up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

/// How many slides make a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PerPage {
    Count(usize),
    /// As many as the view shows.
    Auto,
}

/// The slides and the view they scroll in.
#[derive(Clone, Debug)]
pub struct Geometry<'a> {
    pub slides: &'a [SlideBox],
    /// The length of the view that pages snap within: the viewport less its
    /// scroll padding.
    pub snapport: f64,
    pub padding_start: f64,
    /// The furthest the track scrolls.
    pub max_scroll: f64,
}

/// One page: the slides on it, and the scroll position that shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub slides: Vec<usize>,
    pub position: f64,
    /// The first slide on the page.
    pub leader: usize,
    /// The space before the page's slides when it is lined up.
    pub margin: f64,
}

/// The slides in pages, by index.
#[must_use]
pub fn group_slides(slides: &[SlideBox], snapport: f64, per_page: PerPage) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    match per_page {
        PerPage::Count(count) => {
            let size = count.max(1);
            for index in (0..slides.len()).step_by(size) {
                groups.push((index..(index.saturating_add(size)).min(slides.len())).collect());
            }
        }
        PerPage::Auto => {
            for (index, slide) in slides.iter().enumerate() {
                let fits = groups.last().is_some_and(|group| {
                    slide.start + slide.size - slides[group[0]].start <= snapport + 0.5
                });
                match groups.last_mut().filter(|_| fits) {
                    Some(group) => group.push(index),
                    // A slide wider than the view still makes a page of its own.
                    None => groups.push(vec![index]),
                }
            }
        }
    }
    groups
}

fn align_offset(align: Align, snapport: f64, size: f64) -> f64 {
    match align {
        Align::Start => 0.0,
        Align::Center => (snapport - size) / 2.0,
        Align::End => snapport - size,
    }
}

fn page_of(
    boxes: &[SlideBox],
    group: &[usize],
    align: Align,
    snapport: f64,
    padding_start: f64,
) -> Page {
    let (first, last) = (boxes[group[0]], boxes[group[group.len().saturating_sub(1)]]);
    let margin = align_offset(align, snapport, last.start + last.size - first.start);
    Page {
        slides: group.to_vec(),
        position: first.start - margin - padding_start,
        leader: group[0],
        margin,
    }
}

/// The pages of a carousel that stops at its ends. Pages that would sit at the
/// same position, such as the last few slides of a full row, merge into one.
#[must_use]
pub fn carousel_pages(geometry: &Geometry, align: Align, per_page: PerPage) -> Vec<Page> {
    let mut pages: Vec<Page> = Vec::new();
    for group in group_slides(geometry.slides, geometry.snapport, per_page) {
        let mut page = page_of(
            geometry.slides,
            &group,
            align,
            geometry.snapport,
            geometry.padding_start,
        );
        page.position = page.position.clamp(0.0, geometry.max_scroll.max(0.0));
        match pages
            .last_mut()
            .filter(|previous| (page.position - previous.position).abs() < 1.0)
        {
            Some(previous) => previous.slides.extend(group),
            None => pages.push(page),
        }
    }
    pages
}

/// The page nearest a scroll position. `None` without pages.
#[must_use]
pub fn nearest_page(positions: &[f64], position: f64) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (index, at) in positions.iter().enumerate() {
        if best.is_none_or(|best| (at - position).abs() < (positions[best] - position).abs()) {
            best = Some(index);
        }
    }
    best
}

/// Whether a slide counts as on show: at least half of it, or for a slide larger
/// than the view, at least half of the view.
#[must_use]
pub fn is_slide_in_view(slide: SlideBox, scroll: f64, viewport: f64) -> bool {
    let shown = (slide.start + slide.size).min(scroll + viewport) - slide.start.max(scroll);
    shown >= slide.size.min(viewport) / 2.0
}

/// The page a step of `by` lands on, counting pages from 1. It stops at the ends,
/// or goes round with `wrap`.
#[must_use]
pub fn step_page(page: usize, by: i64, count: usize, wrap: bool) -> usize {
    if count <= 1 {
        return page;
    }
    let next = (page.cast::<i64>()).saturating_add(by);
    if wrap {
        ((next.saturating_sub(1)).rem_euclid(count.cast::<i64>()) as usize).saturating_add(1)
    } else {
        next.clamp(1, count.cast::<i64>()) as usize
    }
}

/// A track of slides as it is when slide 0 comes first: each slide's size, the
/// gap between them, and where the first starts. A looping carousel shows the
/// same slides starting from any of them.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub sizes: Vec<f64>,
    pub gap: f64,
    pub start: f64,
}

/// The view a looping carousel scrolls in.
#[derive(Clone, Copy, Debug)]
pub struct LoopView {
    pub snapport: f64,
    pub padding_start: f64,
    pub max_scroll: f64,
    pub align: Align,
    pub per_page: PerPage,
}

/// Where each slide is when the track starts at slide `first`.
#[must_use]
pub fn place_slides(track: &Track, first: usize) -> Vec<SlideBox> {
    let count = track.sizes.len();
    let mut boxes = vec![
        SlideBox {
            start: 0.0,
            size: 0.0
        };
        count
    ];
    let mut at = track.start;
    for step in 0..count {
        let index = (first.saturating_add(step))
            .checked_rem(count)
            .unwrap_or_default();
        boxes[index] = SlideBox {
            start: at,
            size: track.sizes[index],
        };
        at += track.sizes[index] + track.gap;
    }
    boxes
}

/// Reads the track from slides measured while slide `first` comes first. `None`
/// if they are not laid out that way, or there are too few to rotate.
#[must_use]
pub fn measure_track(slides: &[SlideBox], first: usize) -> Option<Track> {
    if slides.len() < 2 || first >= slides.len() {
        return None;
    }
    let (lead, next) = (
        slides[first],
        slides[(first.saturating_add(1))
            .checked_rem(slides.len())
            .unwrap_or_default()],
    );
    let track = Track {
        sizes: slides.iter().map(|slide| slide.size).collect(),
        gap: next.start - lead.start - lead.size,
        start: lead.start,
    };
    let placed = place_slides(&track, first);
    placed
        .iter()
        .zip(slides)
        .all(|(placed, slide)| (placed.start - slide.start).abs() < 1.0)
        .then_some(track)
}

fn loop_groups(track: &Track, view: &LoopView) -> Vec<Vec<usize>> {
    group_slides(&place_slides(track, 0), view.snapport, view.per_page)
}

/// The pages of a looping carousel while slide `first` comes first. Pages keep
/// their slides' own order wherever the track starts.
#[must_use]
pub fn loop_pages(track: &Track, view: &LoopView, first: usize) -> Vec<Page> {
    let boxes = place_slides(track, first);
    loop_groups(track, view)
        .iter()
        .map(|group| page_of(&boxes, group, view.align, view.snapport, view.padding_start))
        .collect()
}

/// Whether the track can loop without a seam: there must be a page's step of
/// room to scroll on either side.
pub fn can_loop(track: &Track, view: &LoopView) -> bool {
    let groups = loop_groups(track, view);
    if groups.len() < 2 {
        return false;
    }
    let boxes = place_slides(track, 0);
    let length: f64 = track.sizes.iter().map(|size| size + track.gap).sum();
    let step = (0..groups.len())
        .map(|index| {
            let next = &groups[(index.saturating_add(1))
                .checked_rem(groups.len())
                .unwrap_or_default()];
            (boxes[next[0]].start - boxes[groups[index][0]].start + length) % length
        })
        .fold(0.0, f64::max);
    view.max_scroll >= 2.0f64.mul_add(step, -1.0)
}

/// Where a looping carousel is: how far it is scrolled, and which slide the
/// track starts at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopAt {
    pub position: f64,
    pub first: usize,
}

/// The page a looping carousel is heading for, and which way it is going there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Heading {
    pub page: usize,
    pub forward: bool,
}

/// A change to which slide the track starts at, and how far the scroll position
/// shifts with it so that the same slides stay in view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rotation {
    pub first: usize,
    pub delta: f64,
}

/// The slide at the start of the view, which stays put through a rotation.
fn anchor_slide(boxes: &[SlideBox], view_start: f64, first: usize) -> usize {
    let mut anchor = first;
    for (index, slide) in boxes.iter().enumerate() {
        if slide.start <= view_start + 0.5 && slide.start > boxes[anchor].start {
            anchor = index;
        }
    }
    anchor
}

/// The rotation a looping carousel should take. At rest it moves the current
/// page toward the middle of the track, so there is room to scroll either way.
/// Heading for a page, it makes sure that page lies ahead; with `lazy` it leaves
/// the track alone when it already does. `None` when the track is too short to
/// rotate.
#[must_use]
pub fn loop_rotation(
    track: &Track,
    view: &LoopView,
    at: LoopAt,
    to: Option<Heading>,
    lazy: bool,
) -> Option<Rotation> {
    let groups = loop_groups(track, view);
    let now = place_slides(track, at.first);
    let anchor = anchor_slide(&now, at.position + view.padding_start, at.first);
    // The track can start at any page's first slide. Where it starts now is
    // tried first, so it wins a tie.
    let mut starts: Vec<usize> = groups.iter().map(|group| group[0]).collect();
    if starts.contains(&at.first) {
        starts.insert(0, at.first);
    }
    // The best start is the one that leaves most room to scroll either way.
    let mut best: Option<(Rotation, f64)> = None;
    for first in starts {
        let boxes = if first == at.first {
            now.clone()
        } else {
            place_slides(track, first)
        };
        let delta = boxes[anchor].start - now[anchor].start;
        let position = at.position + delta;
        let mut room = position.min(view.max_scroll - position);
        if let Some(heading) = to {
            let page = &groups[heading.page];
            let target =
                page_of(&boxes, page, view.align, view.snapport, view.padding_start).position;
            let lies_ahead = if heading.forward {
                target >= position - 0.5
            } else {
                target <= position + 0.5
            };
            if !lies_ahead {
                continue;
            }
            room = room.min(target).min(view.max_scroll - target);
        }
        if room < -0.5 {
            continue;
        }
        if best.is_none_or(|(_, most)| room > most + 0.5) {
            best = Some((Rotation { first, delta }, room));
        }
        if lazy && first == at.first {
            break;
        }
    }
    best.map(|(rotation, _)| rotation)
}
