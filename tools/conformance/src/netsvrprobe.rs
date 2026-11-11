// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QEMU step 170: `NetServer` leases, locks and print through
//! `tools/smbprobe` (`docs/NETSERVER.md` section 11). The step adds a
//! print share on an output queue (`QZLSADPS`), a share with a maximum of
//! one user and one that requires encryption (`QZLSADFS`), a second user,
//! a guest profile (`QZLSCHSG`) and a driver store, restarts the server
//! with the pending attributes, and runs every scenario; it writes the
//! leased file from the console when the probe waits for it, and finds
//! both printed documents on the output queue, owned by the user, with
//! the bytes the probe sent. Then it sets the minimum dialect to 3.0
//! (`QZLSCHSI` `QSLS0100`), runs `min-dialect`, and puts it and the guest
//! back.

use std::fmt::Write as _;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::netsvrstep::{
    PASSWORD, SCRATCH, SHARE, SLOW, USER, job_gone, job_line, no_client_job, share_setup,
};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};

/// The second user, refused by the share with a maximum of one.
const USER2: &str = "NSUSER2";
const PASSWORD2: &str = "NetSrv27y";
/// The guest profile.
const GUEST_PRF: &str = "NSGUEST";
/// A user name with no profile: signed on as the guest.
const NOBODY: &str = "NSNOBODY";
/// The print share, its output queue and its printer driver.
const PRINT_SHARE: &str = "NSPRT";
const OUTQ: &str = "NSPRTQ";
const DRIVER: &str = "QSProbe";
/// A file share with a maximum of one user, and one that requires
/// encryption.
const MAX_SHARE: &str = "SMBMAX";
const ENC_SHARE: &str = "SMBENC";
/// The driver store `print$` serves.
const STORE: &str = "/QIBM/UserData/OS400/NetServer/print$";
/// The driver file `print-dollar` reads, and what the store holds in it.
const DRIVER_FILE: &str = "x64/3/probe.dll";
const DRIVER_DATA: &str = "MZ-probe-driver";
/// What the console writes into the leased file.
const IFS_TEXT: &str = "WRITTEN-BY-THE-SYSTEM";
/// How long the probe may run.
const PROBE_TIME: Duration = Duration::from_mins(12);

/// The `smbprobe` binary: `QS400_SMBPROBE`, or built here (release).
fn probe_binary() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("QS400_SMBPROBE") {
        return Ok(PathBuf::from(p));
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // A workspace member: the build's own target directory when known.
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| crate::target_dir().join("smbprobe"), PathBuf::from);
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let st = Command::new(cargo)
        .current_dir(&repo)
        .args(["build", "--release", "--locked", "-p", "smbprobe"])
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .map_err(|e| format!("cargo build smbprobe: {e}"))?;
    if !st.success() {
        return Err("cargo build smbprobe failed".into());
    }
    Ok(target.join("release").join(if cfg!(windows) {
        "smbprobe.exe"
    } else {
        "smbprobe"
    }))
}

/// Runs the probe against `port` with `extra`; when it prints `ready
/// lease-break-ifs PATH`, the console writes the file. Its output when no
/// scenario failed.
fn probe(s: &mut Session, port: u16, extra: &[&str]) -> Result<String, String> {
    let bin = probe_binary()?;
    let mut child = Command::new(&bin)
        .args(["127.0.0.1", &port.to_string(), USER, "-", SHARE, SCRATCH])
        .args(extra)
        .env("SMBPROBE_PASSWORD", PASSWORD)
        .env("SMBPROBE_PASSWORD2", PASSWORD2)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{}: {e}", bin.display()))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for l in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(l).is_err() {
                break;
            }
        }
    });
    let end = Instant::now() + PROBE_TIME;
    let mut out = String::new();
    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(l) => {
                if let Some(path) = l.strip_prefix("ready lease-break-ifs ") {
                    // A writer outside SMB: the console, through the IFS.
                    let w =
                        s.command(&format!("svc.ifs put /home/{USER}/{path} {IFS_TEXT}"), SLOW)?;
                    let _ = writeln!(out, "(console: {})", w.trim());
                }
                out.push_str(&l);
                out.push('\n');
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() > end => {
                let _ = child.kill();
                return Err(format!(
                    "smbprobe ran over {}s:\n{out}",
                    PROBE_TIME.as_secs()
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    let _ = reader.join();
    let st = child.wait().map_err(|e| format!("smbprobe: {e}"))?;
    let mut err = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = std::io::Read::read_to_string(&mut e, &mut err);
    }
    let fails: Vec<&str> = out.lines().filter(|l| l.starts_with("fail ")).collect();
    if !st.success() || !fails.is_empty() {
        return Err(format!(
            "smbprobe: {}\n{out}{err}",
            if fails.is_empty() {
                "exit status".into()
            } else {
                fails.join("; ")
            }
        ));
    }
    Ok(out)
}

/// A call that must answer with no message.
fn quiet(s: &mut Session, cmd: &str) -> Result<(), String> {
    let out = s.command(cmd, SLOW)?;
    if out.contains("CPF") || out.contains("CPD") {
        return Err(format!("{cmd}:\n{out}"));
    }
    Ok(())
}

/// A text parameter of `n` characters.
fn chars(text: &str, n: usize) -> String {
    format!("'{text:<n$}'")
}

/// `QZLSADFS` of `name` over the user's home with `max` users and
/// `extra` optional parameters.
fn file_share(name: &str, max: i32, extra: &str) -> String {
    let path = format!("/home/{USER}");
    format!(
        "CALL PGM(QSYS/QZLSADFS) PARM({} '{path}' X'{:08X}' X'00000000' {} X'00000002' X'{:08X}' X'00000000'{extra})",
        chars(name, 12),
        path.len(),
        chars("smbprobe share", 50),
        max.cast_unsigned()
    )
}

/// The shares, users, queue, guest and driver store the scenarios need;
/// what exists is left as it is.
fn setup(s: &mut Session) -> Result<(), String> {
    share_setup(s)?;
    s.command(
        &format!("crtusrprf USRPRF({USER2}) PASSWORD('{PASSWORD2}')"),
        CMD,
    )?;
    s.command(
        &format!("crtusrprf USRPRF({GUEST_PRF}) PASSWORD(*NONE) TEXT('NetServer guest')"),
        CMD,
    )?;
    s.command(
        &format!("crtoutq QGPL/{OUTQ} TEXT('smbprobe print share')"),
        CMD,
    )?;
    let have = s.command(
        "svc.sql SELECT SERVER_SHARE_NAME FROM QSYS2/SERVER_SHARE_INFO;",
        CMD,
    )?;
    if !have.contains(PRINT_SHARE) {
        // Share, output queue (name, library), text, spooled file type 1
        // (*USERASCII), printer driver, error code.
        quiet(
            s,
            &format!(
                "CALL PGM(QSYS/QZLSADPS) PARM({} {} {} X'00000001' {} X'00000000')",
                chars(PRINT_SHARE, 12),
                chars(&format!("{OUTQ:<10}QGPL"), 20),
                chars("smbprobe print share", 50),
                chars(DRIVER, 50)
            ),
        )?;
    }
    if !have.contains(MAX_SHARE) {
        quiet(s, &file_share(MAX_SHARE, 1, ""))?;
    }
    if !have.contains(ENC_SHARE) {
        // Text CCSID 0, no conversion, no extensions, encryption required.
        quiet(
            s,
            &file_share(ENC_SHARE, -1, " X'00000000' '0' ' ' X'00000000' '1'"),
        )?;
    }
    // The driver store: a manifest for the share's driver and its files.
    // The configuration directory is made owner-only; users (and QUSER,
    // which builds the job's server) need to search it to reach print$.
    quiet(
        s,
        "CHGAUT OBJ('/QIBM/UserData/OS400/NetServer') USER(*PUBLIC) DTAAUT(*X) OBJAUT(*NONE)",
    )?;
    let mut dir = String::new();
    for part in format!("{STORE}/x64/3")
        .split('/')
        .filter(|p| !p.is_empty())
    {
        dir = format!("{dir}/{part}");
        s.command(&format!("svc.ifs mkdir {dir}"), CMD)?;
    }
    for (file, text) in [
        (
            format!("{DRIVER}.qsdrv"),
            "driver=probe.dll\\ndata=probe.gpd\\nconfig=probeui.dll".to_string(),
        ),
        ("probe.dll".into(), DRIVER_DATA.into()),
        ("probe.gpd".into(), "*GPD".into()),
        ("probeui.dll".into(), "MZ-probe-ui".into()),
    ] {
        let out = s.command(&format!("svc.ifs put {dir}/{file} {text}"), CMD)?;
        if !out.contains("bytes") {
            return Err(format!("driver store {file}:\n{out}"));
        }
    }
    quiet(
        s,
        &format!(
            "CALL PGM(QSYS/QZLSCHSG) PARM({} X'00000000')",
            chars(GUEST_PRF, 10)
        ),
    )?;
    Ok(())
}

/// Ends the server and starts it with its pending attributes made
/// current (`QZLSSTRS` reset `1`).
fn restart(s: &mut Session) -> Result<(), String> {
    s.command("ENDTCPSVR SERVER(*NETSVR)", CMD)?;
    job_gone(s, "QZLSSERVER")?;
    quiet(s, "CALL PGM(QSYS/QZLSSTRS) PARM('1' X'00000000')")?;
    job_line(s, "QZLSSERVER")?;
    job_line(s, "QZLSFILE")?;
    Ok(())
}

/// `QZLSCHSI` `QSLS0100`: `NetBIOS` off, the minimum dialect, sign-on
/// logging off.
fn min_dialect(s: &mut Session, dialect: &str) -> Result<(), String> {
    quiet(
        s,
        &format!(
            "CALL PGM(QSYS/QZLSCHSI) PARM('0{dialect:<8}0' X'0000000A' 'QSLS0100' X'00000000')"
        ),
    )
}

/// The detail line after `prefix` of scenario `name`.
fn detail(out: &str, name: &str, prefix: &str) -> Option<String> {
    let head = format!("ok {name}");
    out.lines()
        .skip_while(|l| *l != head)
        .skip(1)
        .take_while(|l| l.starts_with("  "))
        .find_map(|l| l.trim_start().strip_prefix(prefix))
        .map(str::to_owned)
}

/// The bytes of a `DSPSPLF VIEW(*HEX)` display.
fn hex_bytes(out: &str) -> Vec<u8> {
    let mut b = Vec::new();
    for l in out.lines() {
        let Some((off, rest)) = l.split_once("  ") else {
            continue;
        };
        if off.len() != 8 || !off.bytes().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        for w in rest.split_whitespace().take(16) {
            match u8::from_str_radix(w, 16) {
                Ok(v) if w.len() == 2 => b.push(v),
                _ => break,
            }
        }
    }
    b
}

/// Every spooled file of the user on the print share's queue: the
/// documents `print-write` and `spoolss` sent are there with their bytes.
fn printed(s: &mut Session, docs: &[String]) -> Result<String, String> {
    let rows = s.command(&format!("wrksplf SELECT({USER}) VIEW(3)"), CMD)?;
    let queues = s.command(&format!("wrksplf SELECT({USER})"), CMD)?;
    let mut found = vec![false; docs.len()];
    let files: Vec<Vec<&str>> = rows
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .filter(|w| w.len() >= 5 && w[1].bytes().all(|c| c.is_ascii_digit()))
        .collect();
    for w in &files {
        let (file, nbr, job, user, number) = (w[0], w[1], w[2], w[3], w[4]);
        let shown = s.command(
            &format!("dspsplf {file} JOB({number}/{user}/{job}) SPLNBR({nbr}) VIEW(*HEX)"),
            CMD,
        )?;
        let data = hex_bytes(&shown);
        for (i, d) in docs.iter().enumerate() {
            if data.windows(d.len()).any(|x| x == d.as_bytes()) {
                found[i] = true;
            }
        }
    }
    if let Some(i) = found.iter().position(|f| !f) {
        return Err(format!(
            "no spooled file of {USER} holds {:?}:\n{rows}\n{queues}",
            docs[i]
        ));
    }
    let on_queue = queues.lines().filter(|l| l.contains(OUTQ)).count();
    if on_queue < docs.len() {
        return Err(format!("{on_queue} of {USER}'s files on {OUTQ}:\n{queues}"));
    }
    Ok(format!(
        "{on_queue} of {USER}'s spooled files on {OUTQ}, both documents' bytes found"
    ))
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 170: leases, locks, durable reconnects, print shares,
    /// spoolss, share enumeration, the guest, share limits, encryption
    /// and the minimum dialect, through `smbprobe` against the guest.
    pub(crate) fn netsvr_probe(&self, s: &mut Session) -> Result<String, String> {
        setup(s)?;
        restart(s)?;
        no_client_job(s)?;
        let port = forward_port(self, GUEST, 445)?;
        let driver = format!("{DRIVER_FILE}={DRIVER_DATA}\n");
        let out = probe(
            s,
            port,
            &[
                "--user2",
                USER2,
                "--print-share",
                PRINT_SHARE,
                "--max-share",
                MAX_SHARE,
                "--enc-share",
                ENC_SHARE,
                "--guest-user",
                NOBODY,
                "--driver-file",
                &driver,
                "--ifs-text",
                IFS_TEXT,
            ],
        )?;
        let oks = out.lines().filter(|l| l.starts_with("ok ")).count();
        let skips: Vec<&str> = out.lines().filter(|l| l.starts_with("skip ")).collect();
        if !skips.iter().all(|l| l.starts_with("skip min-dialect")) {
            return Err(format!("scenarios skipped:\n{out}"));
        }
        let docs: Vec<String> = ["print-write", "spoolss"]
            .iter()
            .filter_map(|n| {
                let d = detail(&out, n, "data: ").or_else(|| {
                    detail(&out, n, "job ")
                        .and_then(|l| l.split_once("data: ").map(|x| x.1.to_owned()))
                })?;
                Some(d)
            })
            .collect();
        if docs.len() != 2 {
            return Err(format!("no document data in:\n{out}"));
        }
        let spool = printed(s, &docs)?;

        // The minimum dialect, then everything put back.
        min_dialect(s, "3.0")?;
        quiet(
            s,
            &format!(
                "CALL PGM(QSYS/QZLSCHSG) PARM({} X'00000000')",
                chars("*NONE", 10)
            ),
        )?;
        restart(s)?;
        let port = forward_port(self, GUEST, 445)?;
        let min = probe(s, port, &["--only", "min-dialect", "--min-dialect", "3.0"]);
        min_dialect(s, "2.0.2")?;
        restart(s)?;
        let min = min?;
        Ok(format!(
            "{oks} scenarios ok, min-dialect: {}; {spool}; breaks from outside: {}",
            detail(&min, "min-dialect", "offering ").unwrap_or_default(),
            detail(&out, "lease-break-ifs", "breaks: ").unwrap_or_default()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{detail, hex_bytes};

    #[test]
    fn reads_a_hex_display() {
        let out = "File  . . . . . :   QPRINT\n00000000  53 4D 42 0D 0A  SMB..\n00000010  41 \n";
        assert_eq!(hex_bytes(out), b"SMB\r\nA");
    }

    #[test]
    fn finds_a_detail_line() {
        let out = "ok spoolss\n  printer: X\n  job 3; data: SMBPROBE SPOOLSS r\nok next\n";
        assert_eq!(
            detail(out, "spoolss", "job ").as_deref(),
            Some("3; data: SMBPROBE SPOOLSS r")
        );
    }
}
