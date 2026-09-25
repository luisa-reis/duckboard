//! The few colours the dashboard uses. Kept apart so the whole panel can be
//! retuned in one place; LEDs render saturated colours far stronger than a
//! monitor does.

use embedded_graphics::pixelcolor::Rgb888;

pub const WHITE: Rgb888 = Rgb888::new(255, 255, 255);
pub const GREY: Rgb888 = Rgb888::new(110, 110, 110);
pub const DIM: Rgb888 = Rgb888::new(45, 45, 45);
pub const AMBER: Rgb888 = Rgb888::new(255, 170, 0);
pub const SKY: Rgb888 = Rgb888::new(80, 170, 255);
pub const BLACK: Rgb888 = Rgb888::new(0, 0, 0);
