// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The local console: a 24×80 cell grid with 5250 attributes, rendered to
//! the Linux framebuffer with a Quartz-drawn 9×16 font. Screens for the
//! IPL (SRC display, IPL menu) and the sign-on display. The keyboard and
//! field editing arrive with the sign-on screen.
//!
//! `unsafe` lives only in `fb::raw` (two ioctls).

#![deny(unsafe_code)]

pub mod font;
pub mod palette;
pub mod screen;
pub mod screens;

#[cfg(target_os = "linux")]
pub mod fb;
#[cfg(target_os = "linux")]
mod render;
#[cfg(target_os = "linux")]
mod service;

#[cfg(target_os = "linux")]
pub use service::{Command, Console};

pub use screen::{Attr, COLS, Cell, Color, Field, ROWS, Screen};
