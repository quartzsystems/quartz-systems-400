// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The local console: a 24×80 cell grid with 5250 attributes, scaled to
//! the Linux framebuffer with a Quartz-drawn 9×16 font, or drawn on a
//! serial terminal with ANSI sequences ([`ansi`]); both are a
//! [`Display`]. The SRC display, evdev keyboards and 5250 field
//! editing. What names a product on a screen comes from the caller
//! ([`Identity`]).
//!
//! The feature `session` (on by default) adds the sign-on display and
//! the console as the terminal of a 5250 session.
//!
//! `unsafe` lives only in `fb::raw` (two ioctls).

#![deny(unsafe_code)]

pub mod ansi;
pub mod display;
pub mod editor;
pub mod font;
pub mod keyboard;
pub mod palette;
pub mod scale;
pub mod screen;
pub mod screens;
#[cfg(feature = "session")]
pub mod terminal;

#[cfg(target_os = "linux")]
pub mod fb;
#[cfg(target_os = "linux")]
mod render;
#[cfg(target_os = "linux")]
mod service;

#[cfg(target_os = "linux")]
pub use service::{Command, Console, INHIBITED, Identity, Input};

pub use display::Display;
pub use keyboard::map::Key;
pub use scale::Scale;
pub use screen::{Attr, COLS, Cell, Color, Field, ROWS, Screen};
