// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPFD`: the file, its attributes, access paths, members, record
//! formats, select/omit and join specifications.

use std::fmt::Write as _;

use dds::Validity;
use qsname::Name;
use store::obj::{ObjectId, ops};
use store::txn::Txn;

use super::{kv, library_name, timestamp, wait_text, yes_no};
use crate::error::Result;
use crate::file::{self, FileAttr, FileKind};
use crate::format::FormatDef;
use crate::member::Core;

/// `TYPE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FdType {
    /// `*ALL`.
    #[default]
    All,
    /// `*BASATR`.
    BasAtr,
    /// `*ATR`.
    Atr,
    /// `*ACCPTH`.
    AccPth,
    /// `*MBRLIST`.
    MbrList,
    /// `*MBR`.
    Mbr,
    /// `*RCDFMT`.
    RcdFmt,
    /// `*SELECT`.
    Select,
    /// `*JOIN`.
    Join,
}

impl FdType {
    /// Parse `*ALL` and the rest.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word.to_ascii_uppercase().as_str() {
            "*ALL" => Self::All,
            "*BASATR" => Self::BasAtr,
            "*ATR" => Self::Atr,
            "*ACCPTH" => Self::AccPth,
            "*MBRLIST" => Self::MbrList,
            "*MBR" => Self::Mbr,
            "*RCDFMT" => Self::RcdFmt,
            "*SELECT" => Self::Select,
            "*JOIN" => Self::Join,
            _ => return None,
        })
    }
}

/// One member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberInfo {
    /// Name.
    pub name: Name,
    /// `SRCTYPE`.
    pub srctype: Option<Name>,
    /// Text.
    pub text: String,
    /// Created, Unix seconds.
    pub created: u64,
    /// Changed, Unix seconds.
    pub changed: u64,
    /// Active records.
    pub records: u64,
    /// Deleted records.
    pub deleted: u64,
    /// Slots in use.
    pub slots: u64,
    /// Bytes of record space.
    pub data_bytes: u64,
    /// Access path entries.
    pub path_entries: u64,
    /// Whether the path is valid.
    pub path_valid: bool,
    /// Logical member: the based-on members.
    pub based_on: Vec<ObjectId>,
}

/// A file described.
#[derive(Clone, Debug)]
pub struct FileDescription {
    /// Name.
    pub name: Name,
    /// Library.
    pub library: String,
    /// Kind.
    pub kind: FileKind,
    /// Text.
    pub text: String,
    /// Owner.
    pub owner: String,
    /// Created.
    pub created: u64,
    /// Changed.
    pub changed: u64,
    /// Attributes.
    pub attr: FileAttr,
    /// Formats.
    pub formats: Vec<FormatDef>,
    /// Members.
    pub members: Vec<MemberInfo>,
    /// Logical file: based-on files as `library/file` with the record
    /// format each supplies.
    pub based_on: Vec<(String, Name)>,
}

/// Describe the file.
pub fn dspfd(txn: &mut Txn<'_>, file: ObjectId) -> Result<FileDescription> {
    crate::auth::check(txn, "file.describe", file)?;
    let desc = file::describe(txn, file)?;
    let mut members = Vec::new();
    for e in &desc.members {
        let (_, h) = ops::read_header(txn, e.object)?;
        let core = Core::load(txn, e.object)?;
        let based_on = if desc.attr.kind == FileKind::Logical {
            core.base_members(txn)?
        } else {
            Vec::new()
        };
        members.push(MemberInfo {
            name: e.name,
            srctype: e.srctype,
            text: ccsid::decode_trimmed(&h.text),
            created: h.created,
            changed: h.changed,
            records: core.attr.record_count,
            deleted: core.attr.deleted_count,
            slots: core.attr.slots,
            data_bytes: core.attr.slots * u64::from(core.attr.slot_len),
            path_entries: core.attr.path_entries,
            path_valid: !core.attr.path_invalid,
            based_on,
        });
    }
    let mut based_on = Vec::new();
    for f in &desc.formats {
        if let Some(l) = &f.logical {
            for (id, m) in &l.based_on {
                let (_, h) = ops::read_header(txn, *id)?;
                let lib = library_name(txn, h.library);
                let entry = (format!("{lib}/{}", h.name), *m);
                if !based_on.contains(&entry) {
                    based_on.push(entry);
                }
            }
        }
    }
    Ok(FileDescription {
        name: desc.name(),
        library: library_name(txn, desc.header.library),
        kind: desc.attr.kind,
        text: ccsid::decode_trimmed(&desc.header.text),
        owner: ccsid::decode_trimmed(&desc.header.owner),
        created: desc.header.created,
        changed: desc.header.changed,
        attr: desc.attr,
        formats: desc.formats,
        members,
        based_on,
    })
}

impl FileDescription {
    /// The `TYPE(*BASATR)` name of the kind.
    #[must_use]
    pub fn kind_text(&self) -> &'static str {
        match self.kind {
            FileKind::Physical => "PF",
            FileKind::Logical => "LF",
            FileKind::Source => "PF-SRC",
        }
    }

    /// Render the sections `ty` asks for.
    #[must_use]
    pub fn render(&self, ty: FdType) -> String {
        let mut out = String::new();
        let want = |t: FdType| ty == FdType::All || ty == t;
        if want(FdType::BasAtr) {
            self.basatr(&mut out);
        }
        if want(FdType::Atr) {
            self.atr(&mut out);
        }
        if want(FdType::AccPth) {
            self.accpth(&mut out);
        }
        if want(FdType::RcdFmt) {
            self.rcdfmt(&mut out);
        }
        if want(FdType::Select) && self.kind == FileKind::Logical {
            self.select(&mut out);
        }
        if want(FdType::Join) && self.kind == FileKind::Logical {
            self.join(&mut out);
        }
        if want(FdType::MbrList) || want(FdType::Mbr) {
            self.members(&mut out, ty != FdType::MbrList);
        }
        out
    }

    fn basatr(&self, out: &mut String) {
        out.push_str("File description\n");
        kv(out, "File", self.name);
        kv(out, "Library", &self.library);
        kv(out, "Type of file", self.kind_text());
        kv(out, "Text", &self.text);
        kv(out, "Owner", &self.owner);
        kv(out, "Created", timestamp(self.created));
        kv(out, "Changed", timestamp(self.changed));
        kv(out, "Record formats", self.formats.len());
        kv(out, "Members", self.members.len());
    }

    fn atr(&self, out: &mut String) {
        let a = &self.attr;
        out.push_str("Attributes\n");
        kv(out, "Record length", a.record_len);
        let max = if a.max_members == 0 {
            "*NOMAX".to_string()
        } else {
            a.max_members.to_string()
        };
        kv(out, "Maximum members                 MAXMBRS", max);
        let size = if a.size.initial == 0 {
            "*NOMAX".to_string()
        } else {
            format!(
                "{} {} {}",
                a.size.initial, a.size.increment, a.size.max_increments
            )
        };
        kv(out, "Member size                     SIZE", size);
        kv(
            out,
            "Reuse deleted records           REUSEDLT",
            yes_no(a.reusedlt),
        );
        kv(out, "Access path maintenance         MAINT", a.maint.name());
        kv(
            out,
            "Access path recovery            RECOVER",
            a.recover.name(),
        );
        kv(
            out,
            "Force keyed access path         FRCACCPTH",
            yes_no(a.frcaccpth),
        );
        kv(
            out,
            "Share open data path            SHARE",
            yes_no(a.share),
        );
        kv(
            out,
            "Record format level check       LVLCHK",
            yes_no(a.lvlchk),
        );
        kv(
            out,
            "Allow update operation          ALWUPD",
            yes_no(a.alwupd),
        );
        kv(
            out,
            "Allow delete operation          ALWDLT",
            yes_no(a.alwdlt),
        );
        kv(
            out,
            "Allocate storage                ALLOCATE",
            yes_no(a.allocate),
        );
        let frc = if a.frcratio == 0 {
            "*NONE".to_string()
        } else {
            a.frcratio.to_string()
        };
        kv(out, "Records to force a write        FRCRATIO", frc);
        kv(
            out,
            "Maximum file wait time          WAITFILE",
            wait_text(a.wait_file, true),
        );
        kv(
            out,
            "Maximum record wait time        WAITRCD",
            wait_text(a.wait_rcd, false),
        );
        kv(out, "Coded character set identifier  CCSID", a.ccsid);
        kv(
            out,
            "Expiration date                 EXPDATE",
            timestamp(a.expdate),
        );
        if self.kind == FileKind::Logical {
            kv(
                out,
                "Dynamic selection               DYNSLT",
                yes_no(a.dynslt),
            );
            kv(
                out,
                "Join default values             JDFTVAL",
                yes_no(a.jdftval),
            );
        }
    }

    fn accpth(&self, out: &mut String) {
        out.push_str("Access path\n");
        for f in &self.formats {
            if f.format.keys.is_empty() {
                kv(
                    out,
                    &format!("Format {}", f.format.name),
                    "arrival sequence",
                );
                continue;
            }
            kv(out, &format!("Format {}", f.format.name), "keyed");
            let unique = match f.unique {
                None => "*NO".to_string(),
                Some(nk) => format!("*YES {nk:?}").to_ascii_uppercase(),
            };
            kv(out, "  Unique key                    UNIQUE", unique);
            if let Some(d) = f.dup_order {
                kv(out, "  Duplicate key order", d.name());
            }
            kv(out, "  Key length", f.key_len);
            for (i, k) in f.format.keys.iter().enumerate() {
                kv(
                    out,
                    &format!("  Key field {}", i + 1),
                    format!("{}  {}", k.name, k.describe()),
                );
            }
        }
        for m in &self.members {
            if self.formats.iter().any(|f| !f.format.keys.is_empty()) {
                kv(
                    out,
                    &format!("Member {} path entries", m.name),
                    format!(
                        "{}{}",
                        m.path_entries,
                        if m.path_valid {
                            ""
                        } else {
                            "  (rebuild pending)"
                        }
                    ),
                );
            }
        }
    }

    fn rcdfmt(&self, out: &mut String) {
        out.push_str("Record formats\n");
        for f in &self.formats {
            kv(out, "Format", f.format.name);
            kv(out, "  Level identifier", &f.level_id);
            kv(out, "  Record length", f.format.record_len);
            kv(out, "  Fields", f.format.fields.len());
            if let Some(t) = &f.format.text {
                kv(out, "  Text", t);
            }
        }
    }

    fn select(&self, out: &mut String) {
        out.push_str("Select/omit\n");
        for f in &self.formats {
            let Some(l) = &f.logical else { continue };
            if l.select_omit.is_empty() {
                kv(out, &format!("Format {}", f.format.name), "none");
                continue;
            }
            kv(out, &format!("Format {}", f.format.name), "");
            for s in &l.select_omit {
                let kind = if s.select { "S" } else { "O" };
                if s.all {
                    let _ = writeln!(out, "  {kind}  ALL");
                    continue;
                }
                for c in &s.conditions {
                    let _ = writeln!(out, "  {kind}  {}  {}", c.field, validity_text(&c.test));
                }
            }
        }
    }

    fn join(&self, out: &mut String) {
        out.push_str("Join\n");
        for (i, (f, m)) in self.based_on.iter().enumerate() {
            kv(
                out,
                &format!("Based-on file {}", i + 1),
                format!("{f}  format {m}"),
            );
        }
        for f in &self.formats {
            let Some(l) = &f.logical else { continue };
            for j in &l.joins {
                let pairs: Vec<String> =
                    j.fields.iter().map(|(a, b)| format!("{a} = {b}")).collect();
                kv(
                    out,
                    &format!("  Join {} to {}", j.from, j.to),
                    pairs.join(", "),
                );
                if let Some((n, desc)) = &j.dupseq {
                    kv(
                        out,
                        "    Duplicate sequence           JDUPSEQ",
                        format!("{n} {}", if *desc { "*DESCEND" } else { "*ASCEND" }),
                    );
                }
            }
        }
    }

    fn members(&self, out: &mut String, full: bool) {
        out.push_str("Members\n");
        for m in &self.members {
            kv(out, "Member", m.name);
            if let Some(st) = m.srctype {
                kv(out, "  Source type                   SRCTYPE", st);
            }
            kv(out, "  Text", &m.text);
            kv(out, "  Current records", m.records);
            kv(out, "  Deleted records", m.deleted);
            if full {
                kv(out, "  Created", timestamp(m.created));
                kv(out, "  Changed", timestamp(m.changed));
                kv(out, "  Record slots", m.slots);
                kv(out, "  Data size (bytes)", m.data_bytes);
                kv(out, "  Access path entries", m.path_entries);
                kv(out, "  Access path valid", yes_no(m.path_valid));
            }
        }
    }
}

/// A select/omit test as DDS spells it.
fn validity_text(v: &Validity) -> String {
    match v {
        Validity::Comp(op, lit) => format!("COMP({} {lit})", op.name()),
        Validity::Range(lo, hi) => format!("RANGE({lo} {hi})"),
        Validity::Values(vals) => {
            let v: Vec<String> = vals.iter().map(ToString::to_string).collect();
            format!("VALUES({})", v.join(" "))
        }
        Validity::Check(words) => format!("CHECK({})", words.join(" ")),
    }
}
