// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The boot test: the SRC sequence must appear on serial, in order,
//! within the timeout.

use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::error::HarnessError;
use crate::qemu;

/// First line of every IPL.
pub const SRC_IPL_STARTED: &str = "C1001000";
/// Last line of a successful IPL.
pub const SRC_IPL_COMPLETE: &str = "C6001090";

/// Codes that must appear in this order on every normal IPL. Codes not
/// listed (attention codes, the abnormal notice) may appear in between.
pub const SRC_SEQUENCE: &[&str] = &[
    SRC_IPL_STARTED,
    "C1001010",
    "C1001020",
    "C1001030",
    "C1001040",
    "C1001060",
    "C6001000",
    "C6001010",
    "C6001020",
    SRC_IPL_COMPLETE,
];

/// One boot test, fully resolved and ready to run.
#[derive(Debug)]
pub struct BootTest {
    /// Raw GPT disk image.
    pub image: PathBuf,
    /// QEMU binary.
    pub qemu: PathBuf,
    /// OVMF code image for `-drive if=pflash`.
    pub firmware: PathBuf,
    /// Use KVM with the host CPU model.
    pub kvm: bool,
    /// Time allowed for the whole sequence.
    pub timeout: Duration,
    /// Codes expected, in order.
    pub expected: Vec<&'static str>,
}

/// What a passing run observed.
#[derive(Debug)]
pub struct BootReport {
    /// Each expected code with the time it appeared.
    pub seen: Vec<(&'static str, Duration)>,
    /// Total wall time.
    pub elapsed: Duration,
    /// KVM was used.
    pub kvm: bool,
    /// Everything captured from serial and QEMU's stderr.
    pub log: String,
}

impl BootTest {
    /// Resolve QEMU, firmware and acceleration.
    /// `QS400_BOOT_TIMEOUT` (seconds) overrides the 60 s timeout.
    pub fn new(image: impl Into<PathBuf>) -> Result<Self, HarnessError> {
        let image = image.into();
        if !image.is_file() {
            return Err(HarnessError::ImageMissing(image));
        }
        let timeout = env::var("QS400_BOOT_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .map_or(Duration::from_secs(60), Duration::from_secs);
        Ok(Self {
            image,
            qemu: qemu::locate()?,
            firmware: qemu::firmware::locate()?,
            kvm: qemu::kvm_available(),
            timeout,
            expected: SRC_SEQUENCE.to_vec(),
        })
    }

    /// Boot, assert, kill QEMU. The log is returned either way.
    pub fn run(&self) -> Result<BootReport, HarnessError> {
        let start = Instant::now();
        let mut child = Command::new(&self.qemu)
            .args(qemu::args(self))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| HarnessError::Spawn {
                qemu: self.qemu.clone(),
                source,
            })?;
        let rx = qemu::serial::stream(child.stdout.take(), child.stderr.take());
        let mut log = String::new();

        let result = self.observe(&rx, &mut log, start);
        let _ = child.kill();
        let _ = child.wait();
        while let Ok(line) = rx.try_recv() {
            push(&mut log, &line);
        }

        match result {
            Ok(seen) => Ok(BootReport {
                seen,
                elapsed: start.elapsed(),
                kvm: self.kvm,
                log,
            }),
            Err(Pending::Exited(waiting_for)) => Err(HarnessError::QemuExited { waiting_for, log }),
            Err(Pending::Timeout(waiting_for)) => Err(HarnessError::Timeout {
                waiting_for,
                after: self.timeout,
                log,
            }),
        }
    }

    /// Consume serial until every expected code has been seen in order.
    fn observe(
        &self,
        rx: &Receiver<String>,
        log: &mut String,
        start: Instant,
    ) -> Result<Vec<(&'static str, Duration)>, Pending> {
        let deadline = start + self.timeout;
        let mut seen = Vec::with_capacity(self.expected.len());
        for code in &self.expected {
            let needle = format!("SRC {code}");
            loop {
                match recv_until(rx, deadline) {
                    Ok(line) => {
                        push(log, &line);
                        if line.contains(&needle) {
                            seen.push((*code, start.elapsed()));
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => return Err(Pending::Timeout(code)),
                    Err(RecvTimeoutError::Disconnected) => return Err(Pending::Exited(code)),
                }
            }
        }
        Ok(seen)
    }
}

/// Why `observe` stopped early.
enum Pending {
    Exited(&'static str),
    Timeout(&'static str),
}

fn recv_until(rx: &Receiver<String>, deadline: Instant) -> Result<String, RecvTimeoutError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(RecvTimeoutError::Timeout);
    }
    rx.recv_timeout(remaining)
}

fn push(log: &mut String, line: &str) {
    log.push_str(line);
    log.push('\n');
}
