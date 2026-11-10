// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QEMU steps 166 to 168: `NetServer` (`docs/NETSERVER.md` section 11).
//! 166 starts `*NETSVR` and finds `QZLSSERVER` (BCH) and `QZLSFILE` (PJ) in
//! `WRKACTJOB`; 167 makes a user and a share (`QZLSADFS`) and runs every
//! `smbtest` scenario against the guest's 445 through a port forward; 168
//! copies 1 GiB in and out and reports MB/s.
//!
//! `tools/smbtest` has its own lockfile outside the workspace, so the steps
//! run its binary: `QS400_SMBTEST` when set, else one built here with
//! `cargo build --release --manifest-path tools/smbtest/Cargo.toml`.
//!
//! The steps are registered only with `QS400_NETSVR_STEPS=1` until the
//! `NetServer` jobs exist ([`enabled`]).

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
/// The scenarios' directory on the share.
const SCRATCH: &str = "smbtest";

/// Whether the `NetServer` steps are registered (`QS400_NETSVR_STEPS=1`).
pub(crate) fn enabled() -> bool {
    std::env::var("QS400_NETSVR_STEPS").is_ok_and(|v| v == "1")
}

/// The `WRKACTJOB` line of `name`, waiting up to 30 seconds for it.
fn job_line(s: &mut Session, name: &str) -> Result<String, String> {
    let end = Instant::now() + Duration::from_secs(30);
    loop {
        let out = s.command("wrkactjob", CMD)?;
        if let Some(l) = out.lines().find(|l| l.split_whitespace().next() == Some(name)) {
            return Ok(l.trim().to_string());
        }
        if Instant::now() > end {
            return Err(format!("{name} never shown in:\n{out}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
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
        .args(["build", "--release", "--locked", "--manifest-path", "tools/smbtest/Cargo.toml"])
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .map_err(|e| format!("cargo build smbtest: {e}"))?;
    if !st.success() {
        return Err("cargo build smbtest failed".into());
    }
    Ok(target.join("release").join(if cfg!(windows) { "smbtest.exe" } else { "smbtest" }))
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
            if fails.is_empty() { "exit status".into() } else { fails.join("; ") },
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(text)
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
        s.command("STRTCPSVR SERVER(*NETSVR)", CMD)?;
        let server = job_line(s, "QZLSSERVER")?;
        let file = job_line(s, "QZLSFILE")?;
        for (line, kind) in [(&server, "BCH"), (&file, "PJ")] {
            if !line.split_whitespace().any(|w| w == kind) {
                return Err(format!("not type {kind}: {line}"));
            }
        }
        Ok(format!("QZLSSERVER BCH and QZLSFILE PJ active: {server} | {file}"))
    }

    /// Step 167: a user, a read/write share over its home directory, and
    /// every `smbtest` scenario against the guest.
    pub(crate) fn netsvr_files(&self, s: &mut Session) -> Result<String, String> {
        s.command(&format!("crtusrprf USRPRF({USER}) PASSWORD('{PASSWORD}') HOMEDIR('/home/{USER}')"), CMD)?;
        s.command(&format!("MKDIR DIR('/home/{USER}')"), CMD)?;
        s.command(&format!("MKDIR DIR('/home/{USER}/{SCRATCH}')"), CMD)?;
        s.command(&format!("CHGOWN OBJ('/home/{USER}') NEWOWN({USER})"), CMD)?;
        s.command(&format!("CHGOWN OBJ('/home/{USER}/{SCRATCH}') NEWOWN({USER})"), CMD)?;
        // QZLSADFS: share name CHAR(12), path, path length, path CCSID (0, the
        // job's), text CHAR(50), permissions (2 read/write), maximum users
        // (-1 none), error code (bytes provided 0: escape messages).
        let path = format!("/home/{USER}");
        s.command(
            &format!(
                "CALL PGM(QSYS/QZLSADFS) PARM('{SHARE:<12}' '{path}' X'{:08X}' X'00000000' '{:<50}' X'00000002' X'FFFFFFFF' X'00000000')",
                path.len(),
                "smbtest share"
            ),
            CMD,
        )?;
        let port = forward_port(self, GUEST, 445)?;
        let out = smbtest(port, &["--copy-bytes", "16M"])?;
        let oks = out.lines().filter(|l| l.starts_with("ok ")).count();
        let skips: Vec<&str> =
            out.lines().filter_map(|l| l.strip_prefix("skip ")).map(|l| l.split(':').next().unwrap_or(l)).collect();
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
    pub(crate) fn netsvr_copy(&self, _s: &mut Session) -> Result<String, String> {
        let port = forward_port(self, GUEST, 445)?;
        let out = smbtest(port, &["--only", "copy,cleanup", "--copy-bytes", "1G"])?;
        let rin = detail(&out, "copy", "in: ").ok_or_else(|| format!("no copy rate in:\n{out}"))?;
        let rout = detail(&out, "copy", "out: ").ok_or_else(|| format!("no copy rate in:\n{out}"))?;
        Ok(format!("1 GiB in {rin}, out {rout}"))
    }
}

#[cfg(test)]
mod tests {
    use super::detail;

    #[test]
    fn finds_detail_lines_of_one_scenario() {
        let out = "ok a\n  in: 1 MB/s\nok copy\n  bytes: 9\n  in: 99.0 MB/s (1s)\nok cleanup\n  in: x\n";
        assert_eq!(detail(out, "copy", "in: "), Some("99.0 MB/s (1s)"));
        assert_eq!(detail(out, "missing", "in: "), None);
    }
}
