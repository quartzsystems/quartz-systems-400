// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::msgs::Msgs` as the user: queues from `list_objects`
//! (`WRKMSGQ`), messages from `msgq_list` (what `DSPMSG` reads), the
//! profile's queue from `QSYRUSRI` (`sec_api Profile`), the history log
//! from `hst_entries` (`DSPLOG`); changes are commands (`run`).

use navigator::env::msgs::{HistEntry, Msgs, QueuedMessage};
use qsweb::msg::{Kind, Message};
use sec::api::{Answer, Query};
use stg::Client;
use wm::obj::msgq::{MsgKind, QMsg, Status};

use super::Link;
use crate::work::web::bridge::Main;

/// Message queues and the history log.
pub struct LiveMsgs {
    /// The job's thread.
    pub link: Link,
}

/// A service failure (`CPF2403 text`) as a message.
pub fn failure(e: &str) -> Message {
    let (id, text) = e.split_once(' ').unwrap_or(("CPF9898", e));
    Message::new(Kind::Error, id, text)
}

/// One queue entry as the pages show it.
fn queued(m: QMsg) -> QueuedMessage {
    let (cause, recovery) = httpadmin::msgs::split_help(&m.help);
    let inquiry = m.kind == MsgKind::Inq;
    QueuedMessage {
        key: format!("{:08X}", m.key),
        id: m.msgid.trim().to_string(),
        severity: m.severity,
        inquiry,
        sent: m.time / 1_000_000,
        text: m.text,
        cause,
        recovery,
        job: format!("{:06}/{}/{}", m.job.2, m.job.1, m.job.0),
        user: m.job.1,
        program: m.from_pgm,
        replies: String::new(),
        default: m.dft,
        reply: (inquiry && m.status == Status::Answered).then_some(m.reply),
    }
}

/// The queues of library `lib`, as `(lib, name)`.
fn queues_in(m: &Main, lib: &str) -> Vec<(String, String)> {
    let l = lib.to_string();
    m.ask(|c| c.list_objects(l, "*ALL".into(), "*MSGQ".into()))
        .into_iter()
        .map(|o| (o.lib, o.name))
        .collect()
}

/// Days since 1970 of a civil date.
fn days(y: i64, mo: i64, d: i64) -> i64 {
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `CYYMMDD` and `HHMMSS`, local, as seconds since 1970.
fn local_secs(date: &str, time: &str, offset_min: i32) -> u64 {
    let n = |s: &str, a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
    if date.len() < 7 || time.len() < 6 {
        return 0;
    }
    let year = 1900 + n(date, 0, 1) * 100 + n(date, 1, 3);
    let d = days(year, n(date, 3, 5), n(date, 5, 7));
    let secs = d * 86_400 + n(time, 0, 2) * 3600 + n(time, 2, 4) * 60 + n(time, 4, 6) - i64::from(offset_min) * 60;
    u64::try_from(secs).unwrap_or(0)
}

impl Msgs for LiveMsgs {
    fn queues(&self, user: &str) -> Vec<(String, String)> {
        self.link.read(user, |m| {
            let mut out: Vec<(String, String)> = queues_in(m, "QSYS")
                .into_iter()
                .filter(|(_, n)| n == "QSYSOPR" || n == "QSYSMSG")
                .collect();
            out.extend(queues_in(m, "QUSRSYS"));
            out
        })
    }

    fn own_queue(&self, user: &str) -> (String, String) {
        let u = user.trim().to_ascii_uppercase();
        let fallback = ("QUSRSYS".to_string(), u.clone());
        let q = self.link.read(user, move |m| match m.sec(Query::Profile { user: u }) {
            Ok(Answer::Profile { data }) => sec::profile::UsrPrf::decode(&data)
                .ok()
                .map(|p| (p.msgq.1, p.msgq.0)),
            _ => None,
        });
        match q {
            Some((lib, name)) if !name.is_empty() && !lib.starts_with('*') => (lib, name),
            Some((_, name)) if !name.is_empty() && !name.starts_with('*') => ("QUSRSYS".into(), name),
            _ => fallback,
        }
    }

    fn messages(&self, user: &str, lib: &str, name: &str) -> Result<Vec<QueuedMessage>, Message> {
        let q = format!("{lib}/{name}");
        self.link
            .read(user, move |m| Some(m.ask(|l| l.msgq_list(q))))
            .unwrap_or_else(|| Err("CPF9897 Navigator could not read the queue.".into()))
            .map(|all| all.into_iter().filter(|m| m.kind != MsgKind::Rpy).map(queued).collect())
            .map_err(|e| failure(&e))
    }

    fn history(&self, user: &str) -> Vec<HistEntry> {
        self.link.read(user, |m| {
            let offset = m.ask(Client::now).offset_min;
            let rows = m.ask(Client::hst_entries).unwrap_or_default();
            rows.into_iter()
                .map(|r| {
                    let f = |i: usize| r.get(i).map_or("", |s| s.trim());
                    let number: u32 = f(7).parse().unwrap_or(0);
                    HistEntry {
                        sent: local_secs(f(0), f(1), offset),
                        id: f(2).to_string(),
                        severity: f(3).parse().unwrap_or(0),
                        job: format!("{number:06}/{}/{}", f(6), f(5)),
                        text: f(8).to_string(),
                    }
                })
                .collect()
        })
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.link.run(user, command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_dates() {
        // 1261101 is 11/01/26; 120000 local at UTC-4 is 16:00 UTC.
        assert_eq!(local_secs("1261101", "120000", -240), 1_793_491_200 + 16 * 3600);
        assert_eq!(local_secs("", "", 0), 0);
    }
}
