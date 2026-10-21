// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Starting a child: its end of a socket pair becomes its standard
//! input, its output and errors go to the serial line.

use std::process::{Command, Stdio};

use ipc::chan::Chan;

use crate::platform::console::{SERIAL, open_write};

/// The storage service is this binary in another role.
pub const SELF: &str = "/init";
/// The job process.
pub const QSJOB: &str = "/qsjob";

/// Start `path` with `args`; its process ID and the channel to it.
pub fn spawn(path: &str, args: &[&str]) -> Result<(u32, Chan), String> {
    let (parent, child) = Chan::pair().map_err(|e| e.to_string())?;
    let out = |what: &str| -> Result<Stdio, String> {
        open_write(SERIAL)
            .map(Stdio::from)
            .ok_or_else(|| format!("{SERIAL} for {what}: not writable"))
    };
    let c = Command::new(path)
        .args(args)
        .env_clear()
        .stdin(Stdio::from(child.into_fd()))
        .stdout(out("stdout").unwrap_or_else(|_| Stdio::null()))
        .stderr(out("stderr").unwrap_or_else(|_| Stdio::null()))
        .spawn()
        .map_err(|e| format!("{path}: {e}"))?;
    Ok((c.id(), parent))
}

/// Wait up to `limit` for `pid` to exit, then kill it.
pub fn wait_or_kill(pid: u32, limit: std::time::Duration) {
    use rustix::process::{Pid, Signal, WaitOptions, kill_process, waitpid};
    let Some(p) = Pid::from_raw(i32::try_from(pid).unwrap_or(0)) else {
        return;
    };
    let until = std::time::Instant::now() + limit;
    while std::time::Instant::now() < until {
        match waitpid(Some(p), WaitOptions::NOHANG) {
            Ok(Some(_)) | Err(_) => return,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
    let _ = kill_process(p, Signal::KILL);
    let _ = waitpid(Some(p), WaitOptions::empty());
}
