// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bodies of the `ROUTINE`, `PARM` and `OPTIONS` entries: a
//! procedure's or function's attributes, its parameters and the `SET
//! OPTION` values it was made with. A `*SQLINF` may hold several
//! routines (external routines over one program); each `ROUTINE` entry
//! is followed by its `PARM` entries and its `OPTIONS`. See
//! `docs/DB_FORMAT.md` §8.1.

use crate::error::{DbError, Result};
use crate::spec::{Field, Layout, field};

/// `SqlRoutine` layout.
pub static ROUTINE_LAYOUT: Layout = Layout {
    name: "SqlRoutine",
    size: None,
    fields: &[
        field("routine_type", 0, 1),
        field("language", 1, 1),
        field("style", 2, 1),
        field("access", 3, 1),
        field("body", 4, 1),
        field("function_type", 5, 1),
        field("flags", 6, 2),
        field("result_sets", 8, 2),
        field("parm_count", 10, 2),
        field("scratchpad", 12, 4),
        field("created", 16, 8),
        field("program_library", 24, 10),
        field("program", 34, 10),
        field("entry_len", 44, 2),
        field("specific_len", 46, 2),
        field("name_len", 48, 2),
        field("sourced_len", 50, 2),
        field("text_len", 52, 4),
        field("body_len", 56, 4),
        field("reserved", 60, 4),
        Field { name: "strings", offset: Some(64), size: None },
    ],
};

/// `SqlParm` layout.
pub static PARM_LAYOUT: Layout = Layout {
    name: "SqlParm",
    size: None,
    fields: &[
        field("ordinal", 0, 2),
        field("mode", 2, 1),
        field("role", 3, 1),
        field("name_len", 4, 2),
        field("type_len", 6, 2),
        Field { name: "name", offset: Some(8), size: None },
        Field { name: "type", offset: None, size: None },
    ],
};

/// `SqlOptions` layout.
pub static OPTIONS_LAYOUT: Layout = Layout {
    name: "SqlOptions",
    size: None,
    fields: &[
        field("count", 0, 2),
        field("reserved", 2, 2),
        Field { name: "options", offset: Some(4), size: None },
    ],
};

/// Bytes of a `SqlRoutine` before its strings.
pub const ROUTINE_HEAD: usize = 64;

/// Routine flag bits.
pub mod flag {
    /// `DETERMINISTIC`.
    pub const DETERMINISTIC: u16 = 1;
    /// `COMMIT ON RETURN YES`.
    pub const COMMIT_ON_RETURN: u16 = 1 << 1;
    /// `PROGRAM TYPE SUB`.
    pub const PROGRAM_SUB: u16 = 1 << 2;
    /// `RETURNS NULL ON NULL INPUT`.
    pub const NULL_INPUT: u16 = 1 << 3;
    /// `NO EXTERNAL ACTION`.
    pub const NO_EXTERNAL_ACTION: u16 = 1 << 4;
    /// `NOT FENCED`.
    pub const NOT_FENCED: u16 = 1 << 5;
    /// `ALLOW PARALLEL`.
    pub const PARALLEL: u16 = 1 << 6;
    /// `FINAL CALL`.
    pub const FINAL_CALL: u16 = 1 << 7;
    /// `DBINFO`.
    pub const DBINFO: u16 = 1 << 8;
    /// `NEW SAVEPOINT LEVEL`.
    pub const NEW_SAVEPOINT_LEVEL: u16 = 1 << 9;
    /// `DEFAULT SPECIAL REGISTERS`.
    pub const DEFAULT_REGISTERS: u16 = 1 << 10;
}

fn bad(what: &str) -> DbError {
    DbError::Corrupt { what: format!("SQL information: {what}") }
}

fn take<'a>(b: &'a [u8], at: &mut usize, n: usize, what: &str) -> Result<&'a [u8]> {
    let s = b.get(*at..*at + n).ok_or_else(|| bad(what))?;
    *at += n;
    Ok(s)
}

fn text(b: &[u8], at: &mut usize, n: usize, what: &str) -> Result<String> {
    Ok(String::from_utf8_lossy(take(b, at, n, what)?).into_owned())
}

fn u16_of(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_of(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4"))
}

fn short(s: &str) -> u16 {
    u16::try_from(s.len()).expect("short text")
}

fn long(s: &str) -> u32 {
    u32::try_from(s.len()).expect("text")
}

/// A name of 10 characters into CP037, blank padded.
fn name10(s: &str) -> [u8; 10] {
    let mut out = [0x40u8; 10];
    let e = ccsid::to_ebcdic(s).unwrap_or_default();
    for (o, b) in out.iter_mut().zip(e) {
        *o = b;
    }
    out
}

fn of10(b: &[u8]) -> String {
    ccsid::from_ebcdic(b).trim_end().to_string()
}

/// A procedure's or function's attributes.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RoutineDef {
    /// 1 procedure, 2 function.
    pub routine_type: u8,
    /// 1 SQL, 2 RPGLE, 3 RPG, 4 CL, 5 C, 6 COBOL, 7 COBOLLE, 8 JAVA, 9 PLI.
    pub language: u8,
    /// 1 SQL, 2 GENERAL, 3 GENERAL WITH NULLS, 4 DB2GENERAL, 5 JAVA, 6 DB2SQL.
    pub style: u8,
    /// 1 NO SQL, 2 CONTAINS SQL, 3 READS SQL DATA, 4 MODIFIES SQL DATA.
    pub access: u8,
    /// 1 SQL, 2 external, 3 sourced.
    pub body: u8,
    /// 0 not a function, 1 scalar, 2 table.
    pub function_type: u8,
    /// [`flag`] bits.
    pub flags: u16,
    /// `DYNAMIC RESULT SETS`.
    pub result_sets: u16,
    /// Parameters (the `PARM` entries that follow with role 0).
    pub parm_count: u16,
    /// `SCRATCHPAD` length, 0 for none.
    pub scratchpad: u32,
    /// When it was made, Unix seconds.
    pub created: u64,
    /// The program's library (`*LIBL` for an unqualified external name).
    pub program_library: String,
    /// The `*PGM` or `*SRVPGM`.
    pub program: String,
    /// A service program's procedure; empty for a program.
    pub entry: String,
    /// The specific name.
    pub specific: String,
    /// The routine's SQL name.
    pub name: String,
    /// A sourced function's source, as `SCHEMA.SPECIFIC`.
    pub sourced: String,
    /// The `CREATE` statement as given.
    pub text: String,
    /// The routine body as given (SQL routines).
    pub body_text: String,
}

impl RoutineDef {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![0u8; ROUTINE_HEAD];
        out[0] = self.routine_type;
        out[1] = self.language;
        out[2] = self.style;
        out[3] = self.access;
        out[4] = self.body;
        out[5] = self.function_type;
        out[6..8].copy_from_slice(&self.flags.to_le_bytes());
        out[8..10].copy_from_slice(&self.result_sets.to_le_bytes());
        out[10..12].copy_from_slice(&self.parm_count.to_le_bytes());
        out[12..16].copy_from_slice(&self.scratchpad.to_le_bytes());
        out[16..24].copy_from_slice(&self.created.to_le_bytes());
        out[24..34].copy_from_slice(&name10(&self.program_library));
        out[34..44].copy_from_slice(&name10(&self.program));
        out[44..46].copy_from_slice(&short(&self.entry).to_le_bytes());
        out[46..48].copy_from_slice(&short(&self.specific).to_le_bytes());
        out[48..50].copy_from_slice(&short(&self.name).to_le_bytes());
        out[50..52].copy_from_slice(&short(&self.sourced).to_le_bytes());
        out[52..56].copy_from_slice(&long(&self.text).to_le_bytes());
        out[56..60].copy_from_slice(&long(&self.body_text).to_le_bytes());
        for s in [&self.entry, &self.specific, &self.name, &self.sourced, &self.text, &self.body_text] {
            out.extend_from_slice(s.as_bytes());
        }
        out
    }

    /// From the entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < ROUTINE_HEAD {
            return Err(bad("routine"));
        }
        let mut at = ROUTINE_HEAD;
        let mut s = |n: usize| text(b, &mut at, n, "routine strings");
        let entry = s(u16_of(b, 44).into())?;
        let specific = s(u16_of(b, 46).into())?;
        let name = s(u16_of(b, 48).into())?;
        let sourced = s(u16_of(b, 50).into())?;
        let text_ = s(u32_of(b, 52) as usize)?;
        let body_text = s(u32_of(b, 56) as usize)?;
        Ok(Self {
            routine_type: b[0],
            language: b[1],
            style: b[2],
            access: b[3],
            body: b[4],
            function_type: b[5],
            flags: u16_of(b, 6),
            result_sets: u16_of(b, 8),
            parm_count: u16_of(b, 10),
            scratchpad: u32_of(b, 12),
            created: u64::from_le_bytes(b[16..24].try_into().expect("8")),
            program_library: of10(&b[24..34]),
            program: of10(&b[34..44]),
            entry,
            specific,
            name,
            sourced,
            text: text_,
            body_text,
        })
    }

    /// Whether a flag is set.
    #[must_use]
    pub fn has(&self, f: u16) -> bool {
        self.flags & f != 0
    }
}

/// One parameter, a table function's result column or a scalar
/// function's result.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ParmDef {
    /// From 1.
    pub ordinal: u16,
    /// 1 IN, 2 OUT, 3 INOUT.
    pub mode: u8,
    /// 0 a parameter, 1 a result column, 2 the result.
    pub role: u8,
    /// Its name; empty when it has none.
    pub name: String,
    /// Its type as SQL writes it (`DECIMAL(9, 2)`).
    pub ty: String,
}

impl ParmDef {
    /// The entry body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.name.len() + self.ty.len());
        out.extend_from_slice(&self.ordinal.to_le_bytes());
        out.push(self.mode);
        out.push(self.role);
        out.extend_from_slice(&short(&self.name).to_le_bytes());
        out.extend_from_slice(&short(&self.ty).to_le_bytes());
        out.extend_from_slice(self.name.as_bytes());
        out.extend_from_slice(self.ty.as_bytes());
        out
    }

    /// From the entry body.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < 8 {
            return Err(bad("parameter"));
        }
        let mut at = 8;
        let name = text(b, &mut at, u16_of(b, 4).into(), "parameter name")?;
        let ty = text(b, &mut at, u16_of(b, 6).into(), "parameter type")?;
        Ok(Self { ordinal: u16_of(b, 0), mode: b[2], role: b[3], name, ty })
    }
}

/// The `OPTIONS` body: `count` pairs of a 2-byte length and the
/// option's name, a 2-byte length and its value.
#[must_use]
pub fn encode_options(opts: &[(String, String)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&u16::try_from(opts.len()).expect("options").to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    for (k, v) in opts {
        out.extend_from_slice(&short(k).to_le_bytes());
        out.extend_from_slice(k.as_bytes());
        out.extend_from_slice(&short(v).to_le_bytes());
        out.extend_from_slice(v.as_bytes());
    }
    out
}

/// From the `OPTIONS` body.
pub fn decode_options(b: &[u8]) -> Result<Vec<(String, String)>> {
    if b.len() < 4 {
        return Err(bad("options"));
    }
    let n = u16_of(b, 0);
    let mut at = 4;
    let mut out = Vec::with_capacity(n.into());
    for _ in 0..n {
        let kl = u16::from_le_bytes(take(b, &mut at, 2, "option")?.try_into().expect("2"));
        let k = text(b, &mut at, kl.into(), "option")?;
        let vl = u16::from_le_bytes(take(b, &mut at, 2, "option")?.try_into().expect("2"));
        let v = text(b, &mut at, vl.into(), "option")?;
        out.push((k, v));
    }
    Ok(out)
}

/// One routine of a `*SQLINF`: its attributes, parameters and options.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Routine {
    /// The attributes.
    pub def: RoutineDef,
    /// Parameters and result columns, in order.
    pub parms: Vec<ParmDef>,
    /// `SET OPTION` values.
    pub options: Vec<(String, String)>,
}

impl Routine {
    /// The entries, in the order they are kept.
    #[must_use]
    pub fn entries(&self) -> Vec<super::Entry> {
        let mut out = vec![super::Entry { tag: super::tag::ROUTINE, body: self.def.encode() }];
        for p in &self.parms {
            out.push(super::Entry { tag: super::tag::PARM, body: p.encode() });
        }
        if !self.options.is_empty() {
            out.push(super::Entry { tag: super::tag::OPTIONS, body: encode_options(&self.options) });
        }
        out
    }

    /// Every routine of `info`, in order.
    pub fn all(info: &super::SqlInfo) -> Result<Vec<Self>> {
        let mut out: Vec<Self> = Vec::new();
        for e in &info.entries {
            match e.tag {
                super::tag::ROUTINE => out.push(Self { def: RoutineDef::decode(&e.body)?, ..Self::default() }),
                super::tag::PARM => {
                    out.last_mut().ok_or_else(|| bad("parameter before its routine"))?.parms.push(ParmDef::decode(&e.body)?);
                }
                super::tag::OPTIONS => {
                    if let Some(r) = out.last_mut() {
                        r.options = decode_options(&e.body)?;
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// `info` with the routine of `specific` replaced by `r` (or
    /// dropped for `None`); its other entries kept.
    pub fn replace(info: &mut super::SqlInfo, specific: &str, r: Option<&Self>) -> Result<()> {
        let mut kept = Vec::new();
        let mut skipping = false;
        for e in std::mem::take(&mut info.entries) {
            match e.tag {
                super::tag::ROUTINE => {
                    skipping = RoutineDef::decode(&e.body)?.specific == specific;
                    if !skipping {
                        kept.push(e);
                    }
                }
                super::tag::PARM | super::tag::OPTIONS if skipping => {}
                _ => {
                    skipping = false;
                    kept.push(e);
                }
            }
        }
        if let Some(r) = r {
            kept.extend(r.entries());
        }
        info.entries = kept;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routines_round_trip() {
        let r = Routine {
            def: RoutineDef {
                routine_type: 1,
                language: 1,
                style: 1,
                access: 4,
                body: 1,
                flags: flag::DETERMINISTIC | flag::COMMIT_ON_RETURN,
                result_sets: 2,
                parm_count: 1,
                created: 7,
                program_library: "QSTEST".into(),
                program: "UPDCUS0001".into(),
                specific: "UPDATE_CUSTOMER".into(),
                name: "UPDATE_CUSTOMER".into(),
                text: "CREATE PROCEDURE ...".into(),
                body_text: "BEGIN END".into(),
                ..RoutineDef::default()
            },
            parms: vec![ParmDef { ordinal: 1, mode: 3, role: 0, name: "ID".into(), ty: "INTEGER".into() }],
            options: vec![("COMMIT".into(), "*NONE".into())],
        };
        let mut info = super::super::SqlInfo::new(super::super::Kind::Procedure);
        info.entries = r.entries();
        assert_eq!(Routine::all(&info).unwrap(), vec![r.clone()]);
        Routine::replace(&mut info, "UPDATE_CUSTOMER", None).unwrap();
        assert!(info.entries.is_empty());
        assert!(RoutineDef::decode(&r.def.encode()[..60]).is_err());
    }
}
