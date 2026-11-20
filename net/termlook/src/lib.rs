// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The terminal's look for any product that shows a console or a 5250
//! screen in a browser: the console's 9x16 font and palette, its rules
//! for drawing a cell ([`look`]), the screen drawn to a picture
//! ([`screen`]: one model, fed from a 5250 screen or from a partition
//! console's grid), the picture as PNG or Print Screen's PDF
//! ([`file`]), and the page's script ([`script`]) with the exchange it
//! speaks ([`exchange`]). It names no product; `docs/WEB_STYLE.md` and
//! `docs/BUILDKIT.md` say how a product uses it.

#![forbid(unsafe_code)]

pub mod exchange;
pub mod file;
pub mod look;
pub mod screen;
pub mod script;
