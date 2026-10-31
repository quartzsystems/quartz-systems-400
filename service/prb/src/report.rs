// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reporting a problem to the service provider: the hook the update
//! service's client fills in. Until it does, [`Unconnected`] keeps the
//! prepared report and the problem stays Prepared.

/// What sends a prepared report.
pub trait Sender {
    /// Send the report of problem `id`; the provider's reference
    /// (becomes a note), or why it was not sent.
    fn send(&mut self, id: &str, report: &str) -> Result<String, String>;
}

/// No connection to a service provider.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unconnected;

/// Why [`Unconnected`] sends nothing.
pub const NOT_CONNECTED: &str = "no connection to a service provider is configured";

impl Sender for Unconnected {
    fn send(&mut self, _id: &str, _report: &str) -> Result<String, String> {
        Err(NOT_CONNECTED.into())
    }
}

/// What reporting a problem did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reported {
    /// Sent: the provider's reference.
    Sent(String),
    /// Prepared and kept; why it was not sent.
    Kept(String),
}
