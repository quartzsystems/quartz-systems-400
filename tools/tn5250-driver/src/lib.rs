// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The scripted 5250 display as a library: the conformance harness
//! drives QS/400 under QEMU with it, the `tn5250-driver` binary drives
//! the oracle systems.

pub mod env;
pub mod ftp;
pub mod golden;
pub mod mask;
pub mod script;
pub mod session;
pub mod trace;

use std::path::PathBuf;

/// Which system a session talks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The QS/400 under test.
    Qs400,
    /// The primary oracle.
    Oracle,
    /// pub400.com: read-only, throttled.
    Pub400,
}

impl Target {
    /// The name used in golden paths and the index.
    pub fn name(self) -> &'static str {
        match self {
            Self::Qs400 => "qs400",
            Self::Oracle => "oracle",
            Self::Pub400 => "pub400",
        }
    }
}

/// The auto-signon substitute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Encoding {
    /// DES (password levels 0 and 1).
    Des,
    /// SHA-1 (password levels 2 and 3).
    #[default]
    Sha,
}

/// The models the driver negotiates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ModelArg {
    /// `IBM-3179-2`, 24×80.
    #[default]
    M3179,
    /// `IBM-3477-FC`, 27×132.
    M3477,
}

/// How to connect and what to say.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)] // the switches of a session
pub struct Connect {
    /// The system.
    pub target: Target,
    /// The host.
    pub host: String,
    /// The telnet port.
    pub port: u16,
    /// The user signing on.
    pub user: String,
    /// The password.
    pub password: String,
    /// Send the password at negotiation (RFC 4777).
    pub auto_signon: bool,
    /// The substitute encoding.
    pub encoding: Encoding,
    /// The device name to ask for.
    pub device: String,
    /// The terminal model.
    pub model: ModelArg,
    /// Claim GUI support.
    pub gui: bool,
    /// Print the negotiation and every record.
    pub trace: bool,
    /// With `trace`: the records in hex too.
    pub hex: bool,
}

impl Connect {
    /// A connection to QS/400 under QEMU on a forwarded port.
    pub fn qs400(port: u16, user: &str, password: &str) -> Self {
        Self {
            target: Target::Qs400,
            host: "127.0.0.1".into(),
            port,
            user: user.into(),
            password: password.into(),
            auto_signon: false,
            encoding: Encoding::Sha,
            device: String::new(),
            model: ModelArg::M3179,
            gui: false,
            trace: false,
            hex: false,
        }
    }
}

/// What the scripts of a session share.
#[derive(Clone, Debug)]
pub struct ScriptOptions {
    /// Where snapshots go (and are compared against).
    pub golden: Option<PathBuf>,
    /// Write snapshots instead of comparing.
    pub update: bool,
    /// The release recorded in the index.
    pub release: String,
    /// Milliseconds a `wait` gives up after.
    pub timeout_ms: u64,
}

impl Default for ScriptOptions {
    fn default() -> Self {
        Self {
            golden: None,
            update: false,
            release: String::new(),
            timeout_ms: 15000,
        }
    }
}
