// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 201 to 206: a standalone QS/400 answers QSI (docs/QSID.md
//! section 12), in a guest of its own, driven from the host by consoles
//! on `qsisess`'s client side over `qsitrust`'s TLS (`peer.rs`).
//!
//! - 201: `STRQSIPAIR` at the console display shows a password and the
//!   fingerprint; a console not pinned is answered only for pairing,
//!   pairs, connects again pinned and is answered; `WRKQSICSL` lists it;
//!   the `QI` audit entry.
//! - 202: every base request; the panel's displays and its refusals at a
//!   normal keylock; a dump taken, downloaded, listed with its space and
//!   deleted, with its events.
//! - 203: five wrong passwords and the password is spent (`QSI0201`); a
//!   replayed exchange (`QSI0202`); a pin removed ends its session; a
//!   password ended with F12; a console nobody pinned gets no session;
//!   the `QI` entries of each.
//! - 204: a controlled restart asked over QSI into a manual IPL; the
//!   IPL's codes as events, replayed to the console that reconnects;
//!   function 21: dedicated service tools on the console, refused while
//!   they show; a second console paired from DST's new option.
//! - 205: a problem opened (by a dump), noted, listed with its severity
//!   and closed, with its events.
//! - 206: a controlled power down asked over QSI.

mod peer;

pub(crate) use peer::{Peer, WAIT, ask, base, refused};

use std::fs;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use qsi::any::{AnyEvent, AnyReply, AnyRequest};
use qsi::host::model::SYSTEM_ID;
use qsi::host::{console, panel};
use qsi::{Empty, PeerKind, service};

use crate::dst::{PASSWORD as DST_PASSWORD, answer_ipl_options};
use crate::httpadm::secofr_password;
use crate::phase5::{SECOFR_PASSWORD, console_sign_on_as, screen_any, screen_wait};
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, SRC_IPL_COMPLETE, StepReport, Suite};
use crate::tcp::{GUEST, forward_port};

/// The steps, in order.
pub(crate) const NAMES: [&str; 6] = [
    "201 QSI pairing from the command",
    "202 QSI base requests",
    "203 QSI pairing refused",
    "204 QSI events across an IPL and function 21",
    "205 QSI problems",
    "206 QSI controlled power down",
];

/// The guest's QSI port.
const PORT: u16 = 2300;

/// What the steps share: the forwarded port, the consoles, the system's
/// fingerprint, the last event seen.
struct State {
    port: u16,
    mc01: Peer,
    mc02: Peer,
    system: String,
    last_event: Option<u64>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn lock() -> MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn with<T>(f: impl FnOnce(&mut State) -> Result<T, String>) -> Result<T, String> {
    let mut g = lock();
    let st = g.as_mut().ok_or("step 201 did not run")?;
    f(st)
}

/// A console command whose output holds one of `want`.
fn expect(s: &mut Session, line: &str, want: &[&str]) -> Result<String, String> {
    let out = s.command(line, Duration::from_secs(60))?;
    if want.iter().any(|w| out.contains(w)) {
        Ok(out)
    } else {
        Err(format!("{line}: none of {want:?} in:\n{out}"))
    }
}

/// Type a line at the console display and press Enter.
fn enter(s: &mut Session, text: &str) -> Result<(), String> {
    s.command(&format!("type {text}"), CMD)?;
    s.command("key enter", CMD).map(|_| ())
}

/// The value after the dots on the line of the screen that names
/// `label`.
fn field(screen: &str, label: &str) -> Option<String> {
    let line = screen.lines().find(|l| l.contains(label))?;
    let (_, v) = line.split_once(':')?;
    Some(v.trim().to_string())
}

/// The fingerprint on the screen: its label's line, then the next.
fn shown_fingerprint(screen: &str) -> Option<String> {
    let mut lines = screen.lines();
    lines
        .by_ref()
        .find(|l| l.contains("System's QSI fingerprint"))?;
    let first = screen
        .lines()
        .find(|l| l.contains("System's QSI fingerprint"))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default();
    let second = lines.next()?.trim().to_string();
    let hex = format!("{first}{second}");
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then_some(hex)
}

/// `STRQSIPAIR` at the console's main menu: the password and the
/// fingerprint shown; Enter leaves it out.
fn strqsipair(s: &mut Session) -> Result<(String, String), String> {
    enter(s, "STRQSIPAIR")?;
    let screen = screen_wait(s, "Start QSI Pairing")?;
    let password = field(&screen, "Pairing password").ok_or_else(|| screen.clone())?;
    let fp = shown_fingerprint(&screen).ok_or_else(|| format!("no fingerprint:\n{screen}"))?;
    s.command("key enter", CMD)?;
    screen_wait(s, "Main Menu")?;
    // The server looks at whether a password is out twice a second.
    std::thread::sleep(Duration::from_millis(1500));
    Ok((password, fp))
}

/// The `QI` entries of the audit journal.
fn audit_qi(s: &mut Session) -> Result<String, String> {
    s.command("DSPAUDJRNE ENTTYP(QI)", Duration::from_secs(30))
}

/// Events of `c` for `wait`, as any family's.
fn events(c: &mut qsisvr::client::Client, wait: Duration) -> Vec<(u64, AnyEvent)> {
    c.events_for(wait)
        .into_iter()
        .filter_map(|e| e.decode().ok().flatten().map(|a| (e.seq, a)))
        .collect()
}

/// Events until `want` finds one, or `within` passes.
fn event_until(
    c: &mut qsisvr::client::Client,
    seen: &mut Vec<(u64, AnyEvent)>,
    within: Duration,
    want: impl Fn(&AnyEvent) -> bool,
) -> bool {
    let until = Instant::now() + within;
    while Instant::now() < until {
        if seen.iter().any(|(_, e)| want(e)) {
            return true;
        }
        seen.extend(events(c, Duration::from_millis(500)));
    }
    seen.iter().any(|(_, e)| want(e))
}

impl Suite {
    /// Steps 201 to 206 in a guest of their own; with `--only` when
    /// named.
    pub(crate) fn qsi_steps(&self) -> Result<Vec<StepReport>, crate::HarnessError> {
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
        let image = self.out_dir.join("qsi.img");
        fs::copy(&base, &image)?;
        let launch = Launch {
            image: Some(image.clone()),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let run = (|| -> Result<Vec<StepReport>, crate::HarnessError> {
            Ok(vec![
                self.step(&mut s, NAMES[0], Self::qsi_pair_command)?,
                self.step(&mut s, NAMES[1], Self::qsi_base)?,
                self.step(&mut s, NAMES[2], Self::qsi_refused)?,
                self.step(&mut s, NAMES[3], Self::qsi_ipl_and_dst)?,
                self.step(&mut s, NAMES[4], Self::qsi_problems)?,
                self.step(&mut s, NAMES[5], Self::qsi_power_down)?,
            ])
        })();
        *lock() = None;
        if !s.wait_exit(Duration::from_secs(5)) {
            s.command("pwrdwn", CMD).ok();
            s.wait_exit(Duration::from_secs(20));
        }
        let _ = fs::remove_file(&image);
        run
    }

    /// 201: pairing from `STRQSIPAIR` at the console.
    #[allow(clippy::unused_self)] // a step's signature
    fn qsi_pair_command(&self, s: &mut Session) -> Result<String, String> {
        s.expect(SRC_IPL_COMPLETE, 4 * self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        secofr_password(s)?;
        s.command(
            "CHGSECAUD QAUDCTL(*AUDLVL) QAUDLVL(*SECCFG *AUTFAIL)",
            Duration::from_secs(60),
        )?;
        // The server starts with TCP/IP and makes the identity at its
        // first start.
        let until = Instant::now() + Duration::from_secs(90);
        let listed = loop {
            let out = s.command("WRKQSICSL", CMD)?;
            if out.contains("System's QSI fingerprint:") && !out.contains("(none yet)") {
                break out;
            }
            if Instant::now() > until {
                return Err(format!("the QSI server never made its identity:\n{out}"));
            }
            std::thread::sleep(Duration::from_secs(2));
        };
        let port = forward_port(self, GUEST, PORT)?;
        let mut first = console_sign_on_as(s, "QSECOFR", SECOFR_PASSWORD, SECOFR_PASSWORD);
        if first.as_ref().is_err_and(|e| e.contains("not correct")) {
            first = console_sign_on_as(s, "QSECOFR", "QSECOFR", SECOFR_PASSWORD);
        }
        if !first?.contains("Main Menu") {
            enter(s, "go main")?;
        }
        let (password, shown) = strqsipair(s)?;
        if !listed.contains(&shown) {
            return Err(format!(
                "the display's fingerprint {shown} is not WRKQSICSL's:\n{listed}"
            ));
        }
        let mc01 = Peer::new("MC01")?;
        // Before its pin, only pairing is answered.
        mc01.pins.pairing(true);
        let mut c = mc01.connect(port)?;
        let id = refused(
            &mut c,
            &AnyRequest::Base(qsi::base::Request::Identify(Empty)),
        )?;
        if id != "QSI0101" {
            return Err(format!("identify before the pin: {id}"));
        }
        c.close();
        let fp = mc01.pair(port, &password)?;
        if fp.hex() != shown {
            return Err(format!(
                "paired with {}, the display showed {shown}",
                fp.hex()
            ));
        }
        let mut c = mc01.connect(port)?;
        let name = match base(&mut c, qsi::base::Request::Identify(Empty))? {
            qsi::base::Reply::Identify(i) if i.kind == PeerKind::Standalone => i.name,
            other => return Err(format!("identify: {other:?}")),
        };
        c.close();
        let listed = s.command("WRKQSICSL", CMD)?;
        if !listed.contains("MC01") || !listed.contains(&mc01.fingerprint().hex()) {
            return Err(format!("WRKQSICSL does not list MC01:\n{listed}"));
        }
        let qi = audit_qi(s)?;
        if !qi.contains("MC01") {
            return Err(format!("no QI entry of the pairing:\n{qi}"));
        }
        *lock() = Some(State {
            port,
            mc01,
            mc02: Peer::new("MC02")?,
            system: shown.clone(),
            last_event: None,
        });
        Ok(format!(
            "STRQSIPAIR showed a password and {}…; identify refused QSI0101 before the pin; MC01 paired and was answered pinned by {name}; WRKQSICSL lists it; a QI entry",
            &shown[..16]
        ))
    }

    /// 202: every base request, the panel, a dump.
    #[allow(clippy::unused_self, clippy::too_many_lines)] // a step's signature; every request
    fn qsi_base(&self, _s: &mut Session) -> Result<String, String> {
        with(|st| {
            let mut c = st.mc01.connect(st.port)?;
            let ready = c.agreed.ok_or("no agreement")?;
            if ready.caps & qsi::caps::STANDALONE != qsi::caps::STANDALONE {
                return Err(format!("agreed on {:#x}", ready.caps));
            }
            let sub = base(
                &mut c,
                qsi::base::Request::Subscribe(qsi::base::Subscribe { since: None }),
            )?;
            let qsi::base::Reply::Subscribe(sub) = sub else {
                return Err(format!("subscribe: {sub:?}"));
            };
            let state = match base(&mut c, qsi::base::Request::State(Empty))? {
                qsi::base::Reply::State(s) => s,
                other => return Err(format!("state: {other:?}")),
            };
            if state.power != qsi::base::PowerState::Running
                || state.keylock != Some(qsi::base::Keylock::Normal)
            {
                return Err(format!("state: {state:?}"));
            }
            let codes = match base(
                &mut c,
                qsi::base::Request::Refcodes(qsi::base::Refcodes {
                    before: None,
                    limit: 200,
                }),
            )? {
                qsi::base::Reply::Refcodes(l) => l,
                other => return Err(format!("refcodes: {other:?}")),
            };
            let complete = SRC_IPL_COMPLETE.trim_start_matches("SRC ");
            if !codes.codes.iter().any(|r| r.code == complete) {
                return Err(format!("no {complete} in the history: {codes:?}"));
            }
            let time = match base(&mut c, qsi::base::Request::Time(Empty))? {
                qsi::base::Reply::Time(t) => t,
                other => return Err(format!("time: {other:?}")),
            };
            let host_ns = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(0));
            if (time.unix_ns - host_ns).abs() > 600_000_000_000 {
                return Err(format!("the clock is far from the host's: {time:?}"));
            }
            let level = match base(&mut c, qsi::base::Request::FixLevel(Empty))? {
                qsi::base::Reply::FixLevel(l) => l,
                other => return Err(format!("fix level: {other:?}")),
            };
            // The panel at the system's own ID.
            let f = |n: u8| {
                AnyRequest::Panel(panel::Request::Function(panel::Function {
                    id: SYSTEM_ID,
                    function: n,
                    args: panel::PanelArgs::default(),
                }))
            };
            let shown = match ask(&mut c, &f(1))? {
                Ok(AnyReply::Panel(panel::Reply::Function(s))) => s.text,
                other => return Err(format!("function 01: {other:?}")),
            };
            for (n, want) in [(21, "QSI0452"), (34, "QSI0451"), (8, "QSI0452")] {
                let got = refused(&mut c, &f(n))?;
                if got != want {
                    return Err(format!("function {n:02}: {got}, not {want}"));
                }
            }
            let other = refused(
                &mut c,
                &AnyRequest::Console(console::Request::Screen(console::Screen { id: 1 })),
            )?;
            if other != "QSI0331" {
                return Err(format!("console of partition 1: {other}"));
            }
            let funcs = match ask(
                &mut c,
                &AnyRequest::Panel(panel::Request::Functions(panel::Functions {
                    id: SYSTEM_ID,
                })),
            )? {
                Ok(AnyReply::Panel(panel::Reply::Functions(v))) => v,
                other => return Err(format!("functions: {other:?}")),
            };
            // A dump: taken, listed, its space, downloaded, deleted.
            let name = match base(
                &mut c,
                qsi::base::Request::TakeDump(qsi::base::TakeDump {
                    kind: qsi::base::DumpKind::ServiceData,
                    kind_name: String::new(),
                }),
            )? {
                qsi::base::Reply::TakeDump(b) => b.name,
                other => return Err(format!("take dump: {other:?}")),
            };
            let dumps = match base(&mut c, qsi::base::Request::Dumps(Empty))? {
                qsi::base::Reply::Dumps(d) => d,
                other => return Err(format!("dumps: {other:?}")),
            };
            let Some(d) = dumps.iter().find(|d| d.name == name).cloned() else {
                return Err(format!("{name} not listed: {dumps:?}"));
            };
            let space = match base(&mut c, qsi::base::Request::DumpSpace(Empty))? {
                qsi::base::Reply::DumpSpace(s) => s,
                other => return Err(format!("dump space: {other:?}")),
            };
            if space.dumps == 0 || space.bytes < d.bytes {
                return Err(format!("dump space {space:?} for {d:?}"));
            }
            let dl = c.ask(
                &AnyRequest::Base(qsi::base::Request::DownloadDump(qsi::base::DownloadDump {
                    name: name.clone(),
                })),
                WAIT,
            )?;
            let stream = dl
                .stream
                .ok_or_else(|| format!("download: {:?}", dl.refusal()))?;
            let mut got = 0usize;
            let until = Instant::now() + WAIT;
            let mut closed = false;
            while !closed && Instant::now() < until {
                c.step();
                c.seen.retain(|i| match i {
                    qsisess::Incoming::Data { stream: x, bytes } if *x == stream => {
                        got += bytes.len();
                        false
                    }
                    qsisess::Incoming::StreamClosed { stream: x } if *x == stream => {
                        closed = true;
                        false
                    }
                    _ => true,
                });
            }
            if got as u64 != d.bytes {
                return Err(format!("downloaded {got} of {} bytes", d.bytes));
            }
            let mut seen = Vec::new();
            if !event_until(
                &mut c,
                &mut seen,
                Duration::from_secs(10),
                |e| matches!(e, AnyEvent::Base(qsi::base::Event::DumpTaken(t)) if t.name == name),
            ) {
                return Err(format!("no DumpTaken event: {seen:?}"));
            }
            match base(
                &mut c,
                qsi::base::Request::DeleteDump(qsi::base::DeleteDump { name: name.clone() }),
            )? {
                qsi::base::Reply::DeleteDump(_) => {}
                other => return Err(format!("delete dump: {other:?}")),
            }
            if !event_until(
                &mut c,
                &mut seen,
                Duration::from_secs(10),
                |e| matches!(e, AnyEvent::Base(qsi::base::Event::DumpRemoved(r)) if r.name == name),
            ) {
                return Err(format!("no DumpRemoved event: {seen:?}"));
            }
            st.last_event = seen.iter().map(|(n, _)| *n).max().or(Some(sub.next - 1));
            c.close();
            Ok(format!(
                "identify, state (running, normal), {} codes with {complete}, time, fix level {:?}, function 01 {shown:?}, 21 and 08 QSI0452, 34 QSI0451, partition 1 QSI0331, {} functions, dump {name} of {} bytes listed, its space, downloaded whole, deleted, with its events",
                codes.codes.len(),
                level.level,
                funcs.len(),
                d.bytes
            ))
        })
    }

    /// 203: wrong passwords, a replay, a removal, an ended password, a
    /// console nobody pinned.
    #[allow(clippy::unused_self)] // a step's signature
    fn qsi_refused(&self, s: &mut Session) -> Result<String, String> {
        let port = with(|st| Ok(st.port))?;
        let mc03 = Peer::new("MC03")?;
        let (password, _) = strqsipair(s)?;
        mc03.pins.pairing(true);
        let mut c = mc03.connect(port)?;
        for i in 0..5 {
            match c.pair("0000-0000-0000-0000", "MC03", WAIT) {
                Err(e) if e.contains("wrong password") => {}
                other => return Err(format!("wrong password {}: {other:?}", i + 1)),
            }
        }
        match c.pair(&password, "MC03", WAIT) {
            Err(e) if e.starts_with("QSI0201") => {}
            other => return Err(format!("after five tries: {other:?}")),
        }
        c.close();
        // A good exchange kept, then replayed on a new connection.
        let (password, _) = strqsipair(s)?;
        let mut c = mc03.connect(port)?;
        let (start, confirm, ok) = c.pair_kept(&password, "MC03", WAIT)?;
        c.close();
        if !ok {
            return Err("MC03 did not pair".into());
        }
        mc03.pins.pairing(false);
        // Pinned now; removed while it has a session.
        let fp_sys = with(|st| Ok(st.mc01.pins.system()))?.ok_or("MC01 pins nothing")?;
        mc03.pins.pin(fp_sys);
        let mut pinned = mc03.connect(port)?;
        base(&mut pinned, qsi::base::Request::Identify(Empty))?;
        expect(s, "RMVQSICSL CONSOLE(MC03)", &["TCP1A87"])?;
        let until = Instant::now() + Duration::from_secs(10);
        while !pinned.is_closed() && Instant::now() < until {
            pinned.step();
        }
        if !pinned.is_closed() {
            return Err("MC03's session went on after its removal".into());
        }
        let (_, _) = strqsipair(s)?;
        mc03.pins.pairing(true);
        let mut c = mc03.connect(port)?;
        let (_, done) = c.pair_raw(&start, &confirm, WAIT)?;
        match done.refusal() {
            Some(("QSI0202", _)) => {}
            other => return Err(format!("the replay: {other:?}")),
        }
        c.close();
        // The password ended with F12; nobody unpinned gets in.
        enter(s, "STRQSIPAIR")?;
        screen_wait(s, "Start QSI Pairing")?;
        s.command("key f12", CMD)?;
        // That it ended shows as MC04 refused at TLS below.
        screen_wait(s, "Main Menu")?;
        std::thread::sleep(Duration::from_millis(1500));
        let mc04 = Peer::new("MC04")?;
        mc04.pins.pairing(true);
        if mc04.connect(port).is_ok() {
            return Err("a console nobody pinned got a session".into());
        }
        let qi = audit_qi(s)?;
        if !qi.contains("MC03") {
            return Err(format!("no QI entries of MC03:\n{qi}"));
        }
        Ok(
            "five wrong passwords then QSI0201; a replayed exchange QSI0202; MC03 removed by RMVQSICSL and its session ended; the password ended with F12; MC04 refused at TLS; QI entries"
                .into(),
        )
    }

    /// 204: a restart over QSI into a manual IPL, the IPL's codes as
    /// events, function 21 and DST's pairing.
    #[allow(clippy::too_many_lines)] // the IPL, the events, DST
    fn qsi_ipl_and_dst(&self, s: &mut Session) -> Result<String, String> {
        let (port, since) = with(|st| Ok((st.port, st.last_event)))?;
        s.command("mode manual", CMD)?;
        let mut c = with(|st| st.mc01.connect(port))?;
        match base(
            &mut c,
            qsi::base::Request::Power(qsi::base::Power {
                action: qsi::base::PowerAction::Restart,
                immediate: false,
                delay: 30,
            }),
        )? {
            qsi::base::Reply::Power(_) => {}
            other => return Err(format!("power: {other:?}")),
        }
        s.expect("SRC D1001000", Duration::from_secs(60))?;
        s.expect("SRC D1001090", Duration::from_secs(120))?;
        drop(c);
        s.expect("IPL or Install the System", self.boot_timeout)?;
        s.send("1")?;
        answer_ipl_options(s, self.boot_timeout)?;
        s.expect(SRC_IPL_COMPLETE, 4 * self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        // The console reconnects and is given what it missed.
        let mut c = with(|st| st.mc01.connect_within(port, Duration::from_secs(120)))?;
        let sub = base(
            &mut c,
            qsi::base::Request::Subscribe(qsi::base::Subscribe { since }),
        )?;
        let qsi::base::Reply::Subscribe(sub) = sub else {
            return Err(format!("subscribe: {sub:?}"));
        };
        if sub.replay != qsi::base::Replay::Resumed {
            return Err(format!("not resumed after the IPL: {sub:?}"));
        }
        let mut seen = Vec::new();
        let complete = SRC_IPL_COMPLETE.trim_start_matches("SRC ");
        let started = event_until(
            &mut c,
            &mut seen,
            Duration::from_secs(10),
            |e| matches!(e, AnyEvent::Base(qsi::base::Event::Refcode(r)) if r.code == "C1001000"),
        );
        let done = event_until(
            &mut c,
            &mut seen,
            Duration::from_secs(10),
            |e| matches!(e, AnyEvent::Base(qsi::base::Event::Refcode(r)) if r.code == complete),
        );
        if !started || !done {
            return Err(format!(
                "the IPL's codes were not replayed ({} events): {seen:?}",
                seen.len()
            ));
        }
        let state = match base(&mut c, qsi::base::Request::State(Empty))? {
            qsi::base::Reply::State(s) => s,
            other => return Err(format!("state: {other:?}")),
        };
        if state.keylock != Some(qsi::base::Keylock::Manual) {
            return Err(format!("the keylock after a manual IPL: {state:?}"));
        }
        // Function 21: the service tools on the console.
        let f21 = AnyRequest::Panel(panel::Request::Function(panel::Function {
            id: SYSTEM_ID,
            function: 21,
            args: panel::PanelArgs::default(),
        }));
        match ask(&mut c, &f21)? {
            Ok(AnyReply::Panel(panel::Reply::Function(_))) => {}
            other => return Err(format!("function 21: {other:?}")),
        }
        screen_wait(s, "Dedicated Service Tools (DST) Sign On")?;
        let busy = refused(&mut c, &f21)?;
        if busy != "QSI0453" {
            return Err(format!("function 21 while DST shows: {busy}"));
        }
        dst_in(s)?;
        enter(s, "5")?;
        screen_wait(s, "Work with DST Environment")?;
        enter(s, "6")?;
        let list = screen_wait(s, "Work with Management Consoles")?;
        if !list.contains("MC01") {
            return Err(format!("DST does not list MC01:\n{list}"));
        }
        s.command("key f6", CMD)?;
        let shown = screen_wait(s, "Make a Pairing Password")?;
        let password = field(&shown, "Pairing password").ok_or_else(|| shown.clone())?;
        std::thread::sleep(Duration::from_millis(1500));
        let system = with(|st| Ok(st.system.clone()))?;
        if !shown.contains(&system) {
            return Err(format!("DST's fingerprint is not the system's:\n{shown}"));
        }
        with(|st| st.mc02.pair(port, &password).map(|_| ()))?;
        s.command("key enter", CMD)?;
        let list = screen_wait(s, "Work with Management Consoles")?;
        if !list.contains("MC02") {
            return Err(format!("DST does not list MC02 after its pairing:\n{list}"));
        }
        s.command("key f3", CMD)?;
        screen_wait(s, "Use Dedicated Service Tools (DST)")?;
        s.command("key f3", CMD)?;
        s.expect("function 21's service tools ended", Duration::from_secs(30))?;
        let mut c2 = with(|st| st.mc02.connect(port))?;
        base(&mut c2, qsi::base::Request::Identify(Empty))?;
        c2.close();
        with(|st| {
            st.last_event = seen.iter().map(|(n, _)| *n).max().or(st.last_event);
            Ok(())
        })?;
        c.close();
        Ok(format!(
            "a controlled restart over QSI into a manual IPL; {} events replayed to MC01 with C1001000 and {complete}; the keylock manual; function 21 showed DST, refused QSI0453 while it showed; DST option 5, 6 listed MC01 and paired MC02 with F6; MC02 answered pinned",
            seen.len()
        ))
    }

    /// 205: a problem opened, noted, listed with its severity, closed.
    #[allow(clippy::unused_self, clippy::too_many_lines)] // a step's signature; the whole life
    fn qsi_problems(&self, _s: &mut Session) -> Result<String, String> {
        with(|st| {
            let mut c = st.mc01.connect(st.port)?;
            base(
                &mut c,
                qsi::base::Request::Subscribe(qsi::base::Subscribe { since: None }),
            )?;
            // A dump logs a problem.
            base(
                &mut c,
                qsi::base::Request::TakeDump(qsi::base::TakeDump {
                    kind: qsi::base::DumpKind::ServiceData,
                    kind_name: String::new(),
                }),
            )?;
            let mut seen = Vec::new();
            if !event_until(&mut c, &mut seen, Duration::from_secs(10), |e| {
                matches!(e, AnyEvent::Service(service::Event::Opened(_)))
            }) {
                return Err(format!("no Opened event: {seen:?}"));
            }
            let list = |c: &mut qsisvr::client::Client| -> Result<service::RatedPage, String> {
                match ask(
                    c,
                    &AnyRequest::Service(service::Request::ListRated(service::List {
                        filter: service::Filter {
                            status: Some(service::ProblemStatus::Open),
                            ..service::Filter::default()
                        },
                        before: None,
                        limit: 10,
                    })),
                )? {
                    Ok(AnyReply::Service(service::Reply::ListRated(p))) => Ok(p),
                    other => Err(format!("list: {other:?}")),
                }
            };
            let page = list(&mut c)?;
            let first = page.problems.first().ok_or("no open problem")?.clone();
            let number = first.summary.number;
            let noted = match ask(
                &mut c,
                &AnyRequest::Service(service::Request::Note(service::Note {
                    number,
                    text: "Looked at from the console".into(),
                })),
            )? {
                Ok(AnyReply::Service(service::Reply::Note(l))) => l,
                other => return Err(format!("note: {other:?}")),
            };
            let got = match ask(
                &mut c,
                &AnyRequest::Service(service::Request::GetRated(service::Get { number })),
            )? {
                Ok(AnyReply::Service(service::Reply::GetRated(p))) => p,
                other => return Err(format!("get: {other:?}")),
            };
            if !got.problem.log.iter().any(|l| l.text == noted.text) {
                return Err(format!("the note is not in the log: {got:?}"));
            }
            let closed = match ask(
                &mut c,
                &AnyRequest::Service(service::Request::Close(service::Close {
                    number,
                    anyway: false,
                })),
            )? {
                Ok(AnyReply::Service(service::Reply::Close(s))) => s,
                other => return Err(format!("close: {other:?}")),
            };
            if closed.status != service::ProblemStatus::Closed || closed.closed_by != "QSMCOPER" {
                return Err(format!("closed: {closed:?}"));
            }
            let again = refused(
                &mut c,
                &AnyRequest::Service(service::Request::Close(service::Close {
                    number,
                    anyway: false,
                })),
            )?;
            if again != "QSI1003" {
                return Err(format!("closed again: {again}"));
            }
            let noted_ev = event_until(
                &mut c,
                &mut seen,
                Duration::from_secs(10),
                |e| matches!(e, AnyEvent::Service(service::Event::Noted(n)) if n.number == number && n.by == "QSMCOPER"),
            );
            let closed_ev = event_until(
                &mut c,
                &mut seen,
                Duration::from_secs(10),
                |e| matches!(e, AnyEvent::Service(service::Event::Closed(n)) if n.problem.number == number),
            );
            if !noted_ev || !closed_ev {
                return Err(format!("no Noted or Closed event: {seen:?}"));
            }
            c.close();
            Ok(format!(
                "problem {number} opened by a dump (severity {:?}), noted by the console's user, read with its note, closed and refused QSI1003 closed again, with its events",
                first.severity
            ))
        })
    }

    /// 206: a controlled power down asked over QSI.
    #[allow(clippy::unused_self)] // a step's signature
    fn qsi_power_down(&self, s: &mut Session) -> Result<String, String> {
        with(|st| {
            let mut c = st.mc01.connect(st.port)?;
            match base(
                &mut c,
                qsi::base::Request::Power(qsi::base::Power {
                    action: qsi::base::PowerAction::Off,
                    immediate: false,
                    delay: 10,
                }),
            )? {
                qsi::base::Reply::Power(_) => Ok(()),
                other => Err(format!("power: {other:?}")),
            }
        })?;
        s.expect("SRC D1001000", Duration::from_secs(60))?;
        s.expect("SRC D1001090", Duration::from_secs(120))?;
        if !s.wait_exit(Duration::from_secs(30)) {
            return Err("the guest did not power off".into());
        }
        Ok(
            "PWRDWNSYS controlled with a delay of 10 asked over QSI; D1001000 to D1001090; off"
                .into(),
        )
    }
}

/// Sign on to the service tools on the console: the shipped `QSECOFR`
/// changed to the harness's password the first time.
fn dst_in(s: &mut Session) -> Result<(), String> {
    for password in ["QSECOFR", DST_PASSWORD] {
        s.command("type QSECOFR", CMD)?;
        s.command("key tab", CMD)?;
        s.command(&format!("type {password}"), CMD)?;
        s.command("key enter", CMD)?;
        let Ok((i, _)) = screen_any(
            s,
            &[
                "Change Service Tools User Password",
                "Use Dedicated Service Tools (DST)",
            ],
        ) else {
            screen_wait(s, "Dedicated Service Tools (DST) Sign On")?;
            continue;
        };
        if i == 1 {
            return Ok(());
        }
        s.command("key tab", CMD)?;
        s.command(&format!("type {password}"), CMD)?;
        s.command("key tab", CMD)?;
        s.command(&format!("type {DST_PASSWORD}"), CMD)?;
        s.command("key tab", CMD)?;
        s.command(&format!("type {DST_PASSWORD}"), CMD)?;
        s.command("key enter", CMD)?;
        screen_wait(s, "Use Dedicated Service Tools (DST)")?;
        return Ok(());
    }
    Err("DST refused QSECOFR's passwords".into())
}
