// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's libraries, objects and source members (`Develop`,
//! version 8, draft), under the rules of the commands each operation
//! stands for:
//!
//! - every connection job has a library list of its own: system part
//!   `QSYS QSYS2 QHLPSYS QUSRSYS`, user part `QGPL QTEMP`, no current
//!   library; `CHGLIBL` refuses a library not there (`CPF2110`), one the
//!   profile may not use (`CPF2182`) and one named twice (`CPF2103`);
//! - libraries `HLINV` and `HLTEST` (`*TEST`), `QGPL`, `QSYS`, `QSYS2`,
//!   `QUSRSYS`, `QHLPSYS`, and `HLPAY`, which only `ANN` may use (`BOB`
//!   gets `CPF2182` and never sees it listed);
//! - source files `HLINV/QRPGLESRC` (data length 100, CCSID 37) with
//!   `ASSETS` (40 dated records), `STKRPT`, `LOCKED` (held `*SHRNUP` by
//!   job 4242, so `InUse` with `CPF1002`) and `BIG` (12,000 records, over
//!   1 MiB, so its `ReadMember` is split with `More`); `HLINV/QDDSSRC`
//!   (data length 80) with `ASSETS` and `ASSETSL1`; `HLTEST/QBINSRC`,
//!   CCSID 65535 (`TCP1A5A`); `HLPAY/QRPGLESRC` with `PAYROLL`;
//! - objects: those files, programs `HLINV/ASSETS` and `HLINV/STKRPT`
//!   (`RPGLE`), the data files `HLINV/ASSETS` (`PF`) and `ASSETSL1`
//!   (`LF`), `HLINV/HLSBS` (`*SBSD`), `QSYS/QCMD`; `HLINV/ASSETS *FILE`
//!   is not a source file (`ReadMember` answers `Error`, `CPF9898`);
//! - a file not there is `CPF9812`, a member `CPF9815`, an object
//!   `CPF9801`; a member taken `CPF5812`; data CCSID 37 cannot hold
//!   `TCP1A5A`; a record longer than the data length `TCP1A62`;
//!   `UnlockMember` of what the job does not hold `TCP1A61`.
//!
//! Dates: a `WriteMember` dates each changed or new record with the
//! system's clock ([`super::System::now`]).

use std::collections::HashMap;

use qap_proto::Status;
use qap_proto::ops::develop::{
    Attribute, ChangeMember, CopyMember, CreateMember, DescribeObject, Library, LibraryKind,
    LibraryList, ListMembers, ListObjects, MemberData, MemberEntry, ObjectDescription, ObjectEntry,
    RenameMember, SetLibraryList,
};
use qap_proto::types::{MemberName, ObjectName, SourceRecord};

use super::spool::No;
use crate::develop::{HELD, NO_CONVERSION, NOT_LOCKED, TOO_LONG, dated, generic};

/// The job that holds `HLINV/QRPGLESRC LOCKED`.
pub const OTHER_JOB: u32 = 4242;
/// Records in `BIG`.
pub const BIG_RECORDS: usize = 12_000;

fn no(status: Status, id: &'static str, data: &[&str]) -> No {
    (status, id, data.iter().map(ToString::to_string).collect())
}

fn up(s: &str) -> String {
    s.trim().to_ascii_uppercase()
}

/// A library.
#[derive(Clone, Debug)]
pub struct Lib {
    /// The library as listed.
    pub lib: Library,
    /// The only profile that may use it; `None` for everyone.
    pub only: Option<&'static str>,
}

/// A member of a source file.
#[derive(Clone, Debug, Default)]
pub struct Mbr {
    /// Its name.
    pub name: String,
    /// Its source type.
    pub srctype: String,
    /// Its text.
    pub text: String,
    /// When it was added and last changed, milliseconds.
    pub created: i64,
    /// Last changed.
    pub changed: i64,
    /// Its records.
    pub records: Vec<SourceRecord>,
}

/// A source physical file.
#[derive(Clone, Debug)]
pub struct SrcFile {
    /// Its library.
    pub lib: String,
    /// Its name.
    pub name: String,
    /// A record's data length.
    pub width: u16,
    /// Its CCSID.
    pub ccsid: u16,
    /// Its members.
    pub members: Vec<Mbr>,
}

/// A lock on a member: the holder.
type Key = (String, String, String);

/// The test system's development state.
#[derive(Default)]
pub struct Develop {
    /// The libraries.
    pub libs: Vec<Lib>,
    /// Every object but the source files' members.
    pub objects: Vec<ObjectEntry>,
    /// The source files.
    pub files: Vec<SrcFile>,
    /// Members held `*SHRNUP`, and by which job.
    pub locks: HashMap<Key, u32>,
    /// Each connection job's library list, once it has one.
    pub libl: HashMap<u32, LibraryList>,
}

const T0: i64 = 1_790_000_000_000;

fn record(n: u32, date: u32, text: &str) -> SourceRecord {
    SourceRecord {
        seq: n * 100,
        date,
        data: text.to_string(),
    }
}

/// `ASSETS`: a small free-form program, its records dated over a month.
fn assets() -> Vec<SourceRecord> {
    let mut v = vec![
        record(1, 260_105, "**FREE"),
        record(2, 260_105, "ctl-opt dftactgrp(*no) actgrp(*caller);"),
        record(3, 260_105, "dcl-f ASSETS keyed usage(*input);"),
        record(4, 260_112, "dcl-s count int(10) inz(0);"),
    ];
    for i in 5..=39u32 {
        v.push(record(
            i,
            260_112 + (i % 7),
            &format!("// line {i}: read every asset and count it"),
        ));
    }
    v.push(record(40, 260_120, "*inlr = *on;"));
    v
}

fn member(name: &str, srctype: &str, text: &str, records: Vec<SourceRecord>) -> Mbr {
    Mbr {
        name: name.into(),
        srctype: srctype.into(),
        text: text.into(),
        created: T0,
        changed: T0 + 86_400_000,
        records,
    }
}

fn object(lib: &str, name: &str, ty: &str, attr: &str, text: &str, size: u64) -> ObjectEntry {
    ObjectEntry {
        library: lib.into(),
        name: name.into(),
        r#type: ty.into(),
        attribute: attr.into(),
        text: text.into(),
        size,
        changed: T0,
        owner: "QPGMR".into(),
    }
}

fn lib(name: &str, kind: LibraryKind, text: &str, only: Option<&'static str>) -> Lib {
    Lib {
        lib: Library {
            name: name.into(),
            kind,
            text: text.into(),
        },
        only,
    }
}

impl Develop {
    /// The fixtures.
    #[must_use]
    #[allow(clippy::too_many_lines)] // the fixtures
    pub fn shipped() -> Self {
        use LibraryKind::{Production, Test};
        let libs = vec![
            lib("HLINV", Test, "Hardware inventory", None),
            lib("HLPAY", Production, "Payroll", Some("ANN")),
            lib("HLTEST", Test, "Test data", None),
            lib("QGPL", Production, "General purpose library", None),
            lib("QHLPSYS", Production, "Help panels", None),
            lib("QSYS", Production, "System library", None),
            lib("QSYS2", Production, "System catalog", None),
            lib("QUSRSYS", Production, "User system data", None),
        ];
        let big: Vec<SourceRecord> = (1..=u32::try_from(BIG_RECORDS).unwrap_or(0))
            .map(|i| record(i, 260_101, &format!("// {i:05} {}", "x".repeat(84))))
            .collect();
        let files = vec![
            SrcFile {
                lib: "HLINV".into(),
                name: "QRPGLESRC".into(),
                width: 100,
                ccsid: 37,
                members: vec![
                    member("ASSETS", "RPGLE", "Asset list", assets()),
                    member("BIG", "RPGLE", "A member over 1 MiB", big),
                    member(
                        "LOCKED",
                        "RPGLE",
                        "Held by another job",
                        vec![record(1, 260_101, "**FREE")],
                    ),
                    member(
                        "STKRPT",
                        "RPGLE",
                        "Stock report",
                        vec![
                            record(1, 260_101, "**FREE"),
                            record(2, 260_101, "*inlr = *on;"),
                        ],
                    ),
                ],
            },
            SrcFile {
                lib: "HLINV".into(),
                name: "QDDSSRC".into(),
                width: 80,
                ccsid: 37,
                members: vec![
                    member(
                        "ASSETS",
                        "PF",
                        "Assets",
                        vec![record(1, 260_101, "     A          R ASSETR")],
                    ),
                    member(
                        "ASSETSL1",
                        "LF",
                        "Assets by location",
                        vec![record(
                            1,
                            260_101,
                            "     A          R ASSETR  PFILE(ASSETS)",
                        )],
                    ),
                ],
            },
            SrcFile {
                lib: "HLTEST".into(),
                name: "QBINSRC".into(),
                width: 80,
                ccsid: 65535,
                members: vec![member("RAW", "TXT", "Binary", vec![record(1, 0, "x")])],
            },
            SrcFile {
                lib: "HLPAY".into(),
                name: "QRPGLESRC".into(),
                width: 100,
                ccsid: 37,
                members: vec![member("PAYROLL", "RPGLE", "Payroll", vec![])],
            },
        ];
        let mut objects: Vec<ObjectEntry> = files
            .iter()
            .map(|f| object(&f.lib, &f.name, "*FILE", "PF", "Source", 65_536))
            .collect();
        objects.extend([
            object("HLINV", "ASSETS", "*PGM", "RPGLE", "Asset list", 196_608),
            object("HLINV", "ASSETS", "*FILE", "PF", "Assets", 73_728),
            object(
                "HLINV",
                "ASSETSL1",
                "*FILE",
                "LF",
                "Assets by location",
                40_960,
            ),
            object("HLINV", "HLSBS", "*SBSD", "", "Inventory subsystem", 8192),
            object("HLINV", "STKRPT", "*PGM", "RPGLE", "Stock report", 131_072),
            object("QSYS", "QCMD", "*PGM", "CLP", "Command Entry", 32_768),
        ]);
        let mut locks = HashMap::new();
        locks.insert(
            ("HLINV".into(), "QRPGLESRC".into(), "LOCKED".into()),
            OTHER_JOB,
        );
        Self {
            libs,
            objects,
            files,
            locks,
            libl: HashMap::new(),
        }
    }

    /// Job `job`'s library list.
    #[must_use]
    pub fn libl(&self, job: u32) -> LibraryList {
        self.libl.get(&job).cloned().unwrap_or_else(|| LibraryList {
            current: None,
            system: ["QSYS", "QSYS2", "QHLPSYS", "QUSRSYS"]
                .map(String::from)
                .to_vec(),
            user: vec!["QGPL".into(), "QTEMP".into()],
        })
    }

    /// Library `name` for `me`: `CPF2110` not there, `CPF2182` not
    /// allowed. `QTEMP` is every job's.
    fn library(&self, me: &str, name: &str) -> Result<(), No> {
        let name = up(name);
        if name == "QTEMP" {
            return Ok(());
        }
        let Some(l) = self.libs.iter().find(|l| l.lib.name == name) else {
            return Err(no(Status::NotFound, "CPF2110", &[&name]));
        };
        if l.only.is_some_and(|o| o != me) {
            return Err(no(Status::NotAuthorized, "CPF2182", &[&name]));
        }
        Ok(())
    }

    /// `CHGLIBL`.
    pub fn set_libl(
        &mut self,
        job: u32,
        me: &str,
        req: &SetLibraryList,
    ) -> Result<LibraryList, No> {
        let user: Vec<String> = req.user.iter().map(|l| up(l)).collect();
        for (i, l) in user.iter().enumerate() {
            self.library(me, l)?;
            if user[..i].contains(l) {
                return Err(no(Status::Error, "CPF2103", &[l]));
            }
        }
        let current = req.current.as_deref().map(up);
        if let Some(c) = &current {
            self.library(me, c)?;
        }
        let mut l = self.libl(job);
        l.user = user;
        l.current = current;
        self.libl.insert(job, l.clone());
        Ok(l)
    }

    /// Libraries `me` may see, matching `pattern`, by name.
    #[must_use]
    pub fn libraries(&self, me: &str, pattern: &str) -> Vec<Library> {
        self.libs
            .iter()
            .filter(|l| l.only.is_none_or(|o| o == me) && generic(&l.lib.name, pattern))
            .map(|l| l.lib.clone())
            .collect()
    }

    /// The libraries a `library` value names, in search order.
    fn search(&self, job: u32, me: &str, library: &str) -> Result<Vec<String>, No> {
        let l = self.libl(job);
        Ok(match up(library).as_str() {
            "*LIBL" | "" => l
                .system
                .iter()
                .chain(l.current.iter())
                .chain(l.user.iter())
                .cloned()
                .collect(),
            "*CURLIB" => vec![l.current.unwrap_or_else(|| "QGPL".into())],
            "*USRLIBL" => l.current.iter().chain(l.user.iter()).cloned().collect(),
            "*ALL" => self
                .libraries(me, "*ALL")
                .into_iter()
                .map(|l| l.name)
                .collect(),
            name => {
                self.library(me, name)?;
                vec![name.to_string()]
            }
        })
    }

    /// `WRKOBJ`.
    pub fn objects(&self, job: u32, me: &str, req: &ListObjects) -> Result<Vec<ObjectEntry>, No> {
        let libs = self.search(job, me, &req.library)?;
        let types: Vec<String> = req.types.iter().map(|t| up(t)).collect();
        let mut out = Vec::new();
        for lib in libs {
            let mut here: Vec<ObjectEntry> = self
                .objects
                .iter()
                .filter(|o| {
                    o.library == lib
                        && generic(&o.name, &req.pattern)
                        && (types.is_empty() || types.contains(&o.r#type))
                        && req.attribute.as_ref().is_none_or(|a| up(a) == o.attribute)
                })
                .cloned()
                .collect();
            here.sort_by(|a, b| (&a.name, &a.r#type).cmp(&(&b.name, &b.r#type)));
            out.extend(here);
        }
        Ok(out)
    }

    /// The library an object or file's `library` resolves to, through
    /// `job`'s list: the first that has `name` as `ty`.
    fn resolve(&self, job: u32, me: &str, o: &ObjectName, ty: &str) -> Result<String, No> {
        let name = up(&o.name);
        let found = self.search(job, me, &o.library)?.into_iter().find(|l| {
            self.objects
                .iter()
                .any(|x| x.library == *l && x.name == name && x.r#type == ty)
        });
        found.ok_or_else(|| {
            let lib = up(&o.library);
            match ty {
                "*FILE" => no(Status::NotFound, "CPF9812", &[&name, &lib]),
                _ => no(Status::NotFound, "CPF9801", &[&name, &lib, ty]),
            }
        })
    }

    /// `DSPOBJD`.
    pub fn describe(
        &self,
        job: u32,
        me: &str,
        req: &DescribeObject,
    ) -> Result<ObjectDescription, No> {
        let ty = up(&req.r#type);
        let lib = self.resolve(job, me, &req.object, &ty)?;
        let o = self
            .objects
            .iter()
            .find(|x| x.library == lib && x.name == up(&req.object.name) && x.r#type == ty)
            .cloned()
            .unwrap_or_default();
        let a = |n: &str, v: String| Attribute {
            name: n.into(),
            value: v,
        };
        Ok(ObjectDescription {
            attributes: vec![
                a("Object", o.name),
                a("Library", o.library),
                a("Type", o.r#type),
                a("Attribute", o.attribute),
                a("Owner", o.owner),
                a("Text", o.text),
                a("Changed", o.changed.to_string()),
                a("Size", o.size.to_string()),
            ],
        })
    }

    fn file_index(&self, job: u32, me: &str, f: &ObjectName) -> Result<usize, No> {
        let lib = self.resolve(job, me, f, "*FILE")?;
        let name = up(&f.name);
        self.files
            .iter()
            .position(|s| s.lib == lib && s.name == name)
            .ok_or_else(|| no(Status::Error, "CPF9898", &[&name, &lib]))
    }

    fn member_index(&self, job: u32, me: &str, m: &MemberName) -> Result<(usize, usize), No> {
        let f = self.file_index(job, me, &m.file)?;
        let name = up(&m.member);
        let file = &self.files[f];
        file.members
            .iter()
            .position(|x| x.name == name)
            .map(|i| (f, i))
            .ok_or_else(|| no(Status::NotFound, "CPF9815", &[&file.name, &file.lib, &name]))
    }

    fn key_of(&self, f: usize, m: usize) -> Key {
        let file = &self.files[f];
        (
            file.lib.clone(),
            file.name.clone(),
            file.members[m].name.clone(),
        )
    }

    /// Another job than `job` holds the member.
    fn held(&self, job: u32, f: usize, m: usize) -> Option<No> {
        let k = self.key_of(f, m);
        self.locks
            .get(&k)
            .filter(|j| **j != job)
            .map(|_| no(Status::InUse, HELD, &[&k.2, &k.0, "*MBR"]))
    }

    /// `WRKMBRPDM`.
    pub fn members(&self, job: u32, me: &str, req: &ListMembers) -> Result<Vec<MemberEntry>, No> {
        let f = self.file_index(job, me, &req.file)?;
        let mut v: Vec<MemberEntry> = self.files[f]
            .members
            .iter()
            .filter(|m| generic(&m.name, &req.pattern))
            .map(|m| MemberEntry {
                name: m.name.clone(),
                srctype: m.srctype.clone(),
                text: m.text.clone(),
                records: u32::try_from(m.records.len()).unwrap_or(u32::MAX),
                created: m.created,
                changed: m.changed,
            })
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(v)
    }

    /// `STRSEU`'s read; with `lock`, held for `job`.
    pub fn read(
        &mut self,
        job: u32,
        me: &str,
        m: &MemberName,
        lock: bool,
    ) -> Result<MemberData, No> {
        let (f, i) = self.member_index(job, me, m)?;
        let file = &self.files[f];
        if file.ccsid == 65535 {
            return Err(no(Status::Error, NO_CONVERSION, &["65535", "1208"]));
        }
        if lock && let Some(n) = self.held(job, f, i) {
            return Err(n);
        }
        let mbr = &file.members[i];
        let data = MemberData {
            srctype: mbr.srctype.clone(),
            text: mbr.text.clone(),
            width: file.width,
            ccsid: file.ccsid,
            records: mbr.records.clone(),
        };
        if lock {
            let k = self.key_of(f, i);
            self.locks.insert(k, job);
        }
        Ok(data)
    }

    /// `STRSEU`'s save: the records dated, converted, written whole.
    #[allow(clippy::too_many_arguments)] // the member and what a write sets
    pub fn write(
        &mut self,
        job: u32,
        me: &str,
        name: &MemberName,
        records: Vec<SourceRecord>,
        renumber: bool,
        (srctype, text): (Option<&str>, Option<&str>),
        (today, now_ms): (u32, i64),
    ) -> Result<u32, No> {
        let (f, i) = self.member_index(job, me, name)?;
        if let Some(n) = self.held(job, f, i) {
            return Err(n);
        }
        let file = &self.files[f];
        if file.ccsid == 65535 {
            return Err(no(Status::Error, NO_CONVERSION, &["1208", "65535"]));
        }
        let codec = ccsid::conv::Codec::of(file.ccsid).ok().flatten();
        for r in &records {
            let mut bytes = Vec::new();
            let exact = r
                .data
                .chars()
                .all(|c| codec.as_ref().is_some_and(|k| k.encode(c, &mut bytes)));
            if !exact {
                return Err(no(
                    Status::Error,
                    NO_CONVERSION,
                    &["1208", &file.ccsid.to_string()],
                ));
            }
            if bytes.len() > usize::from(file.width) {
                let seq = format!("{:04}.{:02}", r.seq / 100, r.seq % 100);
                return Err(no(
                    Status::InvalidRequest,
                    TOO_LONG,
                    &[&seq, &file.width.to_string()],
                ));
            }
        }
        let k = self.key_of(f, i);
        let mbr = &mut self.files[f].members[i];
        let new: Vec<SourceRecord> = dated(&mbr.records, records, renumber, today)
            .into_iter()
            .map(|mut r| {
                r.data = r.data.trim_end().to_string();
                r
            })
            .collect();
        mbr.records = new;
        if let Some(t) = srctype {
            mbr.srctype = up(t);
        }
        if let Some(t) = text {
            mbr.text = t.to_string();
        }
        mbr.changed = now_ms;
        let n = u32::try_from(mbr.records.len()).unwrap_or(u32::MAX);
        if self.locks.get(&k) == Some(&job) {
            self.locks.remove(&k);
        }
        Ok(n)
    }

    /// `DLCOBJ` of the job's own lock.
    pub fn unlock(&mut self, job: u32, me: &str, m: &MemberName) -> Result<(), No> {
        let (f, i) = self.member_index(job, me, m)?;
        let k = self.key_of(f, i);
        if self.locks.get(&k) == Some(&job) {
            self.locks.remove(&k);
            Ok(())
        } else {
            let name = format!("{}/{} {}", k.0, k.1, k.2);
            Err(no(Status::NotFound, NOT_LOCKED, &[&name]))
        }
    }

    /// The job's connection closed: its locks go.
    pub fn release(&mut self, job: u32) {
        self.locks.retain(|_, j| *j != job);
        self.libl.remove(&job);
    }

    /// `ADDPFM`.
    pub fn create(
        &mut self,
        job: u32,
        me: &str,
        req: &CreateMember,
        now_ms: i64,
    ) -> Result<(), No> {
        let f = self.file_index(job, me, &req.member.file)?;
        let name = up(&req.member.member);
        let file = &mut self.files[f];
        if file.members.iter().any(|m| m.name == name) {
            return Err(no(
                Status::Exists,
                "CPF5812",
                &[&name, &file.name, &file.lib],
            ));
        }
        file.members.push(Mbr {
            name,
            srctype: up(&req.srctype),
            text: req.text.clone(),
            created: now_ms,
            changed: now_ms,
            records: Vec::new(),
        });
        Ok(())
    }

    /// `CPYSRCF` of one member, sequence numbers and dates kept.
    pub fn copy(&mut self, job: u32, me: &str, req: &CopyMember, now_ms: i64) -> Result<(), No> {
        let (f, i) = self.member_index(job, me, &req.from)?;
        let from = self.files[f].members[i].clone();
        let t = self.file_index(job, me, &req.to.file)?;
        let name = up(&req.to.member);
        if let Some(j) = self.files[t].members.iter().position(|m| m.name == name) {
            if !req.replace {
                let file = &self.files[t];
                return Err(no(
                    Status::Exists,
                    "CPF5812",
                    &[&name, &file.name, &file.lib],
                ));
            }
            if let Some(n) = self.held(job, t, j) {
                return Err(n);
            }
            let m = &mut self.files[t].members[j];
            m.records = from.records;
            m.srctype = from.srctype;
            m.changed = now_ms;
            return Ok(());
        }
        self.files[t].members.push(Mbr {
            name,
            created: now_ms,
            changed: now_ms,
            ..from
        });
        Ok(())
    }

    /// `RNMM`.
    pub fn rename(&mut self, job: u32, me: &str, req: &RenameMember) -> Result<(), No> {
        let (f, i) = self.member_index(job, me, &req.member)?;
        if let Some(n) = self.held(job, f, i) {
            return Err(n);
        }
        let new = up(&req.new_name);
        let file = &mut self.files[f];
        if file.members.iter().any(|m| m.name == new) {
            return Err(no(
                Status::Exists,
                "CPF5812",
                &[&new, &file.name, &file.lib],
            ));
        }
        file.members[i].name = new;
        Ok(())
    }

    /// `RMVM`.
    pub fn delete(&mut self, job: u32, me: &str, m: &MemberName) -> Result<(), No> {
        let (f, i) = self.member_index(job, me, m)?;
        if let Some(n) = self.held(job, f, i) {
            return Err(n);
        }
        self.files[f].members.remove(i);
        Ok(())
    }

    /// `CHGPFM`.
    pub fn change(
        &mut self,
        job: u32,
        me: &str,
        req: &ChangeMember,
        now_ms: i64,
    ) -> Result<(), No> {
        let (f, i) = self.member_index(job, me, &req.member)?;
        if let Some(n) = self.held(job, f, i) {
            return Err(n);
        }
        let m = &mut self.files[f].members[i];
        if let Some(t) = &req.srctype {
            m.srctype = up(t);
        }
        if let Some(t) = &req.text {
            m.text.clone_from(t);
        }
        m.changed = now_ms;
        Ok(())
    }
}
