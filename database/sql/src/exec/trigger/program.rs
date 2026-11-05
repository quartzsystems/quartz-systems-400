// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The program an SQL trigger compiles to. Its parameters are the
//! trigger buffer and its length, as any trigger program's
//! (`docs/DB_FORMAT.md` §8.3), so record I/O and SQL call it alike. The
//! body compiles through the SQL PL compiler to a procedure whose
//! parameters are the transition variables; the program's entry passes
//! the records' fields in the buffer to it in place, with indicators
//! from the null maps, and for a `BEFORE` trigger writes the indicators
//! of the new record back. A body that ends in an error ends the program
//! with `SQL0723` as an escape: the trigger, its schema, the SQLCODE,
//! the SQLSTATE and the message, in [`DATA_LEN`] bytes.

use std::fmt::Write as _;

use db::format::FormatDef;
use db::trg::buffer::{HEADER_LEN, Parts};
use dds::FieldDef;

use crate::ast::{DataType, ParmMode, TypeSpec};
use crate::pl::types::host_type;
use crate::pl::{ParmGroup, Role, SigParm, Signature};

/// Bytes of the trigger's and schema's names in `SQL0723`'s data.
const NAME_LEN: usize = 128;
/// Bytes of the SQLCODE, as text.
const CODE_LEN: usize = 11;
/// Bytes of the message.
const TEXT_LEN: usize = 70;
/// Bytes of `SQL0723`'s message data.
pub const DATA_LEN: usize = 2 * NAME_LEN + CODE_LEN + 5 + TEXT_LEN;

/// One column of the subject table.
pub(super) struct Column {
    /// Its system name and its SQL name when that differs.
    pub names: Vec<String>,
    /// Its place in the format and the null maps.
    pub index: usize,
    /// Where it is in a record.
    pub offset: u32,
    /// Its SQL type; `None` for a type a trigger's variables do not hold.
    pub ty: Option<TypeSpec>,
    /// What it is, for the refusal of one that is not held.
    pub sql_type: String,
}

/// The type of a field whose storage in a record is the storage a
/// compiled routine gives its SQL type, so the field is passed in place.
fn field_type(f: &FieldDef) -> Option<TypeSpec> {
    use dds::DataType as D;
    if f.ccsid.is_some_and(|c| c != 37) {
        return None;
    }
    let dt = match f.data_type {
        D::Char if f.varlen.is_some() => DataType::Varchar(f.length),
        D::Char => DataType::Char(f.length),
        D::Zoned => DataType::Numeric(u8::try_from(f.length).ok()?, f.decimals),
        D::Packed => DataType::Decimal(u8::try_from(f.length).ok()?, f.decimals),
        D::Binary if f.int_range && f.decimals == 0 => match f.byte_len {
            2 => DataType::SmallInt,
            4 => DataType::Integer,
            8 => DataType::BigInt,
            _ => return None,
        },
        D::Float => match f.precision {
            dds::FloatPrecision::Single => DataType::Real,
            dds::FloatPrecision::Double => DataType::Double,
        },
        D::Date if f.date_fmt.is_none_or(|d| d == dds::DateFmt::Iso) && f.byte_len == 10 => DataType::Date,
        D::Time if f.time_fmt.is_none_or(|t| t == dds::TimeFmt::Iso) && f.byte_len == 8 => DataType::Time,
        D::Timestamp => DataType::Timestamp(D::timestamp_precision(f.length)),
        _ => return None,
    };
    let t = TypeSpec { data_type: dt, ccsid: None, bit_data: false, udt: None };
    // Only where the routine's storage is the record's.
    host_type(&t).ok().filter(|h| h.size == f.byte_len).map(|_| t)
}

/// The columns of a format.
pub(super) fn columns(fmt: &FormatDef) -> Vec<Column> {
    fmt.format
        .fields
        .iter()
        .enumerate()
        .map(|(index, f)| {
            let mut names = vec![f.name.to_string()];
            if let Some(a) = &f.alias
                && *a != names[0]
            {
                names.push(a.clone());
            }
            Column { names, index, offset: f.offset, ty: field_type(f), sql_type: crate::exec::eval::sql_type(f) }
        })
        .collect()
}

/// One parameter of the body: which record, which column.
#[derive(Clone, Copy)]
pub(super) struct Slot {
    /// The new record (else the old).
    pub new: bool,
    /// Index into [`columns`].
    pub col: usize,
}

/// The body's signature: the old record's columns under `old`, the new
/// record's under `new`; the new ones may be set in a `BEFORE` trigger.
pub(super) fn signature(program: &str, routine: &str, cols: &[Column], old: Option<&str>, new: Option<&str>, before: bool) -> (Signature, Vec<Slot>) {
    let mut parms = Vec::new();
    let mut slots = Vec::new();
    let mut groups = Vec::new();
    for (label, is_new) in [(old, false), (new, true)] {
        let Some(label) = label else { continue };
        let mut names = Vec::new();
        for (i, c) in cols.iter().enumerate() {
            let Some(ty) = &c.ty else { continue };
            let k = parms.len();
            let mode = if is_new && before { ParmMode::InOut } else { ParmMode::In };
            parms.push(SigParm { name: String::new(), mode, ty: ty.clone() });
            slots.push(Slot { new: is_new, col: i });
            for n in &c.names {
                names.push((n.clone(), k));
            }
        }
        groups.push(ParmGroup { label: label.to_string(), names, read_only: !(is_new && before) });
    }
    let sig = Signature {
        program: program.to_string(),
        routine: routine.to_string(),
        parms,
        role: Role::Trigger,
        naming: crate::ast::Naming::Sql,
        commit: None,
        groups,
    };
    (sig, slots)
}

fn quoted(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// The program's entry procedure, to follow the body's module text.
pub(super) fn entry(program: &str, (trigger, schema): (&str, &str), cols: &[Column], sig: &Signature, slots: &[Slot], parts: Parts, before: bool) -> String {
    let n = slots.len();
    let old_null = HEADER_LEN + parts.rec_len;
    let new_null = parts.new + parts.rec_len;
    let at = |s: &Slot| (if s.new { parts.new } else { HEADER_LEN }) + cols[s.col].offset as usize;
    let null_at = |s: &Slot| (if s.new { new_null } else { old_null }) + cols[s.col].index;
    let mut o = String::new();
    let _ = writeln!(o, "proc {program}($buf: bytes(1) *VARSIZE, $len: bytes(4)) entry {{");
    for i in 1..=n {
        let _ = writeln!(o, "  auto $i{i}: i16");
    }
    o.push_str("  auto $state: char(5)\n  auto $rname: varchar(517)\n  auto $sname: varchar(128)\n  auto $diag: varchar(70)\n  auto $ca: bytes(136)\n");
    let _ = writeln!(o, "  auto $md: char({DATA_LEN})");
    o.push_str("L0:\n  %b = addr $buf\n  %m1 = const.i16 -1\n  %z0 = const.i16 0\n");
    for (k, s) in slots.iter().enumerate() {
        let _ = writeln!(o, "  %e{k} = str.cmp.eq [%b +{}]:char(1), \"1\"", null_at(s));
        let _ = writeln!(o, "  %v{k} = select %e{k}, %m1, %z0");
        let _ = writeln!(o, "  store $i{}, %v{k}", k + 1);
    }
    let mut args: Vec<String> = slots.iter().zip(&sig.parms).map(|(s, p)| format!("[%b +{}]:{}", at(s), host_type(&p.ty).map(|h| h.qir).unwrap_or_default())).collect();
    args.extend((1..=n).map(|i| format!("$i{i}")));
    args.extend(["$state", "$rname", "$sname", "$diag", "$ca"].map(String::from));
    let _ = writeln!(o, "  callb {}({})", crate::pl::TRIGGER_BODY, args.join(", "));
    o.push_str("  %c = addr $ca\n  %sc = load [%c +12]:i32\n  %bad = cmp.lt %sc, 0\n  brif %bad, L2, L1\nL1:\n");
    if before {
        o.push_str("  %b = addr $buf\n  %f1 = const.i8 -15\n  %f0 = const.i8 -16\n");
        for (k, s) in slots.iter().enumerate().filter(|(_, s)| s.new) {
            let _ = writeln!(o, "  %x{k} = load $i{}", k + 1);
            let _ = writeln!(o, "  %n{k} = cmp.lt %x{k}, 0");
            let _ = writeln!(o, "  %y{k} = select %n{k}, %f1, %f0");
            let _ = writeln!(o, "  store [%b +{}]:i8, %y{k}", null_at(s));
        }
    }
    o.push_str("  ret\nL2:\n  %d = addr $md\n  %c = addr $ca\n");
    let _ = writeln!(o, "  mov [%d +0]:char({NAME_LEN}), {}", quoted(trigger));
    let _ = writeln!(o, "  mov [%d +{NAME_LEN}]:char({NAME_LEN}), {}", quoted(schema));
    let _ = writeln!(o, "  cvt [%d +{}]:char({CODE_LEN}), [%c +12]:i32", 2 * NAME_LEN);
    let _ = writeln!(o, "  mov [%d +{}]:char(5), [%c +131]:char(5)", 2 * NAME_LEN + CODE_LEN);
    let _ = writeln!(o, "  mov [%d +{}]:char({TEXT_LEN}), [%c +18]:char({TEXT_LEN})", 2 * NAME_LEN + CODE_LEN + 5);
    o.push_str("  sndpgmmsg msgid(\"SQL0723\") msgf(\"QSQLMSG\") data($md) type(*ESCAPE) to(*PRV)\n  ret\n}\n");
    o
}

/// The text of a failed SQL trigger's `SQL0723` from its message data.
#[must_use]
pub fn failure_text(data: &str) -> String {
    let c: Vec<char> = data.chars().collect();
    let part = |from: usize, len: usize| -> String { c.iter().skip(from).take(len).collect::<String>().trim().to_string() };
    let trigger = part(0, NAME_LEN);
    let schema = part(NAME_LEN, NAME_LEN);
    let code = part(2 * NAME_LEN, CODE_LEN);
    let state = part(2 * NAME_LEN + CODE_LEN, 5);
    let text = part(2 * NAME_LEN + CODE_LEN + 5, TEXT_LEN);
    let mut s = format!("SQL trigger {trigger} in {schema} failed with SQLCODE {code} SQLSTATE {state}");
    if !text.is_empty() {
        let _ = write!(s, ": {text}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_failure_reads_back() {
        let mut d = format!("{:<128}{:<128}{:<11}{}", "TRB4", "QSTEST", "-438", "75001");
        d.push_str("negative amount");
        assert_eq!(failure_text(&d), "SQL trigger TRB4 in QSTEST failed with SQLCODE -438 SQLSTATE 75001: negative amount");
    }
}
