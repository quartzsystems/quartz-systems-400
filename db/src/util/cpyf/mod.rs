// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CPYF`: copy records from one member to another, selecting by RRN,
//! key, character or relation, mapping formats, creating the to-file.

mod map;
pub mod select;

pub use map::Plan;
pub use select::{CompareOp, Conj, IncChar, IncRel};

use qsname::{MemberSpec, Name};
use store::obj::{JobHandle, LockState, ObjectId};
use store::txn::Txn;

use super::{ErrLvl, Hold, Order, each_record, member_of};
use crate::error::{DbError, Result};
use crate::file::{self, CreateFileOpts, FileDesc, FileKind, MemberOpt};
use crate::format::FormatDef;
use crate::member::Member;
use crate::path;
use crate::record::Value;

/// `MBROPT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MbrOpt {
    /// `*NONE`: the to-member must be empty.
    #[default]
    None,
    /// `*ADD`.
    Add,
    /// `*REPLACE`: the to-member is cleared first.
    Replace,
    /// `*UPDADD`: a record whose key exists updates it.
    UpdAdd,
}

/// `FMTOPT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FmtOpt {
    /// `*NONE`: the formats must have the same level identifier.
    #[default]
    None,
    /// `*NOCHK`: bytes are copied, cut or blank padded.
    NoChk,
    /// `*MAP`: fields by name; to-fields the from-file lacks take
    /// their defaults.
    Map,
    /// `*DROP`: fields by name; from-fields the to-file lacks are dropped.
    Drop,
    /// `*MAP *DROP`.
    MapDrop,
    /// `*CVTSRC`: between a source member and a data member.
    CvtSrc,
}

/// `TOMBR`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToMember {
    /// `*FIRST`.
    #[default]
    First,
    /// `*FROMMBR`: the from-member's name.
    FromMbr,
    /// A name; the member is added when missing.
    Named(Name),
}

/// The to-file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// An existing file.
    File(ObjectId),
    /// No such file: created in `library` under `CRTFILE(*YES)`.
    Create {
        /// The library.
        library: ObjectId,
        /// The file name.
        name: Name,
    },
}

/// The parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpyfOpts {
    /// `FROMMBR`.
    pub from_member: MemberSpec,
    /// `TOMBR`.
    pub to_member: ToMember,
    /// `MBROPT`.
    pub mbropt: MbrOpt,
    /// `CRTFILE(*YES)`.
    pub crtfile: bool,
    /// `FMTOPT`.
    pub fmtopt: FmtOpt,
    /// `FROMRCD`, `None` for `*START`.
    pub fromrcd: Option<u64>,
    /// `TORCD`, `None` for `*END`.
    pub torcd: Option<u64>,
    /// `FROMKEY` values, `None` for `*START`.
    pub fromkey: Option<Vec<Value>>,
    /// `TOKEY` values, `None` for `*END`.
    pub tokey: Option<Vec<Value>>,
    /// `NBRRCDS`, `None` for `*END`.
    pub nbrrcds: Option<u64>,
    /// `INCCHAR`.
    pub incchar: Option<IncChar>,
    /// `INCREL`.
    pub increl: Vec<IncRel>,
    /// `ERRLVL`.
    pub errlvl: ErrLvl,
    /// Creation time for a created file or added member.
    pub now: u64,
}

impl Default for CpyfOpts {
    fn default() -> Self {
        Self {
            from_member: MemberSpec::First,
            to_member: ToMember::First,
            mbropt: MbrOpt::None,
            crtfile: false,
            fmtopt: FmtOpt::None,
            fromrcd: None,
            torcd: None,
            fromkey: None,
            tokey: None,
            nbrrcds: None,
            incchar: None,
            increl: Vec::new(),
            errlvl: ErrLvl::None,
            now: 0,
        }
    }
}

/// What a copy did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CpyfReport {
    /// Records copied.
    pub copied: u64,
    /// Records the selection left out.
    pub excluded: u64,
    /// Records not copied for a recoverable error.
    pub errors: u64,
    /// The file created, if any.
    pub created: Option<ObjectId>,
    /// The to-member written.
    pub to_member: Name,
}

/// Copy records from `from` into `to`.
pub fn cpyf(
    txn: &mut Txn<'_>,
    job: JobHandle,
    from: ObjectId,
    to: &Target,
    opts: &CpyfOpts,
) -> Result<CpyfReport> {
    if opts.fromkey.is_some() && opts.fromrcd.is_some()
        || opts.tokey.is_some() && opts.torcd.is_some()
    {
        return Err(DbError::Copy {
            what: "FROMKEY/TOKEY and FROMRCD/TORCD together".into(),
        });
    }
    let from_desc = file::describe(txn, from)?;
    let from_entry = member_of(&from_desc, opts.from_member)?;
    let mut from_m = Member::open(txn, &from_desc, &from_entry)?;
    let (to_id, created) = match to {
        Target::File(id) => (*id, None),
        Target::Create { library, name } => {
            if !opts.crtfile {
                return Err(DbError::Copy {
                    what: format!("to-file {name} does not exist and CRTFILE(*NO)"),
                });
            }
            let mbr = match opts.to_member {
                ToMember::First | ToMember::FromMbr => from_entry.name,
                ToMember::Named(n) => n,
            };
            let id = create_like(
                txn,
                &from_desc,
                from_m.format(),
                *library,
                *name,
                mbr,
                opts.now,
            )?;
            (id, Some(id))
        }
    };
    let mut to_desc = file::describe(txn, to_id)?;
    if to_desc.attr.kind == FileKind::Logical {
        return Err(DbError::Copy {
            what: format!("to-file {} is a logical file", to_desc.name()),
        });
    }
    let to_name = match opts.to_member {
        ToMember::First => to_desc.members.first().map_or(from_entry.name, |m| m.name),
        ToMember::FromMbr => from_entry.name,
        ToMember::Named(n) => n,
    };
    let to_entry = if let Some(e) = to_desc.member(MemberSpec::Name(to_name)) {
        *e
    } else {
        file::add_member(txn, to_id, to_name, "", None, opts.now)?;
        to_desc = file::describe(txn, to_id)?;
        member_of(&to_desc, MemberSpec::Name(to_name))?
    };
    if to_entry.object == from_entry.object {
        return Err(DbError::Copy {
            what: "from-member and to-member are the same".into(),
        });
    }
    let to_state = if opts.mbropt == MbrOpt::Replace {
        LockState::Exclusive
    } else {
        LockState::SharedUpdate
    };
    let hold = Hold::take(
        txn,
        job,
        &[
            (from, LockState::SharedRead),
            (from_entry.object, LockState::SharedRead),
            (to_id, LockState::SharedUpdate),
            (to_entry.object, to_state),
        ],
        &to_desc.name().to_string(),
    )?;
    let result = Member::open(txn, &to_desc, &to_entry)
        .and_then(|mut to_m| run(txn, &mut from_m, &mut to_m, opts));
    hold.release(txn);
    let (copied, excluded, errors) = result?;
    Ok(CpyfReport {
        copied,
        excluded,
        errors,
        created,
        to_member: to_name,
    })
}

/// Create the to-file after the from-file: its format, kind and
/// attributes, with one member.
fn create_like(
    txn: &mut Txn<'_>,
    from: &FileDesc,
    format: &FormatDef,
    library: ObjectId,
    name: Name,
    member: Name,
    now: u64,
) -> Result<ObjectId> {
    let a = &from.attr;
    let opts = CreateFileOpts {
        text: ccsid::decode_trimmed(&from.header.text),
        owner: ccsid::decode_trimmed(&from.header.owner),
        now,
        member: MemberOpt::Named(member),
        size: a.size,
        reusedlt: a.reusedlt,
        max_members: a.max_members,
        share: a.share,
        lvlchk: a.lvlchk,
        maint: a.maint,
        recover: a.recover,
        frcratio: a.frcratio,
        wait_file: a.wait_file,
        wait_rcd: a.wait_rcd,
        ccsid: a.ccsid,
        allocate: a.allocate,
        alwupd: a.alwupd,
        alwdlt: a.alwdlt,
        frcaccpth: a.frcaccpth,
        ..CreateFileOpts::default()
    };
    if a.kind == FileKind::Source {
        return file::create_srcpf(txn, library, name, format.format.record_len, &opts);
    }
    let mut rf = format.format.clone();
    rf.from = None;
    let def = dds::PfDefinition {
        format: rf,
        unique: format.unique,
        dup_order: format.dup_order,
        reference: None,
        refaccpth: None,
        altseq: None,
        ccsid: None,
        text: None,
    };
    file::create_pf(txn, library, name, &def, &opts)
}

/// The copy itself: (copied, excluded, errors).
fn run(
    txn: &mut Txn<'_>,
    from_m: &mut Member,
    to_m: &mut Member,
    opts: &CpyfOpts,
) -> Result<(u64, u64, u64)> {
    match opts.mbropt {
        MbrOpt::None if to_m.attr().record_count > 0 => {
            return Err(DbError::Copy {
                what: format!("MBROPT(*NONE) and member {} has records", to_m.name()),
            });
        }
        MbrOpt::Replace => to_m.clear(txn)?,
        _ => {}
    }
    let from_fmt = from_m.format().clone();
    let to_fmt = to_m.format().clone();
    let plan = map::plan(
        &from_fmt,
        &to_fmt,
        from_m.file.attr.kind,
        to_m.file.attr.kind,
        opts.fmtopt,
    )?;
    let order = order_of(&from_fmt, opts)?;
    if let Some(ic) = &opts.incchar {
        ic.check(&from_fmt)?;
    }
    for r in &opts.increl {
        r.check(&from_fmt)?;
    }
    let mut copied = 0u64;
    let mut excluded = 0u64;
    let mut errors = 0u64;
    let mut seq = 0u64;
    let updadd = opts.mbropt == MbrOpt::UpdAdd && to_fmt.unique.is_some();
    let mut record = vec![0u8; usize::try_from(to_fmt.format.record_len).expect("fits")];
    let mut nulls = vec![0u8; usize::from(to_fmt.null_map_len)];
    each_record(txn, from_m, &order, &mut |txn, slot| {
        if !select::passes(&from_fmt, &slot, opts.incchar.as_ref(), &opts.increl)? {
            excluded += 1;
            return Ok(true);
        }
        seq += 1;
        let outcome = plan
            .apply(&from_fmt, &to_fmt, &slot, seq, &mut record, &mut nulls)
            .and_then(|()| put(txn, to_m, &to_fmt, &record, &nulls, updadd));
        match outcome {
            Ok(()) => copied += 1,
            Err(e) if recoverable(&e) => {
                errors += 1;
                if !opts.errlvl.allows(errors) {
                    return Err(DbError::Copy {
                        what: format!("record {} of {}: {e}", slot.rrn, from_m_name(&from_fmt)),
                    });
                }
            }
            Err(e) => return Err(e),
        }
        Ok(opts.nbrrcds.is_none_or(|n| copied < n))
    })?;
    Ok((copied, excluded, errors))
}

fn from_m_name(f: &FormatDef) -> Name {
    f.format.name
}

/// Write one converted record, updating the record with its key under
/// `*UPDADD`.
fn put(
    txn: &mut Txn<'_>,
    to_m: &mut Member,
    to_fmt: &FormatDef,
    record: &[u8],
    nulls: &[u8],
    updadd: bool,
) -> Result<()> {
    if updadd {
        let key = path::record_key(to_fmt, record, nulls)?;
        to_m.ensure_path(txn, &mut |_, _| {})?;
        let hit = to_m
            .keys(
                txn,
                std::ops::Bound::Included(key.as_slice()),
                std::ops::Bound::Included(key.as_slice()),
                1,
            )?
            .into_iter()
            .next();
        if let Some(k) = hit {
            return to_m.update(txn, k.rrn, record, nulls);
        }
    }
    to_m.write_new(txn, record, nulls).map(|_| ())
}

/// Errors `ERRLVL` counts rather than stops on.
fn recoverable(e: &DbError) -> bool {
    matches!(
        e,
        DbError::DataMapping { .. } | DbError::DuplicateKey { .. } | DbError::RecordLength { .. }
    )
}

/// The records to visit and their order.
fn order_of(from_fmt: &FormatDef, opts: &CpyfOpts) -> Result<Order> {
    if opts.fromkey.is_none() && opts.tokey.is_none() {
        return Ok(Order {
            keyed: false,
            from_rrn: opts.fromrcd.unwrap_or(0),
            to_rrn: opts.torcd,
            from_key: None,
            to_key: None,
        });
    }
    if from_fmt.key_len == 0 {
        return Err(DbError::Copy {
            what: "FROMKEY/TOKEY on a member in arrival sequence".into(),
        });
    }
    let prefix = |v: &Option<Vec<Value>>| -> Result<Option<Vec<u8>>> {
        v.as_ref()
            .map(|vals| path::values_key(from_fmt, vals))
            .transpose()
    };
    Ok(Order {
        keyed: true,
        from_rrn: 0,
        to_rrn: None,
        from_key: prefix(&opts.fromkey)?,
        to_key: prefix(&opts.tokey)?,
    })
}
