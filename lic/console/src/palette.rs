// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The classic palette: black background, green text, white for high
//! intensity, red for errors. Pixels are `0x00RRGGBB`.

use crate::screen::Color;

/// Screen background.
pub const BACKGROUND: u32 = 0x0000_0000;
/// Default text.
pub const GREEN: u32 = 0x0033_CC33;
/// High intensity.
pub const WHITE: u32 = 0x00E6_E6E6;
/// Errors and attention.
pub const RED: u32 = 0x00E0_3030;
/// Blue.
pub const BLUE: u32 = 0x0050_80FF;
/// Turquoise.
pub const TURQUOISE: u32 = 0x0020_C8C8;
/// Pink.
pub const PINK: u32 = 0x00E0_60C0;
/// Yellow.
pub const YELLOW: u32 = 0x00E6_E040;

/// Brighter variants used with the high-intensity attribute.
pub const GREEN_HI: u32 = 0x0066_FF66;
/// Brighter red.
pub const RED_HI: u32 = 0x00FF_5050;
/// Brighter blue.
pub const BLUE_HI: u32 = 0x0080_A8FF;
/// Brighter turquoise.
pub const TURQUOISE_HI: u32 = 0x0040_FFFF;
/// Brighter pink.
pub const PINK_HI: u32 = 0x00FF_80E0;
/// Brighter yellow.
pub const YELLOW_HI: u32 = 0x00FF_FF60;

/// Pixel value for a color, brighter when `high` is set. Green at high
/// intensity is white, as on a real terminal.
pub fn rgb(color: Color, high: bool) -> u32 {
    match (color, high) {
        (Color::Green, false) => GREEN,
        (Color::Green, true) | (Color::White, _) => WHITE,
        (Color::Red, false) => RED,
        (Color::Red, true) => RED_HI,
        (Color::Blue, false) => BLUE,
        (Color::Blue, true) => BLUE_HI,
        (Color::Turquoise, false) => TURQUOISE,
        (Color::Turquoise, true) => TURQUOISE_HI,
        (Color::Pink, false) => PINK,
        (Color::Pink, true) => PINK_HI,
        (Color::Yellow, false) => YELLOW,
        (Color::Yellow, true) => YELLOW_HI,
    }
}
