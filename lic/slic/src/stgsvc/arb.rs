// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What system job `QSYSARB5` watches, as IBM's system arbiter does:
//! the system ASP against its threshold and `QSTGLOWLMT`, the power
//! supply (`QUPSDLYTIM`, `QUPSMSGQ`), and the batch restricted state's
//! time limit. [`tick`] runs every few seconds; what it decides that a
//! job must do (end the subsystems, power down, submit the exit program
//! jobs, start the controlling subsystem) comes back as commands
//! `QSYSARB5` runs as `QSYS`.
//!
//! - At the ASP threshold (DST and SST, Work with ASP threshold)
//!   `CPF0907` goes to `QSYSOPR` and the history log, again every hour
//!   while the ASP stays there.
//! - Below `QSTGLOWLMT` percent available, `CPI099C` goes to `QSYSMSG`
//!   and `QSYSOPR` and `QSTGLOWACN` is taken: `*MSG` nothing more,
//!   `*CRITMSG` `CPI099B` to `QSECOFR` (QS/400 has no `CHGSRVA`, so the
//!   critical message user is always `QSECOFR`), `*REGFAC` a job per
//!   `QIBM_QWC_QSTGLOWACN` program in `QSYSWRK` (`QSYSJOBD`, user
//!   `QPGMR`), `*ENDSYS` `ENDSBS *ALL *IMMED`, `*PWRDWNSYS`
//!   `PWRDWNSYS *IMMED RESTART(*YES)`; the last three not in the
//!   restricted state. Again every 30 minutes while it stays below, or
//!   at once when either system value changes.
//! - A running `STRASPBAL TYPE(*CAPACITY)` takes its next step.
//! - On battery power `CPF1816` goes to `QUPSMSGQ` (and `QSYSOPR`), and
//!   after the `QUPSDLYTIM` delay the system powers down at once; a low
//!   battery powers it down at once; `*NOMAX` waits, but powers down at
//!   once (`CPF1818`, `CPF1819`) when no one can be told. Utility power
//!   back sends `CPF1817`. The power supply's state comes from the
//!   guest agent (`guestagent::ups`). `BLOCKED(rel)`: a UPS on USB (the
//!   HID power device class) is not read; the kernel is built without
//!   `hidraw` and nothing parses the reports.

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use guestagent::ups::Power;
use wm::obj::msgq::{MsgKind, QMsg};

use crate::ipl::system::System;

/// How often `CPF0907` comes again.
const THRESHOLD_AGAIN: Duration = Duration::from_hours(1);

/// How often the lower limit's action is taken again.
const LIMIT_AGAIN: Duration = Duration::from_mins(30);

/// What the watch remembers between ticks.
#[derive(Default)]
struct Watch {
    /// When `CPF0907` was last sent; `None` below the threshold.
    threshold: Option<Instant>,
    /// When the lower limit's action was last taken, and for which
    /// `QSTGLOWACN` and `QSTGLOWLMT`; `None` above the limit.
    limit: Option<(Instant, String, String)>,
    /// When utility power failed; `None` on utility power.
    battery: Option<Instant>,
    /// The power down the power supply asked for has been asked.
    powering_down: bool,
    /// The batch restricted state's end was asked of its job.
    batch_ending: bool,
    /// `CPI099D` was sent this IPL.
    ipl_told: bool,
}

static WATCH: Mutex<Option<Watch>> = Mutex::new(None);

/// One pass; the commands `QSYSARB5` is to run.
pub fn tick(system: &mut System) -> Vec<String> {
    let mut g = WATCH.lock().unwrap_or_else(PoisonError::into_inner);
    let w = g.get_or_insert_with(Watch::default);
    let mut cmds = Vec::new();
    storage(system, w, &mut cmds);
    power(system, w, Power::read(), &mut cmds);
    batch(system, w, &mut cmds);
    drop(g);
    crate::cpp::aspcmd::balance_tick(system);
    cmds
}

fn sysval(system: &System, name: &str) -> String {
    super::security::sysval_text(system, name)
}

/// The system is in the restricted state: no subsystem is active.
fn restricted(system: &System) -> bool {
    system.sbs.is_empty()
}

/// `(capacity, used, percent used)` of the system ASP.
#[allow(clippy::cast_precision_loss)] // bytes far below 2^52 of precision matters
fn usage(system: &System) -> Option<(u64, u64, f64)> {
    let (total, used) = super::stats::asp_bytes(system);
    (total > 0).then(|| (total, used, used as f64 * 100.0 / total as f64))
}

/// `QSTGLOWLMT`, percent.
fn lower_limit(system: &System) -> f64 {
    sysval(system, "QSTGLOWLMT").trim().parse().unwrap_or(5.0)
}

/// Whether the system ASP is below `QSTGLOWLMT` and `QSTGLOWACN` keeps
/// subsystems from starting (`*REGFAC`, `*ENDSYS`, `*PWRDWNSYS`).
pub fn storage_restricted(system: &System) -> bool {
    let action = sysval(system, "QSTGLOWACN");
    matches!(action.trim(), "*REGFAC" | "*ENDSYS" | "*PWRDWNSYS")
        && usage(system).is_some_and(|(_, _, pct)| 100.0 - pct < lower_limit(system))
}

/// A subsystem was kept from starting ([`storage_restricted`]):
/// `CPI099B` to the operator, and once, while nothing runs yet (the
/// IPL's controlling subsystem), `CPI099D`.
pub fn refused_start(system: &mut System) {
    let Some((total, used, pct)) = usage(system) else {
        return;
    };
    let f = figures(total, used, lower_limit(system), pct);
    let four = [
        (&f[0][..], 20),
        (&f[1][..], 20),
        (&f[2][..], 7),
        (&f[3][..], 7),
    ];
    tell_operator(system, "CPI099B", &four, false);
    let mut g = WATCH.lock().unwrap_or_else(PoisonError::into_inner);
    let w = g.get_or_insert_with(Watch::default);
    if restricted(system) && !w.ipl_told {
        w.ipl_told = true;
        tell_operator(system, "CPI099D", &four, true);
    }
}

/// The figures every storage message carries: capacity, used, the
/// limit or threshold, the percent available.
fn figures(total: u64, used: u64, limit: f64, pct_used: f64) -> [String; 4] {
    [
        total.to_string(),
        used.to_string(),
        format!("{limit:.4}"),
        format!("{:.2}", 100.0 - pct_used),
    ]
}

/// A message from `QSYSARB` to `lib/queue`, its text the message file's.
fn send(system: &mut System, lib: &str, queue: &str, msgid: &str, fields: &[(&str, usize)]) {
    let values: Vec<&str> = fields.iter().map(|(v, _)| *v).collect();
    let text = super::msgs::text(system, msgid, &values);
    let severity = super::msgs::description(system, msgid, &("QCPFMSG".into(), "QSYS".into()))
        .map_or(80, |d| u8::try_from(d.severity).unwrap_or(99));
    let m = QMsg {
        kind: MsgKind::Info,
        msgid: msgid.into(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        severity,
        text,
        data: wm::joblog::data(fields),
        job: ("QSYSARB5".into(), "QSYS".into(), 0),
        from_pgm: "QSYSARB".into(),
        ..QMsg::default()
    };
    let _ = super::msgs::send(system, lib, queue, m);
}

/// `QSYSOPR`, `QSYSMSG` when it exists, and the history log.
fn tell_operator(system: &mut System, msgid: &str, fields: &[(&str, usize)], sysmsg: bool) {
    send(system, "QSYS", "QSYSOPR", msgid, fields);
    if sysmsg {
        send(system, "QSYS", "QSYSMSG", msgid, fields);
    }
    let who = ("QSYSARB5".to_string(), "QSYS".to_string(), 0);
    super::hist::system_message(system, msgid, &wm::joblog::data(fields), "04", who);
}

fn storage(system: &mut System, w: &mut Watch, cmds: &mut Vec<String>) {
    let Some((total, used, pct)) = usage(system) else {
        return;
    };
    let threshold = system
        .storage
        .as_ref()
        .map_or(store::asp::superblock::DEFAULT_THRESHOLD, |s| {
            s.store.asp_threshold()
        });
    if pct >= f64::from(threshold) {
        if w.threshold.is_none_or(|t| t.elapsed() >= THRESHOLD_AGAIN) {
            let f = figures(total, used, f64::from(threshold), pct);
            tell_operator(
                system,
                "CPF0907",
                &[(&f[0], 20), (&f[1], 20), (&f[2], 7), (&f[3], 7)],
                false,
            );
            w.threshold = Some(Instant::now());
        }
    } else {
        w.threshold = None;
    }
    let limit = lower_limit(system);
    let limit_text = sysval(system, "QSTGLOWLMT").trim().to_string();
    let action = sysval(system, "QSTGLOWACN").trim().to_string();
    let below = 100.0 - pct < limit;
    if !below {
        w.limit = None;
        return;
    }
    let due = w
        .limit
        .as_ref()
        .is_none_or(|(t, a, l)| t.elapsed() >= LIMIT_AGAIN || *a != action || *l != limit_text);
    if !due {
        return;
    }
    w.limit = Some((Instant::now(), action.clone(), limit_text));
    let f = figures(total, used, limit, pct);
    let four = [
        (&f[0][..], 20),
        (&f[1][..], 20),
        (&f[2][..], 7),
        (&f[3][..], 7),
    ];
    let mut five = four.to_vec();
    five.push((&action, 10));
    tell_operator(system, "CPI099C", &five, true);
    match action.as_str() {
        "*CRITMSG" => send(system, "QUSRSYS", "QSECOFR", "CPI099B", &four),
        "*REGFAC" if !restricted(system) => cmds.extend(regfac_jobs(system)),
        "*ENDSYS" if !restricted(system) => cmds.push("ENDSBS SBS(*ALL) OPTION(*IMMED)".into()),
        "*PWRDWNSYS" if !restricted(system) => {
            cmds.push("PWRDWNSYS OPTION(*IMMED) RESTART(*YES)".into())
        }
        _ => {}
    }
}

/// A job per program registered for `QIBM_QWC_QSTGLOWACN`, each calling
/// its program without parameters.
fn regfac_jobs(system: &mut System) -> Vec<String> {
    let q = ::regfac::Query::Programs {
        name: "QIBM_QWC_QSTGLOWACN".into(),
        format: "STGL0100".into(),
    };
    let Ok(::regfac::Answer::Programs { programs }) = super::regfac::api(system, None, q) else {
        return Vec::new();
    };
    programs
        .iter()
        .map(|p| {
            let lib = if p.lib.is_empty() { "*LIBL" } else { &p.lib };
            format!(
                "SBMJOB CMD(CALL PGM({lib}/{})) JOB(QSTGLOWACN) JOBD(QSYS/QSYSJOBD) JOBQ(QSYS/QSYSNOMAX) USER(QPGMR)",
                p.pgm
            )
        })
        .collect()
}

/// `QUPSMSGQ` as `(lib, name)`.
fn ups_queue(system: &System) -> (String, String) {
    let v = sysval(system, "QUPSMSGQ");
    let mut w = v.split_whitespace();
    let name = w.next().unwrap_or("QSYSOPR");
    let lib = w.next().unwrap_or("");
    let lib = if lib.is_empty() || lib == "*LIBL" {
        "QSYS"
    } else {
        lib
    };
    (lib.to_string(), name.to_string())
}

/// The local time as `HH:MM:SS`.
fn clock(system: &System) -> String {
    let now = crate::ipl::system::now_unix();
    system.sysvals.as_ref().map_or_else(String::new, |v| {
        let t = v.local_time(now);
        format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second)
    })
}

/// How `QUPSMSGQ` stands for `*NOMAX`.
enum Watched {
    /// Allocated by a job, or `QSYSOPR` or a display's queue in
    /// `*BREAK` or `*NOTIFY` delivery.
    Yes,
    /// The queue cannot be found (`CPF1818`).
    Missing,
    /// No one watches it: IBM's reason code for `CPF1819`.
    No(&'static str),
}

fn monitored(system: &mut System, lib: &str, name: &str) -> Watched {
    let Ok((id, delivery)) = super::msgs::with_queue(system, "msgq.read", lib, name, |q, id| {
        Ok(((id, q.delivery.clone()), false))
    }) else {
        return Watched::Missing;
    };
    if system.break_alloc.contains_key(&id) || matches!(delivery.as_str(), "*BREAK" | "*NOTIFY") {
        return Watched::Yes;
    }
    Watched::No(if lib == "QSYS" && name == "QSYSOPR" {
        "2"
    } else {
        "1"
    })
}

fn power(system: &mut System, w: &mut Watch, p: Power, cmds: &mut Vec<String>) {
    let (lib, name) = ups_queue(system);
    let down = |w: &mut Watch, cmds: &mut Vec<String>| {
        if !w.powering_down {
            w.powering_down = true;
            cmds.push("PWRDWNSYS OPTION(*IMMED) RESTART(*NO)".into());
        }
    };
    if !p.on_battery {
        if w.battery.take().is_some() {
            let t = clock(system);
            send(system, &lib, &name, "CPF1817", &[("", 8), (&t, 8)]);
            if (lib.as_str(), name.as_str()) != ("QSYS", "QSYSOPR") {
                send(system, "QSYS", "QSYSOPR", "CPF1817", &[("", 8), (&t, 8)]);
            }
            w.powering_down = false;
        }
        return;
    }
    let delay = sysval::ups::delay(
        sysval(system, "QUPSDLYTIM")
            .split_whitespace()
            .next()
            .unwrap_or("*CALC"),
    )
    .unwrap_or(Some(sysval::ups::CALC_SECONDS));
    if w.battery.is_none() {
        w.battery = Some(Instant::now());
        let t = clock(system);
        send(system, &lib, &name, "CPF1816", &[("", 8), (&t, 8)]);
        if (lib.as_str(), name.as_str()) != ("QSYS", "QSYSOPR") {
            send(system, "QSYS", "QSYSOPR", "CPF1816", &[("", 8), (&t, 8)]);
        }
        if delay.is_none() {
            // *NOMAX: someone must be watching, or the system ends now.
            match monitored(system, &lib, &name) {
                Watched::Yes => {}
                Watched::Missing => {
                    tell_operator(system, "CPF1818", &[(&name, 10), (&lib, 10)], false);
                    down(w, cmds);
                }
                Watched::No(reason) => {
                    tell_operator(
                        system,
                        "CPF1819",
                        &[(&name, 10), (&lib, 10), (reason, 1)],
                        false,
                    );
                    down(w, cmds);
                }
            }
        }
    }
    let gone = w
        .battery
        .is_some_and(|t| delay.is_some_and(|d| t.elapsed() >= Duration::from_secs(u64::from(d))));
    if p.battery_low || gone {
        down(w, cmds);
    }
}

fn batch(system: &mut System, w: &mut Watch, cmds: &mut Vec<String>) {
    let Some((job, until)) = system.batch_rstd else {
        w.batch_ending = false;
        return;
    };
    if system.jobs.get(job).is_none() {
        // The job ended: the controlling subsystem starts again.
        system.batch_rstd = None;
        system.rstd_keep = None;
        w.batch_ending = false;
        let ctl = super::sbs::controlling(system);
        let lib = sysval(system, "QCTLSBSD")
            .split_whitespace()
            .nth(1)
            .unwrap_or("QSYS")
            .to_string();
        // QSYSWRK first, as the IPL starts it before the controlling
        // subsystem.
        if !system.sbs.contains_key("QSYSWRK") {
            cmds.push("STRSBS SBSD(QSYS/QSYSWRK)".into());
        }
        if !system.sbs.contains_key(&ctl) {
            cmds.push(format!("STRSBS SBSD({lib}/{ctl})"));
        }
        return;
    }
    if !w.batch_ending && until.is_some_and(|u| crate::ipl::system::now_unix() >= u) {
        w.batch_ending = true;
        system.end_requests.insert(job, true);
    }
}
