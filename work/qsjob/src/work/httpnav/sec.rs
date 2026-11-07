// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::sec::Security` as the user: the audit entries are
//! `DSPAUDJRNE`'s listing, run as the 5250 display runs it (`exec`, so
//! the command checks `*AUDIT`); the journal and its receiver are
//! `DSPSECAUD`'s (`secaud_panel`), the receiver's attach time its
//! creation (`object`).

use std::fmt::Write as _;

use navigator::env::sec::{AuditEntry, AuditFilter, AuditReceiver, Security};
use qsweb::msg::{Kind, Message};
use stg::Client;

use super::Link;
use super::db::{cyy_secs, stamp_secs};
use crate::work::web::bridge::Main;

/// The audit journal.
pub struct LiveSecurity {
    /// The job's thread.
    pub link: Link,
}

/// `YYYY-MM-DD` of a UTC second.
fn ymd(secs: u64) -> String {
    ::sec::cmd::civil(secs)
}

/// The command line for a filter: the dates are whole UTC days, the
/// entries are cut to the seconds afterwards.
fn line(f: &AuditFilter) -> String {
    let kind = f.kind.trim().to_ascii_uppercase();
    let user = f.user.trim().to_ascii_uppercase();
    let mut l = format!(
        "DSPAUDJRNE ENTTYP({}) USRPRF({})",
        if kind.is_empty() {
            "*ALL"
        } else {
            kind.as_str()
        },
        if user.is_empty() {
            "*ALL"
        } else {
            user.as_str()
        }
    );
    if f.from != 0 {
        let _ = write!(l, " FROMTIME({})", ymd(f.from));
    }
    if f.to != 0 {
        let _ = write!(l, " TOTIME({})", ymd(f.to));
    }
    l.push_str(" OUTPUT(*)");
    l
}

/// `DSPAUDJRNE`'s listing as entries: under each ` Entry type XX`
/// heading, a row is sequence, date, time, job, user, number, then the
/// type's fields.
fn parse(text: &str) -> Vec<AuditEntry> {
    let mut kind = String::new();
    let mut out = Vec::new();
    for l in text.lines() {
        if let Some(k) = l.trim().strip_prefix("Entry type ") {
            kind = k.trim().to_string();
            continue;
        }
        if kind.is_empty() || l.trim().is_empty() || l.trim_start().starts_with("SEQNBR") {
            continue;
        }
        let mut rest = l.trim_start();
        let mut tok = Vec::new();
        for _ in 0..6 {
            let end = rest.find(' ').unwrap_or(rest.len());
            tok.push(&rest[..end]);
            rest = rest[end..].trim_start();
        }
        if tok[0].parse::<u64>().is_err() {
            continue;
        }
        out.push(AuditEntry {
            at: stamp_secs(&format!("{} {}", tok[1], tok[2])),
            kind: kind.clone(),
            user: tok[4].to_string(),
            job: format!("{}/{}/{}", tok[5], tok[4], tok[3]),
            detail: rest.trim_end().to_string(),
        });
    }
    out
}

fn entries(m: &Main, f: &AuditFilter) -> Result<Vec<AuditEntry>, Message> {
    let text = m.console.host.exec(&line(f)).text;
    let t = text.trim();
    if let Some(id) = crate::command::jobsite::leading_msgid(t) {
        return Err(Message::new(Kind::Error, id, t[7..].trim()));
    }
    let mut all: Vec<AuditEntry> = parse(&text)
        .into_iter()
        .filter(|e| (f.from == 0 || e.at >= f.from) && (f.to == 0 || e.at <= f.to))
        .collect();
    all.sort_by_key(|e| std::cmp::Reverse(e.at));
    Ok(all)
}

fn receiver(m: &Main) -> AuditReceiver {
    let Ok(rows) = m.ask(Client::secaud_panel) else {
        return AuditReceiver::default();
    };
    let get = |k: &str| {
        rows.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    let journal = get("Security journal");
    if journal.is_empty() || journal == "*NONE" {
        return AuditReceiver::default();
    }
    let receiver = get("Current receiver");
    let (lib, _) = journal.split_once('/').unwrap_or(("QSYS", ""));
    let (l, n) = (lib.to_string(), receiver.clone());
    let offset = m.ask(Client::now).offset_min;
    let attached = m
        .ask(|c| c.object(l, n, "*JRNRCV".into()))
        .map_or(0, |o| cyy_secs(&o.created, offset));
    AuditReceiver {
        journal,
        receiver,
        attached,
    }
}

impl Security for LiveSecurity {
    fn audit_entries(&self, user: &str, filter: &AuditFilter) -> Result<Vec<AuditEntry>, Message> {
        let f = filter.clone();
        self.link
            .read(user, move |m| Some(entries(m, &f)))
            .unwrap_or_else(|| {
                Err(Message::new(
                    Kind::Error,
                    "CPF9897",
                    "Navigator could not read the audit journal.",
                ))
            })
    }

    fn audit_receiver(&self, user: &str) -> AuditReceiver {
        self.link.read(user, |m| receiver(m))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing() {
        let text = " Display Audit Journal Entries\n\n Entry type PW\n \
                    SEQNBR     TIMESTAMP                  JOB        USER       NUMBER VIOLATION\n \
                    12         2026-11-01 13:00:05.000001 QPADEV0001 BOB        000123 P    bad password\n\n";
        let e = parse(text);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].kind, "PW");
        assert_eq!(e[0].job, "000123/BOB/QPADEV0001");
        assert_eq!(e[0].detail, "P    bad password");
        assert_eq!(e[0].at, 1_793_491_200 + 13 * 3600 + 5);
        let f = AuditFilter {
            kind: "pw".into(),
            user: "*ALL".into(),
            from: 1_793_491_200,
            to: 0,
        };
        assert_eq!(
            line(&f),
            "DSPAUDJRNE ENTTYP(PW) USRPRF(*ALL) FROMTIME(2026-11-01) OUTPUT(*)"
        );
    }
}
