//! Panning and zooming a canvas, and converting between screen and canvas positions.

use crate::geometry::{Point, Rect, Size};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Where the canvas origin sits on screen.
    pub pan: Point,
    pub zoom: f64,
    pub min_zoom: f64,
    pub max_zoom: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            pan: Point::default(),
            zoom: 1.0,
            min_zoom: 0.1,
            max_zoom: 8.0,
        }
    }
}

impl Viewport {
    #[must_use]
    pub fn to_canvas(&self, screen: Point) -> Point {
        Point::new(
            (screen.x - self.pan.x) / self.zoom,
            (screen.y - self.pan.y) / self.zoom,
        )
    }

    #[must_use]
    pub const fn to_screen(&self, canvas: Point) -> Point {
        Point::new(
            canvas.x.mul_add(self.zoom, self.pan.x),
            canvas.y.mul_add(self.zoom, self.pan.y),
        )
    }

    pub fn pan_by(&mut self, delta: Point) {
        self.pan = self.pan.plus(delta);
    }

    /// Zooms by `factor` while keeping the canvas point under `screen` where it is,
    /// as pinching or scrolling the wheel over a spot does.
    pub fn zoom_at(&mut self, screen: Point, factor: f64) {
        let held = self.to_canvas(screen);
        self.zoom = (self.zoom * factor).clamp(self.min_zoom, self.max_zoom);
        self.pan = Point::new(
            held.x.mul_add(-self.zoom, screen.x),
            held.y.mul_add(-self.zoom, screen.y),
        );
    }

    /// Shows all of `content`, centred in a view of `size` with `padding` around it.
    pub fn fit(&mut self, content: Rect, size: Size, padding: f64) {
        let room = Size {
            width: 2.0f64.mul_add(-padding, size.width).max(1.0),
            height: 2.0f64.mul_add(-padding, size.height).max(1.0),
        };
        let scale =
            (room.width / content.width.max(1.0)).min(room.height / content.height.max(1.0));
        self.zoom = scale.clamp(self.min_zoom, self.max_zoom);
        let center = content.center();
        self.pan = Point::new(
            center.x.mul_add(-self.zoom, size.width / 2.0),
            center.y.mul_add(-self.zoom, size.height / 2.0),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn screen_and_canvas_positions_convert_both_ways() {
        let view = Viewport {
            pan: Point::new(30., -10.),
            zoom: 2.5,
            ..Default::default()
        };
        let canvas = Point::new(12., 40.);
        assert!(close(view.to_canvas(view.to_screen(canvas)), canvas));
        assert!(close(view.to_screen(Point::default()), view.pan));
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer_fixed() {
        let mut view = Viewport {
            pan: Point::new(50., 50.),
            ..Default::default()
        };
        let pointer = Point::new(200., 120.);
        let before = view.to_canvas(pointer);
        view.zoom_at(pointer, 3.0);
        assert_eq!(view.zoom, 3.0);
        assert!(close(view.to_canvas(pointer), before));
    }

    #[test]
    fn zoom_stays_within_its_limits() {
        let mut view = Viewport::default();
        view.zoom_at(Point::default(), 1000.0);
        assert_eq!(view.zoom, 8.0);
        view.zoom_at(Point::default(), 0.000001);
        assert_eq!(view.zoom, 0.1);
    }

    #[test]
    fn fitting_centres_the_content_inside_the_padding() {
        let mut view = Viewport::default();
        let content = Rect::new(100., 100., 400., 200.);
        view.fit(
            content,
            Size {
                width: 500.,
                height: 500.,
            },
            50.,
        );
        assert_eq!(view.zoom, 1.0);
        assert!(close(
            view.to_screen(content.center()),
            Point::new(250., 250.)
        ));
        let left_edge = view.to_screen(Point::new(100., 100.)).x;
        assert!(left_edge >= 50.0 - 1e-9);
    }
}
