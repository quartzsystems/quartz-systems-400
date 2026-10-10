// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Phase 0 boot test: IPL SRC within the timeout, then heartbeats.

use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::error::HarnessError;
use crate::qemu;

/// Serial line slic prints when the IPL starts.
pub const SRC_IPL_STARTED: &str = "SRC C1001000";
/// Serial line slic prints every five seconds.
pub const SRC_HEARTBEAT: &str = "SRC C1001005";

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
    /// Time allowed for the IPL SRC to appear.
    pub ipl_timeout: Duration,
    /// Window after the IPL SRC in which heartbeats are counted.
    pub heartbeat_window: Duration,
    /// Heartbeats required inside the window.
    pub heartbeat_min: usize,
}

/// What a passing run observed.
#[derive(Debug)]
pub struct BootReport {
    /// Time from QEMU start to the IPL SRC line.
    pub ipl_after: Duration,
    /// Heartbeat lines counted.
    pub heartbeats: usize,
    /// Total wall time.
    pub elapsed: Duration,
    /// KVM was used.
    pub kvm: bool,
    /// Everything captured from serial and QEMU's stderr.
    pub log: String,
}

impl BootTest {
    /// Resolve QEMU, firmware and acceleration.
    /// `QS400_BOOT_TIMEOUT` (seconds) overrides the 60 s IPL timeout.
    pub fn new(image: impl Into<PathBuf>) -> Result<Self, HarnessError> {
        let image = image.into();
        if !image.is_file() {
            return Err(HarnessError::ImageMissing(image));
        }
        let ipl_timeout = env::var("QS400_BOOT_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .map_or(Duration::from_secs(60), Duration::from_secs);
        Ok(Self {
            image,
            qemu: qemu::locate()?,
            firmware: qemu::firmware::locate()?,
            kvm: qemu::kvm_available(),
            ipl_timeout,
            heartbeat_window: Duration::from_secs(15),
            heartbeat_min: 2,
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
        // Drain whatever arrived while shutting down.
        while let Ok(line) = rx.try_recv() {
            push(&mut log, &line);
        }

        match result {
            Ok((ipl_after, heartbeats)) => Ok(BootReport {
                ipl_after,
                heartbeats,
                elapsed: start.elapsed(),
                kvm: self.kvm,
                log,
            }),
            Err(Pending::Exited(waiting_for)) => Err(HarnessError::QemuExited { waiting_for, log }),
            Err(Pending::Timeout(waiting_for, after)) => Err(HarnessError::Timeout {
                waiting_for,
                after,
                log,
            }),
            Err(Pending::Heartbeats(seen)) => Err(HarnessError::TooFewHeartbeats {
                seen,
                need: self.heartbeat_min,
                window: self.heartbeat_window,
                log,
            }),
        }
    }

    /// Phase 1: wait for the IPL SRC. Phase 2: count heartbeats.
    fn observe(
        &self,
        rx: &Receiver<String>,
        log: &mut String,
        start: Instant,
    ) -> Result<(Duration, usize), Pending> {
        let deadline = start + self.ipl_timeout;
        loop {
            match recv_until(rx, deadline) {
                Ok(line) => {
                    push(log, &line);
                    if line.contains(SRC_IPL_STARTED) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(Pending::Timeout(SRC_IPL_STARTED, self.ipl_timeout));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(Pending::Exited(SRC_IPL_STARTED));
                }
            }
        }
        let ipl_after = start.elapsed();

        let deadline = Instant::now() + self.heartbeat_window;
        let mut beats = 0;
        while beats < self.heartbeat_min {
            match recv_until(rx, deadline) {
                Ok(line) => {
                    push(log, &line);
                    if line.contains(SRC_HEARTBEAT) {
                        beats += 1;
                    }
                }
                Err(_) => break,
            }
        }
        if beats < self.heartbeat_min {
            return Err(Pending::Heartbeats(beats));
        }
        Ok((ipl_after, beats))
    }
}

/// Why `observe` stopped early.
enum Pending {
    Exited(&'static str),
    Timeout(&'static str, Duration),
    Heartbeats(usize),
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
