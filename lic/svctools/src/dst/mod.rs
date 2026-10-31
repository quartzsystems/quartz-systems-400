// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Dedicated Service Tools: sign-on, the main menu, and the menus
//! under it. Titles and option numbers are the functional interface;
//! every sentence is Quartz's.

pub mod common;
pub mod console_mode;
pub mod disk;
pub mod environment;
pub mod lic;
pub mod menu;
pub mod security;
pub mod signon;
pub mod tools;

use crate::panel::Runner;

/// Shown for an option that exists on the menu but does nothing yet.
pub const NOT_AVAILABLE: &str = "Option not available in this release";
/// Shown when the user lacks the privilege an option needs.
pub const NOT_AUTHORIZED: &str = "Service tools user ID is not authorized to this option";
/// Shown for a selection that is not an option number.
pub const INVALID_SELECTION: &str = "Selection is not valid";
/// Shown when the system values are not loaded yet.
pub const NOT_LOADED: &str = "System values are not loaded yet";

/// Shown when a function fails inside the machine: what was being
/// done, never the internal reason (that goes to the console log).
#[must_use]
pub fn failed(what: &str) -> String {
    format!("{what} did not complete; the console log has the reason")
}

/// Shown for a value a system value does not accept.
#[must_use]
pub fn value_not_valid(name: &str, value: &str) -> String {
    format!("Value '{}' is not valid for {name}", value.trim())
}

/// Shown when the media's release cannot go over the installed one.
#[must_use]
pub fn upgrade_refused(media: &str, installed: &str) -> String {
    format!("Release {media} cannot be installed over release {installed}")
}

/// The DST application, starting at sign-on.
#[must_use]
pub fn start() -> Runner {
    Runner::new(Box::new(signon::SignOn::default()))
}

/// Parse a selection field as an option number.
#[must_use]
pub fn selection(fields: &[String]) -> Option<u8> {
    fields.first()?.trim().parse().ok()
}
