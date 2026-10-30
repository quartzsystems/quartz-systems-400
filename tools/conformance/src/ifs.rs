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
        want(&member, "the member copied back", &["SNDPGMMSG MSG('IFS step')", "ENDPGM"])?;
        let list = s.command("wrklnk OBJ('/home/qs63/*')", CMD)?;
        want(&list, "WRKLNK", &["/home/qs63", "out.txt", "STMF"])?;
        let shown = s.command("dspf STMF('/home/qs63/out.txt')", CMD)?;
        want(&shown, "DSPF", &["PGM", "SNDPGMMSG MSG('IFS step')", "ENDPGM"])?;
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

/// The CL source step 63 copies to a stream file and back.
const SOURCE: &str = "PGM
  SNDPGMMSG MSG('IFS step')
ENDPGM
";
