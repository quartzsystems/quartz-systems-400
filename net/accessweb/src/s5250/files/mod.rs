// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The files QS/400 Access for Desktop keeps a 5250 session in, ported
//! from its `qsa-config` so one file serves both clients: session
//! (`.qsd`), keyboard map (`.qkm`, over [`keymap`]) and macro (`.qsm`).

pub mod doc;
pub mod keymap;
pub mod qkm;
pub mod qsd;
pub mod qsm;
