// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RPG source from the store: a member with its sequence numbers and
//! change dates, the `/COPY` members it names, and the external
//! descriptions of the database, display and printer files it declares.

use std::cell::RefCell;

use qsname::{MemberSpec, Name};
use rpgc::files::ExtFile;
use rpgextdesc::{database_desc, display_desc, printer_desc};
use rpgfront::source::Record;
use rpgfront::{Includes, Member};
use store::Txn;
use store::obj::{ObjectId, ops, types};

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
        Some(m) => {
            MemberSpec::Name(Name::parse(m).map_err(|e| format!("CPD0078 SRCMBR({m}): {e}"))?)
        }
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

/// `/COPY` and `/INCLUDE` members, read from the store, and stream
/// files, read through the IFS as the job.
pub struct StoreIncludes<'a, 'b> {
    /// The system, shared with the file descriptions.
    pub system: &'a RefCell<&'b mut System>,
    /// `INCDIR`.
    pub incdir: Vec<String>,
}

impl Includes for StoreIncludes<'_, '_> {
    fn member(&mut self, lib: &str, file: &str, name: &str) -> Option<Member> {
        let mut sys = self.system.borrow_mut();
        if lib == rpgfront::reader::STMF {
            let mut read = |p: &str| super::super::stmf::read_as_job(&mut sys, p).ok();
            return super::super::stmf::find(&mut read, name, &self.incdir, file);
        }
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
        if let Some(d) = device_file(&mut sys, &lib, name) {
            return Some(d);
        }
        if crate::svc::ile::lend(&mut sys).is_err() {
            return None;
        }
        let desc = StoreProvider::new(sys.lent.clone()).file_desc(&lib, name);
        crate::svc::ile::restore(&mut sys);
        desc.map(|(found, d)| ExtFile {
            lib: found,
            ..database_desc(&d)
        })
    }
}

/// `lib/name` as a display or printer file, `None` when it is neither.
fn device_file(system: &mut System, lib: &str, name: &str) -> Option<ExtFile> {
    in_store(system, |txn, qsys, names| {
        device_in(txn, qsys, names, lib, name)
    })
    .ok()
    .flatten()
}

/// [`device_file`] in a transaction. A program-described printer file
/// has no formats: `None`.
///
/// # Errors
///
/// The file is not found or its object cannot be read.
pub fn device_in(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    lib: &str,
    name: &str,
) -> Result<Option<ExtFile>, String> {
    let id = wm::obj::find(txn, qsys, names, lib, name, types::FILE).map_err(|e| e.to_string())?;
    let (_, h) = ops::read_header(txn, id).map_err(|e| e.to_string())?;
    let attr = ccsid::decode_trimmed(&h.attribute);
    let is_dsp = attr == crate::svc::db::dspf::ATTR;
    if !is_dsp && attr != "PRTF" {
        return Ok(None);
    }
    let found =
        ops::describe(txn, h.library).map_or_else(|_| lib.to_string(), |l| l.name.to_string());
    let payload = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
    if is_dsp {
        let df = dspf::decode(&payload).map_err(|e| e.to_string())?;
        return Ok(Some(display_desc(&df, &found)));
    }
    let f = wm::obj::prtf::Prtf::decode(&payload).map_err(|e| e.to_string())?;
    Ok(dds::prtf::codec::decode(&f.formats).map(|def| printer_desc(&def, &found)))
}
