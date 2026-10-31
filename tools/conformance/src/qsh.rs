// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 QEMU step: QSH in a batch job and PASE.
//!
//! `QIBM_QSH_CMD_OUTPUT=STDOUT` is added at the system level; a
//! submitted `QSH CMD('ls /home; echo $?')` runs in a batch job whose
//! standard output becomes the spooled file `QPRINT`, read back with
//! `DSPSPLF`. The PASE test program (`tools/pase-probe`, built here for
//! the musl target and never part of the image) is pushed into
//! `/QOpenSys/tmp` over the console and run with `QP2SHELL`: it reports
//! its user, PID 1 of its own namespace with no other process in view,
//! the IFS under its root and a forbidden system call refused.

use std::fmt::Write as _;
use std::time::Duration;

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The longest wait for the batch job.
const TRIES: u32 = 120;
/// Hex digits a pushed line carries.
const LINE: usize = 2000;
/// Where the probe goes.
const PROBE: &str = "/QOpenSys/tmp/paseprobe";

/// The `number/user/name` in `SBMJOB`'s message.
fn submitted(out: &str) -> Result<String, String> {
    out.split_whitespace()
        .find(|w| w.split('/').count() == 3 && w.split('/').next().is_some_and(|n| n.len() == 6))
        .map(str::to_string)
        .ok_or_else(|| format!("sbmjob: {out}"))
}

fn poll(s: &mut Session, cmd: &str, tries: u32, done: impl Fn(&str) -> bool) -> Result<String, String> {
    let mut out = String::new();
    for _ in 0..tries {
        out = s.command(cmd, CMD)?;
        if done(&out) {
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(out)
}

/// Build the probe for the image's target; its bytes.
fn probe() -> Result<Vec<u8>, String> {
    let target = crate::target_dir();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let st = std::process::Command::new(cargo)
        .args([
            "build",
            "-p",
            "pase-probe",
            "--release",
            "--target",
            "x86_64-unknown-linux-musl",
        ])
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .map_err(|e| format!("building pase-probe: {e}"))?;
    if !st.success() {
        return Err("pase-probe did not build".into());
    }
    let path = target.join("x86_64-unknown-linux-musl/release/pase-probe");
    std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Push bytes into the console's xfer area as hex lines.
pub(crate) fn push_hex(s: &mut Session, name: &str, bytes: &[u8]) -> Result<(), String> {
    let hex = bytes.iter().fold(String::new(), |mut h, b| {
        let _ = write!(h, "{b:02X}");
        h
    });
    let lines: Vec<&str> = hex
        .as_bytes()
        .chunks(LINE)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    s.send(&format!("svc.xfer put {name} {}", lines.len()))?;
    for (i, l) in lines.iter().enumerate() {
        // A marker of letters outside the hex digits (putx skips them)
        // ends each line, so the echo of this line and no other one is
        // waited for: the tty drops input past its 4 KiB line buffer.
        let mark = marker(i);
        s.send(&format!("{l}{mark}"))?;
        s.expect(&mark, CMD)?;
    }
    let out = s.expect(crate::PROMPT, CMD)?;
    if !out.contains("lines stored") {
        return Err(format!("svc.xfer put {name}: {out}"));
    }
    Ok(())
}

/// `zz` and the line's number in the letters `g` to `p`, then `zz`.
fn marker(i: usize) -> String {
    let digits: String = i
        .to_string()
        .bytes()
        .map(|d| char::from(b'g' + (d - b'0')))
        .collect();
    format!("zz{digits}zz")
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 65: `QSH CMD()` in a batch job with its output spooled, the
    /// environment variable commands, and the PASE probe through
    /// `QP2SHELL` and through the shell.
    pub(crate) fn qsh_pase(&self, s: &mut Session) -> Result<String, String> {
        // System-level variables reach every job that starts after.
        let out = s.command(
            "addenvvar ENVVAR(QIBM_QSH_CMD_OUTPUT) VALUE(STDOUT) LEVEL(*SYS) REPLACE(*YES)",
            CMD,
        )?;
        if !out.contains("CPCA980") {
            return Err(format!("addenvvar: {out}"));
        }
        let list = s.command("wrkenvvar LEVEL(*SYS)", CMD)?;
        if !list.contains("QIBM_QSH_CMD_OUTPUT") || !list.contains("'STDOUT'") {
            return Err(format!("wrkenvvar: {list}"));
        }
        s.command("svc.ifs mkdir /home/QSHTEST", CMD)?;
        // A command at the console: its output is the console's text.
        let here = s.command("qsh CMD('echo hello from qsh; exit 3')", CMD)?;
        if !here.contains("hello from qsh") || !here.contains("QSH0005") || !here.contains(" 3.") {
            return Err(format!("qsh at the console: {here}"));
        }
        // A batch job: STDOUT is QPRINT.
        s.command("strsbs QSYS/QBATCH", CMD)?;
        let out = s.command(
            "sbmjob CMD(qsh CMD('ls /home; echo $?')) JOB(QSHLS) JOBQ(QGPL/QBATCH) MSGQ(QUSRSYS/QSECOFR)",
            CMD,
        )?;
        let job = submitted(&out)?;
        let msgs = poll(s, "dspmsg QUSRSYS/QSECOFR", TRIES, |o| {
            o.contains("QSHLS") && (o.contains("CPF1241") || o.contains("CPF1240"))
        })?;
        if !msgs.contains("QSHLS") {
            return Err(format!("QSHLS did not end:\n{msgs}"));
        }
        let printed = s.command(&format!("dspsplf QPRINT JOB({job})"), CMD)?;
        if !printed.contains("QSHTEST") || !printed.lines().any(|l| l.trim_end().ends_with(" 0")) {
            return Err(format!("QPRINT of {job}:\n{printed}"));
        }
        // PASE: the probe into /QOpenSys/tmp, run through QP2SHELL.
        let bytes = probe()?;
        push_hex(s, "PASEPROBE", &bytes)?;
        let put = s.command(&format!("svc.ifs putx {PROBE} PASEPROBE 755"), CMD)?;
        if !put.contains(&format!("{} bytes", bytes.len())) {
            return Err(format!("svc.ifs putx: {put}"));
        }
        s.command("svc.xfer rm PASEPROBE", CMD)?;
        let ran = s.command(&format!("call QSYS/QP2SHELL PARM('{PROBE}' 'one')"), CMD)?;
        for want in ["uid=", "pid=1\n", "procs=1\n", "tmp: paseprobe", "pidfd_open: refused errno 1", "args: one"] {
            if !ran.replace('\r', "").contains(want) {
                return Err(format!("QP2SHELL: no {want:?} in:\n{ran}"));
            }
        }
        // The shell runs an ELF file as a PASE program in a child job.
        // A child job starts for the program: a wider wait than a line's.
        let shell = s.command(&format!("qsh CMD('{PROBE} two; echo st=$?')"), Duration::from_secs(60))?;
        if !shell.contains("args: two") || !shell.contains("st=0") {
            return Err(format!("qsh running the probe: {shell}"));
        }
        s.command("rmvenvvar ENVVAR(QIBM_QSH_CMD_OUTPUT) LEVEL(*SYS)", CMD)?;
        s.command("endsbs QBATCH OPTION(*IMMED)", CMD).ok();
        Ok(format!(
            "QSH CMD in {job} spooled QPRINT; the {}-byte probe ran under QP2SHELL and from qsh",
            bytes.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_are_not_hex() {
        assert_eq!(marker(0), "zzgzz");
        assert_eq!(marker(129), "zzhipzz");
        assert!(!marker(4567).chars().any(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn job_names_from_sbmjob() {
        assert_eq!(
            submitted("CPC1221 012345/QSECOFR/QSHLS submitted.").unwrap(),
            "012345/QSECOFR/QSHLS"
        );
        assert!(submitted("nothing").is_err());
    }
}
