// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RPG source from the store: a member with its sequence numbers and
//! change dates, the `/COPY` members it names, and the external
//! descriptions of the database files it declares.

use std::cell::RefCell;

use qsname::{MemberSpec, Name};
use rpgc::files::{Dev, ExtField, ExtFile, ExtFormat};
use rpgc::types::{DatFmt, TimFmt, Ty};
use rpgfront::source::Record;
use rpgfront::{Includes, Member};
use store::Txn;
use store::obj::{ObjectId, ops, types};
use sysapi::Provider;

use super::super::direct::{in_store, lib_or};
use crate::ipl::system::System;
use crate::svc::ile::StoreProvider;

/// A member to compile and its text description.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Source {
    /// The member as the compiler reads it.
    pub member: Member,
    /// Its text, for `TEXT(*SRCMBRTXT)`.
    pub text: String,
}

/// Records as the compiler reads them.
#[must_use]
pub fn member_of(lib: &str, file: &str, name: &str, records: &[(u32, u32, String)]) -> Member {
    Member {
        lib: lib.into(),
        file: file.into(),
        name: name.into(),
        ccsid: 37,
        records: records
            .iter()
            .map(|(seq, date, text)| Record {
                seq: format!("{seq:06}"),
                date: format!("{date:06}"),
                text: text.trim_end().to_string(),
            })
            .collect(),
    }
}

/// Read member `mbr` (the first when `None`) of `lib/file`.
///
/// # Errors
///
/// `CPF9801` when the file or member is not found or cannot be read.
pub fn read(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    file: &str,
    lib: &str,
    mbr: Option<&str>,
) -> Result<Source, String> {
    let id = wm::obj::find(txn, qsys, names, &lib_or(lib), file, types::FILE)
        .map_err(|e| format!("CPF9801 {e}"))?;
    let spec = match mbr {
        Some(m) => MemberSpec::Name(Name::parse(m).map_err(|e| format!("CPD0078 {e}"))?),
        None => MemberSpec::First,
    };
    let sm = db::source::read_member(txn, id, spec).map_err(|e| format!("CPF9801 {e}"))?;
    let (_, h) = ops::read_header(txn, id).map_err(|e| format!("CPF9801 {e}"))?;
    let found_lib =
        ops::describe(txn, h.library).map_or_else(|_| lib_or(lib), |l| l.name.to_string());
    let name = match mbr {
        Some(m) => m.to_string(),
        None => ops::describe(txn, sm.object)
            .map(|m| m.name.to_string())
            .map_err(|e| format!("CPF9801 {e}"))?,
    };
    Ok(Source {
        member: member_of(&found_lib, file, &name, &sm.records),
        text: sm.text,
    })
}

/// Whether the source holds embedded SQL: `EXEC SQL` outside comments.
#[must_use]
pub fn has_sql(m: &Member) -> bool {
    m.records.iter().any(|r| {
        let t = r.text.to_ascii_uppercase();
        let code = t.split("//").next().unwrap_or("");
        let fixed_comment = t.chars().nth(6) == Some('*');
        !fixed_comment
            && code
                .split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|w| {
                    (w[0] == "EXEC" || w[0].ends_with("/EXEC"))
                        && w[1].trim_end_matches(';') == "SQL"
                })
    })
}

/// `/COPY` and `/INCLUDE` members, read from the store.
pub struct StoreIncludes<'a, 'b> {
    /// The system, shared with the file descriptions.
    pub system: &'a RefCell<&'b mut System>,
}

impl Includes for StoreIncludes<'_, '_> {
    fn member(&mut self, lib: &str, file: &str, name: &str) -> Option<Member> {
        let mut sys = self.system.borrow_mut();
        let (lib, file, mbr) = (
            lib.to_ascii_uppercase(),
            file.to_ascii_uppercase(),
            name.to_ascii_uppercase(),
        );
        in_store(&mut sys, |txn, qsys, names| {
            read(txn, qsys, names, &file, &lib, Some(&mbr))
        })
        .ok()
        .map(|s| s.member)
    }
}

/// The external descriptions of database files, through the APIs'
/// provider over the lent store.
pub struct StoreFiles<'a, 'b> {
    /// The system, shared with the includes.
    pub system: &'a RefCell<&'b mut System>,
}

impl rpgc::ExtFiles for StoreFiles<'_, '_> {
    fn describe(&mut self, name: &str, lib: &str) -> Option<ExtFile> {
        let mut sys = self.system.borrow_mut();
        let lib = lib_or(lib);
        if crate::svc::ile::lend(&mut sys).is_err() {
            return None;
        }
        let mut p = StoreProvider::new(sys.lent.clone());
        let info = p.file(&lib, name);
        let formats = p.formats(&lib, name);
        crate::svc::ile::restore(&mut sys);
        let info = info?;
        if info.attr != "PF" && info.attr != "LF" {
            // TODO(phase9): display and printer files' formats.
            return None;
        }
        Some(ExtFile {
            lib: info.lib,
            dev: Dev::Disk,
            formats: formats
                .into_iter()
                .map(|f| ExtFormat {
                    name: f.name,
                    level_id: String::new(),
                    len: f.rec_len,
                    fields: f
                        .fields
                        .into_iter()
                        .map(|x| ExtField {
                            ty: rpg_type(x.ty, x.len, x.digits, x.decimals),
                            name: x.name,
                            offset: x.pos.saturating_sub(1),
                            usage: 'B',
                            alias: String::new(),
                            nullable: false,
                        })
                        .collect(),
                    keys: Vec::new(),
                })
                .collect(),
        })
    }
}

/// A DDS data type as RPG's.
#[must_use]
pub fn rpg_type(ty: char, len: u32, digits: u32, decimals: u32) -> Ty {
    let d = u8::try_from(digits).unwrap_or(63);
    let s = u8::try_from(decimals).unwrap_or(0);
    match ty {
        'P' => Ty::Packed(d, s),
        'S' => Ty::Zoned(d, s),
        'B' => Ty::Bin(d, s),
        'F' => Ty::Float(if len == 4 { 4 } else { 8 }),
        'L' => Ty::Date(DatFmt::Iso, '-'),
        'T' => Ty::Time(TimFmt::Iso, '.'),
        'Z' => Ty::Timestamp(6),
        'G' => Ty::Graph(len / 2),
        _ => Ty::Char(len),
    }
}
