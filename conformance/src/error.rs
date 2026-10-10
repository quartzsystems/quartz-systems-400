// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Harness errors. Failures that happen after QEMU started carry the
//! captured serial log.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use thiserror::Error;

/// Why a boot test could not run or did not pass.
#[derive(Debug, Error)]
pub enum HarnessError {
    /// The disk image (or its pointer file) is missing.
    #[error("disk image not found: {0}")]
    ImageMissing(PathBuf),
    /// QEMU is not installed or not on `PATH`.
    #[error("{binary} not found on PATH (or set {env})")]
    QemuNotFound {
        /// Binary name.
        binary: &'static str,
        /// Override variable.
        env: &'static str,
    },
    /// No OVMF code image was found.
    #[error("OVMF not found; set {env} or install it ({hint}); tried {tried:?}")]
    FirmwareNotFound {
        /// Variable that names the firmware file.
        env: &'static str,
        /// Paths that were tried.
        tried: Vec<PathBuf>,
        /// Package hint.
        hint: &'static str,
    },
    /// QEMU could not be started.
    #[error("cannot start {qemu}: {source}")]
    Spawn {
        /// Binary that was launched.
        qemu: PathBuf,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
    /// QEMU exited before the expected line appeared.
    #[error("qemu exited before `{waiting_for}` appeared")]
    QemuExited {
        /// The line being waited for.
        waiting_for: &'static str,
        /// Serial log so far.
        log: String,
    },
    /// The expected line did not appear in time.
    #[error("`{waiting_for}` did not appear within {after:?}")]
    Timeout {
        /// The line being waited for.
        waiting_for: &'static str,
        /// How long we waited.
        after: Duration,
        /// Serial log so far.
        log: String,
    },
    /// Not enough heartbeat lines in the window.
    #[error("only {seen} heartbeat line(s) within {window:?}, need {need}")]
    TooFewHeartbeats {
        /// Heartbeats seen.
        seen: usize,
        /// Heartbeats required.
        need: usize,
        /// Observation window.
        window: Duration,
        /// Serial log so far.
        log: String,
    },
    /// Any other I/O problem.
    #[error(transparent)]
    Io(#[from] io::Error),
}

impl HarnessError {
    /// The captured serial log, when the failure happened after boot started.
    pub fn log(&self) -> Option<&str> {
        match self {
            HarnessError::QemuExited { log, .. }
            | HarnessError::Timeout { log, .. }
            | HarnessError::TooFewHeartbeats { log, .. } => Some(log),
            _ => None,
        }
    }
}
