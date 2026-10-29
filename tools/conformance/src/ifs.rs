// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 QEMU step: the IFS root's filesystem mounted at IPL.

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The mount point slic gives the IFS.
const MOUNT_POINT: &str = "/run/qs400/ifs";

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 59: the QS400-IFS partition is mounted read-write as xfs,
    /// C6001012 was raised, and the capacity is the partition's.
    pub(crate) fn ifs_mount(&self, s: &mut Session) -> Result<String, String> {
        let srcs = s.command("srcs", CMD)?;
        if !srcs.contains("C6001012") {
            return Err(format!("no C6001012 in the SRC log:\n{srcs}"));
        }
        let out = s.command("svc.ifs", CMD)?;
        let mount = out
            .lines()
            .find(|l| l.starts_with("IFS /dev/"))
            .ok_or_else(|| format!("svc.ifs shows no mount:\n{out}"))?;
        if !mount.contains(&format!("on {MOUNT_POINT} type xfs (rw")) {
            return Err(format!("not xfs read-write at {MOUNT_POINT}: {mount}"));
        }
        if !out.contains("label QS400-IFS") {
            return Err(format!("svc.ifs shows no label:\n{out}"));
        }
        let mib = |key: &str| -> Option<u64> {
            let at = out.find(key)? + key.len();
            out[at..]
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
                .map(|b| b >> 20)
        };
        let (Some(size), Some(free)) = (mib("capacity "), mib("free ")) else {
            return Err(format!("svc.ifs shows no capacity:\n{out}"));
        };
        if size < 400 || free == 0 || free > size {
            return Err(format!(
                "implausible capacity {size} MiB, {free} MiB free:\n{out}"
            ));
        }
        let source = mount.split_whitespace().nth(1).unwrap_or("?");
        Ok(format!(
            "{source} mounted rw at {MOUNT_POINT}, {size} MiB, {free} MiB free"
        ))
    }
}
