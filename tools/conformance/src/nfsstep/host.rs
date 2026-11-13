// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host's side: `tools/nfstest` (built as step 167 builds `smbtest`,
//! or `QS400_NFSTEST`), its output read back, and the port forwards to
//! the guest's daemons, the same host port for TCP and UDP so one
//! `--port-map GUEST=HOST` serves both.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The binary once built.
static BUILT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// The `nfstest` binary: `QS400_NFSTEST`, or built into the target
/// directory (once a run).
pub(crate) fn binary() -> Result<PathBuf, String> {
    if let Some(p) = std::env::var_os("QS400_NFSTEST") {
        return Ok(PathBuf::from(p));
    }
    if let Some(p) = BUILT.get() {
        return Ok(p.clone());
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let target = crate::target_dir().join("nfstest");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let st = Command::new(cargo)
        .current_dir(&repo)
        .args([
            "build",
            "--release",
            "--locked",
            "--manifest-path",
            "tools/nfstest/Cargo.toml",
        ])
        .env("CARGO_TARGET_DIR", &target)
        .status()
        .map_err(|e| format!("cargo build nfstest: {e}"))?;
    if !st.success() {
        return Err("cargo build nfstest failed".into());
    }
    let p = target.join("release").join(if cfg!(windows) {
        "nfstest.exe"
    } else {
        "nfstest"
    });
    Ok(BUILT.get_or_init(|| p).clone())
}

/// One run of `nfstest`.
#[derive(Debug)]
pub(crate) struct Run {
    /// The command line, for the reports.
    pub line: String,
    /// Its exit code (`None`: killed at the limit, or no code).
    pub code: Option<i32>,
    /// What it printed.
    pub out: String,
    /// Its standard error.
    pub err: String,
}

/// Runs `nfstest args`, killed after `limit`.
pub(crate) fn run(args: &[String], limit: Duration) -> Result<Run, String> {
    let bin = binary()?;
    let line = format!("{} {}", bin.display(), args.join(" "));
    let mut child = Command::new(&bin)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{line}: {e}"))?;
    let mut so = child.stdout.take().ok_or("no stdout")?;
    let mut se = child.stderr.take().ok_or("no stderr")?;
    let out = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = so.read_to_string(&mut s);
        s
    });
    let err = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = se.read_to_string(&mut s);
        s
    });
    let end = Instant::now() + limit;
    let code = loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            break st.code();
        }
        if Instant::now() > end {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    Ok(Run {
        line,
        code,
        out: out.join().unwrap_or_default(),
        err: err.join().unwrap_or_default(),
    })
}

/// One row of the binder's table as `nfstest probe` prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Reg {
    /// Program number.
    pub prog: u32,
    /// Version.
    pub vers: u32,
    /// `tcp` or `udp`.
    pub proto: String,
    /// The guest's port.
    pub port: u16,
}

/// The binder's rows of a probe (`   program vers proto   port  service`).
pub(crate) fn registrations(probe: &str) -> Vec<Reg> {
    probe
        .lines()
        .filter_map(|l| {
            let w: Vec<&str> = l.split_whitespace().collect();
            if w.len() < 4 {
                return None;
            }
            Some(Reg {
                prog: w[0].parse().ok()?,
                vers: w[1].parse().ok()?,
                proto: match w[2] {
                    p @ ("tcp" | "udp") => p.to_string(),
                    _ => return None,
                },
                port: w[3].parse().ok()?,
            })
        })
        .collect()
}

/// The paths of the probe's export list.
pub(crate) fn exports(probe: &str) -> Vec<String> {
    probe
        .lines()
        .skip_while(|l| !l.starts_with("Export list for"))
        .skip(1)
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect()
}

/// The guest ports the registrations use that `map` does not forward yet.
pub(crate) fn unmapped(regs: &[Reg], map: &[(u16, u16)]) -> Vec<u16> {
    let mut out: Vec<u16> = regs
        .iter()
        .map(|r| r.port)
        .filter(|p| *p != 0 && !map.iter().any(|(g, _)| g == p))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// `--port-map GUEST=HOST` for each forward.
pub(crate) fn map_args(map: &[(u16, u16)]) -> Vec<String> {
    map.iter()
        .flat_map(|(g, h)| ["--port-map".to_string(), format!("{g}={h}")])
        .collect()
}

/// The host port forwarded to `guest`.
pub(crate) fn host_port(map: &[(u16, u16)], guest: u16) -> Option<u16> {
    map.iter().find(|(g, _)| *g == guest).map(|(_, h)| *h)
}

/// A scenario run read back.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Outcome {
    /// Scenarios that passed.
    pub ok: Vec<String>,
    /// `fail NAME: why` lines.
    pub failed: Vec<String>,
    /// `skip NAME: why` lines.
    pub skipped: Vec<String>,
}

impl Outcome {
    /// From `nfstest`'s output.
    pub fn of(out: &str) -> Self {
        let mut o = Outcome::default();
        for l in out.lines() {
            if let Some(n) = l.strip_prefix("ok ") {
                o.ok.push(n.to_string());
            } else if l.starts_with("fail ") {
                o.failed.push(l.to_string());
            } else if l.starts_with("skip ") {
                o.skipped.push(l.to_string());
            }
        }
        o
    }

    /// The name of a `fail`/`skip` line.
    pub fn name(line: &str) -> &str {
        line.split_once(' ')
            .map_or(line, |(_, r)| r.split(':').next().unwrap_or(r))
    }
}

/// The value after `prefix` on a line of the output.
pub(crate) fn value<'a>(out: &'a str, prefix: &str) -> Option<&'a str> {
    out.lines().find_map(|l| l.trim().strip_prefix(prefix))
}

/// A host port free for TCP and UDP alike.
pub(crate) fn free_both() -> Result<u16, String> {
    for _ in 0..64 {
        let t = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let p = t.local_addr().map_err(|e| e.to_string())?.port();
        if std::net::UdpSocket::bind(("127.0.0.1", p)).is_ok() {
            return Ok(p);
        }
    }
    Err("no host port free for both TCP and UDP".into())
}

/// A free UDP port on the loopback (the private link's ends).
pub(crate) fn free_udp() -> Result<u16, String> {
    std::net::UdpSocket::bind("127.0.0.1:0")
        .and_then(|s| s.local_addr())
        .map(|a| a.port())
        .map_err(|e| e.to_string())
}

/// Forward a fresh host port to the guest's `guest` port, TCP and UDP.
pub(crate) fn forward_both(qmp: &Path, guest: u16) -> Result<u16, String> {
    let host = free_both()?;
    for proto in ["tcp", "udp"] {
        hostfwd(qmp, proto, host, guest)?;
    }
    Ok(host)
}

#[cfg(unix)]
fn hostfwd(qmp: &Path, proto: &str, host: u16, guest: u16) -> Result<(), String> {
    let mut q = crate::qemu::qmp::Qmp::connect(qmp)?;
    let out = q.hmp(&format!(
        "hostfwd_add n0 {proto}:127.0.0.1:{host}-{}:{guest}",
        crate::tcp::GUEST
    ))?;
    if out.trim().is_empty() {
        Ok(())
    } else {
        Err(format!("hostfwd_add {proto} {host}->{guest}: {out}"))
    }
}

#[cfg(not(unix))]
fn hostfwd(_: &Path, _: &str, _: u16, _: u16) -> Result<(), String> {
    Err("QMP needs a Unix host".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROBE: &str = "   program vers proto   port  service
    100000    2   tcp    111  portmapper
    100000    2   udp    111  portmapper
    100003    3   tcp   2049  nfs  (host port 40001)
    100005    3   tcp  38211  mountd
    100005    3   udp  38211  mountd
    100021    4   tcp  41000  nlockmgr
    100021    4   udp  41003  nlockmgr
    100024    1   udp  41100  status
Export list for 127.0.0.1:
/nfstest                       (everyone)
/QSYS.LIB/NFSLIB.LIB           192.168.77.2
";

    #[test]
    fn reads_the_probe() {
        let regs = registrations(PROBE);
        assert_eq!(regs.len(), 8);
        assert_eq!(
            regs[2],
            Reg {
                prog: 100_003,
                vers: 3,
                proto: "tcp".into(),
                port: 2049
            }
        );
        assert_eq!(exports(PROBE), ["/nfstest", "/QSYS.LIB/NFSLIB.LIB"]);
        assert!(registrations("rpcinfo: the binder at 127.0.0.1:1 did not answer: x").is_empty());
    }

    #[test]
    fn builds_the_port_map() {
        let regs = registrations(PROBE);
        let map = vec![(111, 50111), (2049, 52049)];
        assert_eq!(unmapped(&regs, &map), [38211, 41000, 41003, 41100]);
        assert_eq!(
            map_args(&map),
            ["--port-map", "111=50111", "--port-map", "2049=52049"]
        );
        assert_eq!(host_port(&map, 2049), Some(52049));
        assert_eq!(host_port(&map, 7), None);
    }

    #[test]
    fn reads_outcomes() {
        let out = "ok null-pmap\n  versions: 2 3 4\nfail nlm-share: NLM4_DENIED\nskip nlm-grace: needs --expect-grace\nok nsm-stat\n";
        let o = Outcome::of(out);
        assert_eq!(o.ok, ["null-pmap", "nsm-stat"]);
        assert_eq!(o.failed, ["fail nlm-share: NLM4_DENIED"]);
        assert_eq!(Outcome::name(&o.failed[0]), "nlm-share");
        assert_eq!(Outcome::name(&o.skipped[0]), "nlm-grace");
        assert_eq!(
            value("bytes: 9\nwrite MiB/s: 81.5\n", "write MiB/s: "),
            Some("81.5")
        );
    }

    #[test]
    fn free_ports() {
        let p = free_both().unwrap();
        assert!(p > 0);
        assert!(free_udp().unwrap() > 0);
    }
}
