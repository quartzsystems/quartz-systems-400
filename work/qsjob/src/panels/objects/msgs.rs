// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The message displays: Additional Message Information (`QUIDSPMSG`,
//! for `DSPMSG` in `dspmsg`), `DSPMSGD`, `WRKRPYLE`, `DSPLOG`.

use cmd::Resolved;
use mi::Job;
use msgf::MsgDesc;
use stg::Client;
use wm::obj::msgq::{MsgKind, QMsg, Status};

use super::{act, at_cursor, common, entry, fill, parm, qual, show, wrap};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};
use crate::panels::dialog;

/// A message type as the displays name it.
fn kind_text(k: MsgKind) -> &'static str {
    match k {
        MsgKind::Info => "Information",
        MsgKind::Inq => "Inquiry",
        MsgKind::Rpy => "Reply",
        MsgKind::Comp => "Completion",
        MsgKind::Diag => "Diagnostic",
        MsgKind::Notify => "Notify",
        MsgKind::Escape => "Escape",
    }
}

/// The second-level text of a message: its own, else its
/// description's with the data filled in.
fn help_of(host: &Host, m: &QMsg) -> String {
    if !m.help.trim().is_empty() || m.msgid.is_empty() {
        return m.help.clone();
    }
    host.link
        .borrow_mut()
        .with(|l| l.describe(m.msgid.clone(), m.msgf.0.clone(), m.msgf.1.clone()))
        .map(|desc| desc.format_help(&m.data))
        .unwrap_or_default()
}

/// A label and its text wrapped under the value column.
fn labelled(label: &str, text: &str, lines: &mut Vec<String>) {
    for (i, row) in wrap(text, 58).into_iter().enumerate() {
        if i == 0 {
            lines.push(format!("{label:<15} :   {row}"));
        } else {
            lines.push(format!("{:20}{row}", ""));
        }
    }
}

/// The lines of Additional Message Information: the message, its
/// help and, with `details`, where it came from.
pub fn info_lines(host: &Host, m: &QMsg, details: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if details {
        labelled(
            "Message ID  . .",
            if m.msgid.is_empty() {
                "*NONE"
            } else {
                &m.msgid
            },
            &mut lines,
        );
        if !m.msgid.is_empty() {
            labelled(
                "Message file  .",
                &format!("{}/{}", m.msgf.1, m.msgf.0),
                &mut lines,
            );
        }
        labelled(
            "From job  . . .",
            &format!("{:06}/{}/{}", m.job.2, m.job.1, m.job.0),
            &mut lines,
        );
        if !m.from_pgm.is_empty() {
            labelled("From program  .", &m.from_pgm, &mut lines);
        }
        lines.push(String::new());
    }
    labelled("Message . . . .", &m.text, &mut lines);
    let help = help_of(host, m);
    if !help.trim().is_empty() {
        lines.push(String::new());
        labelled("Cause . . . . .", &help, &mut lines);
    }
    lines
}

/// Additional Message Information for `m`, a reply taken for an
/// inquiry waiting for one; false when F3 asked to leave everything.
pub(super) fn info(
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    queue: &str,
    m: &QMsg,
) -> Result<bool, Fail> {
    let (date, time) = wm::joblog::stamp_of(m.time);
    d.set("FROMUSR", &m.job.1);
    d.set("MSGSEV", &format!("{:02}", m.severity));
    d.set("MSGTYPE", kind_text(m.kind));
    d.set("SNTDATE", &date);
    d.set("SNTTIME", &time[..8]);
    let inquiry = m.kind == MsgKind::Inq;
    let mut waiting = inquiry && m.status == Status::Unanswered;
    let mut answered = inquiry && m.status == Status::Answered;
    d.set("REPLY", if answered { &m.reply } else { "" });
    let mut details = false;
    d.list_mut("INFO").top = 0;
    loop {
        let panel = if waiting {
            "ADDMSGR"
        } else if answered {
            "ADDMSGA"
        } else {
            "ADDMSGI"
        };
        let lines = info_lines(host, m, details);
        let top = d.list_mut("INFO").top;
        fill(
            d,
            "INFO",
            lines.into_iter().map(|l| entry(&[("LINE", l)])).collect(),
        );
        d.list_mut("INFO").top = top;
        match show(d, surface, panel)? {
            Response::Exit | Response::Gone => return Ok(false),
            Response::Cancel => return Ok(true),
            Response::Return(9) => details = !details,
            Response::Return(21) => {
                d.message = Some((
                    "Choose the assistance level from the message list (F21 there).".into(),
                    false,
                ));
            }
            Response::Print => {
                d.message = Some(("Printing is not available in this release.".into(), true));
            }
            Response::Return(_) => {
                let reply = d.get("REPLY").trim().to_string();
                if !waiting {
                    return Ok(true);
                }
                if reply.is_empty() {
                    d.message = Some((
                        "Type a reply, or press F12 to leave it waiting.".into(),
                        true,
                    ));
                    continue;
                }
                match host
                    .link
                    .borrow_mut()
                    .with(|l| l.msgq_reply(queue.into(), m.key, reply.clone()))
                {
                    Ok(_) => {
                        // The message stays up, answered.
                        d.message = Some(("The reply was sent.".into(), false));
                        waiting = false;
                        answered = true;
                        d.set("REPLY", &reply);
                    }
                    Err(e) => d.message = Some((crate::panels::line(&e), true)),
                }
            }
            _ => {}
        }
    }
}

/// The identifiers `RANGE` names: `*ALL`, one, or a low and a high.
fn range_of(r: &Resolved) -> (String, Option<String>) {
    let up = |v: &cmd::Value| v.source().trim().to_ascii_uppercase();
    let parts: Vec<String> = match r.get("RANGE").and_then(|b| b.value.as_ref()) {
        Some(cmd::Value::Elems(e)) => e.iter().flatten().map(up).collect(),
        Some(cmd::Value::List(e)) => e.iter().map(up).collect(),
        Some(v) => v
            .source()
            .split_whitespace()
            .map(str::to_ascii_uppercase)
            .collect(),
        None => Vec::new(),
    };
    let lo = parts.first().cloned().unwrap_or_else(|| "*ALL".into());
    let hi = parts.get(1).filter(|h| h.as_str() != "*ONLY").cloned();
    (lo, hi)
}

/// `DSPMSGD`: the descriptions of a message file, or one of them.
pub fn dspmsgd(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let (msgf, lib) = qual(r, "MSGF", "*LIBL");
    let msgf = if msgf.is_empty() {
        "QCPFMSG".to_string()
    } else {
        msgf
    };
    let lib = host
        .link
        .borrow_mut()
        .with(|l| l.object(lib.clone(), msgf.clone(), "*MSGF".into()))
        .map_or(lib, |o| o.lib);
    let (lo, hi) = range_of(r);
    let mut d = dialog(host, "QUIDSPMSGD")?;
    d.copyright = true;
    d.set("MSGF", &msgf);
    d.set("MSGFLIB", &lib);
    if lo != "*ALL" && lo != "*FIRST" && hi.is_none() {
        select(host, surface, &mut d, &lo)?;
        return Ok(String::new());
    }
    let all = host
        .link
        .borrow_mut()
        .with(|l| l.msgf_list(msgf.clone(), lib.clone()))
        .map_err(|e| Fail::new("CPF2407", e))?;
    let ids: Vec<(String, String)> = all
        .into_iter()
        .filter(|(id, _)| match (lo.as_str(), &hi) {
            ("*ALL" | "*FIRST", _) => true,
            (l, Some(h)) => id.as_str() >= l && (h == "*LAST" || id.as_str() <= h.as_str()),
            (l, None) => id == l,
        })
        .collect();
    let rows = ids
        .iter()
        .map(|(id, t)| entry(&[("ID", id.clone()), ("IDTEXT", t.clone())]))
        .collect();
    fill(&mut d, "IDS", rows);
    loop {
        match show(&mut d, surface, "LIST")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(choices) => {
                for c in &choices {
                    if let Some((id, _)) = ids.get(c.row)
                        && !select(host, surface, &mut d, id)?
                    {
                        return Ok(String::new());
                    }
                }
                for c in &choices {
                    d.list_mut(&c.list).options.clear();
                }
            }
            _ => {}
        }
    }
}

/// The lines of a description's detail panel; the attributes panel
/// takes its values as variables instead.
fn detail_lines(d: &mut UimDialog, desc: &MsgDesc, panel: &str) -> Vec<String> {
    match panel {
        "TEXT" => {
            let mut lines = Vec::new();
            labelled("Message . . . .", &desc.text, &mut lines);
            if !desc.help.trim().is_empty() {
                lines.push(String::new());
                labelled("Cause . . . . .", &desc.help, &mut lines);
            }
            lines
        }
        "FIELDS" if desc.vars.is_empty() => vec!["(No fields)".to_string()],
        "FIELDS" => desc
            .vars
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let decl = v.declaration();
                let (ty, len) = decl.split_once(' ').unwrap_or((&decl, ""));
                format!("&{:<9}{ty:<19}{len}", i + 1)
            })
            .collect(),
        _ => {
            d.set("SEV", &desc.severity.to_string());
            d.set("LEVEL", "");
            let dump = desc
                .dump
                .iter()
                .map(|n| format!("&{n}"))
                .collect::<Vec<_>>()
                .join(" ");
            d.set("DMPLST", if dump.is_empty() { "*NONE" } else { &dump });
            let replies = !desc.valid_replies.is_empty() || desc.dft_reply.is_some();
            d.set("RPYTYPE", if replies { "*CHAR" } else { "*NONE" });
            let values = desc.valid_replies.join(" ");
            d.set("VALUES", if values.is_empty() { "*NONE" } else { &values });
            d.set("DFTRPY", desc.dft_reply.as_deref().unwrap_or("*NONE"));
            Vec::new()
        }
    }
}

/// Select Message Details to Display for one identifier; false when
/// F3 asked to leave everything.
fn select(host: &Host, surface: &mut Surface, d: &mut UimDialog, id: &str) -> Result<bool, Fail> {
    let (msgf, lib) = (d.get("MSGF"), d.get("MSGFLIB"));
    let desc = host
        .link
        .borrow_mut()
        .with(|l| l.describe(id.into(), msgf.clone(), lib.clone()))
        .ok_or_else(|| {
            Fail::new(
                "CPF2419",
                format!("Message identifier {id} not found in message file {lib}/{msgf}."),
            )
        })?;
    let text: Vec<char> = desc.text.chars().collect();
    let first: String = text.iter().take(45).collect();
    let rest: String = text.iter().skip(45).take(77).collect();
    d.set("MSGID", id);
    d.set("TEXT1", &first);
    d.set("TEXT2", &rest);
    loop {
        let shown: &[&str] = match show(d, surface, "SELECT")? {
            Response::Exit | Response::Gone => return Ok(false),
            Response::Cancel => return Ok(true),
            Response::Return(1) => &["TEXT"],
            Response::Return(2) => &["FIELDS"],
            Response::Return(5) => &["ATTRS"],
            Response::Return(30) => &["TEXT", "FIELDS", "ATTRS"],
            _ => continue,
        };
        for panel in shown {
            let lines = detail_lines(d, &desc, panel);
            fill(
                d,
                "LINES",
                lines.into_iter().map(|l| entry(&[("LINE", l)])).collect(),
            );
            d.page = 0;
            match show(d, surface, panel)? {
                Response::Exit | Response::Gone => return Ok(false),
                Response::Cancel => break,
                _ => {}
            }
        }
    }
}

/// `WRKRPYLE`: the system reply list's entries with their options.
pub fn wrkrpyle(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QUIWRKRPYL")?;
    d.copyright = true;
    let mut last = String::new();
    loop {
        let entries = host
            .link
            .borrow_mut()
            .with(Client::rpyl_entries)
            .map_err(|e| Fail::new("CPF2555", e))?;
        let rows = entries
            .into_iter()
            .map(|(seq, msgid, cmp, start, reply, dump)| {
                let (cmp, start) = if cmp.is_empty() || cmp == "*NONE" {
                    ("*NONE".to_string(), String::new())
                } else {
                    (format!("'{cmp}'"), start.to_string())
                };
                let short: String = reply.chars().take(10).collect();
                entry(&[
                    ("SEQ", seq.to_string()),
                    ("MSGID", msgid),
                    ("RPY", short),
                    ("CMPDTA", cmp),
                    ("START", start),
                    ("RPYLONG", reply),
                    ("DUMP", if dump { "*YES" } else { "*NO" }.to_string()),
                ])
            })
            .collect();
        fill(&mut d, "RPYS", rows);
        let resp = show(&mut d, surface, "WRKRPYLE")?;
        let Some(resp) = common(job, host, &mut d, resp, &mut last) else {
            continue;
        };
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(c) => act(job, host, &mut d, &c, &|_, _| None),
            Response::Return(11) => d.view = 1 - d.view.min(1),
            _ => {}
        }
    }
}

/// The row of `DSPLOG`'s first line, and the lines a page.
const LOG_ROW: usize = 3;
const LOG_ROWS: usize = 17;

/// A history record: date, time, identifier, severity, type, job name,
/// user, number, text.
type Hist = Vec<String>;

/// The history records `JOB` and `MSGID` select.
fn history(host: &Host, r: &Resolved) -> Result<Vec<Hist>, Fail> {
    let job = parm(r, "JOB").filter(|j| j != "*NONE");
    let up = |v: &cmd::Value| v.source().trim().to_ascii_uppercase();
    let ids: Vec<String> = match r.get("MSGID").and_then(|b| b.value.as_ref()) {
        Some(cmd::Value::List(items)) => items.iter().map(up).collect(),
        Some(v) => vec![up(v)],
        None => Vec::new(),
    };
    let ids: Vec<String> = ids
        .into_iter()
        .filter(|i| !i.is_empty() && i != "*ALL" && i != "*NONE")
        .collect();
    let all = host
        .link
        .borrow_mut()
        .with(Client::hst_entries)
        .map_err(|e| Fail::new("CPF2447", e))?;
    Ok(all
        .into_iter()
        .filter(|e| e.len() >= 9)
        .filter(|e| {
            job.as_ref().is_none_or(|j| {
                let full = format!("{}/{}/{}", e[7], e[6], e[5]);
                *j == full || *j == e[5]
            })
        })
        .filter(|e| {
            ids.is_empty()
                || ids.iter().any(|i| {
                    e[2] == *i || (i.len() == 7 && i.ends_with("00") && e[2].starts_with(&i[..5]))
                })
        })
        .collect())
}

/// `CYYMMDD` and `HHMMSS` as `MM/DD/YY` and `HH:MM:SS`.
fn when(date: &str, time: &str) -> (String, String) {
    let g = |s: &str, a: usize, b: usize| s.get(a..b).unwrap_or("").to_string();
    (
        format!("{}/{}/{}", g(date, 3, 5), g(date, 5, 7), g(date, 1, 3)),
        format!("{}:{}:{}", g(time, 0, 2), g(time, 2, 4), g(time, 4, 6)),
    )
}

/// `DSPLOG`: the history log, oldest first; F10 adds when and who, F1
/// on a message shows it whole.
pub fn dsplog(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let entries = history(host, r)?;
    let mut d = dialog(host, "QUIDSPLOG")?;
    let mut details = false;
    let mut first = true;
    loop {
        let mut owner = Vec::new();
        let mut lines = Vec::new();
        for (i, e) in entries.iter().enumerate() {
            lines.push(e[8].clone());
            owner.push(i);
            if details {
                let (date, time) = when(&e[0], &e[1]);
                lines.push(format!(
                    "     {date}  {time}  {:<7}  {}/{}/{}",
                    e[2], e[7], e[6], e[5]
                ));
                owner.push(i);
            }
        }
        if lines.is_empty() {
            lines.push("(No messages in the history log)".into());
        }
        let n = lines.len();
        fill(
            &mut d,
            "LOG",
            lines.into_iter().map(|l| entry(&[("LINE", l)])).collect(),
        );
        if first {
            // The newest messages come into view first.
            d.list_mut("LOG").top = n.saturating_sub(LOG_ROWS);
            first = false;
        }
        match show(&mut d, surface, "DSPLOG")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Return(10) => details = !details,
            Response::Return(1) => {
                let at =
                    at_cursor(&mut d, "LOG", LOG_ROW, LOG_ROWS).and_then(|i| owner.get(i).copied());
                let Some(e) = at.and_then(|i| entries.get(i)) else {
                    d.message = Some((
                        "Place the cursor on a message to see all of it.".into(),
                        true,
                    ));
                    continue;
                };
                let m = QMsg {
                    msgid: e[2].clone(),
                    severity: e[3].trim().parse().unwrap_or(0),
                    kind: kind_of(&e[4]),
                    job: (e[5].clone(), e[6].clone(), e[7].trim().parse().unwrap_or(0)),
                    msgf: ("QCPFMSG".into(), "QSYS".into()),
                    text: e[8].clone(),
                    time: micros_of(&e[0], &e[1]),
                    ..QMsg::default()
                };
                let mut info_d = dialog(host, "QUIDSPMSG")?;
                if !info(host, surface, &mut info_d, "", &m)? {
                    return Ok(String::new());
                }
            }
            _ => {}
        }
    }
}

/// A history record's type code as a message type.
fn kind_of(code: &str) -> MsgKind {
    match code.trim() {
        "05" | "*INQ" => MsgKind::Inq,
        "15" | "*ESCAPE" => MsgKind::Escape,
        "02" | "*DIAG" => MsgKind::Diag,
        "01" | "*COMP" => MsgKind::Comp,
        _ => MsgKind::Info,
    }
}

/// `CYYMMDD` and `HHMMSS` as microseconds since the epoch.
fn micros_of(date: &str, time: &str) -> u64 {
    let num = |s: Option<&str>| s.and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
    let y = 1900 + 100 * num(date.get(0..1)) + num(date.get(1..3));
    let (m, d) = (num(date.get(3..5)), num(date.get(5..7)));
    // Days from the civil date.
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs =
        days * 86_400 + num(time.get(0..2)) * 3600 + num(time.get(2..4)) * 60 + num(time.get(4..6));
    u64::try_from(secs).unwrap_or(0) * 1_000_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_times() {
        assert_eq!(micros_of("0700101", "000001"), 1_000_000);
        assert_eq!(
            when("1261023", "101112"),
            ("10/23/26".into(), "10:11:12".into())
        );
        let (date, time) = wm::joblog::stamp_of(micros_of("1261023", "101112"));
        assert_eq!((date.as_str(), &time[..8]), ("10/23/26", "10:11:12"));
    }
}
