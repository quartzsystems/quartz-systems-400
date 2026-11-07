// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A trigger of a physical file and the body of its `*SQLINF` `TRIGGER`
//! entry (`docs/DB_FORMAT.md` §8.1, `SqlTrigger`).

use qsname::Name;

use crate::error::{DbError, Result};
use crate::spec::{Field, Layout, field};

/// Bytes of the fixed part of the body.
pub const FIXED_LEN: usize = 72;

/// The body layout.
pub static LAYOUT: Layout = Layout {
    name: "SqlTrigger",
    size: None,
    fields: &[
        field("time", 0, 1),
        field("event", 1, 1),
        field("flags", 2, 1),
        field("update_condition", 3, 1),
        field("threadsafe", 4, 1),
        field("mt_action", 5, 1),
        field("mode", 6, 1),
        field("orientation", 7, 1),
        field("number", 8, 4),
        field("issued", 12, 4),
        field("program_lib", 16, 10),
        field("program", 26, 10),
        field("trigger_lib", 36, 10),
        field("name_len", 46, 2),
        field("created", 48, 8),
        field("col_count", 56, 2),
        field("definer_len", 58, 2),
        field("text_len", 60, 2),
        field("reserved", 62, 10),
        Field {
            name: "parts",
            offset: Some(72),
            size: None,
        },
    ],
};

/// `TRGTIME`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Time {
    /// `*BEFORE`.
    Before,
    /// `*AFTER`.
    After,
    /// SQL's `INSTEAD OF` (a view's).
    InsteadOf,
}

impl Time {
    fn code(self) -> u8 {
        match self {
            Self::Before => 1,
            Self::After => 2,
            Self::InsteadOf => 3,
        }
    }

    fn from_code(b: u8) -> Option<Self> {
        Some(match b {
            1 => Self::Before,
            2 => Self::After,
            3 => Self::InsteadOf,
            _ => return None,
        })
    }

    /// The trigger buffer's byte: `1` after, `2` before (and instead of).
    #[must_use]
    pub fn buffer_code(self) -> u8 {
        match self {
            Self::After => b'1',
            Self::Before | Self::InsteadOf => b'2',
        }
    }

    /// As the catalogs show it.
    #[must_use]
    pub fn sql(self) -> &'static str {
        match self {
            Self::Before => "BEFORE",
            Self::After => "AFTER",
            Self::InsteadOf => "INSTEAD",
        }
    }
}

/// `TRGEVENT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Event {
    /// `*INSERT`.
    Insert,
    /// `*DELETE`.
    Delete,
    /// `*UPDATE`.
    Update,
    /// `*READ`.
    Read,
}

impl Event {
    /// The stored code, which is the trigger buffer's digit.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Insert => 1,
            Self::Delete => 2,
            Self::Update => 3,
            Self::Read => 4,
        }
    }

    fn from_code(b: u8) -> Option<Self> {
        Some(match b {
            1 => Self::Insert,
            2 => Self::Delete,
            3 => Self::Update,
            4 => Self::Read,
            _ => return None,
        })
    }

    /// As the catalogs show it.
    #[must_use]
    pub fn sql(self) -> &'static str {
        match self {
            Self::Insert => "INSERT",
            Self::Delete => "DELETE",
            Self::Update => "UPDATE",
            Self::Read => "READ",
        }
    }
}

/// `TRGUPDCND`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateCondition {
    /// Not an update trigger.
    None,
    /// `*ALWAYS`.
    Always,
    /// `*CHANGE`: only when the update changes the record.
    Change,
}

/// `THDSAFE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Threadsafe {
    /// `*UNKNOWN`.
    Unknown,
    /// `*YES`.
    Yes,
    /// `*NO`.
    No,
}

/// `MLTTHDACN`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MtAction {
    /// `*SYSVAL`.
    Sysval,
    /// `*MSG`.
    Msg,
    /// `*NORUN`.
    NoRun,
    /// `*RUN`.
    Run,
}

/// One trigger.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Trigger {
    /// Its name (`TRG`), up to 128 characters.
    pub name: String,
    /// The library its name is in (`TRGLIB`).
    pub library: Name,
    /// When.
    pub time: Time,
    /// On what.
    pub event: Event,
    /// The program called.
    pub program: (Name, Name),
    /// `ALWREPCHG(*YES)`.
    pub alwrepchg: bool,
    /// `TRGUPDCND`.
    pub update_condition: UpdateCondition,
    /// `STATE(*ENABLED)`.
    pub enabled: bool,
    /// Operative (its program is where it was).
    pub operative: bool,
    /// An SQL trigger (`CREATE TRIGGER`).
    pub sql: bool,
    /// `FOR EACH STATEMENT` (SQL).
    pub statement: bool,
    /// `MODE DB2SQL` (SQL).
    pub db2sql: bool,
    /// `THDSAFE`.
    pub threadsafe: Threadsafe,
    /// `MLTTHDACN`.
    pub mt_action: MtAction,
    /// `ACTION_ORDER`: the file's count of triggers added when this one was.
    pub number: u32,
    /// The count when this entry was last written (a replace uses one).
    pub issued: u32,
    /// Unix seconds.
    pub created: u64,
    /// `UPDATE OF` columns (SQL).
    pub columns: Vec<Name>,
    /// Who added it.
    pub definer: String,
    /// `LABEL ON TRIGGER`.
    pub text: String,
    /// What `CREATE TRIGGER` keeps besides; `None` for `ADDPFTRG`'s.
    pub def: Option<Box<SqlDef>>,
}

/// An SQL trigger's definition, as the catalogs show it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlDef {
    /// `REFERENCING OLD AS`.
    pub old_row: String,
    /// `REFERENCING NEW AS`.
    pub new_row: String,
    /// `REFERENCING OLD TABLE AS`.
    pub old_table: String,
    /// `REFERENCING NEW TABLE AS`.
    pub new_table: String,
    /// The `WHEN` condition (`ACTION_CONDITION`), empty for none.
    pub condition: String,
    /// The triggered statement (`ACTION_STATEMENT`).
    pub statement: String,
    /// `COMMENT ON TRIGGER`.
    pub comment: String,
    /// The columns the condition and statement use: library, file and
    /// column, once per use (`SYSTRIGCOL`).
    pub columns: Vec<(Name, Name, Name)>,
    /// The tables and views the statement uses: library and file
    /// (`SYSTRIGDEP`).
    pub objects: Vec<(Name, Name)>,
}

impl SqlDef {
    fn encode(&self, b: &mut Vec<u8>) {
        for s in [
            &self.old_row,
            &self.new_row,
            &self.old_table,
            &self.new_table,
            &self.condition,
            &self.statement,
            &self.comment,
        ] {
            b.extend_from_slice(&u32::try_from(s.len()).expect("short").to_le_bytes());
            b.extend_from_slice(s.as_bytes());
        }
        b.extend_from_slice(
            &u16::try_from(self.columns.len())
                .expect("few")
                .to_le_bytes(),
        );
        for (l, f, c) in &self.columns {
            b.extend_from_slice(l.as_bytes());
            b.extend_from_slice(f.as_bytes());
            b.extend_from_slice(c.as_bytes());
        }
        b.extend_from_slice(
            &u16::try_from(self.objects.len())
                .expect("few")
                .to_le_bytes(),
        );
        for (l, f) in &self.objects {
            b.extend_from_slice(l.as_bytes());
            b.extend_from_slice(f.as_bytes());
        }
    }

    fn decode(b: &[u8]) -> Result<Self> {
        let bad = || DbError::Corrupt {
            what: "trigger entry: SQL definition".into(),
        };
        let mut at = 0;
        let mut take = |n: usize| -> Result<&[u8]> {
            let s = b.get(at..at + n).ok_or_else(bad)?;
            at += n;
            Ok(s)
        };
        let mut strs = Vec::new();
        for _ in 0..7 {
            let n = u32::from_le_bytes(take(4)?.try_into().expect("4")) as usize;
            strs.push(String::from_utf8(take(n)?.to_vec()).map_err(|_| bad())?);
        }
        let name = |s: &[u8]| Name::from_bytes(s.try_into().expect("10"));
        let mut columns = Vec::new();
        for _ in 0..u16::from_le_bytes(take(2)?.try_into().expect("2")) {
            let c = take(30)?;
            columns.push((name(&c[..10]), name(&c[10..20]), name(&c[20..])));
        }
        let mut objects = Vec::new();
        for _ in 0..u16::from_le_bytes(take(2)?.try_into().expect("2")) {
            let o = take(20)?;
            objects.push((name(&o[..10]), name(&o[10..])));
        }
        let mut s = strs.into_iter();
        let mut next = || s.next().unwrap_or_default();
        Ok(Self {
            old_row: next(),
            new_row: next(),
            old_table: next(),
            new_table: next(),
            condition: next(),
            statement: next(),
            comment: next(),
            columns,
            objects,
        })
    }
}

impl Trigger {
    /// An enabled trigger with the defaults `ADDPFTRG` gives.
    #[must_use]
    pub fn new(name: &str, library: Name, time: Time, event: Event, program: (Name, Name)) -> Self {
        Self {
            name: name.to_string(),
            library,
            time,
            event,
            program,
            alwrepchg: false,
            update_condition: if event == Event::Update {
                UpdateCondition::Always
            } else {
                UpdateCondition::None
            },
            enabled: true,
            operative: true,
            sql: false,
            statement: false,
            db2sql: false,
            threadsafe: Threadsafe::Unknown,
            mt_action: MtAction::Sysval,
            number: 0,
            issued: 0,
            created: 0,
            columns: Vec::new(),
            definer: String::new(),
            text: String::new(),
            def: None,
        }
    }

    /// The body of its `TRIGGER` entry.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut b = vec![0u8; FIXED_LEN];
        b[0] = self.time.code();
        b[1] = self.event.code();
        b[2] = u8::from(!self.enabled)
            | u8::from(!self.operative) << 1
            | u8::from(self.alwrepchg) << 2
            | u8::from(self.sql) << 3;
        b[3] = match self.update_condition {
            UpdateCondition::None => 0,
            UpdateCondition::Always => 1,
            UpdateCondition::Change => 2,
        };
        b[4] = match self.threadsafe {
            Threadsafe::Unknown => 0,
            Threadsafe::Yes => 1,
            Threadsafe::No => 2,
        };
        b[5] = match self.mt_action {
            MtAction::Sysval => 0,
            MtAction::Msg => 1,
            MtAction::NoRun => 2,
            MtAction::Run => 3,
        };
        b[6] = u8::from(self.db2sql);
        b[7] = u8::from(self.statement);
        b[8..12].copy_from_slice(&self.number.to_le_bytes());
        b[12..16].copy_from_slice(&self.issued.to_le_bytes());
        b[16..26].copy_from_slice(self.program.0.as_bytes());
        b[26..36].copy_from_slice(self.program.1.as_bytes());
        b[36..46].copy_from_slice(self.library.as_bytes());
        b[46..48].copy_from_slice(&u16::try_from(self.name.len()).expect("short").to_le_bytes());
        b[48..56].copy_from_slice(&self.created.to_le_bytes());
        b[56..58].copy_from_slice(
            &u16::try_from(self.columns.len())
                .expect("few")
                .to_le_bytes(),
        );
        b[58..60].copy_from_slice(
            &u16::try_from(self.definer.len())
                .expect("short")
                .to_le_bytes(),
        );
        b[60..62].copy_from_slice(&u16::try_from(self.text.len()).expect("short").to_le_bytes());
        b.extend_from_slice(self.name.as_bytes());
        for c in &self.columns {
            b.extend_from_slice(c.as_bytes());
        }
        b.extend_from_slice(self.definer.as_bytes());
        b.extend_from_slice(self.text.as_bytes());
        if let Some(d) = &self.def {
            d.encode(&mut b);
        }
        b
    }

    /// From the body of a `TRIGGER` entry.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let bad = |what: &str| DbError::Corrupt {
            what: format!("trigger entry: {what}"),
        };
        if b.len() < FIXED_LEN {
            return Err(bad("short"));
        }
        let u16_at = |i: usize| usize::from(u16::from_le_bytes([b[i], b[i + 1]]));
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().expect("4"));
        let name10 = |i: usize| Name::from_bytes(b[i..i + 10].try_into().expect("10"));
        let mut at = FIXED_LEN;
        let mut take = |n: usize| -> Result<&[u8]> {
            let s = b.get(at..at + n).ok_or_else(|| bad("parts"))?;
            at += n;
            Ok(s)
        };
        let text = |s: &[u8]| String::from_utf8(s.to_vec()).map_err(|_| bad("UTF-8"));
        let name = text(take(u16_at(46))?)?;
        let mut columns = Vec::new();
        for _ in 0..u16_at(56) {
            columns.push(Name::from_bytes(take(10)?.try_into().expect("10")));
        }
        let definer = text(take(u16_at(58))?)?;
        let label = text(take(u16_at(60))?)?;
        let def = if at < b.len() {
            Some(Box::new(SqlDef::decode(&b[at..])?))
        } else {
            None
        };
        Ok(Self {
            name,
            library: name10(36),
            time: Time::from_code(b[0]).ok_or_else(|| bad("time"))?,
            event: Event::from_code(b[1]).ok_or_else(|| bad("event"))?,
            program: (name10(16), name10(26)),
            alwrepchg: b[2] & 4 != 0,
            update_condition: match b[3] {
                0 => UpdateCondition::None,
                1 => UpdateCondition::Always,
                2 => UpdateCondition::Change,
                _ => return Err(bad("update condition")),
            },
            enabled: b[2] & 1 == 0,
            operative: b[2] & 2 == 0,
            sql: b[2] & 8 != 0,
            statement: b[7] != 0,
            db2sql: b[6] != 0,
            threadsafe: match b[4] {
                1 => Threadsafe::Yes,
                2 => Threadsafe::No,
                _ => Threadsafe::Unknown,
            },
            mt_action: match b[5] {
                1 => MtAction::Msg,
                2 => MtAction::NoRun,
                3 => MtAction::Run,
                _ => MtAction::Sysval,
            },
            number: u32_at(8),
            issued: u32_at(12),
            created: u64::from_le_bytes(b[48..56].try_into().expect("8")),
            columns,
            definer,
            text: label,
            def,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let n = |s: &str| Name::parse(s).unwrap();
        let mut t = Trigger::new(
            "A_LONG_TRIGGER_NAME",
            n("QSTEST"),
            Time::After,
            Event::Update,
            (n("QSTEST"), n("TRGPGM")),
        );
        t.alwrepchg = true;
        t.update_condition = UpdateCondition::Change;
        t.enabled = false;
        t.number = 4;
        t.issued = 6;
        t.created = 1_800_000_000;
        t.columns = vec![n("C1"), n("N1")];
        t.definer = "QSECOFR".into();
        t.text = "audit".into();
        assert_eq!(Trigger::decode(&t.encode()).unwrap(), t);
        assert!(Trigger::decode(&t.encode()[..FIXED_LEN + 3]).is_err());
        t.def = Some(Box::new(SqlDef {
            new_row: "N".into(),
            condition: "( N . C1 > 0 ) ".into(),
            statement: "SET N . C1 = 0 ".into(),
            columns: vec![(n("QSTEST"), n("T"), n("C1"))],
            objects: vec![(n("QSTEST"), n("LOG"))],
            ..SqlDef::default()
        }));
        assert_eq!(Trigger::decode(&t.encode()).unwrap(), t);
        assert!(Trigger::decode(&t.encode()[..t.encode().len() - 1]).is_err());
    }
}
