// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The 5250 session's own parts: the console's look ([`look`]), the
//! screen model the pages draw ([`screen`]), and the desktop's session,
//! keyboard map and macro files ([`files`]).

pub mod files;
pub mod look;
pub mod screen;
