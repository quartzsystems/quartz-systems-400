// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 QEMU step: the IFS root's filesystem mounted at IPL.

use std::time::Duration;

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The mount point slic gives the IFS.
const MOUNT_POINT: &str = "/run/qs400/ifs";

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 59: the IFS volume in ASP 1, served as `/dev/ublkbN`, is
    /// mounted read-write as xfs (C6001012) with a plausible capacity;
    /// `QSGROWIFS` grows it and the filesystem with it; the third-party
    /// notices are in `/QQS/ProdData/Notices`.
    pub(crate) fn ifs_mount(&self, s: &mut Session) -> Result<String, String> {
        let srcs = s.command("srcs", CMD)?;
        if !srcs.contains("C6001012") {
            return Err(format!("no C6001012 in the SRC log:\n{srcs}"));
        }
        let out = s.command("svc.ifs", CMD)?;
        let mount = out
            .lines()
            .find(|l| l.starts_with("IFS /dev/ublkb"))
            .ok_or_else(|| format!("svc.ifs shows no ublk mount:\n{out}"))?;
        if !mount.contains(&format!("on {MOUNT_POINT} type xfs (rw")) {
            return Err(format!("not xfs read-write at {MOUNT_POINT}: {mount}"));
        }
        if !out.contains("label QS400-IFS") || !out.contains("volume in ASP 1") {
            return Err(format!("svc.ifs shows no label or volume:\n{out}"));
        }
        let (size, free) = capacity(&out)?;
        if size < 400 || free == 0 || free > size {
            return Err(format!(
                "implausible capacity {size} MiB, {free} MiB free:\n{out}"
            ));
        }
        let grow = s.command("QSGROWIFS SIZE(64)", Duration::from_mins(2))?;
        if !grow.contains("CPC3E20") {
            return Err(format!("QSGROWIFS:\n{grow}"));
        }
        let after = s.command("svc.ifs", CMD)?;
        let (grown, _) = capacity(&after)?;
        if grown < size + 60 {
            return Err(format!(
                "the filesystem did not grow: {size} MiB, then {grown} MiB:\n{after}"
            ));
        }
        // Still usable after the growth.
        let put = s.command("svc.ifs put /tmp/qs59.txt grown", CMD)?;
        let cat = s.command("svc.ifs cat /tmp/qs59.txt", CMD)?;
        if !cat.contains("grown") {
            return Err(format!("a file after the growth:\n{put}\n{cat}"));
        }
        // The third-party notices the storage service keeps (docs/INSTALL.md).
        let notices = s.command("svc.ifs stat /QQS/ProdData/Notices/NOTICES.txt", CMD)?;
        let bytes: u64 = notices
            .lines()
            .find_map(|l| l.strip_prefix("Size"))
            .and_then(|r| r.split_whitespace().next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        if !notices.contains("Owner      QSYS") || bytes < 100_000 {
            return Err(format!("no notices in /QQS/ProdData/Notices:\n{notices}"));
        }
        let source = mount.split_whitespace().nth(1).unwrap_or("?");
        Ok(format!(
            "{source} (in ASP 1) mounted rw at {MOUNT_POINT}, {size} MiB, {free} MiB free; QSGROWIFS SIZE(64) made it {grown} MiB; /QQS/ProdData/Notices/NOTICES.txt present"
        ))
    }

    /// Step 63: the IFS commands at the service console: MD, a stream
    /// file into a source member and back (CPYFRMSTMF, CPYTOSTMF), the
    /// WRKLNK and DSPF text forms, DEL with a pattern, RD.
    pub(crate) fn ifs_commands(&self, s: &mut Session) -> Result<String, String> {
        let want = |out: &str, what: &str, needles: &[&str]| -> Result<(), String> {
            if needles.iter().all(|n| out.contains(n)) {
                Ok(())
            } else {
                Err(format!("{what}: expected {needles:?}:\n{out}"))
            }
        };
        let made = s.command("md DIR('/home/qs63')", CMD)?;
        want(&made, "MD", &["/home/qs63"])?;
        s.command("crtsrcpf FILE(QGPL/QIFSSRC) RCDLEN(92)", CMD)?;
        crate::qir::qemu::push(s, "IFSSRC", SOURCE)?;
        let put = s.command(
            "svc.srcput QGPL/QIFSSRC IN SRCTYPE(CLLE) FROMSTMF(IFSSRC)",
            CMD,
        )?;
        want(&put, "srcput", &["written"])?;
        let back = s.command(
            "cpytostmf FROMMBR('/QSYS.LIB/QGPL.LIB/QIFSSRC.FILE/IN.MBR') TOSTMF('/home/qs63/out.txt') STMFCCSID(1208)",
            CMD,
        )?;
        want(&back, "CPYTOSTMF", &["copied"])?;
        let into = s.command(
            "cpyfrmstmf FROMSTMF('/home/qs63/out.txt') TOMBR('/QSYS.LIB/QGPL.LIB/QIFSSRC.FILE/IN2.MBR') MBROPT(*REPLACE)",
            CMD,
        )?;
        want(&into, "CPYFRMSTMF", &["copied"])?;
        let member = s.command("svc.srcget QGPL/QIFSSRC IN2", CMD)?;
        want(
            &member,
            "the member copied back",
            &["SNDPGMMSG MSG('IFS step')", "ENDPGM"],
        )?;
        let list = s.command("wrklnk OBJ('/home/qs63/*')", CMD)?;
        want(&list, "WRKLNK", &["/home/qs63", "out.txt", "STMF"])?;
        let shown = s.command("dspf STMF('/home/qs63/out.txt')", CMD)?;
        want(
            &shown,
            "DSPF",
            &["PGM", "SNDPGMMSG MSG('IFS step')", "ENDPGM"],
        )?;
        let gone = s.command("del OBJLNK('/home/qs63/*.txt')", CMD)?;
        want(&gone, "DEL", &["/home/qs63/out.txt"])?;
        let rd = s.command("rd DIR('/home/qs63')", CMD)?;
        want(&rd, "RD", &["/home/qs63"])?;
        let after = s.command("svc.ifs ls /home", CMD)?;
        if after.contains("qs63") {
            return Err(format!("/home/qs63 is still there:\n{after}"));
        }
        s.command("svc.dltf QGPL/QIFSSRC", CMD).ok();
        Ok("MD, a member to a stream file and back (CPYTOSTMF, CPYFRMSTMF), WRKLNK and DSPF text, DEL by pattern, RD".into())
    }
}

/// `svc.ifs`'s capacity and free space, MiB.
fn capacity(out: &str) -> Result<(u64, u64), String> {
    let mib = |key: &str| -> Option<u64> {
        let at = out.find(key)? + key.len();
        out[at..]
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
            .map(|b| b >> 20)
    };
    match (mib("capacity "), mib("free ")) {
        (Some(size), Some(free)) => Ok((size, free)),
        _ => Err(format!("svc.ifs shows no capacity:\n{out}")),
    }
}

/// The CL source step 63 copies to a stream file and back.
const SOURCE: &str = "PGM
  SNDPGMMSG MSG('IFS step')
ENDPGM
";
