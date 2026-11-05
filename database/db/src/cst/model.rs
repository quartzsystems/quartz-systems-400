// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A physical file constraint and the body of its `*SQLINF`
//! `CONSTRAINT` entry (`docs/DB_FORMAT.md` §8.1, `SqlConstraint`).

use qsname::Name;
use store::obj::ObjectId;

use super::check;
use crate::error::{DbError, Result};
use crate::spec::{Field, Layout, field};

/// Bytes of the fixed part of the body.
pub const FIXED_LEN: usize = 48;

/// The body layout.
pub static LAYOUT: Layout = Layout {
    name: "SqlConstraint",
    size: None,
    fields: &[
        field("kind", 0, 1),
        field("state", 1, 1),
        field("delete_rule", 2, 1),
        field("update_rule", 3, 1),
        field("number", 4, 2),
        field("col_count", 6, 2),
        field("other", 8, 16),
        field("name_len", 24, 2),
        field("other_name_len", 26, 2),
        field("text_len", 28, 4),
        field("program_len", 32, 4),
        field("reserved", 36, 12),
        Field { name: "parts", offset: Some(48), size: None },
    ],
};

/// What a constraint is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// `*UNQCST`.
    Unique,
    /// `*PRIKEY`.
    PrimaryKey,
    /// `*CHKCST`.
    Check,
    /// `*REFCST`, kept on the dependent file.
    Referential,
    /// On the parent file: a referential constraint of another file
    /// names this one as its parent. `other` is the dependent file.
    Dependent,
}

impl Kind {
    fn code(self) -> u8 {
        match self {
            Self::Unique => 1,
            Self::PrimaryKey => 2,
            Self::Check => 3,
            Self::Referential => 4,
            Self::Dependent => 5,
        }
    }

    fn from_code(b: u8) -> Option<Self> {
        Some(match b {
            1 => Self::Unique,
            2 => Self::PrimaryKey,
            3 => Self::Check,
            4 => Self::Referential,
            5 => Self::Dependent,
            _ => return None,
        })
    }

    /// The `TYPE` word of `ADDPFCST`.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Unique => "*UNQCST",
            Self::PrimaryKey => "*PRIKEY",
            Self::Check => "*CHKCST",
            Self::Referential | Self::Dependent => "*REFCST",
        }
    }

    /// `CONSTRAINT_TYPE` in `SYSCST`.
    #[must_use]
    pub fn sql_type(self) -> &'static str {
        match self {
            Self::Unique => "UNIQUE",
            Self::PrimaryKey => "PRIMARY KEY",
            Self::Check => "CHECK",
            Self::Referential | Self::Dependent => "FOREIGN KEY",
        }
    }

    /// Whether the constraint keeps keys in the member's constraint tree.
    #[must_use]
    pub fn keyed(self) -> bool {
        matches!(self, Self::Unique | Self::PrimaryKey | Self::Referential)
    }

    /// Whether its keys are unique.
    #[must_use]
    pub fn unique(self) -> bool {
        matches!(self, Self::Unique | Self::PrimaryKey)
    }
}

/// A delete or update rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Rule {
    /// `NO ACTION`: checked after the change.
    #[default]
    NoAction,
    /// `RESTRICT`: checked before the change.
    Restrict,
    /// `CASCADE`.
    Cascade,
    /// `SET NULL`.
    SetNull,
    /// `SET DEFAULT`.
    SetDefault,
}

impl Rule {
    fn code(self) -> u8 {
        match self {
            Self::NoAction => 0,
            Self::Restrict => 1,
            Self::Cascade => 2,
            Self::SetNull => 3,
            Self::SetDefault => 4,
        }
    }

    fn from_code(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::NoAction,
            1 => Self::Restrict,
            2 => Self::Cascade,
            3 => Self::SetNull,
            4 => Self::SetDefault,
            _ => return None,
        })
    }

    /// The `DLTRULE`/`UPDRULE` word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::NoAction => "*NOACTION",
            Self::Restrict => "*RESTRICT",
            Self::Cascade => "*CASCADE",
            Self::SetNull => "*SETNULL",
            Self::SetDefault => "*SETDFT",
        }
    }

    /// From the `DLTRULE`/`UPDRULE` word.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word.trim().to_ascii_uppercase().as_str() {
            "*NOACTION" => Self::NoAction,
            "*RESTRICT" => Self::Restrict,
            "*CASCADE" => Self::Cascade,
            "*SETNULL" => Self::SetNull,
            "*SETDFT" => Self::SetDefault,
            _ => return None,
        })
    }

    /// `UPDATE_RULE`/`DELETE_RULE` in `SYSREFCST`.
    #[must_use]
    pub fn sql(self) -> &'static str {
        match self {
            Self::NoAction => "NO ACTION",
            Self::Restrict => "RESTRICT",
            Self::Cascade => "CASCADE",
            Self::SetNull => "SET NULL",
            Self::SetDefault => "SET DEFAULT",
        }
    }

    /// Whether the rule changes the dependent file (and so needs both
    /// files journaled).
    #[must_use]
    pub fn changes_dependents(self) -> bool {
        matches!(self, Self::Cascade | Self::SetNull | Self::SetDefault)
    }
}

/// One constraint of a physical file.
#[derive(Clone, Debug, PartialEq)]
pub struct Constraint {
    /// The SQL name, as the catalogs show it.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// `STATE(*DISABLED)` when false (referential and check only).
    pub enabled: bool,
    /// In check pending: records break it.
    pub check_pending: bool,
    /// `DLTRULE`.
    pub delete_rule: Rule,
    /// `UPDRULE`.
    pub update_rule: Rule,
    /// Its number in the member's constraint tree, unique in the file.
    pub number: u16,
    /// Key fields (unique, primary, the foreign key) or the fields a
    /// check reads, by system name.
    pub columns: Vec<Name>,
    /// Referential: the parent file. Dependent: the dependent file.
    pub other: ObjectId,
    /// Referential: the parent's unique or primary key constraint.
    /// Dependent: the dependent file's referential constraint (its name).
    pub other_name: String,
    /// Check: the condition as the catalogs show it.
    pub text: String,
    /// Check: the compiled condition.
    pub check: Option<check::Expr>,
}

impl Constraint {
    /// A constraint with no rules or check, enabled.
    #[must_use]
    pub fn new(name: &str, kind: Kind, columns: Vec<Name>) -> Self {
        Self {
            name: name.to_string(),
            kind,
            enabled: true,
            check_pending: false,
            delete_rule: Rule::NoAction,
            update_rule: Rule::NoAction,
            number: 0,
            columns,
            other: ObjectId::NONE,
            other_name: String::new(),
            text: String::new(),
            check: None,
        }
    }

    /// Whether the constraint is enforced now: enabled and not pending.
    #[must_use]
    pub fn active(&self) -> bool {
        self.enabled && !self.check_pending
    }

    /// The body of the `CONSTRAINT` entry.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let program = self.check.as_ref().map(check::Expr::encode).unwrap_or_default();
        let mut b = vec![0u8; FIXED_LEN];
        b[0] = self.kind.code();
        b[1] = u8::from(!self.enabled) | (u8::from(self.check_pending) << 1);
        b[2] = self.delete_rule.code();
        b[3] = self.update_rule.code();
        b[4..6].copy_from_slice(&self.number.to_le_bytes());
        b[6..8].copy_from_slice(&u16::try_from(self.columns.len()).expect("few").to_le_bytes());
        b[8..24].copy_from_slice(&self.other.to_bytes());
        b[24..26].copy_from_slice(&u16::try_from(self.name.len()).expect("short").to_le_bytes());
        b[26..28].copy_from_slice(&u16::try_from(self.other_name.len()).expect("short").to_le_bytes());
        b[28..32].copy_from_slice(&u32::try_from(self.text.len()).expect("fits").to_le_bytes());
        b[32..36].copy_from_slice(&u32::try_from(program.len()).expect("fits").to_le_bytes());
        b.extend_from_slice(self.name.as_bytes());
        for c in &self.columns {
            b.extend_from_slice(c.as_bytes());
        }
        b.extend_from_slice(self.other_name.as_bytes());
        b.extend_from_slice(self.text.as_bytes());
        b.extend_from_slice(&program);
        b
    }

    /// From the body of a `CONSTRAINT` entry.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let bad = |what: &str| DbError::Corrupt { what: format!("constraint entry: {what}") };
        if b.len() < FIXED_LEN {
            return Err(bad("short"));
        }
        let u16_at = |i: usize| usize::from(u16::from_le_bytes([b[i], b[i + 1]]));
        let u32_at = |i: usize| u32::from_le_bytes(b[i..i + 4].try_into().expect("4")) as usize;
        let kind = Kind::from_code(b[0]).ok_or_else(|| bad("kind"))?;
        let cols = u16_at(6);
        let (name_len, other_len, text_len, prog_len) = (u16_at(24), u16_at(26), u32_at(28), u32_at(32));
        let mut at = FIXED_LEN;
        let mut take = |n: usize| -> Result<&[u8]> {
            let s = b.get(at..at + n).ok_or_else(|| bad("parts"))?;
            at += n;
            Ok(s)
        };
        let text = |s: &[u8]| String::from_utf8(s.to_vec()).map_err(|_| bad("UTF-8"));
        let name = text(take(name_len)?)?;
        let mut columns = Vec::with_capacity(cols);
        for _ in 0..cols {
            let raw: [u8; 10] = take(10)?.try_into().expect("10");
            columns.push(Name::from_bytes(raw));
        }
        let other_name = text(take(other_len)?)?;
        let ctext = text(take(text_len)?)?;
        let prog = take(prog_len)?;
        let check = if prog.is_empty() { None } else { Some(check::Expr::decode(prog)?) };
        Ok(Self {
            name,
            kind,
            enabled: b[1] & 1 == 0,
            check_pending: b[1] & 2 != 0,
            delete_rule: Rule::from_code(b[2]).ok_or_else(|| bad("delete rule"))?,
            update_rule: Rule::from_code(b[3]).ok_or_else(|| bad("update rule"))?,
            number: u16::from_le_bytes([b[4], b[5]]),
            columns,
            other: ObjectId::from_bytes(&b[8..24]),
            other_name,
            text: ctext,
            check,
        })
    }
}
