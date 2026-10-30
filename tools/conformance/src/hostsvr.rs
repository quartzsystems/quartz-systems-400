// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 10 QEMU step: the host servers. The daemons and prestart jobs
//! are active, a connection to the remote command port reaches a
//! prestart job (the descriptor relay), and when the harness host has a
//! JDK (`QS_JAVA_HOME`, else `PATH`) and `jt400.jar` (`QS_JT400_JAR`),
//! `JTOpen`'s `CommandCall` and a JDBC query run against the guest
//! (`tools/conformance/jtopen/src/Smoke.java`).

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The guest's host server ports, each forwarded from a free host port.
pub fn forwards(taken: Option<u16>) -> Vec<(u16, u16)> {
    // Held open together so no two are the same, nor the telnet port.
    let mut held = Vec::new();
    let mut out = Vec::new();
    for g in std::iter::once(449).chain(8470..=8476) {
        while let Ok(l) = std::net::TcpListener::bind("127.0.0.1:0") {
            let p = l.local_addr().map_or(0, |a| a.port());
            held.push(l);
            if p != 0 && Some(p) != taken {
                out.push((p, g));
                break;
            }
        }
    }
    out
}

/// A port mapper lookup through the forwarded 449.
fn lookup(port: u16, service: &str) -> Result<Vec<u8>, String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("connect {port}: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(20))).map_err(|e| e.to_string())?;
    s.write_all(service.as_bytes()).map_err(|e| e.to_string())?;
    let mut b = [0u8; 5];
    s.read_exact(&mut b).map_err(|e| format!("no port mapper reply: {e}"))?;
    Ok(b.to_vec())
}

/// The jobs the step waits for.
const JOBS: &[&str] = &["QZSOSMAPD", "QZSOSGND", "QZRCSRVSD", "QZDASRVSD", "QZRCSRVS", "QZDASOINIT"];

fn host_port(suite: &Suite, guest: u16) -> Result<u16, String> {
    suite
        .launch
        .host_ports
        .iter()
        .find(|(_, g)| *g == guest)
        .map(|(h, _)| *h)
        .ok_or_else(|| format!("guest port {guest} is not forwarded"))
}

/// A seed exchange on the remote command server: its reply's ID.
fn seed_exchange(port: u16) -> Result<u16, String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("connect {port}: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(20))).map_err(|e| e.to_string())?;
    let mut f = vec![0, 0, 0, 28, 3, 0, 0xE0, 0x08];
    f.extend([0; 8]);
    f.extend([0, 8, 0x70, 0x01]);
    f.extend([1, 2, 3, 4, 5, 6, 7, 8]);
    s.write_all(&f).map_err(|e| e.to_string())?;
    let mut head = [0u8; 20];
    s.read_exact(&mut head).map_err(|e| format!("no reply on {port}: {e}"))?;
    Ok(u16::from_be_bytes([head[18], head[19]]))
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The JDK's tools: `QS_JAVA_HOME/bin` when set (a Windows JDK from
/// WSL works: its paths are converted), else `PATH`.
fn tool(name: &str) -> (String, bool) {
    match std::env::var("QS_JAVA_HOME") {
        Ok(home) if Path::new(&format!("{home}/bin/{name}.exe")).exists() => (format!("{home}/bin/{name}.exe"), true),
        Ok(home) => (format!("{home}/bin/{name}"), false),
        Err(_) => (name.to_string(), false),
    }
}

/// A path as the tool sees it.
fn native(p: &Path, windows: bool) -> String {
    if !windows {
        return p.display().to_string();
    }
    Command::new("wslpath")
        .arg("-w")
        .arg(p)
        .output()
        .ok()
        .map_or_else(|| p.display().to_string(), |o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// The suite's classes by scenario (the port mapper's needs port 449
/// on the harness host, which a user's QEMU cannot forward).
const SCENARIOS: &[(&str, &str)] = &[
    ("signon", "Signon"),
    ("pwdlevel", "PwdLevel"),
    ("cmdcall", "CmdCall"),
    ("pgmcall", "PgmCall"),
    ("sysval", "SysVal"),
    ("central", "Central"),
    ("jdbc", "Jdbc"),
    ("dtaq", "DtaQ"),
    ("ifs", "Ifs"),
    ("splf", "Splf"),
];

/// Compile the suite; the java to run it with and the class path.
fn compile(suite: &Suite, jar: &str) -> Result<(String, String), String> {
    let classes = suite.out_dir.join("jtopen");
    std::fs::create_dir_all(&classes).map_err(|e| e.to_string())?;
    let src = repo().join("tools/conformance/jtopen/src");
    let (javac, win) = tool("javac");
    let jar_p = native(Path::new(jar), win);
    let mut c = Command::new(&javac);
    c.arg("-d").arg(native(&classes, win)).arg("-cp").arg(&jar_p);
    for e in std::fs::read_dir(&src).map_err(|e| e.to_string())?.flatten() {
        if e.path().extension().is_some_and(|x| x == "java") {
            c.arg(native(&e.path(), win));
        }
    }
    let out = c.output().map_err(|e| format!("{javac}: {e}"))?;
    if !out.status.success() {
        return Err(format!("javac: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let (java, win) = tool("java");
    Ok((java, format!("{}{}{jar_p}", native(&classes, win), if win { ";" } else { ":" })))
}

/// Run one class of the suite against the guest; its output.
fn run_class(suite: &Suite, java: &str, cp: &str, class: &str) -> Result<String, String> {
    let mut c = Command::new(java);
    c.arg("-cp").arg(cp).arg(class);
    let mut vars = vec![
        ("QS_HS_HOST".to_string(), "127.0.0.1".to_string()),
        ("QS_HS_USER".to_string(), "QSECOFR".to_string()),
        ("QS_HS_PASS".to_string(), SECOFR_PASSWORD.to_string()),
        ("QS_HS_SKIP_SIGNON".to_string(), "0".to_string()),
    ];
    for (h, g) in &suite.launch.host_ports {
        vars.push((format!("QS_HS_PORT_{g}"), h.to_string()));
    }
    // A Windows JDK started from WSL sees the variables WSLENV names.
    let names: Vec<String> = vars.iter().map(|(k, _)| k.clone()).collect();
    c.env("WSLENV", names.join(":"));
    for (k, v) in vars {
        c.env(k, v);
    }
    let out = c.output().map_err(|e| format!("{java}: {e}"))?;
    Ok(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

/// The suite against the guest (`QS_HS_SUITE`): each scenario's result
/// lines under the run's directory; how many steps of each passed.
fn whole_suite(suite: &Suite, java: &str, cp: &str) -> String {
    let dir = suite.out_dir.join("jtopen-qs400");
    let _ = std::fs::create_dir_all(&dir);
    let mut counts = Vec::new();
    for (name, class) in SCENARIOS {
        let out = run_class(suite, java, cp, class).unwrap_or_else(|e| format!("fail run: {e}\n"));
        let lines: String = out
            .lines()
            .filter(|l| l.starts_with("ok") || l.starts_with("fail") || l.starts_with("     "))
            .fold(String::new(), |mut acc, l| {
                acc.push_str(l);
                acc.push('\n');
                acc
            });
        let _ = std::fs::write(dir.join(format!("{name}.out")), &lines);
        let _ = std::fs::write(dir.join(format!("{name}.log")), &out);
        let ok = lines.lines().filter(|l| l.starts_with("ok")).count();
        let all = lines.lines().filter(|l| l.starts_with("ok") || l.starts_with("fail")).count();
        counts.push(format!("{name} {ok}/{all}"));
    }
    counts.join(", ")
}

impl Suite {
    /// Step 63.
    pub(crate) fn host_servers(&self, s: &mut Session) -> Result<String, String> {
        // Step 62's ENDTCP ended them and STRTCP does not (AUTOSTART *NO):
        // as on IBM i, STRHOSTSVR brings back the ones not running.
        s.command("STRHOSTSVR SERVER(*ALL)", CMD)?;
        let until = Instant::now() + Duration::from_secs(90);
        let jobs = loop {
            let out = s.command("WRKACTJOB", CMD)?;
            if JOBS.iter().all(|j| out.contains(j)) {
                break out;
            }
            if Instant::now() >= until {
                let missing: Vec<&str> = JOBS.iter().copied().filter(|j| !out.contains(j)).collect();
                return Err(format!("host server jobs missing {missing:?}:\n{out}"));
            }
            std::thread::sleep(Duration::from_secs(2));
        };
        let _ = jobs;
        let mapped = lookup(host_port(self, 449)?, "as-rmtcmd").map_err(|e| {
            // Whether the telnet port answers on the same network, for the report.
            let telnet = self.launch.telnet_port.map_or_else(
                || "no telnet port".to_string(),
                |p| match TcpStream::connect(("127.0.0.1", p)).and_then(|mut t| {
                    t.set_read_timeout(Some(Duration::from_secs(10)))?;
                    let mut b = [0u8; 1];
                    t.read_exact(&mut b).map(|()| b[0])
                }) {
                    Ok(b) => format!("telnet answers ({b:#04x})"),
                    Err(e) => format!("telnet does not answer either: {e}"),
                },
            );
            let ports: Vec<String> = self.launch.host_ports.iter().map(|(h, g)| format!("{h}->{g}")).collect();
            format!("{e}; {telnet}; forwards {}", ports.join(" "))
        })?;
        if mapped != [0x2B, 0, 0, 0x21, 0x1B] {
            return Err(format!("the port mapper answered {mapped:02x?} for as-rmtcmd"));
        }
        let id = seed_exchange(host_port(self, 8475)?)?;
        if id != 0xF001 {
            return Err(format!("the remote command server answered {id:04x}, not f001"));
        }
        let mut report = String::from("daemons and prestart jobs active; the port mapper answered; a seed exchange reached QZRCSRVS");
        match (std::env::var("QS_JT400_JAR"), Command::new(tool("java").0).arg("-version").output()) {
            (Ok(jar), Ok(_)) => {
                // What the suite works in on the oracle: QSTEST and /home/QSTEST.
                let _ = s.command("CRTLIB LIB(QSTEST) TEXT('Host server tests')", CMD);
                let _ = s.command("svc.ifs mkdir /home", CMD);
                let _ = s.command("svc.ifs mkdir /home/QSTEST", CMD);
                // The suite's password, whether or not an earlier step set it.
                let set = s.command(&format!("CHGUSRPRF USRPRF(QSECOFR) PASSWORD('{SECOFR_PASSWORD}') PWDEXP(*NO)"), CMD)?;
                if set.trim_start().starts_with("CPF") {
                    return Err(format!("CHGUSRPRF QSECOFR: {set}"));
                }
                let (java, cp) = compile(self, &jar)?;
                let out = run_class(self, &java, &cp, "Smoke")?;
                // What the jobs wrote meanwhile reaches the log.
                let _ = s.command("WRKACTJOB", CMD);
                for want in ["ok   command", "ok   jdbc"] {
                    if !out.contains(want) {
                        return Err(format!("JTOpen smoke test lacks '{want}':\n{out}"));
                    }
                }
                report.push_str("; JTOpen CommandCall and a JDBC query ran");
                if std::env::var_os("QS_HS_SUITE").is_some() {
                    let _ = write!(report, "; the suite: {}", whole_suite(self, &java, &cp));
                }
            }
            _ => report.push_str("; JTOpen not run (no java or QS_JT400_JAR)"),
        }
        Ok(report)
    }
}
