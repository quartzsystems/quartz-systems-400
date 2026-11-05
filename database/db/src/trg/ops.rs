// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Adding, removing and changing triggers: the library functions behind
//! `ADDPFTRG`, `RMVPFTRG`, `CHGPFTRG` and SQL's `CREATE TRIGGER` and
//! `DROP TRIGGER`. The rules and numbering are the primary's
//! (`tools/conformance/sql/oracle/trigger/primary-facts.md`). The caller
//! holds the file exclusively.

use qsname::Name;
use store::obj::ObjectId;
use store::txn::Txn;

use super::model::{Event, MtAction, Threadsafe, Time, Trigger, UpdateCondition};
use super::{MAX_TRIGGERS, list, store};
use crate::error::{DbError, Result};
use crate::file::{self, FileKind};

/// Why a trigger operation failed; the escape is `CPF32C6` with the
/// operation code, after the diagnostic each names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Not a physical file (`CPF3210`).
    NotPhysical,
    /// The trigger name is used on the file (`CPD32E7`).
    Exists,
    /// No such trigger (`CPD321B`).
    NotFound,
    /// The file has [`MAX_TRIGGERS`] already (`CPD32E7`).
    TooMany,
    /// No trigger at the time and event given (`CPD32E7`).
    NoneThere,
}

impl Refusal {
    /// The diagnostic.
    #[must_use]
    pub fn msgid(self) -> &'static str {
        match self {
            Self::NotPhysical => "CPF3210",
            Self::Exists | Self::TooMany | Self::NoneThere => "CPD32E7",
            Self::NotFound => "CPD321B",
        }
    }
}

/// The operation codes `CPF32C6` names.
pub mod op {
    /// Add a trigger.
    pub const ADD: u8 = 1;
    /// Remove a trigger.
    pub const REMOVE: u8 = 2;
    /// Change a trigger.
    pub const CHANGE: u8 = 3;
}

fn refuse(op: u8, reason: Refusal, what: impl Into<String>) -> DbError {
    DbError::Trigger { op, reason, what: what.into() }
}

/// A trigger to add.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct NewTrg {
    /// `TRG`; `None` for `*GEN`.
    pub name: Option<String>,
    /// `TRGLIB`; `None` for `*FILE`.
    pub library: Option<Name>,
    /// `TRGTIME`.
    pub time: Time,
    /// `TRGEVENT`.
    pub event: Event,
    /// `PGM`: library, name.
    pub program: (Name, Name),
    /// `ALWREPCHG(*YES)`.
    pub alwrepchg: bool,
    /// `TRGUPDCND(*CHANGE)` (an update trigger's).
    pub on_change: bool,
    /// `RPLTRG(*YES)`.
    pub replace: bool,
    /// `THDSAFE`.
    pub threadsafe: Threadsafe,
    /// `MLTTHDACN`.
    pub mt_action: MtAction,
    /// Who adds it.
    pub definer: String,
}

impl NewTrg {
    /// `ADDPFTRG`'s defaults for a trigger calling `program`.
    #[must_use]
    pub fn new(time: Time, event: Event, program: (Name, Name)) -> Self {
        Self {
            name: None,
            library: None,
            time,
            event,
            program,
            alwrepchg: false,
            on_change: false,
            replace: false,
            threadsafe: Threadsafe::Unknown,
            mt_action: MtAction::Sysval,
            definer: String::new(),
        }
    }
}

/// IBM's name for an unnamed trigger: `QSYS_TRIG_`, the library and the
/// file each padded with `_` to ten, `_` after each, and six digits, the
/// first free.
#[must_use]
pub fn generated_name(lib: &str, file: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let pad = |s: &str| format!("{s:_<10}");
    let stem = format!("QSYS_TRIG_{}_{}_", pad(lib), pad(file));
    (1..=999_999u32)
        .map(|n| format!("{stem}{n:06}"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| format!("{stem}999999"))
}

fn physical(txn: &mut Txn<'_>, file: ObjectId, op: u8) -> Result<file::FileDesc> {
    let desc = file::describe(txn, file)?;
    if desc.attr.kind != FileKind::Physical {
        return Err(refuse(op, Refusal::NotPhysical, format!("file {} in library {} is not a physical data file", desc.name(), desc.library_name)));
    }
    Ok(desc)
}

/// Add a trigger; its name. The program's existence is the caller's to
/// check (`CPF9811`).
pub fn add(txn: &mut Txn<'_>, file: ObjectId, new: &NewTrg, now: u64) -> Result<String> {
    let desc = physical(txn, file, op::ADD)?;
    let mut trgs = list(txn, file)?;
    let library = new.library.unwrap_or(desc.library_name);
    let name = match &new.name {
        Some(n) => n.clone(),
        None => generated_name(
            &desc.library_name.to_string(),
            &desc.name().to_string(),
            &|n| trgs.iter().any(|t| t.name == n),
        ),
    };
    let next = trgs.iter().map(|t| t.number.max(t.issued)).max().unwrap_or(0) + 1;
    let at = trgs.iter().position(|t| t.name == name && t.library == library);
    if at.is_some() && !new.replace {
        return Err(refuse(op::ADD, Refusal::Exists, format!("trigger {name} in library {library} is already on file {}", desc.name())));
    }
    if at.is_none() && trgs.len() >= MAX_TRIGGERS {
        return Err(refuse(op::ADD, Refusal::TooMany, format!("file {} has {MAX_TRIGGERS} triggers", desc.name())));
    }
    let mut t = Trigger::new(&name, library, new.time, new.event, new.program);
    t.alwrepchg = new.alwrepchg;
    if new.event == Event::Update && new.on_change {
        t.update_condition = UpdateCondition::Change;
    }
    t.threadsafe = new.threadsafe;
    t.mt_action = new.mt_action;
    t.definer.clone_from(&new.definer);
    t.created = now;
    t.issued = next;
    // A replaced trigger keeps its place and number.
    if let Some(i) = at {
        t.number = trgs[i].number;
        trgs[i] = t;
    } else {
        t.number = next;
        trgs.push(t);
    }
    store(txn, file, &trgs, now)?;
    Ok(name)
}

/// Which triggers an operation names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// One by name and library (`None` for any library).
    Named(String, Option<Name>),
    /// Every one at `time` (any time when `None`) for `event` (any when `None`).
    When(Option<Time>, Option<Event>),
}

impl Pick {
    fn takes(&self, t: &Trigger) -> bool {
        match self {
            Self::Named(n, lib) => t.name == *n && lib.is_none_or(|l| l == t.library),
            Self::When(time, event) => time.is_none_or(|x| x == t.time) && event.is_none_or(|e| e == t.event),
        }
    }
}

/// Remove the triggers `pick` names; their names. A name not on the file,
/// or a time and event with none, is refused.
pub fn remove(txn: &mut Txn<'_>, file: ObjectId, pick: &Pick, now: u64) -> Result<Vec<String>> {
    let desc = physical(txn, file, op::REMOVE)?;
    let trgs = list(txn, file)?;
    let (gone, kept): (Vec<Trigger>, Vec<Trigger>) = trgs.into_iter().partition(|t| pick.takes(t));
    if gone.is_empty() {
        return Err(match pick {
            Pick::Named(n, _) => refuse(op::REMOVE, Refusal::NotFound, format!("trigger {n} is not on file {}", desc.name())),
            Pick::When(..) => refuse(op::REMOVE, Refusal::NoneThere, format!("file {} has no trigger at that time and event", desc.name())),
        });
    }
    store(txn, file, &kept, now)?;
    Ok(gone.into_iter().map(|t| t.name).collect())
}

/// `CHGPFTRG STATE`: enable or disable the triggers `pick` names; how
/// many.
pub fn set_state(txn: &mut Txn<'_>, file: ObjectId, pick: &Pick, enabled: bool, now: u64) -> Result<usize> {
    let desc = physical(txn, file, op::CHANGE)?;
    let mut trgs = list(txn, file)?;
    let mut n = 0;
    for t in trgs.iter_mut().filter(|t| pick.takes(t)) {
        t.enabled = enabled;
        n += 1;
    }
    if n == 0 {
        if let Pick::Named(name, _) = pick {
            return Err(refuse(op::CHANGE, Refusal::NotFound, format!("trigger {name} is not on file {}", desc.name())));
        }
        return Ok(0);
    }
    store(txn, file, &trgs, now)?;
    Ok(n)
}
