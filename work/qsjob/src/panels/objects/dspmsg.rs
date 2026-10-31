// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPMSG` and `WRKMSG` (`QUIDSPMSG`): the queues `MSGQ` names, the
//! assistance level `ASTLVL` picks, Display Messages (intermediate:
//! oldest first, a reply field under each inquiry that waits) and Work
//! with Messages (basic: newest first, the inquiries that wait apart).

use std::cell::Cell;

use cmd::Resolved;
use stg::Client;
use wm::obj::msgq::{MsgKind, MsgQ, QMsg, Status};

use super::msgs::info;
use super::{at_cursor, entry, fill, parm, qual, show, wrap};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};
use crate::panels::dialog;

/// The row of Display Messages' first message line, and lines a page.
const MSG_ROW: usize = 8;
const MSG_ROWS: usize = 13;

/// The operator's queue.
const QSYSOPR: &str = "QSYS/QSYSOPR";

/// An assistance level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Work with Messages.
    Basic,
    /// Display Messages.
    Intermediate,
}

thread_local! {
    /// The level the job last showed messages at (`ASTLVL(*PRV)`).
    static PREVIOUS: Cell<Option<Level>> = const { Cell::new(None) };
}

/// A level value (`*BASIC`, `*INTERMED`, `*ADVANCED`) as a level; the
/// advanced level shows the intermediate display, as the system's does.
fn named(v: &str) -> Option<Level> {
    match v.trim().to_ascii_uppercase().as_str() {
        "*BASIC" => Some(Level::Basic),
        "*INTERMED" | "*ADVANCED" => Some(Level::Intermediate),
        _ => None,
    }
}

/// `ASTLVL`: a level named, else the job's previous one (`*PRV`), else
/// the profile's, `*SYSVAL` there meaning `QASTLVL`.
#[must_use]
pub fn level(parm: &str, previous: Option<Level>, profile: &str, sysval: &str) -> Level {
    let profile = || named(profile).or_else(|| named(sysval)).unwrap_or(Level::Intermediate);
    match parm.trim().to_ascii_uppercase().as_str() {
        "*USRPRF" => profile(),
        "" | "*PRV" => previous.unwrap_or_else(profile),
        other => named(other).unwrap_or_else(profile),
    }
}

/// The queue a profile's `MSGQ` names: `*USRPRF` is the profile's own
/// in `QUSRSYS`.
#[must_use]
pub fn profile_queue(user: &str, msgq: &(String, String)) -> String {
    let (name, lib) = (msgq.0.trim().to_ascii_uppercase(), msgq.1.trim().to_ascii_uppercase());
    if name.is_empty() || name == "*USRPRF" {
        return format!("QUSRSYS/{}", user.to_ascii_uppercase());
    }
    let lib = if lib.is_empty() { "*LIBL".to_string() } else { lib };
    format!("{lib}/{name}")
}

/// The queues `MSGQ(name lib)` shows, in order: `*USRPRF` the profile's
/// queue; `*WRKSTN` the display's (`QSYS/<device>`); `*WRKUSR` the
/// display's then the profile's; `*SYSOPR` `QSYSOPR`; `*ALLWS` every
/// display's (`all_ws`). `own` is the profile's queue, `device` the
/// job's display (blank for none).
#[must_use]
pub fn queues(name: &str, lib: &str, own: &str, device: &str, all_ws: &[String]) -> Vec<String> {
    let ws = (!device.is_empty()).then(|| format!("QSYS/{device}"));
    match name {
        "" | "*WRKUSR" => ws.into_iter().chain([own.to_string()]).collect(),
        "*USRPRF" => vec![own.to_string()],
        "*WRKSTN" => vec![ws.unwrap_or_else(|| own.to_string())],
        "*SYSOPR" => vec![QSYSOPR.to_string()],
        "*ALLWS" if all_ws.is_empty() => vec![own.to_string()],
        "*ALLWS" => all_ws.iter().map(|d| format!("QSYS/{d}")).collect(),
        n => {
            let lib = if lib.is_empty() { "*LIBL" } else { lib };
            vec![format!("{lib}/{n}")]
        }
    }
}

/// A queue named through the library list, with the library it is in.
fn resolved(host: &Host, queue: &str) -> String {
    let (lib, name) = wm::obj::params::qualified(queue);
    if !lib.starts_with('*') {
        return queue.to_string();
    }
    host.link
        .borrow_mut()
        .with(|l| l.object(lib.clone(), name.clone(), "*MSGQ".into()))
        .map_or_else(|| queue.to_string(), |o| format!("{}/{name}", o.lib))
}


/// The job user's profile: its `MSGQ` and `ASTLVL`.
fn profile(host: &Host) -> ((String, String), String) {
    let user = host.start.user.clone();
    let answer = host
        .link
        .borrow_mut()
        .with(|l| l.sec_api(sec::api::Query::Profile { user }));
    match answer {
        Ok(sec::api::Answer::Profile { data }) => sec::profile::UsrPrf::decode(&data)
            .map(|p| (p.msgq, p.astlvl))
            .unwrap_or_default(),
        _ => Default::default(),
    }
}

/// The displays with a message queue in `QSYS`, for `*ALLWS`.
fn workstations(host: &Host) -> Vec<String> {
    let mut l = host.link.borrow_mut();
    let devs = l.with(|c| c.list_objects("QSYS".into(), "*ALL".into(), "*DEVD".into()));
    devs.into_iter()
        .map(|o| o.name)
        .filter(|n| {
            l.with(|c| c.object("QSYS".into(), n.clone(), "*MSGQ".into()))
                .is_some()
        })
        .collect()
}

/// A queue's messages, the replies left out (an inquiry shows its own);
/// `None` when the queue cannot be read.
fn messages(host: &Host, queue: &str) -> Option<Vec<QMsg>> {
    host.link
        .borrow_mut()
        .with(|l| l.msgq_list(queue.to_string()))
        .ok()
        .map(|all| all.into_iter().filter(|m| m.kind != MsgKind::Rpy).collect())
}

/// Remove messages from a queue by key.
fn remove(host: &Host, queue: &str, keys: &[u32]) {
    for k in keys {
        let _ = host
            .link
            .borrow_mut()
            .with(|l| l.msgq_receive(queue.into(), None, Some(*k), true));
    }
}

/// Send a reply; the error's text when it was refused.
fn reply(host: &Host, queue: &str, key: u32, text: &str) -> Result<(), String> {
    host.link
        .borrow_mut()
        .with(|l| l.msgq_reply(queue.into(), key, text.to_string()))
        .map(|_| ())
}

/// The `From` line's value part: user, date and time sent.
fn from_text(m: &QMsg) -> String {
    let (date, time) = wm::joblog::stamp_of(m.time);
    format!("{:<10}     {date}   {}", m.job.1, &time[..8.min(time.len())])
}

/// One line of Display Messages: its text, and whether a reply field
/// follows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The text from column 2.
    pub text: String,
    /// The line takes a reply.
    pub field: bool,
    /// The message it belongs to.
    pub owner: usize,
}

/// The lines of Display Messages: an immediate message's sender, each
/// message's text, an inquiry's reply or its field.
#[must_use]
pub fn display_lines(msgs: &[QMsg]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut push = |text: String, field: bool, owner: usize| {
        lines.push(Line { text, field, owner });
    };
    for (i, m) in msgs.iter().enumerate() {
        if m.msgid.is_empty() {
            push(format!("  From  . . . :   {}", from_text(m)), false, i);
        }
        for row in wrap(&m.text, 76) {
            push(format!("  {row}"), false, i);
        }
        match (m.kind, m.status) {
            (MsgKind::Inq, Status::Unanswered) => push("    Reply . . .".into(), true, i),
            (MsgKind::Inq, Status::Answered) => {
                push(format!("    Reply . . :   {}", m.reply), false, i);
            }
            _ => {}
        }
    }
    lines
}

/// One row of Work with Messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrkRow {
    /// The message text or sender line, from column 8.
    pub text: String,
    /// A section heading instead.
    pub head: String,
    /// The message the row belongs to, and whether it is its first.
    pub owner: Option<(usize, bool)>,
}

/// A heading centered on the screen, as text from column 8.
fn centered(text: &str) -> String {
    let pad = (80 - text.chars().count()) / 2 - 7;
    format!("{:pad$}{text}", "")
}

/// The rows of Work with Messages: the inquiries waiting for a reply,
/// then the rest, each newest first. `needs` and `rest` head them.
#[must_use]
pub fn work_rows(msgs: &[QMsg], needs: &str, rest: &str) -> Vec<WrkRow> {
    let waiting = |m: &QMsg| m.kind == MsgKind::Inq && m.status == Status::Unanswered;
    let mut rows = Vec::new();
    for (n, (head, want)) in [(needs, true), (rest, false)].into_iter().enumerate() {
        if n > 0 {
            rows.push(WrkRow::default());
        }
        rows.push(WrkRow {
            head: centered(head),
            ..WrkRow::default()
        });
        let mut any = false;
        for (i, m) in msgs.iter().enumerate().rev() {
            if waiting(m) != want {
                continue;
            }
            any = true;
            for (j, row) in wrap(&m.text, 72).into_iter().enumerate() {
                rows.push(WrkRow {
                    text: row,
                    owner: Some((i, j == 0)),
                    ..WrkRow::default()
                });
            }
            let mut more = Vec::new();
            if m.msgid.is_empty() {
                more.push(format!("  From  . . :   {}", from_text(m)));
            }
            if m.kind == MsgKind::Inq && m.status == Status::Answered {
                more.push(format!("  Reply . . :   {}", m.reply));
            }
            for text in more {
                rows.push(WrkRow {
                    text,
                    owner: Some((i, false)),
                    ..WrkRow::default()
                });
            }
        }
        if !any {
            rows.push(WrkRow {
                text: "(No messages available)".into(),
                ..WrkRow::default()
            });
        }
    }
    rows
}

/// What a display asked for when it ended.
enum End {
    /// Leave `DSPMSG`.
    Leave,
    /// Go on to the next queue.
    Onward,
    /// Show the other assistance level.
    Switch,
}

/// `DSPMSG`, `WRKMSG`.
pub fn dspmsg(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let (name, lib) = qual(r, "MSGQ", "");
    let (msgq, astlvl) = profile(host);
    let user = host.start.user.clone();
    let own = profile_queue(&user, &msgq);
    let all_ws = if name == "*ALLWS" {
        workstations(host)
    } else {
        Vec::new()
    };
    let lib = if lib == "*LIBL" { String::new() } else { lib };
    let list: Vec<String> = queues(&name, &lib, &own, &host.start.device, &all_ws)
        .into_iter()
        .map(|q| resolved(host, &q))
        .collect();
    let sysval = host
        .link
        .borrow_mut()
        .with(|l| l.sysval("QASTLVL".into()))
        .map(|v| ccsid::from_ebcdic(&v.data))
        .unwrap_or_default();
    let asked = parm(r, "ASTLVL").unwrap_or_default();
    let mut lvl = level(&asked, PREVIOUS.with(Cell::get), &astlvl, &sysval);
    let sev: u8 = parm(r, "SEV").and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut d = dialog(host, "QUIDSPMSG")?;
    let system = d.system.clone();
    d.set("SYSNM", &system);
    // The workstation's queue shows under *WRKUSR only when it holds
    // messages; one that cannot be read is passed over.
    let wrkusr = name.is_empty() || name == "*WRKUSR";
    let shown: Vec<String> = list
        .iter()
        .enumerate()
        .filter(|(i, q)| {
            let at_end = *i + 1 == list.len();
            match messages(host, q) {
                Some(m) => at_end || !wrkusr || !m.is_empty(),
                None => at_end,
            }
        })
        .map(|(_, q)| q.clone())
        .collect();
    loop {
        PREVIOUS.with(|p| p.set(Some(lvl)));
        let next = match lvl {
            Level::Intermediate => intermediate(host, surface, &mut d, &shown, sev)?,
            Level::Basic => basic(host, surface, &mut d, &shown)?,
        };
        match next {
            End::Switch => {
                lvl = match lvl {
                    Level::Basic => Level::Intermediate,
                    Level::Intermediate => Level::Basic,
                };
            }
            End::Leave | End::Onward => return Ok(String::new()),
        }
    }
}

/// The queue's attributes onto the header.
fn header(host: &Host, d: &mut UimDialog, queue: &str) {
    let (qlib, qname) = wm::obj::params::qualified(queue);
    let attrs = host
        .link
        .borrow_mut()
        .with(|l| l.wm_object(queue.to_string(), "*MSGQ".into()))
        .ok()
        .and_then(|b| MsgQ::decode(&b).ok())
        .unwrap_or_default();
    d.set("MSGQ", &qname);
    d.set("MSGQLIB", &qlib);
    d.set("SEV", &format!("{:02}", attrs.severity));
    d.set("DLVRY", &attrs.delivery);
    d.set("PGM", if attrs.brkpgm.0.is_empty() { "*DSPMSG" } else { &attrs.brkpgm.0 });
    d.set("PGMLIB", &attrs.brkpgm.1);
}

/// The next key set of a two-set key list.
fn next_set(d: &mut UimDialog) {
    let other = d.get("KSET") == "0";
    d.set("KSET", if other { "1" } else { "0" });
}

/// Display Messages over each queue in turn.
fn intermediate(
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    queues: &[String],
    sev: u8,
) -> Result<End, Fail> {
    for queue in queues {
        match one_queue(host, surface, d, queue, sev)? {
            End::Onward => {}
            other => return Ok(other),
        }
    }
    Ok(End::Leave)
}

/// Display Messages for one queue.
fn one_queue(
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    queue: &str,
    sev: u8,
) -> Result<End, Fail> {
    let mut all = false;
    d.set("KSET", "0");
    d.list_mut("MSGS").top = 0;
    loop {
        header(host, d, queue);
        let msgs: Vec<QMsg> = messages(host, queue)
            .ok_or_else(|| Fail::new("CPF2403", format!("Message queue {queue} not found.")))?
            .into_iter()
            .filter(|m| all || m.severity >= sev)
            .collect();
        let lines = display_lines(&msgs);
        let rows = if lines.is_empty() {
            vec![entry(&[("LINE", "  (No messages available)".into())])]
        } else {
            lines
                .iter()
                .map(|l| {
                    entry(&[
                        ("LINE", l.text.clone()),
                        ("RPYOK", if l.field { "Y" } else { "" }.into()),
                        ("RPY", String::new()),
                    ])
                })
                .collect()
        };
        fill(d, "MSGS", rows);
        let under = |d: &mut UimDialog| {
            at_cursor(d, "MSGS", MSG_ROW, MSG_ROWS).and_then(|i| lines.get(i).map(|l| l.owner))
        };
        match show(d, surface, "DSPMSG")? {
            Response::Exit | Response::Gone | Response::Cancel => return Ok(End::Leave),
            Response::Return(0) => {
                // The replies typed, each to its inquiry.
                let typed: Vec<(usize, String)> = d
                    .list_mut("MSGS")
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(i, row)| {
                        let v = row.iter().find(|(k, _)| k == "RPY")?.1.trim().to_string();
                        (!v.is_empty()).then_some((i, v))
                    })
                    .collect();
                let mut failed = None;
                for (i, text) in typed {
                    let Some(m) = lines.get(i).and_then(|l| msgs.get(l.owner)) else {
                        continue;
                    };
                    if let Err(e) = reply(host, queue, m.key, &text) {
                        failed = Some(e);
                    }
                }
                match failed {
                    Some(e) => d.message = Some((e, true)),
                    None => return Ok(End::Onward),
                }
            }
            Response::Return(1) => match under(d) {
                Some(i) => {
                    if !info(host, surface, d, queue, &msgs[i])? {
                        return Ok(End::Leave);
                    }
                }
                None => {
                    d.message = Some(("Place the cursor on a message to see all of it.".into(), true));
                }
            },
            Response::Return(10) => all = true,
            Response::Return(11) => match under(d) {
                Some(i) => remove(host, queue, &[msgs[i].key]),
                None => {
                    d.message = Some(("Place the cursor on the message to remove.".into(), true));
                }
            },
            Response::Return(13) => {
                let keys: Vec<u32> = msgs.iter().map(|m| m.key).collect();
                remove(host, queue, &keys);
            }
            Response::Return(16) => {
                let keys: Vec<u32> = msgs
                    .iter()
                    .filter(|m| m.status != Status::Unanswered)
                    .map(|m| m.key)
                    .collect();
                remove(host, queue, &keys);
            }
            Response::Return(21) => return Ok(End::Switch),
            Response::Return(22) => {
                d.message = Some(("Each message shows whole: there are no more details to add.".into(), false));
            }
            Response::Return(24) => next_set(d),
            _ => {}
        }
    }
}

/// Where a Work with Messages row's message lives: its queue and the
/// message.
type Owned = (String, QMsg);

/// Work with Messages over the queues together; F6 shows `QSYSOPR`'s.
fn basic(
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    queues: &[String],
) -> Result<End, Fail> {
    let mut operator = queues.len() == 1 && queues[0] == QSYSOPR;
    let mut nested = false;
    d.set("KSET", "0");
    loop {
        let shown: Vec<String> = if operator {
            vec![QSYSOPR.to_string()]
        } else {
            queues.to_vec()
        };
        let mut owned: Vec<Owned> = Vec::new();
        for q in &shown {
            for m in messages(host, q).unwrap_or_default() {
                owned.push((q.clone(), m));
            }
        }
        owned.sort_by_key(|(_, m)| m.time);
        let msgs: Vec<QMsg> = owned.iter().map(|(_, m)| m.clone()).collect();
        let rows = work_rows(&msgs, "Messages needing a reply", "Messages not needing a reply");
        let title = if operator {
            "System Operator".to_string()
        } else {
            let last = shown.last().cloned().unwrap_or_default();
            wm::obj::params::qualified(&last).1
        };
        d.set("MSGQ", &title);
        fill(
            d,
            "WMSGS",
            rows.iter()
                .map(|r| {
                    let first = r.owner.is_some_and(|(_, f)| f);
                    entry(&[
                        ("WLINE", r.text.clone()),
                        ("WHEAD", r.head.clone()),
                        ("FIRST", if first { "Y" } else { "" }.into()),
                    ])
                })
                .collect(),
        );
        let resp = show(d, surface, if operator { "WRKOPR" } else { "WRKMSG" })?;
        match resp {
            // F6's display goes back to the user's messages.
            Response::Exit | Response::Cancel if operator && nested => {
                operator = false;
                d.list_mut("WMSGS").top = 0;
            }
            Response::Exit | Response::Cancel | Response::Gone => return Ok(End::Leave),
            Response::Options(choices) => {
                for c in &choices {
                    let Some((i, _)) = rows.get(c.row).and_then(|r| r.owner) else {
                        continue;
                    };
                    let (queue, m) = &owned[i];
                    match c.option.as_str() {
                        "4" if !operator => remove(host, queue, &[m.key]),
                        // F3 there comes back to the list.
                        "5" => {
                            info(host, surface, d, queue, m)?;
                        }
                        o => d.message = Some((format!("Option {o} is not valid here."), true)),
                    }
                }
                for c in &choices {
                    d.list_mut(&c.list).options.clear();
                }
            }
            Response::Return(6) => {
                operator = !operator;
                nested = operator;
                d.list_mut("WMSGS").top = 0;
            }
            Response::Return(16) => {
                for (queue, m) in &owned {
                    if m.status != Status::Unanswered && !operator {
                        remove(host, queue, &[m.key]);
                    }
                }
            }
            Response::Return(21) => return Ok(End::Switch),
            Response::Return(22) => {
                d.message = Some(("Each message shows whole: there are no more details to add.".into(), false));
            }
            Response::Return(24) => next_set(d),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(n: &str, l: &str) -> (String, String) {
        (n.into(), l.into())
    }

    #[test]
    fn usrprf_is_the_profiles_queue() {
        // The shipped default: the profile's own queue in QUSRSYS.
        let own = profile_queue("ALICE", &pair("*USRPRF", ""));
        assert_eq!(own, "QUSRSYS/ALICE");
        assert_eq!(queues("*USRPRF", "", &own, "QPADEV0001", &[]), vec!["QUSRSYS/ALICE"]);
        // QSYSOPR only when the profile names it.
        let opr = profile_queue("BOB", &pair("QSYSOPR", "QSYS"));
        assert_eq!(opr, "QSYS/QSYSOPR");
        assert_eq!(queues("*USRPRF", "", &opr, "", &[]), vec!["QSYS/QSYSOPR"]);
        assert_eq!(profile_queue("CAROL", &pair("MINE", "")), "*LIBL/MINE");
    }

    #[test]
    fn special_values() {
        let own = "QUSRSYS/ALICE";
        assert_eq!(
            queues("*WRKUSR", "", own, "QPADEV0001", &[]),
            vec!["QSYS/QPADEV0001", own]
        );
        assert_eq!(queues("", "", own, "", &[]), vec![own]);
        assert_eq!(queues("*WRKSTN", "", own, "DSP01", &[]), vec!["QSYS/DSP01"]);
        assert_eq!(queues("*SYSOPR", "", own, "DSP01", &[]), vec![QSYSOPR]);
        assert_eq!(
            queues("*ALLWS", "", own, "DSP01", &["DSP01".into(), "DSP02".into()]),
            vec!["QSYS/DSP01", "QSYS/DSP02"]
        );
        assert_eq!(queues("QSYSOPR", "", own, "", &[]), vec!["*LIBL/QSYSOPR"]);
        assert_eq!(queues("Q1", "MYLIB", own, "", &[]), vec!["MYLIB/Q1"]);
    }

    #[test]
    fn levels() {
        assert_eq!(level("*BASIC", None, "*SYSVAL", "*INTERMED"), Level::Basic);
        assert_eq!(level("*PRV", Some(Level::Basic), "*INTERMED", ""), Level::Basic);
        assert_eq!(level("*PRV", None, "*BASIC", ""), Level::Basic);
        assert_eq!(level("*PRV", None, "*SYSVAL", "*BASIC"), Level::Basic);
        assert_eq!(level("*USRPRF", Some(Level::Basic), "*ADVANCED", ""), Level::Intermediate);
        assert_eq!(level("", None, "", ""), Level::Intermediate);
    }

    fn msg(text: &str, kind: MsgKind, status: Status, msgid: &str) -> QMsg {
        QMsg {
            text: text.into(),
            kind,
            status,
            msgid: msgid.into(),
            job: ("QPADEV0001".into(), "ALICE".into(), 1),
            ..QMsg::default()
        }
    }

    #[test]
    fn display_lines_take_replies() {
        let msgs = [
            msg("Job started.", MsgKind::Info, Status::Old, "CPF1124"),
            msg("Go on?", MsgKind::Inq, Status::Unanswered, ""),
        ];
        let lines = display_lines(&msgs);
        assert_eq!(lines[0].text, "  Job started.");
        assert!(lines[1].text.starts_with("  From  . . . :   ALICE          "));
        assert_eq!(lines[2].text, "  Go on?");
        assert_eq!(
            lines[3],
            Line {
                text: "    Reply . . .".into(),
                field: true,
                owner: 1
            }
        );
    }

    #[test]
    fn work_rows_split_and_newest_first() {
        let mut answered = msg("Asked.", MsgKind::Inq, Status::Answered, "");
        answered.reply = "N".into();
        let msgs = [
            msg("Old one.", MsgKind::Info, Status::Old, "CPF0001"),
            answered,
            msg("Waiting?", MsgKind::Inq, Status::Unanswered, "CPA0001"),
        ];
        let rows = work_rows(&msgs, "Needs", "Rest");
        let texts: Vec<String> = rows
            .iter()
            .map(|r| if r.head.is_empty() { r.text.clone() } else { r.head.trim().to_string() })
            .collect();
        assert_eq!(
            texts,
            vec!["Needs", "Waiting?", "", "Rest", "Asked.", &texts[5], "  Reply . . :   N", "Old one."]
        );
        assert_eq!(rows[1].owner, Some((2, true)));
        assert_eq!(rows[5].owner, Some((1, false)));
        assert_eq!(centered("Messages needing a reply").len(), 21 + 24);
    }
}
