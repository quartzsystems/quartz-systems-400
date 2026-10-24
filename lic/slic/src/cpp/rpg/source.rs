// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RPG source from the store: a member with its sequence numbers and
//! change dates, the `/COPY` members it names, and the external
//! descriptions of the database and display files it declares.

use std::cell::RefCell;

use qsname::{MemberSpec, Name};
use rpgc::files::{Dev, DspLayout, ExtField, ExtFile, ExtFormat};
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
        Some(m) => MemberSpec::Name(Name::parse(m).map_err(|e| format!("CPD0078 SRCMBR({m}): {e}"))?),
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
        if let Some(d) = display_file(&mut sys, &lib, name) {
            return Some(d);
        }
        if crate::svc::ile::lend(&mut sys).is_err() {
            return None;
        }
        let mut p = StoreProvider::new(sys.lent.clone());
        let info = p.file(&lib, name);
        let formats = p.formats(&lib, name);
        crate::svc::ile::restore(&mut sys);
        let info = info?;
        if info.attr != "PF" && info.attr != "LF" {
            // TODO(phase9): printer files' formats.
            return None;
        }
        Some(ExtFile {
            lib: info.lib,
            dev: Dev::Disk,
            indara: false,
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
                    dsp: None,
                })
                .collect(),
        })
    }
}

/// `lib/name` as a display file, `None` when it is not one.
fn display_file(system: &mut System, lib: &str, name: &str) -> Option<ExtFile> {
    in_store(system, |txn, qsys, names| {
        let id = wm::obj::find(txn, qsys, names, lib, name, types::FILE).map_err(|e| e.to_string())?;
        let (_, h) = ops::read_header(txn, id).map_err(|e| e.to_string())?;
        if ccsid::decode_trimmed(&h.attribute) != crate::svc::db::dspf::ATTR {
            return Ok(None);
        }
        let found = ops::describe(txn, h.library).map_or_else(|_| lib.to_string(), |l| l.name.to_string());
        let df = dspf::decode(&wm::obj::read(txn, id).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        Ok(Some(display_desc(&df, &found)))
    })
    .ok()
    .flatten()
}

/// A display file as the compiler sees it: each record format's named
/// fields and both buffers' layouts.
#[must_use]
pub fn display_desc(df: &dspf::DisplayFile, lib: &str) -> ExtFile {
    use dspf::TypeClass;
    let formats = df
        .records
        .iter()
        .map(|r| {
            let named: Vec<(usize, &dspf::Field)> =
                r.fields.iter().enumerate().filter(|(_, f)| f.name.is_some()).collect();
            let out_of = |i: usize| r.layout.output.field(i).map(|b| b.offset);
            let in_of = |i: usize| r.layout.input.field(i).map(|b| b.offset);
            let ty = |f: &dspf::Field| {
                let d = u8::try_from(f.length).unwrap_or(63);
                match f.ty.class {
                    TypeClass::Char => Ty::Char(f.length),
                    TypeClass::Graphic => Ty::Graph(f.length),
                    TypeClass::Zoned => Ty::Zoned(d, f.decimals),
                    TypeClass::Packed => Ty::Packed(d, f.decimals),
                    TypeClass::Binary => Ty::Bin(d, f.decimals),
                    TypeClass::Float => Ty::Float(if f.bytes == 4 { 4 } else { 8 }),
                    TypeClass::Date => Ty::Date(DatFmt::Iso, '-'),
                    TypeClass::Time => Ty::Time(TimFmt::Iso, '.'),
                    TypeClass::Timestamp => Ty::Timestamp(6),
                }
            };
            ExtFormat {
                name: r.name.clone(),
                level_id: r.layout.level_id.clone(),
                len: r.layout.output.len.max(r.layout.input.len),
                fields: named
                    .iter()
                    .map(|(i, f)| ExtField {
                        name: f.name.clone().unwrap_or_default(),
                        ty: ty(f),
                        offset: out_of(*i).or_else(|| in_of(*i)).unwrap_or(0),
                        usage: f.usage.letter(),
                        alias: String::new(),
                        nullable: false,
                    })
                    .collect(),
                keys: Vec::new(),
                dsp: Some(DspLayout {
                    out_len: r.layout.output.len,
                    in_len: r.layout.input.len,
                    option_inds: r.layout.output.indicators.clone(),
                    response_inds: r.layout.input.indicators.clone(),
                    offsets: named.iter().map(|(i, _)| (out_of(*i), in_of(*i))).collect(),
                    subfile: r.kind == dspf::RecordKind::Subfile,
                    sflctl: r.subfile.as_ref().map(|s| s.record.clone()),
                }),
            }
        })
        .collect();
    ExtFile {
        lib: lib.to_string(),
        dev: Dev::Workstn,
        formats,
        indara: df.file.indara,
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
