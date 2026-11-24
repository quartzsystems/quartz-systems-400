// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a console asks of the system, answered from the storage
//! service's state: what it says of itself, its clock and zone, its fix
//! level, power down (as `PWRDWNSYS` does it), the problem log as the
//! service family's problems, and the system dumps.

use std::sync::OnceLock;

use prb::{Origin, PrbStatus, Problem};
use qsi::PeerKind;
use qsi::base::{self, DumpKind, Identity, Level, Space, SystemDump, Time};
use qsi::service::{
    self, Line, ProblemStatus, Rated, RatedPage, RatedProblem, Resource, ResourceKind, Severity,
    Source, SourceKind, Summary,
};
use qsisvr::pump::note_text;
use stg::qsi::{QsiProblem, QsiReply};

use super::super::served::JobCtx;
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::security::sysval_text;
use crate::stgsvc::{dumps, prblog};

fn failed(e: &str) -> QsiReply {
    QsiReply::refused("CPF9898", e)
}

/// `identify`.
pub fn identify(system: &mut System) -> QsiReply {
    QsiReply::Identify {
        identity: Identity {
            kind: PeerKind::Standalone,
            name: system.system_name(),
            release: release::RELEASE.to_string(),
            serial: sysval_text(system, "QSRLNBR").trim().to_string(),
            model: sysval_text(system, "QMODEL").trim().to_string(),
            build: release::build_id().to_string(),
        },
    }
}

/// `QUTCOFFSET` (`+hhmm`) in seconds.
fn offset(text: &str) -> i32 {
    let t = text.trim();
    let (sign, digits) = match t.strip_prefix('-') {
        Some(d) => (-1, d),
        None => (1, t.trim_start_matches('+')),
    };
    let h: i32 = digits.get(..2).and_then(|d| d.parse().ok()).unwrap_or(0);
    let m: i32 = digits.get(2..4).and_then(|d| d.parse().ok()).unwrap_or(0);
    sign * (h * 3600 + m * 60)
}

/// The clock, the offset and the zone.
pub fn time(system: &mut System) -> QsiReply {
    let ns = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX));
    QsiReply::Time {
        time: Time {
            unix_ns: ns,
            offset_s: offset(&sysval_text(system, "QUTCOFFSET")),
            zone: sysval_text(system, "QTIMZON").trim().to_string(),
        },
    }
}

/// The cumulative fix level, read the first time it is asked for after
/// the IPL: a change by an immediate PTF shows at the next IPL, as the
/// partition service channel's does.
pub fn fix_level(system: &mut System) -> QsiReply {
    static LEVEL: OnceLock<String> = OnceLock::new();
    let level = LEVEL
        .get_or_init(|| crate::cpp::ptfcmd::fix_level(system))
        .clone();
    QsiReply::Level {
        level: Level { level, date: 0 },
    }
}

/// What the system has and uses.
pub fn usage(system: &mut System) -> QsiReply {
    QsiReply::Resources {
        resources: crate::stgsvc::stats::qsi_resources(system),
    }
}

/// Power down, as `PWRDWNSYS` from a job: the job's profile needs
/// `*JOBCTL`; a power down under way is refused.
pub fn power(
    system: &mut System,
    ctx: &JobCtx,
    immediate: bool,
    delay: u32,
    restart: bool,
    user: &str,
) -> QsiReply {
    let me = ctx.session.borrow().principal.clone();
    if let Err(e) = me.require(sec::profile::SpecialAuth::JOBCTL, "power down the system") {
        return QsiReply::refused("CPF222E", &e.to_string());
    }
    if std::path::Path::new(crate::ipl::power::REQUEST_FILE).exists() {
        return QsiReply::refused("CPF9898", "A power down of the system has already started.");
    }
    system.current_job = ctx.job.number;
    eprintln!(
        "qsstg: QSI: PWRDWNSYS OPTION({}) DELAY({delay}) RESTART({}) for console user {user:?}",
        if immediate { "*IMMED" } else { "*CNTRLD" },
        if restart { "*YES" } else { "*NO" }
    );
    if let Err(e) = std::fs::write(
        crate::ipl::power::REQUEST_FILE,
        format!("{} {delay} {} 0\n", u8::from(immediate), u8::from(restart)),
    ) {
        return failed(&e.to_string());
    }
    match rustix::process::kill_process(rustix::process::Pid::INIT, rustix::process::Signal::USR1) {
        Ok(()) => QsiReply::Done,
        Err(e) => failed(&e.to_string()),
    }
}

// ---- Problems ----

fn number(p: &Problem) -> u64 {
    p.id.parse().unwrap_or(0)
}

fn severity(p: &Problem) -> Severity {
    match p.severity {
        1 => Severity::Terminal,
        2 => Severity::Attention,
        _ => Severity::Information,
    }
}

/// Who closed a problem from a console: the user of its last close note.
fn closed_by(p: &Problem) -> String {
    p.notes
        .iter()
        .rev()
        .find_map(|n| {
            n.strip_suffix(CLOSED)
                .and_then(|t| t.strip_prefix(qsisvr::pump::NOTE_PREFIX))
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// The end of a note that says a console closed the problem.
const CLOSED: &str = ": closed from a management console";

fn summary(p: &Problem, system_name: &str) -> Summary {
    let closed = p.status == PrbStatus::Closed;
    let (code, msgid) = match &p.origin {
        Origin::Hardware { src, .. } => (src.clone(), String::new()),
        Origin::Message(m) => (m.clone(), m.clone()),
        Origin::Dump(_) | Origin::Software(_) | Origin::Analysis => (String::new(), String::new()),
    };
    Summary {
        number: number(p),
        status: if closed {
            ProblemStatus::Closed
        } else {
            ProblemStatus::Open
        },
        first_seen: p.opened,
        last_seen: p.changed.max(p.opened),
        count: 1,
        refcode: base::Refcode {
            seq: 0,
            code,
            words: Vec::new(),
            text: p.text.clone(),
            time_ms: p.opened.saturating_mul(1000),
        },
        source: Source {
            kind: SourceKind::System,
            id: 0,
            name: system_name.into(),
        },
        description: p.text.chars().take(256).collect(),
        msgid,
        holds: false,
        closed_by: if closed { closed_by(p) } else { String::new() },
        closed_at: if closed { p.changed } else { 0 },
    }
}

fn notes(p: &Problem) -> Vec<Line> {
    p.notes
        .iter()
        .map(|n| Line {
            time: p.changed,
            text: n.clone(),
        })
        .collect()
}

fn resources(p: &Problem) -> Vec<Resource> {
    match &p.origin {
        Origin::Hardware { resource, .. } if !resource.is_empty() => {
            let kind = if resource.starts_with("DD") || resource.starts_with("DMP") {
                ResourceKind::DiskUnit
            } else if resource.starts_with("CMN") || resource.starts_with("ETH") {
                ResourceKind::NetworkInterface
            } else {
                ResourceKind::System
            };
            vec![Resource {
                kind,
                name: resource.clone(),
                location: String::new(),
                serial: String::new(),
            }]
        }
        _ => Vec::new(),
    }
}

fn problem(p: &Problem, system_name: &str) -> service::Problem {
    let mut log = vec![Line {
        time: p.opened,
        text: p.symptoms.clone(),
    }];
    log.extend(notes(p));
    service::Problem {
        summary: summary(p, system_name),
        resources: resources(p),
        log,
    }
}

fn no_problem(n: u64) -> QsiReply {
    QsiReply::Refused {
        notes: vec![
            (
                service::msgid::NO_PROBLEM.into(),
                format!("No problem {n:010} is in the problem log."),
            ),
            (
                "CPF3925".into(),
                format!("Problem {n:010} not found in the problem log."),
            ),
        ],
    }
}

fn body(r: &service::Reply) -> QsiReply {
    QsiReply::Body {
        body: qsi::Message::encode(r),
    }
}

/// A request of the service family.
pub fn service(system: &mut System, op: u16, body_bytes: &[u8], user: &str) -> QsiReply {
    use service::{Reply as R, Request as Q};
    let Ok(Some(req)) = <Q as qsi::Message>::decode(op, body_bytes) else {
        return QsiReply::refused(qsi::msgid::BAD, "The request does not read.");
    };
    let name = system.system_name();
    let mut log = match prblog::load(system) {
        Ok(l) => l,
        Err(e) => return failed(&e),
    };
    match &req {
        Q::List(l) | Q::ListRated(l) => {
            let page = list(&log, l, &name);
            if matches!(req, Q::List(_)) {
                body(&R::List(service::Page {
                    problems: page.problems.into_iter().map(|r| r.summary).collect(),
                    more: page.more,
                }))
            } else {
                body(&R::ListRated(page))
            }
        }
        Q::Get(g) | Q::GetRated(g) => {
            let Some(p) = log.get(&format!("{:010}", g.number)) else {
                return no_problem(g.number);
            };
            let pr = problem(p, &name);
            if matches!(req, Q::Get(_)) {
                body(&R::Get(pr))
            } else {
                body(&R::GetRated(RatedProblem {
                    severity: severity(p),
                    problem: pr,
                }))
            }
        }
        Q::Close(c) => {
            let id = format!("{:010}", c.number);
            let Some(p) = log.get_mut(&id) else {
                return no_problem(c.number);
            };
            if p.status == PrbStatus::Closed {
                return QsiReply::Refused {
                    notes: vec![(
                        service::msgid::CLOSED.into(),
                        format!("Problem {id} is closed already."),
                    )],
                };
            }
            p.status = PrbStatus::Closed;
            p.changed = now_unix();
            p.notes
                .push(format!("{}{user}{CLOSED}", qsisvr::pump::NOTE_PREFIX));
            let s = summary(p, &name);
            if let Err(e) = prblog::store(system, &log) {
                return failed(&e);
            }
            body(&R::Close(s))
        }
        Q::Note(n) => {
            let id = format!("{:010}", n.number);
            let Some(p) = log.get_mut(&id) else {
                return no_problem(n.number);
            };
            if p.status == PrbStatus::Closed {
                return QsiReply::Refused {
                    notes: vec![(
                        service::msgid::CLOSED.into(),
                        format!("Problem {id} is closed: it takes no note."),
                    )],
                };
            }
            let text = note_text(user, &n.text);
            p.notes.push(text.clone());
            p.changed = now_unix();
            let line = Line {
                time: p.changed,
                text,
            };
            if let Err(e) = prblog::store(system, &log) {
                return failed(&e);
            }
            body(&R::Note(line))
        }
    }
}

fn list(log: &prb::ProblemLog, l: &service::List, system_name: &str) -> RatedPage {
    let all = log.list(&prb::Filter::default());
    let mut picked: Vec<&Problem> = all
        .into_iter()
        .filter(|p| {
            let closed = p.status == PrbStatus::Closed;
            let status_ok = match l.filter.status {
                Some(ProblemStatus::Open) => !closed,
                Some(ProblemStatus::Closed) => closed,
                Some(_) => false,
                None => true,
            };
            let source_ok = l
                .filter
                .source
                .as_ref()
                .is_none_or(|s| s.kind == SourceKind::System && s.id == 0);
            let since_ok = l.filter.since.is_none_or(|s| p.changed.max(p.opened) >= s);
            let before_ok = l.before.is_none_or(|b| number(p) < b);
            status_ok && source_ok && since_ok && before_ok
        })
        .collect();
    picked.sort_by_key(|p| std::cmp::Reverse(number(p)));
    let limit = usize::try_from(l.limit.clamp(1, 256)).unwrap_or(1);
    let more = (picked.len() > limit).then(|| number(picked[limit - 1]));
    picked.truncate(limit);
    RatedPage {
        problems: picked
            .into_iter()
            .map(|p| Rated {
                severity: severity(p),
                summary: summary(p, system_name),
            })
            .collect(),
        more,
    }
}

/// Every problem with its notes, for the events.
pub fn problems(system: &mut System) -> QsiReply {
    let name = system.system_name();
    match prblog::load(system) {
        Ok(log) => QsiReply::Problems {
            list: log
                .list(&prb::Filter::default())
                .into_iter()
                .map(|p| QsiProblem {
                    summary: summary(p, &name),
                    notes: notes(p),
                })
                .collect(),
        },
        Err(e) => failed(&e),
    }
}

// ---- Dumps ----

fn system_dumps(system: &mut System) -> Result<Vec<SystemDump>, String> {
    Ok(dumps::list(system)?
        .into_iter()
        .map(|d| SystemDump {
            name: d.name,
            kind: DumpKind::ServiceData,
            kind_name: qsisvr::families::DUMP_KIND.into(),
            bytes: d.size,
            time: d.taken,
            by: d.text,
        })
        .collect())
}

/// The system dumps, oldest first.
pub fn dumps(system: &mut System) -> QsiReply {
    match system_dumps(system) {
        Ok(list) => QsiReply::Dumps { list },
        Err(e) => failed(&e),
    }
}

fn no_dump(name: &str) -> QsiReply {
    QsiReply::Refused {
        notes: vec![
            (
                base::msgid::NO_DUMP.into(),
                format!("No system dump {name} is on the system."),
            ),
            ("CPF39D2".into(), format!("System dump {name} not found.")),
        ],
    }
}

/// Take a system dump.
pub fn take_dump(system: &mut System, why: &str, user: &str) -> QsiReply {
    let why = format!("{why} ({user})");
    match dumps::take(system, why.trim()) {
        Ok(name) => QsiReply::Name { name },
        Err(e) => failed(&e),
    }
}

/// Delete a system dump.
pub fn delete_dump(system: &mut System, name: &str, _user: &str) -> QsiReply {
    match dumps::delete(system, name) {
        Ok(()) => QsiReply::Done,
        Err(e) if e.starts_with("CPF39D2") => no_dump(name),
        Err(e) => failed(&e),
    }
}

/// What the dumps take.
pub fn dump_space(system: &mut System) -> QsiReply {
    match system_dumps(system) {
        Ok(list) => QsiReply::Space {
            space: Space {
                bytes: list.iter().map(|d| d.bytes).sum(),
                limit: 0,
                dumps: u32::try_from(list.len()).unwrap_or(u32::MAX),
                max_dumps: 0,
            },
        },
        Err(e) => failed(&e),
    }
}

/// A piece of a dump's text.
pub fn dump_read(system: &mut System, name: &str, offset: u64, len: u32) -> QsiReply {
    let name = name.trim().to_ascii_uppercase();
    if !name.starts_with("QSD") {
        return no_dump(&name);
    }
    match prblog::load_text(system, "QSYS", &name) {
        Ok(Some(text)) => {
            let b = text.as_bytes();
            let at = usize::try_from(offset).unwrap_or(usize::MAX).min(b.len());
            let end = at
                .saturating_add(usize::try_from(len.min(64 * 1024)).unwrap_or(0))
                .min(b.len());
            QsiReply::Bytes {
                bytes: b[at..end].to_vec(),
                total: b.len() as u64,
            }
        }
        Ok(None) => no_dump(&name),
        Err(e) => failed(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_read() {
        assert_eq!(offset("+0000"), 0);
        assert_eq!(offset("-0500"), -18_000);
        assert_eq!(offset("+0530"), 19_800);
    }
}
