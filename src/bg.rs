use embedded_graphics::{
    image::Image,
    pixelcolor::BinaryColor,
    prelude::Point,
    primitives::{Circle, PrimitiveStyle, StyledDrawable},
    Drawable,
};
use tinybmp::Bmp;

use crate::DisplayType;

const BACKGROUND_WIDTH: i32 = 256;

pub struct Background<'a> {
    far_bg_shift: i32,
    near_bg_shift: i32,
    far_bg_shift_mod: i32,

    bg_far: Bmp<'a, BinaryColor>,
    bg_near: Bmp<'a, BinaryColor>,
}

impl<'a> Background<'a> {
    pub fn new() -> Self {
        // Bitmap image
        let bg_stars_data = include_bytes!("../sprites/bg_stars.bmp");
        let bg_meadow_data = include_bytes!("../sprites/background.bmp");
        // Parse the BMP file.
        let bg_stars = Bmp::from_slice(bg_stars_data).unwrap();
        let bg_meadow = Bmp::from_slice(bg_meadow_data).unwrap();

        Self {
            far_bg_shift: 0,
            near_bg_shift: 0,
            far_bg_shift_mod: 0,

            bg_far: bg_stars,
            bg_near: bg_meadow,
        }
    }

    pub fn shift_bg(&mut self, near_bg_shift: i32) {
        assert!(near_bg_shift < 0, "shift should always be less than zero");

        // BG states
        self.far_bg_shift_mod = (self.far_bg_shift_mod + 1) % 20;
        if self.far_bg_shift_mod == 9 {
            self.far_bg_shift = wrap_shift_background(self.far_bg_shift, -1);
        }

        self.near_bg_shift = wrap_shift_background(self.near_bg_shift, near_bg_shift);
    }

    pub fn draw_bg(&self, display: &mut DisplayType) {
        // The order of drawing is quite important, it also takes a bit of time.
        draw_background(display, &self.bg_far, self.far_bg_shift, 0);
        draw_background(display, &self.bg_near, self.near_bg_shift, 16);
        draw_moon(display, self.far_bg_shift);
    }
}

fn draw_background(display: &mut DisplayType, image: &Bmp<BinaryColor>, shift: i32, y: i32) {
    let _image = Image::new(image, Point::new(shift, y)).draw(display);
    let _image = Image::new(image, Point::new(shift + BACKGROUND_WIDTH, y)).draw(display);
}

fn draw_moon(display: &mut DisplayType, shift: i32) {
    let style = PrimitiveStyle::with_fill(BinaryColor::On);
    let _ = Circle::new(Point::new(shift + 40, 2), 10).draw_styled(&style, display);
    let _ =
        Circle::new(Point::new(shift + BACKGROUND_WIDTH + 40, 2), 10).draw_styled(&style, display);
}

// Only negative shift only
fn wrap_shift_background(pos: i32, shift: i32) -> i32 {
    if pos <= -BACKGROUND_WIDTH {
        pos + BACKGROUND_WIDTH + shift
    } else {
        pos + shift
    }
}
