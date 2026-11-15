// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 195 to 198, in a guest of their own dressed as a partition of
//! QS/VS (`docs/QSID.md`): SMBIOS naming QS/VS, a virtio-console device
//! with the service port and the console port on Unix sockets, and the
//! LIC record's console mode set to the partition's console port, as
//! the hypervisor writes it at activation. The harness is the peer on
//! both ports, connected from QEMU's start.
//!
//! - 195: the hello, every code of the IPL in order, the state, a ping,
//!   `QSHWQUAL`'s channel lines.
//! - 196: the console as a grid: the SRC display, the sign-on display
//!   against its golden, keys that sign on and run a command, a detach
//!   and attach mid-session.
//! - 197: function 21 refused at a normal keylock; a manual IPL driven
//!   from the console port (the IPL Options screen against its golden);
//!   function 21 then: DST's sign-on and main menu against their
//!   goldens, refused while they show, the session back after.
//! - 198: a controlled power down with a delay; then a terminal code
//!   (the storage service killed) and a halt without a power off.

use std::fs;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use qsipart::console::{self as con, Grid};
use qsipart::service::{
    FromSystem, Keylock, Panel, PeerHello, Ping, PowerDown, SysState, ToSystem, msgid,
};

use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, SRC_IPL_COMPLETE, SRC_SEQUENCE, StepReport, Suite};

/// The steps' names, for `--only`.
const NAMES: [&str; 4] = [
    "195 partition service channel",
    "196 partition console",
    "197 partition panel 21",
    "198 partition power down and halt",
];

/// The shipped DST password after its first change (`crate::dst`).
const DST_PASSWORD: &str = crate::dst::PASSWORD;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The LIC record's console mode on a disk image, as QS/VS writes it.
fn set_console_mode(img: &Path, mode: licpart::ConsoleMode) -> Result<(), String> {
    let mut f = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(img)
        .map_err(|e| format!("{}: {e}", img.display()))?;
    let table = diskimg::read_table(&mut f).map_err(|e| e.to_string())?;
    let (_, _, first, _) = table
        .iter()
        .find(|p| p.1 == diskimg::LIC_TYPE_GUID)
        .ok_or("the image has no LIC partition")?;
    let at = first * diskimg::SECTOR;
    let len = 2 * licpart::SLOT_BYTES;
    let mut bytes = vec![0u8; len];
    f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
    f.read_exact(&mut bytes).map_err(|e| e.to_string())?;
    let mut part = licpart::LicPart::open(Cursor::new(bytes));
    let mut rec = part.read().map_err(|e| e.to_string())?;
    mode.store(&mut rec).map_err(|e| e.to_string())?;
    part.write(&mut rec).map_err(|e| e.to_string())?;
    let bytes = part.into_device().into_inner();
    f.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
    f.write_all(&bytes[..len]).map_err(|e| e.to_string())
}

fn connect(path: &Path) -> Result<UnixStream, String> {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        match UnixStream::connect(path) {
            Ok(s) => return Ok(s),
            Err(e) if Instant::now() > until => return Err(format!("{}: {e}", path.display())),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// The hypervisor's side of the service port.
struct ServicePeer {
    w: UnixStream,
    got: Arc<Mutex<Vec<FromSystem>>>,
}

impl ServicePeer {
    fn connect(path: &Path) -> Result<Self, String> {
        let s = connect(path)?;
        let mut r = s.try_clone().map_err(|e| e.to_string())?;
        let got = Arc::new(Mutex::new(Vec::new()));
        let into = Arc::clone(&got);
        std::thread::spawn(move || {
            let mut frames = qsipart::Reader::new();
            let mut buf = [0u8; 8192];
            while let Ok(n) = r.read(&mut buf) {
                if n == 0 {
                    break;
                }
                frames.push(&buf[..n]);
                while let Some(f) = frames.next_frame() {
                    if let Ok(Some(m)) = FromSystem::decode(&f) {
                        lock(&into).push(m);
                    }
                }
            }
        });
        Ok(Self { w: s, got })
    }

    fn send(&mut self, m: &ToSystem) -> Result<(), String> {
        let b = m.encode().map_err(|e| e.to_string())?;
        self.w.write_all(&b).map_err(|e| e.to_string())
    }

    fn len(&self) -> usize {
        lock(&self.got).len()
    }

    fn all(&self) -> Vec<FromSystem> {
        lock(&self.got).clone()
    }

    /// The first message from `from` on that `want` takes, and its index.
    fn wait(
        &self,
        from: usize,
        what: &str,
        timeout: Duration,
        want: impl Fn(&FromSystem) -> bool,
    ) -> Result<(usize, FromSystem), String> {
        let until = Instant::now() + timeout;
        loop {
            {
                let g = lock(&self.got);
                if let Some((i, m)) = g.iter().enumerate().skip(from).find(|(_, m)| want(m)) {
                    return Ok((i, m.clone()));
                }
            }
            if Instant::now() > until {
                return Err(format!(
                    "no {what} on the service port within {timeout:?}; got {:?}",
                    lock(&self.got).iter().skip(from).collect::<Vec<_>>()
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// What the console peer has seen.
#[derive(Default)]
struct Seen {
    grid: Grid,
    /// `hello`, `frame`, `update`, in order.
    kinds: Vec<&'static str>,
    /// The SRC display's codes as they showed.
    srcs: Vec<String>,
    /// Updates that did not apply.
    bad: usize,
}

/// The hypervisor's side of the console port.
struct ConsolePeer {
    w: UnixStream,
    seen: Arc<Mutex<Seen>>,
}

impl ConsolePeer {
    fn connect(path: &Path) -> Result<Self, String> {
        let s = connect(path)?;
        let mut r = s.try_clone().map_err(|e| e.to_string())?;
        let mut refresh = s.try_clone().map_err(|e| e.to_string())?;
        let seen = Arc::new(Mutex::new(Seen::default()));
        let into = Arc::clone(&seen);
        std::thread::spawn(move || {
            let mut frames = qsipart::Reader::new();
            let mut buf = vec![0u8; 32 * 1024];
            while let Ok(n) = r.read(&mut buf) {
                if n == 0 {
                    break;
                }
                frames.push(&buf[..n]);
                while let Some(f) = frames.next_frame() {
                    let Ok(Some(m)) = con::FromSystem::decode(&f) else {
                        continue;
                    };
                    let mut s = lock(&into);
                    s.kinds.push(match m {
                        con::FromSystem::Hello(_) => "hello",
                        con::FromSystem::Frame(_) => "frame",
                        con::FromSystem::Update(_) => "update",
                    });
                    if s.grid.apply(&m).is_err() {
                        s.bad += 1;
                        if let Ok(b) = con::ToSystem::Refresh.encode() {
                            let _ = refresh.write_all(&b);
                        }
                    }
                    if let Some(src) = s.grid.status.src.as_ref().map(|x| x.code.clone())
                        && s.srcs.last() != Some(&src)
                    {
                        s.srcs.push(src);
                    }
                }
            }
        });
        let mut p = Self { w: s, seen };
        p.send(&con::ToSystem::Hello(con::Hello {
            version: qsipart::VERSION,
            caps: qsipart::caps::CONSOLE_V1,
        }))?;
        Ok(p)
    }

    fn send(&mut self, m: &con::ToSystem) -> Result<(), String> {
        let b = m.encode().map_err(|e| e.to_string())?;
        self.w.write_all(&b).map_err(|e| e.to_string())
    }

    fn key(&mut self, k: con::Key) -> Result<(), String> {
        self.send(&con::ToSystem::Key(k))?;
        std::thread::sleep(Duration::from_millis(30));
        Ok(())
    }

    fn typed(&mut self, text: &str) -> Result<(), String> {
        for c in text.chars() {
            self.key(con::Key::Char(c))?;
        }
        Ok(())
    }

    fn text(&self) -> String {
        lock(&self.seen).grid.text()
    }

    /// The grid once it holds `needle`.
    fn wait_text(&self, needle: &str, timeout: Duration) -> Result<String, String> {
        self.wait_any(&[needle], timeout).map(|(_, t)| t)
    }

    /// The grid once it holds one of `needles`: which, and the text.
    fn wait_any(&self, needles: &[&str], timeout: Duration) -> Result<(usize, String), String> {
        let until = Instant::now() + timeout;
        loop {
            let t = self.text();
            if let Some(i) = needles.iter().position(|n| t.contains(n)) {
                // Let the rest of the screen arrive.
                std::thread::sleep(Duration::from_millis(400));
                return Ok((i, self.text()));
            }
            if Instant::now() > until {
                return Err(format!(
                    "the console port shows none of {needles:?} within {timeout:?}:\n{t}"
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// `golden/<dir>/<name>.txt` against the grid, every line with its
/// trailing blanks dropped; `skip` lines (1-based) are not compared, nor
/// any after line `upto` when it is not 0.
fn golden(dir: &str, name: &str, grid: &str, skip: &[usize], upto: usize) -> Result<(), String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("golden")
        .join(dir)
        .join(format!("{name}.txt"));
    let want = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let norm = |t: &str| -> Vec<String> {
        let mut v: Vec<String> = t.lines().map(|l| l.trim_end().to_string()).collect();
        while v.last().is_some_and(String::is_empty) {
            v.pop();
        }
        v
    };
    let (g, w) = (norm(grid), norm(&want));
    let mut last = g.len().max(w.len());
    if upto != 0 {
        last = last.min(upto);
    }
    for i in 0..last {
        if skip.contains(&(i + 1)) {
            continue;
        }
        if g.get(i) != w.get(i) {
            return Err(format!(
                "the console port's grid differs from {dir}/{name}.txt at line {}:\n  got:  {:?}\n  want: {:?}",
                i + 1,
                g.get(i),
                w.get(i)
            ));
        }
    }
    Ok(())
}

/// The codes of every `refcode` message, in order.
fn codes(msgs: &[FromSystem]) -> Vec<String> {
    msgs.iter()
        .filter_map(|m| match m {
            FromSystem::Refcode(r) => Some(r.code.clone()),
            _ => None,
        })
        .collect()
}

/// `want` in order within `got`, others between.
fn in_order(got: &[String], want: &[&str]) -> Result<(), String> {
    let mut it = got.iter();
    for w in want {
        if !it.any(|g| g == w) {
            return Err(format!("code {w} missing or out of order in {got:?}"));
        }
    }
    Ok(())
}

/// The peers, kept from one step to the next.
struct Peers {
    svc: ServicePeer,
    con: ConsolePeer,
}

/// Where the guest's sockets and image are.
struct Paths {
    service: PathBuf,
    console: PathBuf,
}

/// The peers between steps: the steps are plain functions of the suite
/// and the session.
static PEERS: Mutex<Option<Peers>> = Mutex::new(None);
static PATHS: Mutex<Option<Paths>> = Mutex::new(None);

fn peers<R>(f: impl FnOnce(&mut Peers) -> Result<R, String>) -> Result<R, String> {
    let mut g = lock(&PEERS);
    let p = g.as_mut().ok_or("the peers are not connected")?;
    f(p)
}

impl Suite {
    /// Steps 195 to 198 in their own guest; with `--only` when named.
    pub(crate) fn partition_steps(&self) -> Result<Vec<StepReport>, crate::HarnessError> {
        if let Some(only) = &self.only
            && !NAMES
                .iter()
                .any(|n| only.iter().any(|p| n.starts_with(p.as_str())))
        {
            return Ok(Vec::new());
        }
        let failed = |reason: String| crate::HarnessError::Failed {
            step: NAMES[0],
            reason,
            log: String::new(),
        };
        let base = self
            .launch
            .image
            .clone()
            .ok_or_else(|| failed("no disk image".into()))?;
        let image = self.out_dir.join("partition.img");
        fs::copy(&base, &image)?;
        set_console_mode(&image, licpart::ConsoleMode::Partition).map_err(failed)?;
        let paths = Paths {
            service: self.out_dir.join("qsi-service.sock"),
            console: self.out_dir.join("qsi-console.sock"),
        };
        let _ = fs::remove_file(&paths.service);
        let _ = fs::remove_file(&paths.console);
        let launch = Launch {
            image: Some(image.clone()),
            partition: Some((paths.service.clone(), paths.console.clone())),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        // Both peers from the start, as the hypervisor is.
        let svc = ServicePeer::connect(&paths.service).map_err(failed)?;
        let con = ConsolePeer::connect(&paths.console).map_err(failed)?;
        *lock(&PEERS) = Some(Peers { svc, con });
        *lock(&PATHS) = Some(paths);
        let mut reports = Vec::new();
        let run = (|| -> Result<(), crate::HarnessError> {
            reports.push(self.step(&mut s, NAMES[0], Self::partition_service)?);
            reports.push(self.step(&mut s, NAMES[1], Self::partition_console)?);
            reports.push(self.step(&mut s, NAMES[2], Self::partition_panel)?);
            reports.push(self.step(&mut s, NAMES[3], Self::partition_down)?);
            Ok(())
        })();
        *lock(&PEERS) = None;
        if !s.wait_exit(Duration::from_secs(2)) {
            s.kill();
        }
        let _ = fs::remove_file(&image);
        run?;
        Ok(reports)
    }

    /// 195: hello, the IPL's codes in order, the state, a ping, QSHWQUAL.
    fn partition_service(&self, s: &mut Session) -> Result<String, String> {
        s.expect(SRC_IPL_COMPLETE, 4 * self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        peers(|p| {
            let (_, complete) = p.svc.wait(0, "complete state", CMD, |m| {
                matches!(m, FromSystem::State(st) if st.state == SysState::Complete)
            })?;
            let msgs = p.svc.all();
            if !matches!(msgs.first(), Some(FromSystem::Hello(_))) {
                return Err(format!("the first message is not the hello: {:?}", msgs.first()));
            }
            // Said again as its facts become known: the last one holds.
            let hellos = msgs.iter().filter(|m| matches!(m, FromSystem::Hello(_))).count();
            let Some(FromSystem::Hello(h)) =
                msgs.iter().rev().find(|m| matches!(m, FromSystem::Hello(_)))
            else {
                unreachable!()
            };
            if h.version != qsipart::VERSION
                || h.release != release::RELEASE.to_string()
                || h.system.is_empty()
                || h.serial.is_empty()
                || h.slot != "A"
                || h.keylock != Keylock::Normal
            {
                return Err(format!("the hello: {h:?}"));
            }
            let got = codes(&msgs);
            let want: Vec<&str> = SRC_SEQUENCE
                .iter()
                .map(|c| c.trim_start_matches("SRC "))
                .collect();
            in_order(&got, &want)?;
            if got.first().map(String::as_str) != Some("C1001000") {
                return Err(format!("the first code sent is not C1001000: {got:?}"));
            }
            let states: Vec<SysState> = msgs
                .iter()
                .filter_map(|m| match m {
                    FromSystem::State(st) => Some(st.state),
                    _ => None,
                })
                .collect();
            if states.first() != Some(&SysState::Ipl) {
                return Err(format!("the states: {states:?}"));
            }
            p.svc.send(&ToSystem::Hello(PeerHello {
                version: qsipart::VERSION,
                caps: qsipart::caps::SERVICE_V1,
                name: "conformance harness".into(),
            }))?;
            let from = p.svc.len();
            p.svc.send(&ToSystem::Ping(Ping { id: 41 }))?;
            p.svc.wait(from, "pong", CMD, |m| matches!(m, FromSystem::Pong(x) if x.id == 41))?;
            let FromSystem::State(st) = complete else {
                unreachable!()
            };
            Ok(format!(
                "{hellos} hello(s), the last {} {} serial {}; {} codes, the IPL's {} in order from C1001000; states {states:?}; fix level {:?}, {} address(es); pong",
                h.system,
                h.release,
                h.serial,
                got.len(),
                want.len(),
                st.fix_level,
                st.addresses.len()
            ))
        })
        .and_then(|detail| {
            for want in ["slic: partition service channel", "qsid: peer connected"] {
                if !s.log.contains(want) {
                    return Err(format!("the serial log has no `{want}`"));
                }
            }
            if s.log.contains("slic: guest agent ") {
                return Err("the QEMU guest agent started under QS/VS".into());
            }
            let out = s.command("QSHWQUAL", CMD)?;
            for want in [
                "Hypervisor . . . . . :   QS/VS",
                "the partition's service channel",
                "the partition's console port",
            ] {
                if !out.contains(want) {
                    return Err(format!("QSHWQUAL shows no `{want}`:\n{out}"));
                }
            }
            if out.contains("No guest agent channel") || out.contains("No service channel") {
                return Err(format!("QSHWQUAL warns of a missing channel:\n{out}"));
            }
            Ok(format!("{detail}; QSHWQUAL names both ports"))
        })
    }

    /// 196: the console port's grid, sign-on and a command by its keys,
    /// detach and attach.
    fn partition_console(&self, _s: &mut Session) -> Result<String, String> {
        peers(|p| {
            let c = &mut p.con;
            let text = c.wait_text("Sign On", self.boot_timeout)?;
            // The sign-on display's first ten rows, as step 2 holds them; at
            // QPWDLVL 2 or 3 (set by an earlier step) the long password form.
            let long = text.lines().nth(9).is_some_and(|l| l.contains("Program/procedure"));
            let name = if long { "sign-on-pwdlvl2" } else { "sign-on" };
            golden("signon", name, &text, &[], 12).map_err(|e| format!("{e}\n{text}"))?;
            let (srcs, kinds) = {
                let seen = lock(&c.seen);
                (seen.srcs.clone(), seen.kinds.clone())
            };
            if !srcs.iter().any(|x| x.starts_with("C1")) || !srcs.iter().any(|x| x == "C6001090")
            {
                return Err(format!("the SRC display's codes on the port: {srcs:?}"));
            }
            if kinds.first() != Some(&"hello") || kinds.get(1) != Some(&"frame") {
                return Err(format!("the console port began with {:?}", &kinds[..kinds.len().min(4)]));
            }
            sign_on(c)?;
            c.typed("DSPLIBL")?;
            c.key(con::Key::Enter)?;
            c.wait_text("Library List", CMD)?;
            let before = c.text();
            let updates = lock(&c.seen).kinds.iter().filter(|k| **k == "update").count();
            Ok((srcs.len(), updates, before))
        })
        .and_then(|(srcs, updates, before)| {
            // Detach mid-session, attach again: a whole frame.
            let path = lock(&PATHS)
                .as_ref()
                .map(|x| x.console.clone())
                .ok_or("no console socket")?;
            {
                let mut g = lock(&PEERS);
                let p = g.as_mut().ok_or("no peers")?;
                let _ = p.con.w.shutdown(std::net::Shutdown::Both);
                std::thread::sleep(Duration::from_secs(2));
                p.con = ConsolePeer::connect(&path)?;
            }
            peers(|p| {
                let until = Instant::now() + CMD;
                while !lock(&p.con.seen).grid.ready() && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(50));
                }
                let kinds = lock(&p.con.seen).kinds.clone();
                if kinds.first() != Some(&"hello") || kinds.get(1) != Some(&"frame") {
                    return Err(format!("after attaching again: {kinds:?}"));
                }
                let after = p.con.text();
                if after != before {
                    return Err(format!(
                        "the whole frame after attaching is not the screen:\n{after}\n---\n{before}"
                    ));
                }
                // Back to the menu for the next step.
                p.con.key(con::Key::Function(12))?;
                Ok(format!(
                    "{srcs} SRC display codes; sign-on as its golden; signed on and ran DSPLIBL by keys ({updates} updates); attached again to a whole frame equal to the screen"
                ))
            })
        })
    }

    /// 197: function 21 refused at a normal keylock, a manual IPL from
    /// the console port, function 21 then.
    fn partition_panel(&self, s: &mut Session) -> Result<String, String> {
        peers(|p| {
            let from = p.svc.len();
            p.svc.send(&ToSystem::Panel(Panel {
                id: 51,
                function: 21,
            }))?;
            p.svc.wait(from, "refusal of 21", CMD, |m| {
                matches!(m, FromSystem::Refused(r) if r.id == 51 && r.msgid == msgid::NOT_MANUAL)
            })?;
            p.svc.send(&ToSystem::Panel(Panel {
                id: 50,
                function: 3,
            }))?;
            p.svc.wait(
                from,
                "refusal of 3",
                CMD,
                |m| matches!(m, FromSystem::Refused(r) if r.id == 50 && r.msgid == msgid::FUNCTION),
            )?;
            Ok(())
        })?;
        s.command("mode manual", CMD)?;
        let mark = peers(|p| Ok(p.svc.len()))?;
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001090", CMD)?;
        let ipl_options = peers(|p| {
            // The service port says hello again, at a manual keylock.
            p.svc.wait(
                mark,
                "hello after the restart",
                self.boot_timeout,
                |m| matches!(m, FromSystem::Hello(h) if h.keylock == Keylock::Manual),
            )?;
            let c = &mut p.con;
            c.wait_text("IPL or Install the System", self.boot_timeout)?;
            c.typed("1")?;
            c.key(con::Key::Enter)?;
            let text = c.wait_text("IPL Options", CMD)?;
            // The date and time are the clock's.
            golden("install", "ipl-options", &text, &[4, 5], 0)?;
            c.key(con::Key::Enter)?;
            c.wait_text("Set Major System Options", CMD)?;
            c.key(con::Key::Enter)?;
            Ok(text)
        })?;
        s.expect(SRC_IPL_COMPLETE, 4 * self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let detail = peers(|p| {
            p.con.wait_text("Sign On", Duration::from_secs(90))?;
            let from = p.svc.len();
            p.svc.send(&ToSystem::Panel(Panel {
                id: 52,
                function: 21,
            }))?;
            p.svc.wait(
                from,
                "ack of 21",
                CMD,
                |m| matches!(m, FromSystem::Ack(a) if a.id == 52),
            )?;
            let c = &mut p.con;
            let text = c.wait_text("Dedicated Service Tools (DST) Sign On", CMD)?;
            golden("dst", "signon", &text, &[], 0)?;
            dst_sign_on(c)?;
            let main = c.wait_text("Use Dedicated Service Tools (DST)", CMD)?;
            golden("dst", "main", &main, &[], 0)?;
            let from = p.svc.len();
            p.svc.send(&ToSystem::Panel(Panel {
                id: 53,
                function: 21,
            }))?;
            p.svc.wait(
                from,
                "refusal while DST shows",
                CMD,
                |m| matches!(m, FromSystem::Refused(r) if r.id == 53 && r.msgid == msgid::DST_BUSY),
            )?;
            p.con.key(con::Key::Function(3))?;
            p.con.wait_text("Sign On", CMD)?;
            Ok(format!(
                "21 refused at a normal keylock, 3 refused; manual IPL from the console port (IPL Options as its golden, {} rows); 21 accepted: DST sign-on and main menu as their goldens, 21 refused while they show; the session back after F3",
                ipl_options.lines().count()
            ))
        })?;
        s.expect("function 21's service tools ended", CMD)?;
        Ok(detail)
    }

    /// 198: a controlled power down with a delay; then a terminal code
    /// and a halt.
    fn partition_down(&self, s: &mut Session) -> Result<String, String> {
        let mark = peers(|p| {
            let from = p.svc.len();
            p.svc.send(&ToSystem::PowerDown(PowerDown {
                id: 61,
                immediate: false,
                delay: 10,
                restart: false,
            }))?;
            p.svc.wait(
                from,
                "ack of the power down",
                CMD,
                |m| matches!(m, FromSystem::Ack(a) if a.id == 61),
            )?;
            Ok(from)
        })?;
        let started = Instant::now();
        s.expect("SRC D1001090", 3 * CMD)?;
        if !s.wait_exit(Duration::from_secs(60)) {
            return Err("the partition did not power off".into());
        }
        let down = peers(|p| {
            let msgs: Vec<FromSystem> = p.svc.all().into_iter().skip(mark).collect();
            let got = codes(&msgs);
            in_order(&got, &["D1001000", "D1001005", "D1001007", "D1001090"])?;
            let states: Vec<SysState> = msgs
                .iter()
                .filter_map(|m| match m {
                    FromSystem::State(st) => Some(st.state),
                    _ => None,
                })
                .collect();
            if !states.ends_with(&[SysState::Down]) || !states.contains(&SysState::Ending) {
                return Err(format!("the states of the power down: {states:?}"));
            }
            Ok(format!("{} codes, {states:?}", got.len()))
        })?;
        let secs = started.elapsed().as_secs_f64();
        // Up again, then the storage service killed: B1001060 and a halt.
        let (service, console) = {
            let g = lock(&PATHS);
            let x = g.as_ref().ok_or("no sockets")?;
            (x.service.clone(), x.console.clone())
        };
        let launch = Launch {
            image: Some(self.out_dir.join("partition.img")),
            partition: Some((service.clone(), console.clone())),
            ..self.launch.clone()
        };
        *s = Session::spawn(&launch).map_err(|e| e.to_string())?;
        {
            let svc = ServicePeer::connect(&service)?;
            let con = ConsolePeer::connect(&console)?;
            *lock(&PEERS) = Some(Peers { svc, con });
        }
        s.expect(SRC_IPL_COMPLETE, 4 * self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let mark = peers(|p| Ok(p.svc.len()))?;
        let out = s.command("dbgkill JOB(*STG)", CMD)?;
        if !out.contains("killed") {
            return Err(format!("dbgkill JOB(*STG): {out}"));
        }
        s.expect("SRC B1001060", CMD)?;
        let halt = peers(|p| {
            let (at, _) = p.svc.wait(
                mark,
                "the terminal code",
                CMD,
                |m| matches!(m, FromSystem::Refcode(r) if r.code == "B1001060"),
            )?;
            p.svc.wait(
                at,
                "the down state",
                CMD,
                |m| matches!(m, FromSystem::State(st) if st.state == SysState::Down),
            )?;
            Ok(())
        });
        halt?;
        s.expect("slic: IPL halted", CMD)?;
        // Halted, not powered off: QEMU keeps running.
        if s.wait_exit(Duration::from_secs(10)) {
            return Err("the partition powered off after its terminal code".into());
        }
        Ok(format!(
            "controlled power down, DELAY(10): {down}, off after {secs:.1}s; B1001060 then down, still running 10s later"
        ))
    }
}

/// Sign on at the console's sign-on display by its keys, whatever state
/// QSECOFR's password is in; leaves the first menu showing.
fn sign_on(c: &mut ConsolePeer) -> Result<(), String> {
    let password = crate::phase5::SECOFR_PASSWORD;
    for pw in [password, "QSECOFR"] {
        c.key(con::Key::Home)?;
        c.typed("QSECOFR")?;
        c.key(con::Key::FieldExit)?;
        c.typed(pw)?;
        c.key(con::Key::FieldExit)?;
        c.key(con::Key::Enter)?;
        let (i, _) = c.wait_any(
            &[
                "===>",
                "Change Password",
                "not correct",
                "Sign-on Information",
            ],
            CMD,
        )?;
        match i {
            0 => return Ok(()),
            1 => {
                c.typed(pw)?;
                c.key(con::Key::FieldAdvance)?;
                c.typed(password)?;
                c.key(con::Key::FieldAdvance)?;
                c.typed(password)?;
                c.key(con::Key::Enter)?;
                let (i, _) = c.wait_any(&["===>", "Sign-on Information"], CMD)?;
                if i == 1 {
                    c.key(con::Key::Enter)?;
                }
                c.wait_text("===>", CMD)?;
                return Ok(());
            }
            3 => {
                c.key(con::Key::Enter)?;
                c.wait_text("===>", CMD)?;
                return Ok(());
            }
            _ => {}
        }
    }
    Err(format!("QSECOFR could not sign on:\n{}", c.text()))
}

/// Sign on to DST by the console port's keys: the shipped password,
/// changed when it is expired, or the changed one.
fn dst_sign_on(c: &mut ConsolePeer) -> Result<(), String> {
    for pw in ["QSECOFR", DST_PASSWORD] {
        c.key(con::Key::Home)?;
        c.typed("QSECOFR")?;
        c.key(con::Key::FieldExit)?;
        c.typed(pw)?;
        c.key(con::Key::FieldExit)?;
        c.key(con::Key::Enter)?;
        let (i, _) = c.wait_any(
            &[
                "Use Dedicated Service Tools (DST)",
                "Change Service Tools User Password",
                "not correct",
            ],
            CMD,
        )?;
        match i {
            0 => return Ok(()),
            1 => {
                c.key(con::Key::FieldAdvance)?;
                c.typed("QSECOFR")?;
                c.key(con::Key::FieldAdvance)?;
                c.typed(DST_PASSWORD)?;
                c.key(con::Key::FieldAdvance)?;
                c.typed(DST_PASSWORD)?;
                c.key(con::Key::Enter)?;
                c.wait_text("Use Dedicated Service Tools (DST)", CMD)?;
                return Ok(());
            }
            _ => {}
        }
    }
    Err(format!("QSECOFR could not sign on to DST:\n{}", c.text()))
}
