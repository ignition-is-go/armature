//! The geometry of cropping an image. Reading and writing pixels belongs to the
//! platform; this decides what to cut and how large the result is.

/// A quarter turn clockwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rotation {
    #[default]
    None,
    Quarter,
    Half,
    ThreeQuarters,
}

impl Rotation {
    pub fn degrees(self) -> u16 {
        match self {
            Rotation::None => 0,
            Rotation::Quarter => 90,
            Rotation::Half => 180,
            Rotation::ThreeQuarters => 270,
        }
    }

    /// The image's size once turned. An odd quarter turn swaps width and height.
    pub fn turned(self, width: f64, height: f64) -> (f64, f64) {
        match self {
            Rotation::Quarter | Rotation::ThreeQuarters => (height, width),
            _ => (width, height),
        }
    }
}

/// A region of an image, measured from the edges of the image after it is turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: Rotation,
    pub flip_x: bool,
    pub flip_y: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmptyCrop;

impl Crop {
    /// The pixel size of the cropped file. With neither dimension asked for it is the
    /// crop's own size; with one, the other keeps the crop's proportions. Each side is
    /// at least one pixel.
    pub fn output_size(
        &self,
        width: Option<f64>,
        height: Option<f64>,
    ) -> Result<(u32, u32), EmptyCrop> {
        if self.width <= 0.0 || self.height <= 0.0 {
            return Err(EmptyCrop);
        }
        let ratio = self.width / self.height;
        let out_width = width.unwrap_or_else(|| height.map_or(self.width, |height| height * ratio));
        let out_height = height.unwrap_or(out_width / ratio);
        let pixels = |side: f64| side.round().max(1.0) as u32;
        Ok((pixels(out_width), pixels(out_height)))
    }

    /// The crop moved and shrunk as little as needed to lie inside an image of the
    /// given size (before turning).
    pub fn clamped(&self, image_width: f64, image_height: f64) -> Crop {
        let (bound_w, bound_h) = self.rotation.turned(image_width, image_height);
        let width = self.width.min(bound_w);
        let height = self.height.min(bound_h);
        Crop {
            x: self.x.clamp(0.0, bound_w - width),
            y: self.y.clamp(0.0, bound_h - height),
            width,
            height,
            ..*self
        }
    }
}

/// The name for a cropped file: the source's name with the extension of the type it
/// was written as.
pub fn cropped_name(source_name: Option<&str>, mime_type: &str) -> String {
    let extension = match mime_type {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        other => other
            .rsplit('/')
            .next()
            .filter(|sub| !sub.is_empty())
            .unwrap_or("png"),
    };
    let base = source_name
        .map(|name| name.rsplit_once('.').map_or(name, |(base, _)| base))
        .filter(|base| !base.is_empty());
    format!("{}.{extension}", base.unwrap_or("image"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crop(width: f64, height: f64) -> Crop {
        Crop {
            x: 0.0,
            y: 0.0,
            width,
            height,
            rotation: Rotation::None,
            flip_x: false,
            flip_y: false,
        }
    }

    #[test]
    fn the_output_keeps_the_crops_proportions() {
        assert_eq!(crop(400.0, 200.0).output_size(None, None), Ok((400, 200)));
        assert_eq!(
            crop(400.0, 200.0).output_size(Some(100.0), None),
            Ok((100, 50))
        );
        assert_eq!(
            crop(400.0, 200.0).output_size(None, Some(100.0)),
            Ok((200, 100))
        );
        assert_eq!(
            crop(400.0, 200.0).output_size(Some(64.0), Some(64.0)),
            Ok((64, 64))
        );
    }

    #[test]
    fn an_output_side_is_never_less_than_a_pixel_and_an_empty_crop_is_refused() {
        assert_eq!(crop(1000.0, 1.0).output_size(Some(10.0), None), Ok((10, 1)));
        assert_eq!(crop(0.0, 50.0).output_size(None, None), Err(EmptyCrop));
    }

    #[test]
    fn a_quarter_turn_swaps_the_sides_a_crop_is_held_within() {
        assert_eq!(Rotation::Quarter.turned(300.0, 100.0), (100.0, 300.0));
        let turned = Crop {
            x: 90.0,
            y: 250.0,
            rotation: Rotation::Quarter,
            ..crop(50.0, 80.0)
        };
        let held = turned.clamped(300.0, 100.0);
        assert_eq!((held.x, held.y), (50.0, 220.0));
    }

    #[test]
    fn a_cropped_file_is_named_after_its_source_and_its_type() {
        assert_eq!(
            cropped_name(Some("holiday.heic"), "image/jpeg"),
            "holiday.jpg"
        );
        assert_eq!(cropped_name(None, "image/webp"), "image.webp");
        assert_eq!(cropped_name(Some("scan"), "image/avif"), "scan.avif");
    }
}
