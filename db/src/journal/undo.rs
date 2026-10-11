// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Replaying journal entries onto members, forward (`APYJRNCHG`) and
//! backward (`RMVJRNCHG`, rollback, recovery).

use std::collections::{HashMap, HashSet};

use jrn::entry::flags;
use jrn::{Entry, Filter, Journal, RecordImage, code, dspjrn};
use qsname::Name;
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

use super::JrnCtx;
use crate::error::{DbError, Result};
use crate::file::{self, FileDesc, MemberEntry};
use crate::member::Member;

/// Journaled members by (library, file, member) name.
pub type MemberMap = HashMap<(Name, Name, Name), (FileDesc, MemberEntry)>;

/// The members of the files journaled to `journal`, or of `only`.
pub fn resolve_members(
    txn: &mut Txn<'_>,
    journal: &Journal,
    only: Option<&[ObjectId]>,
) -> Result<MemberMap> {
    let mut map = HashMap::new();
    for o in journal.objects(txn)? {
        if only.is_some_and(|ids| !ids.contains(&o.object)) {
            continue;
        }
        let Ok(desc) = file::describe(txn, o.object) else {
            continue;
        };
        for m in &desc.members {
            map.insert((desc.library_name, desc.name(), m.name), (desc.clone(), *m));
        }
    }
    Ok(map)
}

/// What a replay did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeReport {
    /// Entries applied or undone.
    pub applied: u64,
    /// Entries that could not be (an unknown member, a record in the
    /// wrong state) or do not replay (`UP` on undo, `UB` on apply).
    pub skipped: u64,
    /// First and last sequence numbers seen.
    pub range: Option<(u64, u64)>,
}

impl ChangeReport {
    fn saw(&mut self, seq: u64) {
        self.range = Some(match self.range {
            None => (seq, seq),
            Some((a, b)) => (a.min(seq), b.max(seq)),
        });
    }
}

/// Members opened once per replay.
struct Cache {
    members: HashMap<ObjectId, Member>,
}

impl Cache {
    fn get(
        &mut self,
        txn: &mut Txn<'_>,
        map: &MemberMap,
        e: &Entry,
        ctx: JrnCtx,
    ) -> Result<Option<&mut Member>> {
        let Some(o) = &e.object else {
            return Ok(None);
        };
        let Some(m) = o.member else {
            return Ok(None);
        };
        let Some((desc, entry)) = map.get(&(o.library, o.object, m)) else {
            return Ok(None);
        };
        if let std::collections::hash_map::Entry::Vacant(v) = self.members.entry(entry.object) {
            let mut member = Member::open(txn, desc, entry)?;
            member.jrn = ctx;
            v.insert(member);
        }
        Ok(self.members.get_mut(&entry.object))
    }
}

fn image(e: &Entry) -> Result<RecordImage> {
    Ok(RecordImage::decode(&e.data)?)
}

/// Undo `entries` (given in journal order) newest first.
pub fn undo(
    txn: &mut Txn<'_>,
    entries: &[Entry],
    map: &MemberMap,
    ctx: JrnCtx,
) -> Result<ChangeReport> {
    let mut cache = Cache {
        members: HashMap::new(),
    };
    let mut report = ChangeReport::default();
    for e in entries.iter().rev() {
        if e.code != code::RECORD {
            continue;
        }
        report.saw(e.seq);
        let Some(m) = cache.get(txn, map, e, ctx)? else {
            report.skipped += 1;
            continue;
        };
        let kind = std::str::from_utf8(&e.kind).unwrap_or("");
        let done = match kind {
            "PT" | "PX" => match m.read(txn, e.count)? {
                Some(s) if !s.deleted => {
                    m.delete(txn, e.count)?;
                    true
                }
                _ => false,
            },
            "UB" | "BR" => {
                let img = image(e)?;
                match m.read(txn, e.count)? {
                    Some(s) if !s.deleted => {
                        m.update(txn, e.count, &img.record, &img.null_map)?;
                        true
                    }
                    _ => false,
                }
            }
            "DL" | "DR" => {
                let img = image(e)?;
                match m.read(txn, e.count)? {
                    Some(s) if !s.deleted => false,
                    _ => {
                        m.write_at(txn, e.count, &img.record, &img.null_map)?;
                        true
                    }
                }
            }
            _ => false,
        };
        if done {
            report.applied += 1;
        } else {
            report.skipped += 1;
        }
    }
    Ok(report)
}

/// Apply `entries` in journal order.
pub fn apply(
    txn: &mut Txn<'_>,
    entries: &[Entry],
    map: &MemberMap,
    ctx: JrnCtx,
) -> Result<ChangeReport> {
    let mut cache = Cache {
        members: HashMap::new(),
    };
    let mut report = ChangeReport::default();
    for e in entries {
        if e.code != code::RECORD && !e.is(code::FILE, "MC") {
            continue;
        }
        report.saw(e.seq);
        let Some(m) = cache.get(txn, map, e, ctx)? else {
            report.skipped += 1;
            continue;
        };
        let kind = std::str::from_utf8(&e.kind).unwrap_or("");
        let done = match (e.code, kind) {
            (code::FILE, "MC") => {
                m.clear(txn)?;
                true
            }
            (code::RECORD, "PT" | "PX") => {
                let img = image(e)?;
                match m.read(txn, e.count)? {
                    Some(s) if !s.deleted => false,
                    _ => {
                        m.write_at(txn, e.count, &img.record, &img.null_map)?;
                        true
                    }
                }
            }
            (code::RECORD, "UP" | "UR") => {
                let img = image(e)?;
                match m.read(txn, e.count)? {
                    Some(s) if !s.deleted => {
                        m.update(txn, e.count, &img.record, &img.null_map)?;
                        true
                    }
                    _ => false,
                }
            }
            (code::RECORD, "DL" | "DR") => match m.read(txn, e.count)? {
                Some(s) if !s.deleted => {
                    m.delete(txn, e.count)?;
                    true
                }
                _ => false,
            },
            _ => false,
        };
        if done {
            report.applied += 1;
        } else {
            report.skipped += 1;
        }
    }
    Ok(report)
}

/// `FROMENT`/`TOENT` or `FROMTIME`/`TOTIME`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JrnRange {
    /// First sequence number, `None` for the start.
    pub from_seq: Option<u64>,
    /// Last sequence number, `None` for the end.
    pub to_seq: Option<u64>,
    /// Earliest time.
    pub from_time: Option<u64>,
    /// Latest time.
    pub to_time: Option<u64>,
}

/// The entries in range; under `CMTBDY(*YES)` only those outside any
/// cycle or in a cycle that reached its commit or rollback in range.
fn select(
    txn: &mut Txn<'_>,
    journal: ObjectId,
    range: &JrnRange,
    cmtbdy: bool,
) -> Result<Vec<Entry>> {
    let f = Filter {
        from_seq: range.from_seq,
        to_seq: range.to_seq,
        from_time: range.from_time,
        to_time: range.to_time,
        ..Filter::default()
    };
    let mut entries = dspjrn(txn, journal, &f)?;
    if cmtbdy {
        let closed: HashSet<u64> = entries
            .iter()
            .filter(|e| e.is(code::COMMIT, "CM") || e.is(code::COMMIT, "RB"))
            .map(|e| e.commit_cycle)
            .collect();
        entries.retain(|e| e.commit_cycle == 0 || closed.contains(&e.commit_cycle));
    }
    // Entries written by an apply are not applied again.
    entries.retain(|e| e.flags & flags::APPLIED == 0);
    Ok(entries)
}

fn check_journal(txn: &mut Txn<'_>, journal: ObjectId) -> Result<Journal> {
    Journal::load(txn, journal).map_err(DbError::from)
}

/// `APYJRNCHG`: replay the changes to `files` (every journaled file when
/// `None`) forward over the range.
pub fn apyjrnchg(
    txn: &mut Txn<'_>,
    job: JobHandle,
    journal: ObjectId,
    files: Option<&[ObjectId]>,
    range: &JrnRange,
    cmtbdy: bool,
    now: u64,
) -> Result<ChangeReport> {
    let j = check_journal(txn, journal)?;
    let map = resolve_members(txn, &j, files)?;
    let entries = select(txn, journal, range, cmtbdy)?;
    let ctx = JrnCtx {
        job,
        commit_cycle: 0,
        now: Some(now),
        rollback: false,
        applied: true,
    };
    apply(txn, &entries, &map, ctx)
}

/// `RMVJRNCHG`: undo the changes to `files` backward over the range.
pub fn rmvjrnchg(
    txn: &mut Txn<'_>,
    job: JobHandle,
    journal: ObjectId,
    files: Option<&[ObjectId]>,
    range: &JrnRange,
    cmtbdy: bool,
    now: u64,
) -> Result<ChangeReport> {
    let j = check_journal(txn, journal)?;
    let map = resolve_members(txn, &j, files)?;
    let entries = select(txn, journal, range, cmtbdy)?;
    let ctx = JrnCtx {
        job,
        commit_cycle: 0,
        now: Some(now),
        rollback: true,
        applied: true,
    };
    undo(txn, &entries, &map, ctx)
}
