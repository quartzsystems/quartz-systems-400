// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One running guest: serial in and out with expect-style matching.

use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::error::HarnessError;
use crate::qemu::{self, Launch};

/// The service console prompt.
pub const PROMPT: &str = "QS400-SVC> ";

/// A guest under test.
pub struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    buf: String,
    /// Everything received so far.
    pub log: String,
    started: Instant,
}

impl Session {
    /// Launch QEMU.
    pub fn spawn(launch: &Launch) -> Result<Self, HarnessError> {
        let _ = std::fs::remove_file(&launch.qmp);
        let mut child = Command::new(&launch.qemu)
            .args(qemu::args(launch))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| HarnessError::Spawn {
                qemu: launch.qemu.clone(),
                source,
            })?;
        let rx = qemu::serial::stream(child.stdout.take(), child.stderr.take());
        let stdin = child.stdin.take();
        Ok(Self {
            child,
            stdin,
            rx,
            buf: String::new(),
            log: String::new(),
            started: Instant::now(),
        })
    }

    /// QEMU's process ID.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Seconds since launch.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Wait until `needle` appears. Everything up to and including it is
    /// consumed; the text before it is returned.
    pub fn expect(&mut self, needle: &str, timeout: Duration) -> Result<String, String> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(i) = self.buf.find(needle) {
                let before = self.buf[..i].to_string();
                self.buf.drain(..i + needle.len());
                return Ok(before);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(format!("`{needle}` did not appear within {timeout:?}"));
            }
            match self.rx.recv_timeout(remaining) {
                Ok(chunk) => {
                    self.buf.push_str(&chunk);
                    self.log.push_str(&chunk);
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!("`{needle}` did not appear within {timeout:?}"));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(format!("qemu exited before `{needle}` appeared"));
                }
            }
        }
    }

    /// Wait for each needle in turn. Returns when each appeared.
    pub fn expect_in_order(
        &mut self,
        needles: &[&str],
        timeout: Duration,
    ) -> Result<Vec<(String, Duration)>, String> {
        let deadline = Instant::now() + timeout;
        let mut seen = Vec::with_capacity(needles.len());
        for n in needles {
            self.expect(n, deadline.saturating_duration_since(Instant::now()))?;
            seen.push(((*n).to_string(), self.elapsed()));
        }
        Ok(seen)
    }

    /// Type a line on the serial console.
    pub fn send(&mut self, line: &str) -> Result<(), String> {
        let stdin = self.stdin.as_mut().ok_or("serial input closed")?;
        stdin
            .write_all(format!("{line}\n").as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("serial write: {e}"))
    }

    /// Run a service console command: send it, wait for the next prompt,
    /// return the output with the echoed command line removed.
    pub fn command(&mut self, line: &str, timeout: Duration) -> Result<String, String> {
        self.send(line)?;
        let out = self.expect(PROMPT, timeout)?;
        let mut lines = out.lines();
        // The tty echoes what was typed; drop that first line.
        if let Some(first) = lines.next()
            && !first.trim().ends_with(line.trim())
        {
            return Ok(out);
        }
        Ok(lines.collect::<Vec<_>>().join("\n"))
    }

    /// Drop pending output (used before a fresh command).
    pub fn discard(&mut self) {
        while let Ok(chunk) = self.rx.try_recv() {
            self.log.push_str(&chunk);
        }
        self.buf.clear();
    }

    /// Kill the guest and reap it.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.discard();
    }

    /// Wait for QEMU to exit on its own.
    pub fn wait_exit(&mut self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(Some(_)) = self.child.try_wait() {
                self.discard();
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
