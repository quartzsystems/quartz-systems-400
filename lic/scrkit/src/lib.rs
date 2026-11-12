// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The screen kit an attended IPL, an installer or a service tools
//! console is built from. Every screen is a `console::Screen` made by
//! the builders in [`ui`]; an application is a stack of panels
//! ([`panel`]) driven by Enter and the function keys.
//!
//! The kit knows no product. Its fixed text (`Selection`, `Select one
//! of the following:`, `System:`) is Quartz's own; titles, the system
//! name, the function key line and every message come from the caller,
//! as does the environment its panels ask for everything else.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod panel;
pub mod ui;

pub use panel::{End, Flow, Panel, Runner, Stack};
