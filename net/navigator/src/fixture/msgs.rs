// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample message queues and history log: `QSYSOPR` with 41 messages,
//! three of them inquiries waiting for a reply, the users' queues, and
//! 214 `QHST` entries on the fixture's day. The message commands act on
//! them as the system would.

use std::collections::BTreeMap;
use std::sync::Mutex;

use qsweb::msg::Message;

use super::cl::{done, fail, parse, split, unquote};
use crate::env::msgs::{HistEntry, Msgs, QueuedMessage};
use crate::util::secs;

/// The sample system's messages.
pub struct FixtureMsgs {
    /// The queues by `lib/name`, oldest first.
    pub queues: Mutex<BTreeMap<String, Vec<QueuedMessage>>>,
    /// Commands run, as typed.
    pub ran: Mutex<Vec<String>>,
    /// The next message key.
    next: Mutex<u32>,
}

/// Seconds of 10/29/26 `h:m:s`.
fn today(h: u32, m: u32, s: u32) -> u64 {
    secs(2026, 10, 29, h, m, s)
}

/// An informational message.
fn info(sent: u64, id: &str, sev: u8, text: &str, job: &str, program: &str) -> QueuedMessage {
    QueuedMessage {
        id: id.into(),
        severity: sev,
        sent,
        text: text.into(),
        job: job.into(),
        user: job.split('/').nth(1).unwrap_or_default().into(),
        program: program.into(),
        cause: format!("The system sent this message from program {program}."),
        recovery: "None.".into(),
        ..QueuedMessage::default()
    }
}

/// An inquiry waiting for its reply.
fn inquiry(
    m: QueuedMessage,
    replies: &str,
    default: &str,
    cause: &str,
    recovery: &str,
) -> QueuedMessage {
    QueuedMessage {
        inquiry: true,
        replies: replies.into(),
        default: default.into(),
        cause: cause.into(),
        recovery: recovery.into(),
        ..m
    }
}

/// `QSYSOPR`'s ten newest messages, newest first, as the prototype
/// shows them.
fn qsysopr_newest() -> Vec<QueuedMessage> {
    vec![
        info(
            today(14, 1, 55),
            "CPF1164",
            0,
            "Job 123499/QPGMR/RGZPFM00 ended on 10/29/26 at 14:01:55; 1 seconds used; end code 0.",
            "123499/QPGMR/RGZPFM00",
            "QWTMCEOJ",
        ),
        inquiry(
            info(
                today(13, 58, 2),
                "CPA5305",
                99,
                "Record 4120 in use by job 123503/TESTUSER/INVBATCH, member ASSETS file ASSETS in HLINV. (C R)",
                "123503/TESTUSER/INVBATCH",
                "QDBSIGEX",
            ),
            "C R",
            "C",
            "Record 4120 of member ASSETS in file ASSETS in library HLINV is locked by another job and the wait time of 60 seconds expired. Job 123488/TESTUSER/QPADEV0003 holds the lock (program HLINV/INVMAIN, update pending).",
            "Reply C to cancel the request, or R to try the operation again after the record has been released. To find the job holding the lock, use the Locks page of this job or the command DSPRCDLCK FILE(HLINV/ASSETS) MBR(ASSETS) RCDNBR(4120).",
        ),
        info(
            today(13, 40, 11),
            "CPF0907",
            40,
            "Serious storage condition may exist. Press HELP.",
            "123105/QSYS/QSPLMAINT",
            "QSPLMAINT",
        ),
        inquiry(
            info(
                today(13, 22, 7),
                "CPA4002",
                99,
                "Verify alignment on printer PRT01. (I G N R C)",
                "123150/QSPLJOB/PRT01",
                "QSPWTR",
            ),
            "I G N R C",
            "I",
            "The writer for printer PRT01 stopped at the first page of a spooled file so that the forms can be aligned.",
            "Check the forms in the printer, then reply I to print, G to print and stop asking, N to print the first page again, R to start the file again, or C to end the writer.",
        ),
        info(
            today(12, 50, 0),
            "CPC1129",
            0,
            "Job 123490/OPERATOR/QPADEV0004 started.",
            "123200/QSYS/QINTER",
            "QWTSCSBS",
        ),
        info(
            today(12, 10, 33),
            "CPF2204",
            30,
            "User profile WEBUSER not found.",
            "123457/QTMHHTTP/ADMIN",
            "QZSRHTTP",
        ),
        info(
            today(11, 42, 17),
            "HTP8110",
            20,
            "Request from 172.16.0.9 for /qs400/httpadmin/... failed, user WEBUSER not authorized.",
            "123456/QTMHHTTP/ADMIN",
            "QZHBMAIN",
        ),
        info(
            today(11, 0, 0),
            "CPI2209",
            0,
            "User profile GUEST disabled because maximum number of sign-on attempts reached.",
            "123200/QSYS/QINTER",
            "QWTSCSBS",
        ),
        inquiry(
            info(
                today(10, 31, 49),
                "CPA0702",
                99,
                "CPF0001 received by procedure INVLOAD. (C D I R)",
                "123503/TESTUSER/INVBATCH",
                "INVLOAD",
            ),
            "C D I R",
            "C",
            "Procedure INVLOAD in program HLINV/INVBATCH did not handle escape message CPF0001 sent to it.",
            "Reply C to cancel the program, D to cancel it and dump the job, I to ignore the message, or R to try the failing statement again.",
        ),
        info(
            today(9, 15, 12),
            "TCP1A03",
            10,
            "Interface 10.0.10.21 on line ETHLIN02 started.",
            "123380/QTCP/QTCPIP",
            "QTOCIFC",
        ),
    ]
}

/// `QSYSOPR`'s 31 older messages of the night, newest first.
fn qsysopr_night() -> Vec<QueuedMessage> {
    (0..31u32)
        .map(|i| {
            let t = today(9, 0, 0) - u64::from(i + 1) * 900;
            let n = 123_330 - i;
            let job = format!("{n}/QPGMR/NIGHT{:03}", 31 - i);
            if i % 2 == 0 {
                info(
                    t,
                    "CPF1164",
                    0,
                    &format!(
                        "Job {job} ended on 10/29/26 at {}; 2 seconds used; end code 0.",
                        crate::util::mdy(t, 0).1
                    ),
                    &job,
                    "QWTMCEOJ",
                )
            } else {
                info(
                    t,
                    "CPF1241",
                    0,
                    &format!("Job {job} completed normally."),
                    &job,
                    "QWTMCEOJ",
                )
            }
        })
        .collect()
}

/// The history log's 19 newest entries, newest first.
const QHST_NEWEST: &[(u32, u32, u32, &str, u8, &str, &str)] = &[
    (
        14,
        1,
        55,
        "CPF1164",
        0,
        "123499/QPGMR/RGZPFM00",
        "Job 123499/QPGMR/RGZPFM00 ended on 10/29/26 at 14:01:55; 1 seconds used; end code 0.",
    ),
    (
        14,
        1,
        54,
        "CPF1124",
        0,
        "123499/QPGMR/RGZPFM00",
        "Job 123499/QPGMR/RGZPFM00 started on 10/29/26 at 14:01:54 in subsystem QBATCH in QSYS.",
    ),
    (
        13,
        58,
        2,
        "CPA5305",
        99,
        "123503/TESTUSER/INVBATCH",
        "Record 4120 in use by job 123503/TESTUSER/INVBATCH, member ASSETS file ASSETS in HLINV. (C R)",
    ),
    (
        13,
        40,
        11,
        "CPF0907",
        40,
        "123105/QSYS/QSPLMAINT",
        "Serious storage condition may exist. Press HELP.",
    ),
    (
        13,
        22,
        7,
        "CPA4002",
        99,
        "123150/QSPLJOB/PRT01",
        "Verify alignment on printer PRT01. (I G N R C)",
    ),
    (
        13,
        5,
        10,
        "CPF1241",
        0,
        "123488/TESTUSER/QPADEV0003",
        "Job 123488/TESTUSER/QPADEV0003 submitted to job queue QBATCH in QGPL.",
    ),
    (
        12,
        50,
        0,
        "CPC1129",
        0,
        "123200/QSYS/QINTER",
        "Job 123490/OPERATOR/QPADEV0004 started.",
    ),
    (
        12,
        49,
        58,
        "CPF1164",
        0,
        "123489/OPERATOR/QPADEV0004",
        "Job 123489/OPERATOR/QPADEV0004 ended on 10/29/26 at 12:49:58; 3 seconds used; end code 0.",
    ),
    (
        12,
        10,
        33,
        "CPF2204",
        30,
        "123457/QTMHHTTP/ADMIN",
        "User profile WEBUSER not found.",
    ),
    (
        11,
        42,
        17,
        "HTP8110",
        20,
        "123456/QTMHHTTP/ADMIN",
        "Request from 172.16.0.9 for /qs400/httpadmin/instance/WEBSTORE/general failed, user WEBUSER not authorized.",
    ),
    (
        11,
        0,
        0,
        "CPI2209",
        0,
        "123200/QSYS/QINTER",
        "User profile GUEST disabled because maximum number of sign-on attempts reached.",
    ),
    (
        10,
        59,
        58,
        "CPF1107",
        30,
        "123200/QSYS/QINTER",
        "Password not correct for user profile GUEST.",
    ),
    (
        10,
        59,
        51,
        "CPF1107",
        30,
        "123200/QSYS/QINTER",
        "Password not correct for user profile GUEST.",
    ),
    (
        10,
        31,
        49,
        "CPA0702",
        99,
        "123503/TESTUSER/INVBATCH",
        "CPF0001 received by procedure INVLOAD. (C D I R)",
    ),
    (
        9,
        15,
        12,
        "TCP1A03",
        10,
        "123380/QTCP/QTCPIP",
        "Interface 10.0.10.21 on line ETHLIN02 started.",
    ),
    (
        9,
        0,
        2,
        "CPC3703",
        0,
        "123350/QSECOFR/SAVDAILY",
        "2 objects saved from library QGPL.",
    ),
    (
        6,
        0,
        4,
        "HTP8001",
        0,
        "123471/QTMHHTTP/API",
        "HTTP server instance API started, listening on *:9443.",
    ),
    (
        6,
        0,
        3,
        "HTP8001",
        0,
        "123456/QTMHHTTP/ADMIN",
        "HTTP server instance ADMIN started, listening on *:2001.",
    ),
    (
        6,
        0,
        0,
        "CPF0927",
        0,
        "123202/QSYS/QHTTPSVR",
        "Subsystem QHTTPSVR active.",
    ),
];

/// The history log, oldest first: the night's batch jobs started and
/// ended (195 entries before 06:00), then the day's.
fn history() -> Vec<HistEntry> {
    let mut out = vec![HistEntry {
        sent: today(0, 0, 30),
        id: "CPF0927".into(),
        severity: 0,
        job: "123201/QSYS/QBATCH".into(),
        text: "Subsystem QBATCH active.".into(),
    }];
    for k in 0..97u32 {
        let start = today(0, 1, 0) + u64::from(k) * 220;
        let end = start + 40;
        let job = format!("{}/QPGMR/NIGHT{:03}", 123_234 + k, k + 1);
        let (sd, st) = crate::util::mdy(start, 0);
        let (ed, et) = crate::util::mdy(end, 0);
        out.push(HistEntry {
            sent: start,
            id: "CPF1124".into(),
            severity: 0,
            job: job.clone(),
            text: format!("Job {job} started on {sd} at {st} in subsystem QBATCH in QSYS."),
        });
        out.push(HistEntry {
            sent: end,
            id: "CPF1164".into(),
            severity: 0,
            job: job.clone(),
            text: format!("Job {job} ended on {ed} at {et}; 40 seconds used; end code 0."),
        });
    }
    out.extend(
        QHST_NEWEST
            .iter()
            .rev()
            .map(|&(h, m, s, id, sev, job, text)| HistEntry {
                sent: today(h, m, s),
                id: id.into(),
                severity: sev,
                job: job.into(),
                text: text.into(),
            }),
    );
    out
}

/// The queues and their messages, oldest first, keys given.
fn queues() -> (BTreeMap<String, Vec<QueuedMessage>>, u32) {
    let mut opr: Vec<QueuedMessage> = qsysopr_newest();
    opr.extend(qsysopr_night());
    opr.reverse();
    let own = vec![
        QueuedMessage {
            severity: 80,
            sent: secs(2026, 10, 28, 17, 40, 0),
            text: "can you release job queue QPGMR when the RGZPFM finishes? Thanks.".into(),
            job: "123311/TESTUSER/QPADEV0003".into(),
            user: "TESTUSER".into(),
            program: "QWCSNDMS".into(),
            ..QueuedMessage::default()
        },
        info(
            today(9, 0, 2),
            "CPC3703",
            0,
            "2 objects saved from library QGPL.",
            "123350/QSECOFR/SAVDAILY",
            "QSRSAVO",
        ),
        info(
            today(13, 5, 10),
            "CPF1241",
            0,
            "Job 123488/TESTUSER/QPADEV0003 submitted to job queue QBATCH in QGPL.",
            "123488/TESTUSER/QPADEV0003",
            "QWTSCSBJ",
        ),
    ];
    let sysmsg = vec![info(
        today(5, 12, 40),
        "CPF0907",
        40,
        "Serious storage condition may exist. Press HELP.",
        "123105/QSYS/QSPLMAINT",
        "QSPLMAINT",
    )];
    let testuser = vec![info(
        today(10, 31, 50),
        "CPF1241",
        0,
        "Job 123503/TESTUSER/INVBATCH submitted to job queue QBATCH in QGPL.",
        "123488/TESTUSER/QPADEV0003",
        "QWTSCSBJ",
    )];
    let qtcp = vec![info(
        today(9, 15, 12),
        "TCP1A03",
        10,
        "Interface 10.0.10.21 on line ETHLIN02 started.",
        "123380/QTCP/QTCPIP",
        "QTOCIFC",
    )];
    let mut map = BTreeMap::from([
        ("QSYS/QSYSOPR".to_string(), opr),
        ("QSYS/QSYSMSG".to_string(), sysmsg),
        ("QUSRSYS/QSECOFR".to_string(), own),
        ("QUSRSYS/TESTUSER".to_string(), testuser),
        ("QUSRSYS/QTCP".to_string(), qtcp),
        ("QUSRSYS/QPGMR".to_string(), Vec::new()),
        ("QUSRSYS/WEBUSER".to_string(), Vec::new()),
    ]);
    let mut key = 0x100;
    for q in map.values_mut() {
        for m in q.iter_mut() {
            m.key = format!("{key:08X}");
            key += 1;
        }
    }
    (map, key)
}

impl Default for FixtureMsgs {
    fn default() -> Self {
        let (q, next) = queues();
        Self {
            queues: Mutex::new(q),
            ran: Mutex::new(Vec::new()),
            next: Mutex::new(next),
        }
    }
}

/// `lib/name` of a `MSGQ` or `TOMSGQ` value, `*SYSOPR` and bare names
/// resolved.
fn queue_of(v: &str) -> String {
    let v = v.trim().to_ascii_uppercase();
    match v.as_str() {
        "*SYSOPR" | "QSYSOPR" | "QSYS/QSYSOPR" => "QSYS/QSYSOPR".into(),
        _ => {
            let (lib, name) = split(&v, "QUSRSYS");
            format!("{lib}/{name}")
        }
    }
}

/// Whether `user` may read or change queue `q`: its own, the system's,
/// or with `*ALLOBJ`.
fn may(user: &str, q: &str) -> bool {
    q.starts_with("QSYS/") || q == format!("QUSRSYS/{user}") || super::has_special(user, "*ALLOBJ")
}

impl FixtureMsgs {
    fn key(&self) -> String {
        let mut k = self.next.lock().expect("key");
        *k += 1;
        format!("{:08X}", *k - 1)
    }

    #[allow(clippy::too_many_lines)]
    fn run_in(
        &self,
        user: &str,
        name: &str,
        p: &BTreeMap<String, String>,
    ) -> Result<Message, Message> {
        let get = |k: &str| p.get(k).map(String::as_str).unwrap_or_default();
        let mut qs = self.queues.lock().expect("queues");
        match name {
            "SNDMSG" => {
                let text = unquote(get("MSG"));
                if text.trim().is_empty() {
                    return fail("CPD0071", &["MSG"]);
                }
                let q = if p.contains_key("TOMSGQ") {
                    queue_of(get("TOMSGQ"))
                } else if p.contains_key("TOUSR") {
                    queue_of(get("TOUSR"))
                } else {
                    return fail("CPD0071", &["TOMSGQ"]);
                };
                let (lib, qn) = split(&q, "QUSRSYS");
                let (lib, qn) = (lib.to_string(), qn.to_string());
                let inq = get("MSGTYPE") == "*INQ";
                let key = self.key();
                let Some(queue) = qs.get_mut(&q) else {
                    return fail("CPF2403", &[&qn, &lib]);
                };
                queue.push(QueuedMessage {
                    key,
                    severity: if inq { 99 } else { 80 },
                    inquiry: inq,
                    sent: super::now(),
                    text,
                    job: format!("123600/{user}/NAVIGATOR"),
                    user: user.into(),
                    program: "QWCSNDMS".into(),
                    ..QueuedMessage::default()
                });
                done("CPC2402", &[&qn, &lib])
            }
            "SNDRPYMSG" => {
                let q = queue_of(get("MSGQ"));
                let key = get("MSGKEY").trim().to_ascii_uppercase();
                if !may(user, &q) {
                    let (lib, qn) = split(&q, "QUSRSYS");
                    return fail("CPF2207", &[qn, lib, "*MSGQ"]);
                }
                let Some(m) = qs
                    .get_mut(&q)
                    .and_then(|v| v.iter_mut().find(|m| m.key == key))
                else {
                    return fail("CPF2410", &[&key]);
                };
                if !m.waiting() {
                    return fail("CPF2422", &[&key]);
                }
                let r = unquote(get("RPY")).trim().to_ascii_uppercase();
                let r = if r.is_empty() || r == "*DFT" {
                    m.default.clone()
                } else {
                    r
                };
                m.reply = Some(r.clone());
                if get("RMV") != "*NO" {
                    qs.get_mut(&q).expect("queue").retain(|m| m.key != key);
                }
                done("CPC2403", &[&r, &key])
            }
            "RMVMSG" | "CLRMSGQ" => {
                let q = queue_of(get("MSGQ"));
                let (lib, qn) = split(&q, "QUSRSYS");
                let (lib, qn) = (lib.to_string(), qn.to_string());
                if !may(user, &q) {
                    return fail("CPF2207", &[&qn, &lib, "*MSGQ"]);
                }
                let Some(queue) = qs.get_mut(&q) else {
                    return fail("CPF2403", &[&qn, &lib]);
                };
                let clear = if name == "CLRMSGQ" {
                    Some(if p.contains_key("CLEAR") {
                        get("CLEAR")
                    } else {
                        "*ALL"
                    })
                } else {
                    p.get("CLEAR")
                        .map(String::as_str)
                        .filter(|c| *c != "*BYKEY")
                };
                if let Some(c) = clear {
                    let before = queue.len();
                    queue.retain(|m| c == "*KEEPUNANS" && m.waiting());
                    let n = (before - queue.len()).to_string();
                    return done("CPC2405", &[&n, &qn, &lib]);
                }
                let key = get("MSGKEY").trim().to_ascii_uppercase();
                let before = queue.len();
                queue.retain(|m| m.key != key);
                if queue.len() == before {
                    return fail("CPF2410", &[&key]);
                }
                done("CPC2404", &[&key, &qn, &lib])
            }
            _ => fail("CPF9899", &[name]),
        }
    }
}

impl Msgs for FixtureMsgs {
    fn queues(&self, _user: &str) -> Vec<(String, String)> {
        [
            "QSYS/QSYSOPR",
            "QSYS/QSYSMSG",
            "QUSRSYS/QSECOFR",
            "QUSRSYS/TESTUSER",
            "QUSRSYS/QTCP",
        ]
        .iter()
        .map(|q| {
            let (l, n) = split(q, "QUSRSYS");
            (l.to_string(), n.to_string())
        })
        .collect()
    }

    fn own_queue(&self, user: &str) -> (String, String) {
        if user == "QSYSOPR" {
            ("QSYS".into(), "QSYSOPR".into())
        } else {
            ("QUSRSYS".into(), user.into())
        }
    }

    fn messages(&self, user: &str, lib: &str, name: &str) -> Result<Vec<QueuedMessage>, Message> {
        let q = format!("{lib}/{name}");
        let qs = self.queues.lock().expect("queues");
        let Some(v) = qs.get(&q) else {
            return Err(fail("CPF2403", &[name, lib]).unwrap_err());
        };
        if !may(user, &q) {
            return Err(fail("CPF2207", &[name, lib, "*MSGQ"]).unwrap_err());
        }
        Ok(v.clone())
    }

    fn history(&self, _user: &str) -> Vec<HistEntry> {
        history()
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.ran.lock().expect("ran").push(command.to_string());
        let (name, p) = parse(command);
        self.run_in(user, &name, &p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts() {
        let m = FixtureMsgs::default();
        let opr = m.messages("QSECOFR", "QSYS", "QSYSOPR").unwrap();
        assert_eq!(opr.len(), 41);
        assert_eq!(opr.iter().filter(|m| m.waiting()).count(), 3);
        assert_eq!(history().len(), 214);
        assert!(history().windows(2).all(|w| w[0].sent <= w[1].sent));
    }
}
