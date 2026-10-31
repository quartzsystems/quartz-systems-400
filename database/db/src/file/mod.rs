// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*FILE` objects: creation from a compiled definition, description,
//! the member and dependents tables, and deletion.

pub mod attr;
mod change;
pub mod dependents;
pub mod members;

pub use attr::{FileAttr, FileKind, Maint, Recover, Size, Wait};
pub use change::{change_attr, chgpfm, replace_formats};
pub use dependents::DependentEntry;
pub use members::MemberEntry;

use dds::{DataType, FieldDef, FieldSource, FileRef, LfDefinition, PfDefinition, RecordFormat};
use qsname::{MemberSpec, Name};
use store::obj::header::{self, ObjectHeader, text_of};
use store::obj::ops::{self, CreateParams};
use store::obj::space::Space;
use store::obj::{JobHandle, ObjectId, types};
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::format::{FormatDef, sections};
use crate::lf::{JoinDef, LogicalDef};
use crate::member::{self, Member};

/// Which member a new file starts with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MemberOpt {
    /// `MBR(*FILE)`: one member named after the file.
    #[default]
    File,
    /// `MBR(*NONE)`.
    None,
    /// `MBR(name)`.
    Named(Name),
}

/// Everything `CRTPF` and `CRTLF` take besides the definition.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct CreateFileOpts {
    /// `TEXT`.
    pub text: String,
    /// Owner profile.
    pub owner: String,
    /// Created-on system name.
    pub system: String,
    /// Unix seconds.
    pub now: u64,
    /// `MBR`.
    pub member: MemberOpt,
    /// `SIZE`.
    pub size: Size,
    /// `REUSEDLT`.
    pub reusedlt: bool,
    /// `MAXMBRS`, 0 for `*NOMAX`.
    pub max_members: u32,
    /// `SHARE`.
    pub share: bool,
    /// `LVLCHK`.
    pub lvlchk: bool,
    /// `MAINT`.
    pub maint: Maint,
    /// `RECOVER`.
    pub recover: Recover,
    /// `FRCRATIO`, 0 for `*NONE`.
    pub frcratio: u16,
    /// `WAITFILE`.
    pub wait_file: Wait,
    /// `WAITRCD`.
    pub wait_rcd: Wait,
    /// `CCSID`.
    pub ccsid: u16,
    /// `ALLOCATE`.
    pub allocate: bool,
    /// `ALWUPD`.
    pub alwupd: bool,
    /// `ALWDLT`.
    pub alwdlt: bool,
    /// `FRCACCPTH`.
    pub frcaccpth: bool,
}

impl Default for CreateFileOpts {
    fn default() -> Self {
        Self {
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: 0,
            member: MemberOpt::File,
            size: Size::default(),
            reusedlt: false,
            max_members: 1,
            share: false,
            lvlchk: true,
            maint: Maint::Immed,
            recover: Recover::No,
            frcratio: 0,
            wait_file: Wait::Immed,
            wait_rcd: Wait::Seconds(60),
            ccsid: 37,
            allocate: false,
            alwupd: true,
            alwdlt: true,
            frcaccpth: false,
        }
    }
}

/// A file, described.
#[derive(Clone, Debug)]
pub struct FileDesc {
    /// The object.
    pub id: ObjectId,
    /// Its header page.
    pub page: store::PageNo,
    /// The header.
    pub header: ObjectHeader,
    /// The attributes.
    pub attr: FileAttr,
    /// The record formats, in order.
    pub formats: Vec<FormatDef>,
    /// The members, in creation order.
    pub members: Vec<MemberEntry>,
    /// Physical file: the logical members its members maintain.
    pub dependents: Vec<DependentEntry>,
    /// The library's name.
    pub library_name: Name,
}

impl FileDesc {
    /// The file name.
    #[must_use]
    pub fn name(&self) -> Name {
        self.header.name
    }

    /// The member a `MBR` parameter means.
    #[must_use]
    pub fn member(&self, spec: MemberSpec) -> Option<&MemberEntry> {
        match spec {
            MemberSpec::Name(n) => self.members.iter().find(|m| m.name == n),
            MemberSpec::First | MemberSpec::All => self.members.first(),
            MemberSpec::Last => self.members.last(),
        }
    }
}

/// The three-field format of a source physical file.
#[must_use]
pub fn source_format(rcdlen: u32) -> RecordFormat {
    let name = |s: &str| Name::parse(s).expect("valid");
    let mut f = RecordFormat::new(name("QSRCSEQ"));
    f.fields = vec![
        FieldDef::new(name("SRCSEQ"), DataType::Zoned, 6, 2),
        FieldDef::new(name("SRCDAT"), DataType::Zoned, 6, 0),
        FieldDef::new(
            name("SRCDTA"),
            DataType::Char,
            rcdlen.saturating_sub(12).max(1),
            0,
        ),
    ];
    f.fields[0].text = Some("Sequence number".into());
    f.fields[1].text = Some("Change date".into());
    f.fields[2].text = Some("Source statement".into());
    f.lay_out();
    f
}

/// `CRTPF` from a compiled definition.
pub fn create_pf(
    txn: &mut Txn<'_>,
    library: ObjectId,
    name: Name,
    def: &PfDefinition,
    opts: &CreateFileOpts,
) -> Result<ObjectId> {
    let format = FormatDef::new(def.format.clone(), def.unique, def.dup_order)?;
    create_file(
        txn,
        library,
        name,
        FileKind::Physical,
        &[format],
        opts,
        (false, false),
    )
}

/// `CRTPF RCDLEN(n)`: one character field.
pub fn create_pf_rcdlen(
    txn: &mut Txn<'_>,
    library: ObjectId,
    name: Name,
    rcdlen: u32,
    opts: &CreateFileOpts,
) -> Result<ObjectId> {
    let mut f = RecordFormat::new(name);
    f.fields = vec![FieldDef::new(name, DataType::Char, rcdlen, 0)];
    f.lay_out();
    let format = FormatDef::new(f, None, None)?;
    create_file(
        txn,
        library,
        name,
        FileKind::Physical,
        &[format],
        opts,
        (false, false),
    )
}

/// A physical file of character fields, as the system makes them for
/// output files (`CPYAUDJRNE`).
pub fn create_pf_fields(
    txn: &mut Txn<'_>,
    library: ObjectId,
    name: Name,
    fields: &[(Name, u32)],
    opts: &CreateFileOpts,
) -> Result<ObjectId> {
    let mut f = RecordFormat::new(name);
    f.fields = fields
        .iter()
        .map(|(n, len)| FieldDef::new(*n, DataType::Char, *len, 0))
        .collect();
    f.lay_out();
    let format = FormatDef::new(f, None, None)?;
    create_file(
        txn,
        library,
        name,
        FileKind::Physical,
        &[format],
        opts,
        (false, false),
    )
}

/// `CRTSRCPF`.
pub fn create_srcpf(
    txn: &mut Txn<'_>,
    library: ObjectId,
    name: Name,
    rcdlen: u32,
    opts: &CreateFileOpts,
) -> Result<ObjectId> {
    let format = FormatDef::new(source_format(rcdlen), None, None)?;
    create_file(
        txn,
        library,
        name,
        FileKind::Source,
        &[format],
        opts,
        (false, false),
    )
}

/// Resolves a `LIB/FILE` reference of a logical file's DDS to the
/// physical file object. In the storage service names resolve through
/// the requesting job's library list.
pub type Resolve<'a> = &'a mut dyn FnMut(&FileRef) -> Result<ObjectId>;

fn definition(what: String) -> DbError {
    DbError::Definition { what }
}

/// `CRTLF` from a compiled definition.
pub fn create_lf(
    txn: &mut Txn<'_>,
    library: ObjectId,
    name: Name,
    def: &LfDefinition,
    resolve: Resolve<'_>,
    opts: &CreateFileOpts,
) -> Result<ObjectId> {
    let mut formats = Vec::new();
    for lf in &def.formats {
        let mut based_on = Vec::new();
        let mut base_formats = Vec::new();
        for (i, fr) in lf.based_on.iter().enumerate() {
            let id = resolve(fr)?;
            crate::auth::check(txn, "file.base-on", id)?;
            let desc = describe(txn, id)?;
            if desc.attr.kind == FileKind::Logical {
                return Err(definition(format!("{fr} is a logical file")));
            }
            let want = lf.base_formats.get(i).copied();
            let base = desc
                .formats
                .iter()
                .find(|f| Some(f.format.name) == want)
                .or_else(|| desc.formats.first())
                .ok_or_else(|| definition(format!("{fr} has no record format")))?;
            based_on.push((id, base.format.name));
            base_formats.push(base.clone());
        }
        check_sources(lf, &base_formats)?;
        let joins = lf
            .joins
            .iter()
            .map(|j| JoinDef {
                from: u8::try_from(j.from).expect("few"),
                to: u8::try_from(j.to).expect("few"),
                fields: j.fields.clone(),
                dupseq: j.dupseq,
            })
            .collect();
        let mut fd = FormatDef::new(lf.format.clone(), def.unique, def.dup_order)?;
        fd.logical = Some(LogicalDef {
            based_on,
            sources: lf.sources.clone(),
            select_omit: lf.select_omit.clone(),
            joins,
            join: lf.join,
        });
        formats.push(fd);
    }
    create_file(
        txn,
        library,
        name,
        FileKind::Logical,
        &formats,
        opts,
        (def.dynslt, def.jdftval),
    )
}

/// Every field source must name a field the based-on format has.
fn check_sources(lf: &dds::LfFormat, bases: &[FormatDef]) -> Result<()> {
    let has = |file: usize, name: &Name| {
        bases
            .get(file)
            .is_some_and(|b| b.format.field(name).is_some())
    };
    for (f, src) in lf.format.fields.iter().zip(&lf.sources) {
        let ok = match src {
            FieldSource::Base { file, field } | FieldSource::Substring { file, field, .. } => {
                has(*file, field)
            }
            FieldSource::Concat { file, fields } => fields.iter().all(|n| has(*file, n)),
        };
        if !ok {
            return Err(definition(format!(
                "field {} of format {} is not in its based-on file",
                f.name, lf.format.name
            )));
        }
    }
    Ok(())
}

fn create_file(
    txn: &mut Txn<'_>,
    library: ObjectId,
    name: Name,
    kind: FileKind,
    formats: &[FormatDef],
    opts: &CreateFileOpts,
    (dynslt, jdftval): (bool, bool),
) -> Result<ObjectId> {
    let attribute = match kind {
        FileKind::Physical | FileKind::Source => "PF",
        FileKind::Logical => "LF",
    };
    let params = CreateParams {
        library,
        name,
        type_code: types::FILE,
        attribute: attribute.into(),
        text: opts.text.clone(),
        owner: opts.owner.clone(),
        system: opts.system.clone(),
        now: opts.now,
        by_system: false,
    };
    let id = ops::create(txn, &params)?;
    let blob = sections::encode(formats);
    let table = members::encode_table(&[], members::INITIAL_CAP);
    let def_len = blob.len() + table.len();
    let space = Space::ensure(txn, id, u64::try_from(def_len).expect("fits"), 0)?;
    space.write(txn, 0, &blob)?;
    space.write(txn, u64::try_from(blob.len()).expect("fits"), &table)?;
    let attr = FileAttr {
        kind,
        maint: opts.maint,
        recover: opts.recover,
        share: opts.share,
        reusedlt: opts.reusedlt,
        frcaccpth: opts.frcaccpth,
        lvlchk: opts.lvlchk,
        alwupd: opts.alwupd,
        alwdlt: opts.alwdlt,
        allocate: opts.allocate,
        dynslt,
        jdftval,
        record_len: formats
            .iter()
            .map(|f| f.format.record_len)
            .max()
            .unwrap_or(0),
        format_count: u16::try_from(formats.len()).expect("few"),
        member_count: 0,
        max_members: opts.max_members,
        size: opts.size,
        frcratio: opts.frcratio,
        wait_file: opts.wait_file,
        wait_rcd: opts.wait_rcd,
        ccsid: opts.ccsid,
        expdate: 0,
        def_len: u64::try_from(def_len).expect("fits"),
        formats_off: 0,
        formats_len: u64::try_from(blob.len()).expect("fits"),
        members_off: u64::try_from(blob.len()).expect("fits"),
        member_cap: members::INITIAL_CAP,
        dependent_count: 0,
        dependents_off: 0,
        dependent_cap: 0,
        journal: ObjectId::NONE,
        jrn_both_images: false,
        jrn_omit_opnclo: false,
    };
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 0, &attr.encode())?;
    let first = match opts.member {
        MemberOpt::None => None,
        MemberOpt::File => Some(name),
        MemberOpt::Named(m) => Some(m),
    };
    if let Some(m) = first {
        if kind == FileKind::Logical {
            add_lf_member(txn, id, m, &opts.text, None, opts.now)?;
        } else {
            add_member(txn, id, m, &opts.text, None, opts.now)?;
        }
    }
    Ok(id)
}

/// Describe a file.
pub fn describe(txn: &mut Txn<'_>, id: ObjectId) -> Result<FileDesc> {
    let (page, h) = ops::read_header(txn, id)?;
    let a = ops::read_attr(txn, page)?;
    if h.id.type_code != types::FILE || &a[..8] != attr::MAGIC {
        return Err(DbError::NotAFile {
            name: h.name.to_string(),
            library: library_name(txn, h.library),
        });
    }
    let attr = FileAttr::decode(&a[..attr::ATTR_LEN])?;
    let (space, _) = Space::load(txn, id)?;
    let blob = space.read(
        txn,
        attr.formats_off,
        usize::try_from(attr.formats_len).expect("fits"),
    )?;
    let formats = sections::decode(&blob)?;
    let table = space.read(
        txn,
        attr.members_off,
        usize::from(attr.member_count) * members::ENTRY_LEN,
    )?;
    let members = members::decode_table(&table, attr.member_count);
    let dependents = if attr.dependent_count > 0 {
        let t = space.read(
            txn,
            attr.dependents_off,
            usize::try_from(attr.dependent_count).expect("fits") * dependents::ENTRY_LEN,
        )?;
        dependents::decode_table(&t, attr.dependent_count)
    } else {
        Vec::new()
    };
    let library_name = ops::read_header(txn, h.library).map_or(h.name, |(_, l)| l.name);
    Ok(FileDesc {
        id,
        page,
        header: h,
        attr,
        formats,
        members,
        dependents,
        library_name,
    })
}

fn library_name(txn: &mut Txn<'_>, id: ObjectId) -> String {
    ops::read_header(txn, id).map_or_else(|_| id.to_string(), |(_, h)| h.name.to_string())
}

fn save_attr(txn: &mut Txn<'_>, desc: &FileDesc) -> Result<()> {
    ops::write_attr(txn, desc.page, 0, &desc.attr.encode())?;
    Ok(())
}

fn member_params(desc: &FileDesc, name: Name, text: &str, now: u64) -> CreateParams {
    CreateParams {
        library: desc.header.library,
        name,
        type_code: types::MBR,
        attribute: String::new(),
        text: text.to_string(),
        owner: text_of(&desc.header.owner),
        system: text_of(&desc.header.system),
        now,
        by_system: true,
    }
}

fn check_new_member(desc: &FileDesc, name: Name) -> Result<()> {
    if desc.members.iter().any(|m| m.name == name) {
        return Err(DbError::MemberExists {
            file: desc.name().to_string(),
            member: name.to_string(),
        });
    }
    if desc.attr.max_members > 0 && u32::from(desc.attr.member_count) >= desc.attr.max_members {
        return Err(DbError::TooManyMembers {
            file: desc.name().to_string(),
            max: desc.attr.max_members,
        });
    }
    Ok(())
}

/// `ADDPFM`: a member of the first format.
pub fn add_member(
    txn: &mut Txn<'_>,
    file: ObjectId,
    name: Name,
    text: &str,
    srctype: Option<Name>,
    now: u64,
) -> Result<ObjectId> {
    crate::auth::check(txn, "file.member", file)?;
    let mut desc = describe(txn, file)?;
    if desc.attr.kind == FileKind::Logical {
        return add_lf_member(txn, file, name, text, None, now);
    }
    check_new_member(&desc, name)?;
    let format = desc.formats.first().ok_or_else(|| DbError::Corrupt {
        what: format!("file {} has no record format", desc.name()),
    })?;
    let params = member_params(&desc, name, text, now);
    let object = member::create(
        txn,
        file,
        format,
        member::MemberParams {
            format_index: 0,
            max_records: desc.attr.size.max_records(),
            increment: desc.attr.size.increment,
            reuse_deleted: desc.attr.reusedlt,
        },
        &params,
    )?;
    desc.members.push(MemberEntry {
        name,
        srctype,
        object,
    });
    write_members(txn, &mut desc)?;
    let ctx = crate::journal::JrnCtx {
        now: Some(now),
        ..Default::default()
    };
    crate::journal::member_entry(txn, &desc, name, "MA", ctx)?;
    Ok(object)
}

/// `ADDLFM`: a logical member over `bases`, one physical member per
/// format, or each based-on file's first member.
pub fn add_lf_member(
    txn: &mut Txn<'_>,
    file: ObjectId,
    name: Name,
    text: &str,
    bases: Option<&[ObjectId]>,
    now: u64,
) -> Result<ObjectId> {
    let mut desc = describe(txn, file)?;
    check_new_member(&desc, name)?;
    let mut base_members = Vec::new();
    let mut base_files = Vec::new();
    for (fi, f) in desc.formats.iter().enumerate() {
        let log = f.logical.as_ref().ok_or_else(|| DbError::Corrupt {
            what: format!("format {fi} of {} is not logical", desc.name()),
        })?;
        let base_file = log
            .based_on
            .first()
            .map(|b| b.0)
            .ok_or_else(|| DbError::Corrupt {
                what: format!("format {fi} of {} has no based-on file", desc.name()),
            })?;
        let member = if let Some(m) = bases.and_then(|b| b.get(fi)) {
            *m
        } else {
            let base = describe(txn, base_file)?;
            base.members
                .first()
                .map(|m| m.object)
                .ok_or_else(|| DbError::MemberNotFound {
                    file: base.name().to_string(),
                    member: "*FIRST".into(),
                })?
        };
        base_members.push(member);
        base_files.push(base_file);
    }
    let params = member_params(&desc, name, text, now);
    let object = member::create_logical(txn, file, &desc.formats, &base_members, &params)?;
    for (base_file, base_member) in base_files.iter().zip(&base_members) {
        add_dependent(
            txn,
            *base_file,
            DependentEntry {
                lf_file: file,
                lf_member: object,
                base_member: *base_member,
            },
        )?;
    }
    let entry = MemberEntry {
        name,
        srctype: None,
        object,
    };
    desc.members.push(entry);
    write_members(txn, &mut desc)?;
    let desc = describe(txn, file)?;
    let mut m = Member::open(txn, &desc, &entry)?;
    m.rebuild_path(txn, &mut |_, _| {})?;
    Ok(object)
}

/// Write the member table back, growing it when full.
fn write_members(txn: &mut Txn<'_>, desc: &mut FileDesc) -> Result<()> {
    let count = u32::try_from(desc.members.len()).expect("fits");
    if count > desc.attr.member_cap {
        let cap = (desc.attr.member_cap * 2).max(count);
        desc.attr.members_off = desc.attr.def_len;
        desc.attr.member_cap = cap;
        desc.attr.def_len += u64::from(cap) * members::ENTRY_LEN as u64;
    }
    let table = members::encode_table(&desc.members, desc.attr.member_cap);
    let space = Space::ensure(txn, desc.id, desc.attr.def_len, 0)?;
    space.write(txn, desc.attr.members_off, &table)?;
    desc.attr.member_count = u16::try_from(desc.members.len()).expect("at most 32767");
    save_attr(txn, desc)
}

/// Write the dependents table back, growing it when full.
fn write_dependents(txn: &mut Txn<'_>, desc: &mut FileDesc) -> Result<()> {
    let count = u32::try_from(desc.dependents.len()).expect("fits");
    if count > desc.attr.dependent_cap {
        let cap = (desc.attr.dependent_cap * 2)
            .max(count)
            .max(dependents::INITIAL_CAP);
        desc.attr.dependents_off = desc.attr.def_len;
        desc.attr.dependent_cap = cap;
        desc.attr.def_len += u64::from(cap) * dependents::ENTRY_LEN as u64;
    }
    if desc.attr.dependent_cap > 0 {
        let table = dependents::encode_table(&desc.dependents, desc.attr.dependent_cap);
        let space = Space::ensure(txn, desc.id, desc.attr.def_len, 0)?;
        space.write(txn, desc.attr.dependents_off, &table)?;
    }
    desc.attr.dependent_count = count;
    save_attr(txn, desc)
}

/// Register a dependent logical member with a physical file.
pub fn add_dependent(txn: &mut Txn<'_>, base_file: ObjectId, entry: DependentEntry) -> Result<()> {
    let mut desc = describe(txn, base_file)?;
    if desc.dependents.contains(&entry) {
        return Ok(());
    }
    desc.dependents.push(entry);
    write_dependents(txn, &mut desc)
}

/// Drop the dependents whose logical member is gone (after a restore
/// brought a physical file back without them). Returns how many.
pub fn prune_dependents(txn: &mut Txn<'_>, file: ObjectId) -> Result<usize> {
    let mut desc = describe(txn, file)?;
    let before = desc.dependents.len();
    let mut kept = Vec::with_capacity(before);
    for d in std::mem::take(&mut desc.dependents) {
        if ops::read_header(txn, d.lf_member).is_ok() {
            kept.push(d);
        }
    }
    desc.dependents = kept;
    let gone = before - desc.dependents.len();
    if gone > 0 {
        write_dependents(txn, &mut desc)?;
    }
    Ok(gone)
}

/// Forget a logical member in every based-on file.
fn remove_dependents(
    txn: &mut Txn<'_>,
    lf_member: ObjectId,
    base_files: &[ObjectId],
) -> Result<()> {
    for base in base_files {
        let mut desc = describe(txn, *base)?;
        let before = desc.dependents.len();
        desc.dependents.retain(|d| d.lf_member != lf_member);
        if desc.dependents.len() != before {
            write_dependents(txn, &mut desc)?;
        }
    }
    Ok(())
}

/// The based-on files of a logical file, one per format.
fn base_files(desc: &FileDesc) -> Vec<ObjectId> {
    desc.formats
        .iter()
        .filter_map(|f| {
            f.logical
                .as_ref()
                .and_then(|l| l.based_on.first())
                .map(|b| b.0)
        })
        .collect()
}

fn has_dependents(desc: &FileDesc, member: Option<ObjectId>) -> Result<()> {
    let dependent = desc
        .dependents
        .iter()
        .find(|d| member.is_none_or(|m| d.base_member == m));
    if let Some(d) = dependent {
        return Err(DbError::HasDependents {
            file: desc.name().to_string(),
            dependent: d.lf_file.to_string(),
        });
    }
    Ok(())
}

/// `RMVM`.
pub fn remove_member(txn: &mut Txn<'_>, job: JobHandle, file: ObjectId, name: Name) -> Result<()> {
    crate::auth::check(txn, "file.remove-member", file)?;
    let mut desc = describe(txn, file)?;
    let at = desc
        .members
        .iter()
        .position(|m| m.name == name)
        .ok_or_else(|| DbError::MemberNotFound {
            file: desc.name().to_string(),
            member: name.to_string(),
        })?;
    let entry = desc.members[at];
    if desc.attr.kind == FileKind::Logical {
        remove_dependents(txn, entry.object, &base_files(&desc))?;
    } else {
        has_dependents(&desc, Some(entry.object))?;
    }
    let ctx = crate::journal::JrnCtx {
        job,
        ..Default::default()
    };
    crate::journal::member_entry(txn, &desc, name, "MD", ctx)?;
    desc.members.remove(at);
    member::destroy(txn, job, entry.object)?;
    write_members(txn, &mut desc)
}

/// `RNMM`.
pub fn rename_member(
    txn: &mut Txn<'_>,
    file: ObjectId,
    from: Name,
    to: Name,
    now: u64,
) -> Result<()> {
    crate::auth::check(txn, "file.member", file)?;
    let mut desc = describe(txn, file)?;
    if desc.members.iter().any(|m| m.name == to) {
        return Err(DbError::MemberExists {
            file: desc.name().to_string(),
            member: to.to_string(),
        });
    }
    let file_name = desc.name().to_string();
    let entry = desc
        .members
        .iter_mut()
        .find(|m| m.name == from)
        .ok_or_else(|| DbError::MemberNotFound {
            file: file_name,
            member: from.to_string(),
        })?;
    entry.name = to;
    let (page, mut h) = ops::read_header(txn, entry.object)?;
    h.name = to;
    h.changed = now;
    ops::write_header(txn, page, &h)?;
    write_members(txn, &mut desc)
}

/// `DLTF`: the members, then the file. A physical file with dependent
/// logical members is refused.
pub fn delete_file(txn: &mut Txn<'_>, job: JobHandle, file: ObjectId) -> Result<()> {
    crate::auth::check(txn, "obj.delete", file)?;
    let desc = describe(txn, file)?;
    if desc.attr.kind == FileKind::Logical {
        for m in &desc.members {
            remove_dependents(txn, m.object, &base_files(&desc))?;
        }
    } else {
        has_dependents(&desc, None)?;
    }
    for m in &desc.members {
        member::destroy(txn, job, m.object)?;
    }
    ops::delete(txn, job, file)?;
    Ok(())
}

/// The header's attribute area offset, for callers that read raw pages.
pub const ATTR_OFF: usize = header::ATTR_OFF;
