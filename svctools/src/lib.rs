// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Service tools: the Dedicated Service Tools screens that run before
//! the object store opens, the service tools user IDs, and the screen
//! kit the attended IPL and the installer are built from. Every screen
//! is a `console::Screen` built by [`ui`]; an application is a stack of
//! [`panel::Panel`]s driven by Enter and function keys from the console
//! and asking its environment ([`env::DstEnv`]) for everything else.
//! Phase 8's SST shows the same panels inside a job through another
//! environment.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod dst;
pub mod env;
pub mod panel;
pub mod ui;
pub mod users;

pub use env::DstEnv;
pub use panel::{Flow, Outcome, Panel, Runner};
pub use users::{Privilege, ServiceUser, UserDb};
