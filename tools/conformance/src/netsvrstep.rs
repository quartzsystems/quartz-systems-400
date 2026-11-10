// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QEMU steps 166 to 169: `NetServer` (`docs/NETSERVER.md` section 11).
//! 166 starts `*NETSVR` and finds `QZLSSERVER` (BCH) and `QZLSFILE` (PJ) in
//! `WRKACTJOB`; 167 makes a user and a share (`QZLSADFS`) and runs every
//! `smbtest` scenario against the guest's 445 through a port forward; 168
//! copies 1 GiB in and out and reports MB/s; 169 the listener in `NETSTAT`
//! and `QSYS2.NETSTAT_INFO`, a maximum users through `QZLSCHFS` seen in
//! `QSYS2.SERVER_SHARE_INFO`, and `ENDTCPSVR`, `QZLSSTRS`,
//! `QZLSENDS` and `STRTCPSVR`.
//!
//! `tools/smbtest` has its own lockfile outside the workspace, so the steps
//! run its binary: `QS400_SMBTEST` when set, else one built here with
//! `cargo build --release --manifest-path tools/smbtest/Cargo.toml`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

/// The user the steps make.
const USER: &str = "NSUSER";
/// Its password.
const PASSWORD: &str = "NetSrv26x";
/// The share the steps add, over the user's home directory.
const SHARE: &str = "SMBTEST";
/// How long a configuration change may take: the service writes the
/// configuration files, which waits for the IFS to flush after step 168.
const SLOW: Duration = Duration::from_secs(90);
/// The scenarios' directory on the share.
const SCRATCH: &str = "smbtest";

/// The `WRKACTJOB` line of `name`, waiting up to 30 seconds for it.
fn job_line(s: &mut Session, name: &str) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let out = s.command("wrkactjob", CMD)?;
        if let Some(l) = out
            .lines()
            .find(|l| l.split_whitespace().next() == Some(name))
        {
            return Ok(l.trim().to_string());
        }
        if Instant::now() > end {
            return Err(format!("{name} never shown in:\n{out}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Waits up to 30 seconds for `name` to leave `WRKACTJOB`.
fn job_gone(s: &mut Session, name: &str) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let out = s.command("wrkactjob", CMD)?;
        if !out
            .lines()
            .any(|l| l.split_whitespace().next() == Some(name))
        {
            return Ok(());
        }
        if Instant::now() > end {
            return Err(format!("{name} still active:\n{out}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Waits up to two minutes for every `QZLSFILE` job to be back in its
/// pool (`PSRW`): no client is served.
fn no_client_job(s: &mut Session) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(120);
    loop {
        let out = s.command("wrkactjob", CMD)?;
        let busy = out
            .lines()
            .any(|l| l.split_whitespace().next() == Some("QZLSFILE") && !l.contains("PSRW"));
        if !busy {
            return Ok(());
        }
        if Instant::now() > end {
            return Err(format!(
                "a QZLSFILE job still serves a client:
{out}"
            ));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

/// The output of `cmd`, which must hold every one of `want`.
fn expect(s: &mut Session, cmd: &str, want: &[&str]) -> Result<String, String> {
    let out = s.command(cmd, CMD)?;
    match want.iter().find(|w| !out.contains(**w)) {
        Some(w) => Err(format!("{cmd}: no {w:?} in:\n{out}")),
        None => Ok(out),
    }
}

/// `QZLSADFS` or `QZLSCHFS` of the step's share with `max` users.
fn share_call(api: &str, max: i32) -> String {
    let path = format!("/home/{USER}");
    format!(
        "CALL PGM(QSYS/{api}) PARM('{SHARE:<12}' '{path}' X'{:08X}' X'00000000' '{:<50}' X'00000002' X'{:08X}' X'00000000')",
        path.len(),
        "smbtest share",
        max.cast_unsigned()
    )
}

/// The `smbtest` binary: `QS400_SMBTEST`, or built into the target directory.
fn smbtest_binary() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("QS400_SMBTEST") {
        return Ok(PathBuf::from(p));
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = crate::target_dir().join("smbtest");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let st = Command::new(cargo)
        .current_dir(&repo)
        .args([
            "build",
            "--release",
            "--locked",
            "--manifest-path",
            "tools/smbtest/Cargo.toml",
        ])
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .map_err(|e| format!("cargo build smbtest: {e}"))?;
    if !st.success() {
        return Err("cargo build smbtest failed".into());
    }
    Ok(target.join("release").join(if cfg!(windows) {
        "smbtest.exe"
    } else {
        "smbtest"
    }))
}

/// Runs `smbtest` against the forwarded port; its output when no scenario failed.
fn smbtest(port: u16, extra: &[&str]) -> Result<String, String> {
    let bin = smbtest_binary()?;
    let port = port.to_string();
    let out = Command::new(&bin)
        .args(["127.0.0.1", &port, USER, "-", SHARE, SCRATCH])
        .args(extra)
        .env("SMBTEST_PASSWORD", PASSWORD)
        .output()
        .map_err(|e| format!("{}: {e}", bin.display()))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let fails: Vec<&str> = text.lines().filter(|l| l.starts_with("fail ")).collect();
    if !out.status.success() || !fails.is_empty() {
        return Err(format!(
            "smbtest: {}\n{text}{}",
            if fails.is_empty() {
                "exit status".into()
            } else {
                fails.join("; ")
            },
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(text)
}

/// The user, its home and scratch directories and the read/write share
/// over them; what exists already is left as it is, so 168 also runs
/// without 167.
fn share_setup(s: &mut Session) -> Result<(), String> {
    s.command(
        &format!("crtusrprf USRPRF({USER}) PASSWORD('{PASSWORD}') HOMEDIR('/home/{USER}')"),
        CMD,
    )?;
    s.command(&format!("MKDIR DIR('/home/{USER}')"), CMD)?;
    s.command(&format!("MKDIR DIR('/home/{USER}/{SCRATCH}')"), CMD)?;
    s.command(&format!("CHGOWN OBJ('/home/{USER}') NEWOWN({USER})"), CMD)?;
    s.command(
        &format!("CHGOWN OBJ('/home/{USER}/{SCRATCH}') NEWOWN({USER})"),
        CMD,
    )?;
    // QZLSADFS: share name CHAR(12), path, path length, path CCSID (0, the
    // job's), text CHAR(50), permissions (2 read/write), maximum users
    // (-1 none), error code (bytes provided 0: escape messages).
    s.command(&share_call("QZLSADFS", -1), CMD)?;
    Ok(())
}

/// The value after `prefix` on an indented line of scenario `name`.
fn detail<'a>(out: &'a str, name: &str, prefix: &str) -> Option<&'a str> {
    let head = format!("ok {name}");
    out.lines()
        .skip_while(|l| *l != head)
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .find_map(|l| l.trim_start().strip_prefix(prefix))
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 166: `STRTCPSVR SERVER(*NETSVR)`, the server job and a prestart job.
    pub(crate) fn netsvr_start(&self, s: &mut Session) -> Result<String, String> {
        let started = s.command("STRTCPSVR SERVER(*NETSVR)", CMD)?;
        let server = match job_line(s, "QZLSSERVER") {
            Ok(l) => l,
            Err(e) => {
                let opr = s.command("DSPMSG MSGQ(QSYSOPR)", CMD).unwrap_or_default()
                    + &s.command("WRKJOBQ JOBQ(QSYS/QPWFSERVER)", CMD)
                        .unwrap_or_default();
                return Err(format!("{e}\nSTRTCPSVR: {started}\nQSYSOPR:\n{opr}"));
            }
        };
        let file = job_line(s, "QZLSFILE")?;
        for (line, kind) in [(&server, "BCH"), (&file, "PJ")] {
            if !line.split_whitespace().any(|w| w == kind) {
                return Err(format!("not type {kind}: {line}"));
            }
        }
        Ok(format!(
            "QZLSSERVER BCH and QZLSFILE PJ active: {server} | {file}"
        ))
    }

    /// Step 167: a user, a read/write share over its home directory, and
    /// every `smbtest` scenario against the guest.
    pub(crate) fn netsvr_files(&self, s: &mut Session) -> Result<String, String> {
        share_setup(s)?;
        let port = forward_port(self, GUEST, 445)?;
        // Every scenario; QS400_NETSVR_ONLY, when set, runs only those it names
        // (`copy,cleanup`).
        let only = std::env::var("QS400_NETSVR_ONLY").ok();
        let mut args = vec!["--copy-bytes", "16M"];
        if let Some(o) = &only {
            args.extend(["--only", o.as_str()]);
        }
        let out = smbtest(port, &args)?;
        let oks = out.lines().filter(|l| l.starts_with("ok ")).count();
        let skips: Vec<&str> = out
            .lines()
            .filter_map(|l| l.strip_prefix("skip "))
            .map(|l| l.split(':').next().unwrap_or(l))
            .collect();
        let dialects = ["2.0.2", "2.1", "3.0", "3.0.2", "3.1.1"]
            .iter()
            .filter(|d| out.contains(&format!("ok negotiate-{d}\n")))
            .count();
        Ok(format!(
            "{oks} scenarios ok ({dialects} dialects; cipher on 3.1.1 {}), skipped: {}",
            detail(&out, "encrypt-3.1.1", "cipher: ").unwrap_or("?"),
            skips.join(", ")
        ))
    }

    /// Step 168: 1 GiB copied in and out through the share, MB/s.
    pub(crate) fn netsvr_copy(&self, s: &mut Session) -> Result<String, String> {
        // The guest's IFS volume starts at 1 GiB and grows a step at a time
        // as it fills; room for the file first, or a write meets a full disk
        // (STATUS_DISK_FULL) before the volume grows.
        share_setup(s)?;
        let grow = s.command("QSGROWIFS SIZE(1536)", Duration::from_mins(2))?;
        if !grow.contains("CPC3E20") {
            return Err(format!("QSGROWIFS:\n{grow}"));
        }
        let port = forward_port(self, GUEST, 445)?;
        // QS400_NETSVR_COPY sizes it otherwise (`512M`), to find a limit.
        let size = std::env::var("QS400_NETSVR_COPY").unwrap_or_else(|_| "1G".into());
        let mark = s.log.len();
        let out = smbtest(port, &["--only", "copy,cleanup", "--copy-bytes", &size]);
        // The service's stall lines (`stgsvc/stall.rs`) from the copy, read
        // in with a command and shown with the result.
        let ifs = s.command("svc.ifs", CMD).unwrap_or_default();
        let stalls: Vec<&str> = s.log[mark..]
            .lines()
            .filter(|l| {
                [
                    "qsstg: slow request",
                    "qsstg: request ",
                    "qsstg: IFS",
                    "qsstg:   ",
                ]
                .iter()
                .any(|k| l.contains(k))
            })
            .collect();
        for l in &stalls {
            eprintln!("168: {l}");
        }
        let out = out.map_err(|e| format!("{e}\n{}\n{ifs}", stalls.join("\n")))?;
        let rin = detail(&out, "copy", "in: ").ok_or_else(|| format!("no copy rate in:\n{out}"))?;
        let rout =
            detail(&out, "copy", "out: ").ok_or_else(|| format!("no copy rate in:\n{out}"))?;
        let slow = stalls.iter().filter(|l| l.contains("slow request")).count();
        Ok(format!(
            "{size} in {rin}, out {rout}; {slow} slow service request(s)"
        ))
    }
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 169: the listener in the network status, the share in the SQL
    /// view, a maximum users, and the server ended and started by the
    /// commands and the APIs.
    pub(crate) fn netsvr_admin(&self, s: &mut Session) -> Result<String, String> {
        expect(s, "NETSTAT OPTION(*CNN)", &["445", "Listen"])?;
        expect(
            s,
            "svc.sql SELECT * FROM QSYS2/NETSTAT_INFO WHERE LOCAL_PORT = 445;",
            &["LISTEN", "row(s)"],
        )?;
        s.command(&share_call("QZLSCHFS", 1), SLOW)?;
        let shares = expect(
            s,
            &format!(
                "svc.sql SELECT * FROM QSYS2/SERVER_SHARE_INFO WHERE SERVER_SHARE_NAME = '{SHARE}';"
            ),
            &[SHARE, "/home/NSUSER", "1 row(s)"],
        )?;
        if !shares
            .lines()
            .any(|l| l.contains(SHARE) && l.split_whitespace().any(|w| w == "1"))
        {
            return Err(format!("SERVER_SHARE_INFO: no maximum of 1 in:\n{shares}"));
        }
        // One user may connect: a single sign-on and tree connect still
        // does (refusing the next is proven in memory, the netserver tests).
        // The previous step's client counts as a user until its job is
        // released (its last durable opens and sessions gone): wait for it.
        no_client_job(s)?;
        let port = forward_port(self, GUEST, 445)?;
        smbtest(port, &["--only", "signon-good", "--no-bad-signon"])?;
        s.command(&share_call("QZLSCHFS", -1), SLOW)?;
        s.command("ENDTCPSVR SERVER(*NETSVR)", CMD)?;
        job_gone(s, "QZLSSERVER")?;
        // QZLSSTRS: reset '0', error code (bytes provided 0).
        s.command("CALL PGM(QSYS/QZLSSTRS) PARM('0' X'00000000')", CMD)?;
        job_line(s, "QZLSSERVER")?;
        s.command("CALL PGM(QSYS/QZLSENDS) PARM(X'00000000')", CMD)?;
        job_gone(s, "QZLSSERVER")?;
        s.command("STRTCPSVR SERVER(*NETSVR)", CMD)?;
        job_line(s, "QZLSSERVER")?;
        Ok("445 listening in NETSTAT and NETSTAT_INFO; maximum users 1 through QZLSCHFS, in SERVER_SHARE_INFO; ENDTCPSVR, QZLSSTRS, QZLSENDS, STRTCPSVR".into())
    }
}

#[cfg(test)]
mod tests {
    use super::detail;

    #[test]
    fn finds_detail_lines_of_one_scenario() {
        let out =
            "ok a\n  in: 1 MB/s\nok copy\n  bytes: 9\n  in: 99.0 MB/s (1s)\nok cleanup\n  in: x\n";
        assert_eq!(detail(out, "copy", "in: "), Some("99.0 MB/s (1s)"));
        assert_eq!(detail(out, "missing", "in: "), None);
    }
}
