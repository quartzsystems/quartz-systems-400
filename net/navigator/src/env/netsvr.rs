// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NetServer: the shares, attributes, sessions and disabled users the
//! `QZLS*` APIs list, and every change through the same APIs
//! (`netsvrcfg::client`), as the signed-on user: the API checks the
//! authority, validates and words the refusal.

#![allow(clippy::doc_markdown)] // NetServer

pub use netsvrcfg::{Attributes, Session, Settings, Share};
use qsweb::msg::Message;

/// Whether the server runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    /// `QZLSSERVER` is active in `QSERVER`.
    pub active: bool,
    /// Its job (`number/QSYS/QZLSSERVER`), empty when not active.
    pub job: String,
}

/// A user disabled for NetServer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Disabled {
    /// The profile.
    pub user: String,
    /// When (seconds since 1970).
    pub since: u64,
}

/// What the NetServer pages need. `user` is the signed-on user; each
/// `Err` is the API's message.
pub trait NetServer: Send + Sync {
    /// Whether the server runs.
    fn status(&self, user: &str) -> Status;
    /// Every share, by name (`QZLSOLST ZLSL0101`).
    fn shares(&self, user: &str) -> Result<Vec<Share>, Message>;
    /// The attributes, current and pending (`ZLSL0201`, `QSLL0100`).
    fn attributes(&self, user: &str) -> Result<Attributes, Message>;
    /// The sessions and their connections.
    fn sessions(&self, user: &str) -> Vec<Session>;
    /// The users disabled for NetServer, by name.
    fn disabled(&self, user: &str) -> Vec<Disabled>;
    /// Add share `s` (`QZLSADFS`, `QZLSADPS`).
    fn add_share(&self, user: &str, s: &Share) -> Result<(), Message>;
    /// Change share `s` (`QZLSCHFS`, `QZLSCHPS`).
    fn change_share(&self, user: &str, s: &Share) -> Result<(), Message>;
    /// Remove share `name` (`QZLSRMS`).
    fn remove_share(&self, user: &str, name: &str) -> Result<(), Message>;
    /// Take the pending attributes `from` to `to` (`QZLSCHSN`,
    /// `QZLSCHSG`, `QZLSCHSI`).
    fn change_attributes(&self, user: &str, from: &Settings, to: &Settings) -> Result<(), Message>;
    /// Start the server (`QZLSSTRS`); `reset` makes the pending
    /// attributes current.
    fn start(&self, user: &str, reset: bool) -> Result<(), Message>;
    /// End it (`QZLSENDS`).
    fn end(&self, user: &str) -> Result<(), Message>;
    /// End session `id` (`QZLSENSS`).
    fn end_session(&self, user: &str, id: u64) -> Result<(), Message>;
    /// Enable `name` for NetServer again (`QZLSCHSI ZLSS0200`).
    fn enable(&self, user: &str, name: &str) -> Result<(), Message>;
}
