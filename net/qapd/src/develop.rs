// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Develop` service (version 8, draft; docs/QAP.md "Service 9
//! `Develop`"): the library list and `Compile` in the lane ([`lane`]),
//! everything else on a secondary thread ([`serve`]). This module
//! decodes, pages, checks a `WriteMember`'s sequence numbers and keeps
//! its pieces until the last ([`Pieces`]); the host does what the
//! commands do. [`dated`] is the date rule both hosts apply.

use std::collections::HashMap;
use std::sync::Mutex;

use ipc::Wire;
use qap_proto::ops::command::CommandText;
use qap_proto::ops::develop::{
    ChangeMember, CompileReply, CopyMember, CreateMember, DescribeObject, Libraries, ListLibraries,
    ListMembers, ListObjects, MemberRef, Members, Objects, ReadMember, RenameMember,
    SetLibraryList, WriteMember, Written,
};
use qap_proto::types::{Diagnostic, MemberName, SourceRecord};
use qap_proto::{Envelope, Op, Status};

use crate::conn::{Answer, lock};
use crate::host::{Host, Ran, Refusal, ThreadHost};
use crate::sql::page;

/// The message of a `Develop` or `Jobs` operation a host does not
/// offer: `&1` is not available through this QAP server.
pub const NOT_AVAILABLE: &str = "TCP1A60";
/// `UnlockMember` of a member the connection does not hold.
pub const NOT_LOCKED: &str = "TCP1A61";
/// A record longer than the file's data length.
pub const TOO_LONG: &str = "TCP1A62";
/// Data that does not convert to the file's CCSID (version 5's).
pub const NO_CONVERSION: &str = "TCP1A5A";
/// Another job holds the member: cannot allocate it.
pub const HELD: &str = "CPF1002";

/// The highest sequence number: `9999.99`.
pub const MAX_SEQ: u32 = 999_999;
/// The most records `renumber` numbers in `0001.00` steps.
pub const MAX_RENUMBERED: usize = 9999;

/// What a compile left: the command's run, and the diagnostics the
/// compiler reported for it (`rpg/compdiag`), as the listing shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compiled {
    /// How the command ended, its job and messages.
    pub ran: Ran,
    /// The compiler's diagnostics; empty for a command that compiles
    /// nothing.
    pub diagnostics: Vec<Diagnostic>,
}

/// Whether `name` matches the generic name `pattern`: a name, a name
/// ending in `*`, or `*ALL`; empty is `*ALL`. Without case.
#[must_use]
pub fn generic(name: &str, pattern: &str) -> bool {
    let p = pattern.trim().to_ascii_uppercase();
    let n = name.trim().to_ascii_uppercase();
    if p.is_empty() || p == "*ALL" {
        return true;
    }
    match p.strip_suffix('*') {
        Some(prefix) => n.starts_with(prefix),
        None => n == p,
    }
}

/// The date `unix` seconds fall on, UTC, as a source record's date:
/// `yymmdd`.
#[must_use]
pub fn yymmdd(unix: u64) -> u32 {
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = i64::try_from(unix / 86_400).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    u32::try_from((y % 100) * 10_000 + m * 100 + d).unwrap_or(0)
}

/// A member name as the server keys it: upper case, trimmed.
fn key(m: &MemberName) -> (String, String, String) {
    let up = |s: &str| s.trim().to_ascii_uppercase();
    (up(&m.file.library), up(&m.file.name), up(&m.member))
}

/// One member's write in progress: the pieces so far.
#[derive(Debug, Default)]
struct Staged {
    records: Vec<SourceRecord>,
}

/// The connection's `WriteMember`s in progress, by member.
#[derive(Debug, Default)]
pub(crate) struct Pieces(Mutex<HashMap<(String, String, String), Staged>>);

/// Number `records` from `0001.00` in `0001.00` steps.
fn renumbered(records: &mut [SourceRecord]) {
    for (r, n) in records.iter_mut().zip(1u32..) {
        r.seq = n * 100;
    }
}

/// The records a `WriteMember` writes: each whose data equals the data
/// of `old`'s record with the same sequence number (as sent, before
/// renumbering) keeps the date sent, every other is dated `today`
/// (`yymmdd`); then renumbered when asked.
#[must_use]
pub fn dated(
    old: &[SourceRecord],
    mut new: Vec<SourceRecord>,
    renumber: bool,
    today: u32,
) -> Vec<SourceRecord> {
    let was: HashMap<u32, &str> = old.iter().map(|r| (r.seq, r.data.as_str())).collect();
    for r in &mut new {
        let same = was
            .get(&r.seq)
            .is_some_and(|d| d.trim_end() == r.data.trim_end());
        if !same {
            r.date = today;
        }
    }
    if renumber {
        renumbered(&mut new);
    }
    new
}

/// Whether `records` may be written: ascending sequence numbers from 1
/// to [`MAX_SEQ`], or at most [`MAX_RENUMBERED`] when renumbered.
fn valid(records: &[SourceRecord], renumber: bool) -> bool {
    if renumber {
        return records.len() <= MAX_RENUMBERED;
    }
    records.iter().all(|r| (1..=MAX_SEQ).contains(&r.seq))
        && records.windows(2).all(|w| w[0].seq < w[1].seq)
}

fn body<B: Wire>(req: &Envelope, f: impl FnOnce(B) -> Answer) -> Answer {
    match req.body_as::<B>() {
        Ok(b) => f(b),
        Err(_) => Answer::status(Status::InvalidRequest),
    }
}

fn ok(r: Result<impl Wire, Refusal>) -> Answer {
    match r {
        Ok(b) => Answer::ok(&b),
        Err(r) => Answer::refused(r),
    }
}

fn done(r: Result<(), Refusal>) -> Answer {
    match r {
        Ok(()) => Answer::status(Status::Ok),
        Err(r) => Answer::refused(r),
    }
}

/// Run `GetLibraryList`, `SetLibraryList` or `Compile` in the lane.
pub(crate) fn lane<H: Host + ?Sized>(
    host: &mut H,
    op: Op,
    req: &Envelope,
    cancelled: &dyn Fn() -> bool,
) -> Answer {
    match op {
        Op::GetLibraryList => ok(host.library_list()),
        Op::SetLibraryList => body(req, |b: SetLibraryList| ok(host.set_library_list(&b))),
        Op::Compile => body(req, |b: CommandText| {
            match host.compile(&b.command, cancelled) {
                Ok(c) => Answer {
                    status: c.ran.status,
                    message: c.ran.escape,
                    body: ipc::encode(&CompileReply {
                        job: c.ran.reply.job,
                        messages: c.ran.reply.messages,
                        diagnostics: c.diagnostics,
                    }),
                },
                Err(r) => Answer::refused(r),
            }
        }),
        _ => Answer::status(Status::InvalidRequest),
    }
}

/// Answer `Develop` 3 to 14 on a secondary thread; `pieces` are the
/// connection's writes in progress.
pub(crate) fn serve(host: &mut dyn ThreadHost, pieces: &Pieces, op: Op, req: &Envelope) -> Answer {
    match op {
        Op::ListLibraries => body(req, |b: ListLibraries| {
            ok(host.list_libraries(&b).map(|all| {
                let (total, libraries) = page(all, b.start, b.max);
                Libraries { total, libraries }
            }))
        }),
        Op::ListObjects => body(req, |b: ListObjects| {
            ok(host.list_objects(&b).map(|all| {
                let (total, objects) = page(all, b.start, b.max);
                Objects { total, objects }
            }))
        }),
        Op::DescribeObject => body(req, |b: DescribeObject| ok(host.describe_object(&b))),
        Op::ListMembers => body(req, |b: ListMembers| {
            ok(host.list_members(&b).map(|all| {
                let (total, members) = page(all, b.start, b.max);
                Members { total, members }
            }))
        }),
        Op::ReadMember => body(req, |b: ReadMember| ok(host.read_member(&b.member, b.lock))),
        Op::WriteMember => body(req, |b: WriteMember| write(host, pieces, b)),
        Op::UnlockMember => body(req, |b: MemberRef| done(host.unlock_member(&b.member))),
        Op::CreateMember => body(req, |b: CreateMember| done(host.create_member(&b))),
        Op::CopyMember => body(req, |b: CopyMember| done(host.copy_member(&b))),
        Op::RenameMember => body(req, |b: RenameMember| done(host.rename_member(&b))),
        Op::DeleteMember => body(req, |b: MemberRef| done(host.delete_member(&b.member))),
        Op::ChangeMember => body(req, |b: ChangeMember| done(host.change_member(&b))),
        _ => Answer::status(Status::InvalidRequest),
    }
}

/// One piece of a `WriteMember`: kept, or with the last the member
/// written.
fn write(host: &mut dyn ThreadHost, pieces: &Pieces, b: WriteMember) -> Answer {
    let k = key(&b.member);
    let records = {
        let mut all = lock(&pieces.0);
        if b.first {
            all.remove(&k);
        } else if !all.contains_key(&k) {
            return Answer::status(Status::InvalidRequest);
        }
        let staged = all.entry(k.clone()).or_default();
        staged.records.extend(b.records);
        if !b.last {
            let n = u32::try_from(staged.records.len()).unwrap_or(u32::MAX);
            return Answer::ok(&Written { records: n });
        }
        all.remove(&k).map(|s| s.records).unwrap_or_default()
    };
    if !valid(&records, b.renumber) {
        return Answer::status(Status::InvalidRequest);
    }
    ok(host
        .write_member(
            &b.member,
            records,
            b.renumber,
            b.srctype.as_deref(),
            b.text.as_deref(),
        )
        .map(|records| Written { records }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(seq: u32, date: u32, text: &str) -> SourceRecord {
        SourceRecord {
            seq,
            date,
            data: text.into(),
        }
    }

    #[test]
    fn dates_of_days() {
        assert_eq!(yymmdd(0), 700_101);
        assert_eq!(yymmdd(1_793_000_000), 261_026);
        assert_eq!(yymmdd(951_782_400), 229);
    }

    #[test]
    fn generic_names() {
        assert!(generic("ASSETS", "ASSET*"));
        assert!(generic("ASSETS", "*ALL"));
        assert!(generic("ASSETS", ""));
        assert!(generic("assets", "ASSETS"));
        assert!(!generic("ASSETS", "ASSET"));
        assert!(!generic("STOCK", "A*"));
    }

    #[test]
    fn dates_kept_for_the_same_data_only() {
        let old = vec![rec(100, 260_101, "a"), rec(200, 260_102, "b")];
        let new = vec![
            rec(100, 260_101, "a"),
            rec(150, 0, "new"),
            rec(200, 260_102, "B"),
        ];
        let d = dated(&old, new, false, 261_107);
        assert_eq!(d[0].date, 260_101);
        assert_eq!(d[1].date, 261_107);
        assert_eq!(d[2].date, 261_107);
        let d = dated(
            &old,
            vec![rec(200, 260_102, "b"), rec(100, 260_101, "a")],
            true,
            261_107,
        );
        assert_eq!((d[0].seq, d[0].date), (100, 260_102));
        assert_eq!((d[1].seq, d[1].date), (200, 260_101));
    }

    #[test]
    fn sequence_numbers_checked() {
        assert!(valid(&[rec(100, 0, ""), rec(200, 0, "")], false));
        assert!(!valid(&[rec(200, 0, ""), rec(100, 0, "")], false));
        assert!(!valid(&[rec(100, 0, ""), rec(100, 0, "")], false));
        assert!(!valid(&[rec(0, 0, "")], false));
        assert!(!valid(&[rec(1_000_000, 0, "")], false));
        assert!(valid(&[rec(200, 0, ""), rec(100, 0, "")], true));
        assert!(!valid(&vec![rec(1, 0, ""); 10_000], true));
    }
}
