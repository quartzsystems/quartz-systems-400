// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPJRN` filters and text, `DSPJRNRCVA`, and the `RCVJRNE`-style
//! cursor over the receiver chain.

use std::fmt::Write as _;

use qsname::Name;
use store::obj::ObjectId;
use store::txn::Txn;

use crate::entry::Entry;
use crate::error::Result;
use crate::journal::Journal;
use crate::rcv::{Cursor, Receiver};

/// What `DSPJRN` shows. Every `None` or empty filter means everything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// `FILE`/`OBJ`: library, object, member (each `None` for any).
    pub object: Option<(Option<Name>, Name, Option<Name>)>,
    /// `JOB`: job number.
    pub job: Option<u32>,
    /// `JRNCDE`: journal codes.
    pub codes: Vec<u8>,
    /// `ENTTYP`: entry types.
    pub types: Vec<[u8; 2]>,
    /// `FROMENT`.
    pub from_seq: Option<u64>,
    /// `TOENT`.
    pub to_seq: Option<u64>,
    /// `FROMTIME`, Unix seconds.
    pub from_time: Option<u64>,
    /// `TOTIME`, Unix seconds.
    pub to_time: Option<u64>,
    /// `RCVRNG`: first and last receiver; `None` for `*CURCHAIN`.
    pub receivers: Option<(ObjectId, ObjectId)>,
    /// `*CURRENT`: the attached receiver only.
    pub current_only: bool,
    /// `CMTCYCID`.
    pub commit_cycle: Option<u64>,
}

impl Filter {
    /// Whether the entry passes.
    #[must_use]
    pub fn passes(&self, e: &Entry) -> bool {
        if let Some((lib, obj, mbr)) = &self.object {
            let Some(o) = &e.object else {
                return false;
            };
            if o.object != *obj
                || lib.is_some_and(|l| l != o.library)
                || mbr.is_some_and(|m| o.member != Some(m))
            {
                return false;
            }
        }
        if self.job.is_some_and(|j| j != e.job.number) {
            return false;
        }
        if !self.codes.is_empty() && !self.codes.contains(&e.code) {
            return false;
        }
        if !self.types.is_empty() && !self.types.contains(&e.kind) {
            return false;
        }
        if self.from_seq.is_some_and(|s| e.seq < s) || self.to_seq.is_some_and(|s| e.seq > s) {
            return false;
        }
        if self.from_time.is_some_and(|t| e.time < t) || self.to_time.is_some_and(|t| e.time > t) {
            return false;
        }
        if self.commit_cycle.is_some_and(|c| c != e.commit_cycle) {
            return false;
        }
        true
    }
}

/// Reads entries across the receiver chain, oldest first.
#[derive(Clone, Debug)]
pub struct ChainCursor {
    receivers: Vec<Receiver>,
    at: usize,
    cursor: Option<Cursor>,
}

impl ChainCursor {
    /// Over the receivers the filter names: a range, the current one,
    /// or the whole chain.
    pub fn new(txn: &mut Txn<'_>, jrn: &Journal, filter: &Filter) -> Result<Self> {
        let mut receivers = jrn.chain(txn)?;
        if filter.current_only {
            receivers.retain(|r| r.id == jrn.attr.attached);
        } else if let Some((first, last)) = filter.receivers {
            let a = receivers.iter().position(|r| r.id == first);
            let b = receivers.iter().position(|r| r.id == last);
            receivers = match (a, b) {
                (Some(a), Some(b)) if a <= b => receivers.drain(a..=b).collect(),
                _ => {
                    return Err(crate::JrnError::Range {
                        journal: jrn.name().to_string(),
                        what: "receiver range is not part of the chain".into(),
                    });
                }
            };
        }
        Ok(Self {
            receivers,
            at: 0,
            cursor: None,
        })
    }

    /// The next entry, or `None` at the end of the chain.
    pub fn next(&mut self, txn: &mut Txn<'_>) -> Result<Option<Entry>> {
        loop {
            if self.cursor.is_none() {
                let Some(r) = self.receivers.get(self.at) else {
                    return Ok(None);
                };
                self.cursor = Some(Cursor::new(r));
            }
            if let Some(e) = self.cursor.as_mut().expect("set").next(txn)? {
                return Ok(Some(e));
            }
            self.cursor = None;
            self.at += 1;
        }
    }

    /// Skip to the first entry at or after `seq`.
    pub fn seek(&mut self, txn: &mut Txn<'_>, seq: u64) -> Result<()> {
        // Receivers hold contiguous ranges; skip whole ones first.
        while self
            .receivers
            .get(self.at)
            .is_some_and(|r| r.attr.entry_count > 0 && r.attr.last_seq < seq)
        {
            self.at += 1;
        }
        self.cursor = None;
        let Some(r) = self.receivers.get(self.at) else {
            return Ok(());
        };
        let mut c = Cursor::new(r);
        let mut probe = c.clone();
        while let Some(e) = probe.next(txn)? {
            if e.seq >= seq {
                break;
            }
            c = probe.clone();
        }
        self.cursor = Some(c);
        Ok(())
    }
}

/// `DSPJRN`: the entries that pass the filter.
pub fn dspjrn(txn: &mut Txn<'_>, jrn: ObjectId, filter: &Filter) -> Result<Vec<Entry>> {
    store::obj::guard::check_id(txn, "jrn.read", jrn)?;
    let j = Journal::load(txn, jrn)?;
    let mut c = ChainCursor::new(txn, &j, filter)?;
    if let Some(from) = filter.from_seq {
        c.seek(txn, from)?;
    }
    let mut out = Vec::new();
    while let Some(e) = c.next(txn)? {
        if filter.to_seq.is_some_and(|t| e.seq > t) {
            break;
        }
        if filter.passes(&e) {
            out.push(e);
        }
    }
    Ok(out)
}

/// `yyyy-mm-dd hh:mm:ss` of a Unix time.
fn stamp(secs: u64) -> String {
    let z = i64::try_from(secs / 86_400).expect("fits") + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let s = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        s / 3600,
        s % 3600 / 60,
        s % 60
    )
}

/// One line per entry: sequence, code and type, time, job, object,
/// count, commit cycle, data length.
#[must_use]
pub fn render(entries: &[Entry]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:>10} {:<4} {:<19} {:<24} {:<32} {:>10} {:>8} {:>6}",
        "Sequence", "Type", "Timestamp", "Job", "Object", "Count/RRN", "Cycle", "Data"
    );
    for e in entries {
        let object = e.object.as_ref().map_or(String::new(), |o| {
            let mut s = format!("{}/{}", o.library, o.object);
            if let Some(m) = o.member {
                let _ = write!(s, " {m}");
            }
            s
        });
        let _ = writeln!(
            out,
            "{:>10} {:<4} {:<19} {:<24} {:<32} {:>10} {:>8} {:>6}",
            e.seq,
            e.type_name(),
            stamp(e.time),
            e.job.qualified(),
            object,
            e.count,
            e.commit_cycle,
            e.data.len()
        );
    }
    out
}

/// `DSPJRNRCVA`: the receiver's attributes as text.
pub fn dspjrnrcva(txn: &mut Txn<'_>, rcv: ObjectId) -> Result<String> {
    store::obj::guard::check_id(txn, "obj.describe", rcv)?;
    let r = Receiver::load(txn, rcv)?;
    let (_, lib) = store::obj::ops::read_header(txn, r.header.library)?;
    let name_of = |txn: &mut Txn<'_>, id: ObjectId| -> String {
        if id.is_none() {
            return "*NONE".into();
        }
        store::obj::ops::read_header(txn, id)
            .map_or_else(|_| id.to_string(), |(_, h)| h.name.to_string())
    };
    let journal = name_of(txn, r.attr.journal);
    let prev = name_of(txn, r.attr.prev);
    let next = name_of(txn, r.attr.next);
    let mut out = String::new();
    let kv = |out: &mut String, k: &str, v: &dyn std::fmt::Display| {
        let _ = writeln!(out, "{k:<32}:  {v}");
    };
    out.push_str("Journal receiver attributes\n");
    kv(&mut out, "Journal receiver", &r.name());
    kv(&mut out, "Library", &lib.name);
    kv(&mut out, "Text", &ccsid::decode_trimmed(&r.header.text));
    kv(&mut out, "Status", &r.attr.status.name());
    kv(&mut out, "Journal", &journal);
    kv(
        &mut out,
        "Attached",
        &if r.attr.attached == 0 {
            "*NONE".to_string()
        } else {
            stamp(r.attr.attached)
        },
    );
    kv(
        &mut out,
        "Detached",
        &if r.attr.detached == 0 {
            "*NONE".to_string()
        } else {
            stamp(r.attr.detached)
        },
    );
    kv(&mut out, "First sequence number", &r.attr.first_seq);
    kv(&mut out, "Last sequence number", &r.attr.last_seq);
    kv(&mut out, "Number of entries", &r.attr.entry_count);
    kv(&mut out, "Size (bytes)", &r.attr.bytes_used);
    kv(
        &mut out,
        "Threshold (KB)",
        &if r.attr.threshold_kb == 0 {
            "*NONE".to_string()
        } else {
            r.attr.threshold_kb.to_string()
        },
    );
    kv(&mut out, "Previous receiver", &prev);
    kv(&mut out, "Next receiver", &next);
    Ok(out)
}
