// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database utilities as library functions: `CLRPFM`, `RGZPFM`,
//! `CPYF`, `CPYTOIMPF` and `CPYFRMIMPF`. Each allocates what it touches
//! as the command would and runs in the caller's transaction.

pub mod chkpath;
pub mod clrpfm;
pub mod cpyf;
pub mod impf;
pub mod lines;
pub mod rgzpfm;

pub use chkpath::{PathReport, verify_all, verify_file};
pub use clrpfm::clrpfm;
pub use cpyf::{CpyfOpts, CpyfReport, FmtOpt, MbrOpt, Target, ToMember, cpyf};
pub use impf::{DtaFmt, ImpfOpts, ImpfReport, RcdDlm, RmvBlank, cpyfrmimpf, cpytoimpf};
pub use lines::put_lines;
pub use rgzpfm::{KeyFile, RgzReport, rgzpfm};

use std::ops::Bound;

use qsname::MemberSpec;
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::file::{FileDesc, FileKind, MemberEntry};
use crate::member::{Core, Member, Slot};
use crate::odp;
use crate::path::PathKey;

/// `ERRLVL`: recoverable errors a copy tolerates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ErrLvl {
    /// At most this many.
    #[default]
    None,
    /// At most `n`.
    Count(u64),
    /// `*NOMAX`.
    NoMax,
}

impl ErrLvl {
    /// Whether `errors` so far is still within the level.
    #[must_use]
    pub fn allows(self, errors: u64) -> bool {
        match self {
            Self::None => errors == 0,
            Self::Count(n) => errors <= n,
            Self::NoMax => true,
        }
    }
}

/// The member a `MBR` parameter names, or the not-found error.
pub fn member_of(desc: &FileDesc, spec: MemberSpec) -> Result<MemberEntry> {
    desc.member(spec)
        .copied()
        .ok_or_else(|| DbError::MemberNotFound {
            file: desc.name().to_string(),
            member: spec.to_string(),
        })
}

/// A physical file, or the error a utility reports for a logical one.
pub fn physical(desc: &FileDesc, command: &str) -> Result<()> {
    if desc.attr.kind == FileKind::Logical {
        return Err(DbError::Copy {
            what: format!("{command}: {} is a logical file", desc.name()),
        });
    }
    Ok(())
}

/// Objects a utility holds while it runs; released in reverse order.
pub struct Hold {
    job: JobHandle,
    held: Vec<(ObjectId, LockState)>,
}

impl Hold {
    /// Allocate every object in order; on failure nothing stays held.
    pub fn take(
        txn: &mut Txn<'_>,
        job: JobHandle,
        wanted: &[(ObjectId, LockState)],
        file: &str,
    ) -> Result<Self> {
        let mut hold = Self {
            job,
            held: Vec::new(),
        };
        for &(id, state) in wanted {
            if let Err(e) = odp::allocate(txn, job, id, state, file) {
                hold.release(txn);
                return Err(e);
            }
            hold.held.push((id, state));
        }
        Ok(hold)
    }

    /// Release everything held.
    pub fn release(mut self, txn: &mut Txn<'_>) {
        while let Some((id, state)) = self.held.pop() {
            odp::deallocate(txn, self.job, id, state);
        }
    }
}

/// Which records of a member a utility visits, and in what order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Order {
    /// Key order over the member's path; else arrival sequence.
    pub keyed: bool,
    /// First RRN (arrival), 0 for the start.
    pub from_rrn: u64,
    /// Last RRN (arrival), `None` for the end.
    pub to_rrn: Option<u64>,
    /// First key prefix (keyed).
    pub from_key: Option<Vec<u8>>,
    /// Last key prefix (keyed): entries past every key with it stop.
    pub to_key: Option<Vec<u8>>,
}

const BATCH: usize = 256;

/// Visit the active records of `m` in `order`; `f` returns `false` to
/// stop. A logical member yields its derived records.
pub fn each_record(
    txn: &mut Txn<'_>,
    m: &mut Member,
    order: &Order,
    f: &mut dyn FnMut(&mut Txn<'_>, Slot) -> Result<bool>,
) -> Result<()> {
    if order.keyed {
        return each_keyed(txn, m, order, f);
    }
    let slots = if m.is_logical() {
        Core::load(txn, m.bases[0].member)?.attr.slots
    } else {
        m.attr().slots
    };
    let hi = order.to_rrn.map_or(slots, |t| t.min(slots));
    for rrn in order.from_rrn.max(1)..=hi {
        let slots = if m.is_logical() {
            m.read_entries(
                txn,
                &PathKey {
                    key: Vec::new(),
                    format: 0,
                    rrn,
                },
            )?
        } else {
            m.read(txn, rrn)?
                .filter(|s| !s.deleted)
                .into_iter()
                .collect()
        };
        for s in slots {
            if !f(txn, s)? {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn each_keyed(
    txn: &mut Txn<'_>,
    m: &mut Member,
    order: &Order,
    f: &mut dyn FnMut(&mut Txn<'_>, Slot) -> Result<bool>,
) -> Result<()> {
    m.ensure_path(txn, &mut |_, _| {})?;
    let p = m.path().ok_or_else(|| DbError::Copy {
        what: format!("member {} is in arrival sequence", m.name()),
    })?;
    let lo = order
        .from_key
        .as_ref()
        .map_or(Bound::Unbounded, |k| Bound::Included(k.as_slice()));
    let mut batch = p.scan(txn, lo, Bound::Unbounded, BATCH)?;
    while let Some(last) = batch.last().cloned() {
        for k in &batch {
            if let Some(to) = &order.to_key
                && k.key.as_slice() > to.as_slice()
                && !k.key.starts_with(to)
            {
                return Ok(());
            }
            for s in m.read_entries(txn, k)? {
                if !f(txn, s)? {
                    return Ok(());
                }
            }
        }
        batch = p.scan_from(txn, Bound::Excluded(&last), BATCH)?;
    }
    Ok(())
}
