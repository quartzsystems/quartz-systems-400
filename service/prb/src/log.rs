// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The problem log: create, list, change, note, delete, report.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::analyze::Detected;
use crate::problem::{Origin, PrbStatus, PrbType, Problem};
use crate::report::{Reported, Sender};

/// What `WRKPRB`, `DLTPRB` and `PRTERRLOG`-style lists select.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Statuses; empty for all.
    pub status: Vec<PrbStatus>,
    /// Highest severity number to include (1 high); 0 for all.
    pub max_severity: u8,
    /// Only problems opened at or after (Unix seconds).
    pub since: Option<u64>,
    /// Only problems opened before (Unix seconds); `DLTPRB DAYS`.
    pub before: Option<u64>,
    /// Only hardware problems (`WRKPRB HARDWARE(...)`).
    pub hardware: bool,
}

impl Filter {
    fn takes(&self, p: &Problem) -> bool {
        (self.status.is_empty() || self.status.contains(&p.status))
            && (self.max_severity == 0 || p.severity <= self.max_severity)
            && self.since.is_none_or(|t| p.opened >= t)
            && self.before.is_none_or(|t| p.opened < t)
            && (!self.hardware || matches!(p.origin, Origin::Hardware { .. }))
    }
}

/// Every problem, by identifier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProblemLog {
    /// The problems.
    pub problems: BTreeMap<String, Problem>,
    /// The next sequence within the day.
    pub next_seq: u32,
    /// Prepared reports kept for the service provider, by problem.
    pub reports: BTreeMap<String, String>,
}

/// `yyddd` of a Unix time, UTC.
fn yyddd(t: u64) -> String {
    let days = t / 86_400;
    // Civil year from days since 1970 (proleptic Gregorian).
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let jan1 = {
        let y = y - 1;
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + 306;
        era * 146_097 + doe - 719_468
    };
    let day = i64::try_from(days).unwrap_or(0) - jan1 + 1;
    format!("{:02}{:03}", y.rem_euclid(100), day)
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\t', "\\t").replace('\n', "\\n")
}

fn unesc(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl ProblemLog {
    /// Log a problem; an open problem with the same symptoms gets a note
    /// instead of a second entry. The identifier.
    pub fn create(&mut self, kind: PrbType, d: &Detected, system: &str, now: u64) -> String {
        let (origin, severity, text, symptoms) = (d.origin.clone(), d.severity, &d.text, &d.symptoms);
        if let Some(p) = self
            .problems
            .values_mut()
            .find(|p| p.open() && p.symptoms == *symptoms && p.origin == origin)
        {
            p.notes.push(format!("occurred again at {now}"));
            p.changed = now;
            return p.id.clone();
        }
        let id = format!("{}{:05}", yyddd(now), self.next_seq % 100_000);
        self.next_seq += 1;
        self.problems.insert(
            id.clone(),
            Problem {
                id: id.clone(),
                status: PrbStatus::Opened,
                kind,
                severity: severity.clamp(1, 4),
                origin,
                text: text.clone(),
                symptoms: symptoms.clone(),
                system: system.into(),
                opened: now,
                changed: now,
                assigned_to: String::new(),
                notes: Vec::new(),
                fixes: Vec::new(),
            },
        );
        id
    }

    /// One problem.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Problem> {
        self.problems.get(id)
    }

    /// One problem, to change.
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Problem> {
        self.problems.get_mut(id)
    }

    /// The problems a filter selects, newest first.
    #[must_use]
    pub fn list(&self, f: &Filter) -> Vec<&Problem> {
        let mut v: Vec<&Problem> = self.problems.values().filter(|p| f.takes(p)).collect();
        v.sort_by(|a, b| b.opened.cmp(&a.opened).then(b.id.cmp(&a.id)));
        v
    }

    /// `DLTPRB`: delete what the filter selects. How many.
    pub fn delete(&mut self, f: &Filter) -> usize {
        let before = self.problems.len();
        self.problems.retain(|_, p| !f.takes(p));
        before - self.problems.len()
    }

    /// The report a "report to the service provider" sends: the problem,
    /// its symptoms and notes, as text. The problem becomes Prepared.
    pub fn prepare(&mut self, id: &str, now: u64) -> Option<String> {
        let p = self.problems.get_mut(id)?;
        if p.status < PrbStatus::Prepared {
            p.status = PrbStatus::Prepared;
            p.changed = now;
        }
        let mut r = String::new();
        let _ = writeln!(r, "problem {}", p.id);
        let _ = writeln!(r, "system {}", p.system);
        let _ = writeln!(r, "type {} severity {}", p.kind.number(), p.severity);
        let _ = writeln!(r, "opened {}", p.opened);
        let _ = writeln!(r, "text {}", p.text);
        let _ = writeln!(r, "symptoms {}", p.symptoms);
        for n in &p.notes {
            let _ = writeln!(r, "note {n}");
        }
        Some(r)
    }

    /// Analysis done: an opened problem becomes Ready. Whether it exists.
    pub fn analyzed(&mut self, id: &str, now: u64) -> bool {
        let Some(p) = self.problems.get_mut(id) else {
            return false;
        };
        if p.status == PrbStatus::Opened {
            p.status = PrbStatus::Ready;
            p.changed = now;
        }
        true
    }

    /// "Report to the service provider": prepare the report, keep it,
    /// hand it to `sender`. A sent problem becomes Sent; otherwise the
    /// report is kept and the problem stays Prepared. `None` for no
    /// such problem.
    pub fn report(&mut self, id: &str, sender: &mut dyn Sender, now: u64) -> Option<Reported> {
        let text = self.prepare(id, now)?;
        self.reports.insert(id.to_string(), text.clone());
        let p = self.problems.get_mut(id)?;
        Some(match sender.send(id, &text) {
            Ok(r) => {
                p.status = PrbStatus::Sent;
                p.notes.push(format!("sent to the service provider: {r}"));
                Reported::Sent(r)
            }
            Err(why) => Reported::Kept(why),
        })
    }

    /// The stored text: a header line, then one line per problem with
    /// tab-separated fields.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut out = format!("QS400PRB 1 {}\n", self.next_seq);
        for (id, r) in &self.reports {
            let _ = writeln!(out, "R\t{id}\t{}", esc(r));
        }
        for p in self.problems.values() {
            let origin = match &p.origin {
                Origin::Message(m) => format!("M:{m}"),
                Origin::Hardware { resource, src } => format!("H:{resource}:{src}"),
                Origin::Dump(d) => format!("D:{d}"),
                Origin::Analysis => "A:".into(),
            };
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                p.id,
                p.status.display(),
                p.kind.number(),
                p.severity,
                esc(&origin),
                esc(&p.text),
                esc(&p.symptoms),
                esc(&p.system),
                p.opened,
                p.changed,
                esc(&p.assigned_to),
                esc(&p.notes.join("\n")),
                esc(&p.fixes.join(" "))
            );
        }
        out
    }

    /// Parse the stored text; lines that do not parse are dropped.
    #[must_use]
    pub fn decode(text: &str) -> Self {
        let mut lines = text.lines();
        let next_seq = lines
            .next()
            .and_then(|h| h.split_whitespace().nth(2))
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let mut log = Self {
            next_seq,
            ..Self::default()
        };
        for line in lines {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() == 3 && f[0] == "R" {
                log.reports.insert(f[1].into(), unesc(f[2]));
                continue;
            }
            if f.len() != 13 {
                continue;
            }
            let (Some(status), Some(kind)) = (
                PrbStatus::parse(f[1]),
                f[2].parse().ok().and_then(PrbType::from_number),
            ) else {
                continue;
            };
            let o = unesc(f[4]);
            let origin = match o.split_once(':') {
                Some(("M", m)) => Origin::Message(m.into()),
                Some(("H", rest)) => {
                    let (r, s) = rest.split_once(':').unwrap_or((rest, ""));
                    Origin::Hardware {
                        resource: r.into(),
                        src: s.into(),
                    }
                }
                Some(("D", d)) => Origin::Dump(d.into()),
                _ => Origin::Analysis,
            };
            let notes = unesc(f[11]);
            let fixes = unesc(f[12]);
            let p = Problem {
                id: f[0].into(),
                status,
                kind,
                severity: f[3].parse().unwrap_or(3),
                origin,
                text: unesc(f[5]),
                symptoms: unesc(f[6]),
                system: unesc(f[7]),
                opened: f[8].parse().unwrap_or(0),
                changed: f[9].parse().unwrap_or(0),
                assigned_to: unesc(f[10]),
                notes: notes.lines().map(str::to_string).collect(),
                fixes: fixes.split_whitespace().map(str::to_string).collect(),
            };
            log.problems.insert(p.id.clone(), p);
        }
        log
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn julian_days() {
        assert_eq!(yyddd(0), "70001");
        // 2026-10-30 is day 303.
        assert_eq!(yyddd(1_793_318_400), "26303");
    }
}
