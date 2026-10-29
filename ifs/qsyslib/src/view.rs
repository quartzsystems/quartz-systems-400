// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/QSYS.LIB` over the object store: libraries and objects with
//! IBM's naming, members of physical files as byte streams, `*USRSPC`
//! as binary streams, authority by the Phase 6 search, creation and
//! deletion. Every call is one store transaction. The store's own
//! guard checks the job's authority to what it touches; the view's
//! `authority` answer is what the resolver maps to `*RWX`.

use db::file::FileKind;
use ifs::vfs::qsys::{MemberData, QsysEntry, QsysNode, QsysView};
use ifs::{Errno, Result};
use qsname::{MemberSpec, Name};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::auth::search;
use store::obj::header::{ObjectHeader, text_of};
use store::obj::{CreateParams, ObjectId, ops, type_by_code, type_by_name, types, usrspc};
use store::txn::Txn;

use crate::access::{StoreAccess, db_errno, store_errno};
use crate::savf;

/// The store seen as `/QSYS.LIB`.
pub struct StoreQsys<S> {
    access: S,
}

impl<S: StoreAccess> StoreQsys<S> {
    /// Over `access`.
    pub fn new(access: S) -> Self {
        Self { access }
    }

    /// The access.
    pub fn access(&self) -> &S {
        &self.access
    }

    fn run<T>(&self, mut f: impl FnMut(&mut Txn<'_>, ObjectId) -> Result<T>) -> Result<T> {
        self.access.with_main(&mut f)
    }
}

fn name(s: &str) -> Result<Name> {
    Name::parse(s).map_err(|_| Errno::ENOENT)
}

/// The library `lib` (`QSYS` is the root itself).
fn library(txn: &mut Txn<'_>, qsys: ObjectId, lib: &str) -> Result<ObjectId> {
    if lib.eq_ignore_ascii_case("QSYS") {
        return Ok(qsys);
    }
    ops::find_library(txn, qsys, name(lib)?)
        .map(|f| f.id)
        .map_err(|e| store_errno(&e))
}

/// `lib/name` of a type.
fn object(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    lib: &str,
    obj: &str,
    objtype: &str,
) -> Result<ObjectId> {
    if objtype == "*LIB" {
        return library(txn, qsys, obj);
    }
    let code = type_by_name(objtype).ok_or(Errno::ENOENT)?.code;
    let lib_id = library(txn, qsys, lib)?;
    let l = store::obj::library::Library::load(txn, lib_id).map_err(|e| store_errno(&e))?;
    l.lookup(txn, code, name(obj)?)
        .map_err(|e| store_errno(&e))?
        .ok_or(Errno::ENOENT)
}

/// What a header says as an entry.
fn entry_of(txn: &mut Txn<'_>, h: &ObjectHeader) -> QsysEntry {
    let objtype = type_by_code(h.id.type_code).map_or("*UNKNOWN", |t| t.name);
    let (attr, size, ccsid) = match h.id.type_code {
        types::FILE => {
            let d = db::file::describe(txn, h.id).ok();
            let attr = d.as_ref().map_or("", |d| match d.attr.kind {
                FileKind::Source => "PF-SRC",
                FileKind::Physical => "PF-DTA",
                FileKind::Logical => "LF",
            });
            (
                attr.to_string(),
                h.size_pages * 4096,
                d.map_or(37, |d| d.attr.ccsid),
            )
        }
        types::USRSPC => (
            String::new(),
            usrspc::describe(txn, h.id).map_or(0, |a| a.size),
            65535,
        ),
        _ if objtype == "*SAVF" => ("SAVF".into(), savf::len(txn, h.id).unwrap_or(0), 65535),
        _ => (text_of(&h.attribute), h.size_pages * 4096, 65535),
    };
    QsysEntry {
        name: h.name.to_string(),
        objtype: objtype.into(),
        attr,
        size,
        owner: text_of(&h.owner),
        created: i64::try_from(h.created).unwrap_or(0) * 1_000_000_000,
        changed: i64::try_from(h.changed).unwrap_or(0) * 1_000_000_000,
        ccsid,
        text: text_of(&h.text),
    }
}

/// A member's shape and entry.
fn member_entry(
    txn: &mut Txn<'_>,
    desc: &db::FileDesc,
    e: &db::file::members::MemberEntry,
) -> QsysEntry {
    let (records, width, source) = db::Member::open(txn, desc, e).map_or((0, 0, false), |m| {
        let fmt = m.format();
        let source = desc.attr.kind == FileKind::Source;
        let width = if source {
            fmt.format.fields.get(2).map_or(80, |f| u64::from(f.length))
        } else {
            u64::from(fmt.format.record_len)
        };
        (m.attr().record_count, width, source)
    });
    let size = if source {
        records * (width + 2)
    } else {
        records * width
    };
    QsysEntry {
        name: e.name.to_string(),
        objtype: "*MBR".into(),
        attr: e.srctype.map(|n| n.to_string()).unwrap_or_default(),
        size,
        owner: text_of(&desc.header.owner),
        created: i64::try_from(desc.header.created).unwrap_or(0) * 1_000_000_000,
        changed: i64::try_from(desc.header.changed).unwrap_or(0) * 1_000_000_000,
        ccsid: desc.attr.ccsid,
        text: String::new(),
    }
}

impl<S: StoreAccess + Send + Sync> QsysView for StoreQsys<S> {
    fn libraries(&self) -> Result<Vec<QsysEntry>> {
        self.run(|txn, qsys| {
            let mut v = Vec::new();
            for (_, id) in ops::libraries(txn, qsys).map_err(|e| store_errno(&e))? {
                // QSYS lists itself; it is the root, not a library in it.
                if id == qsys {
                    continue;
                }
                if let Ok(h) = ops::describe(txn, id) {
                    v.push(entry_of(txn, &h));
                }
            }
            Ok(v)
        })
    }

    fn objects(&self, lib: &str) -> Result<Vec<QsysEntry>> {
        self.run(|txn, qsys| {
            let id = library(txn, qsys, lib)?;
            let mut v = Vec::new();
            for (k, oid) in ops::list(txn, id, None).map_err(|e| store_errno(&e))? {
                if k.type_code == types::LIB || k.type_code == types::MBR {
                    continue;
                }
                if let Ok(h) = ops::describe(txn, oid) {
                    v.push(entry_of(txn, &h));
                }
            }
            Ok(v)
        })
    }

    fn object(&self, lib: &str, name: &str, objtype: &str) -> Result<QsysEntry> {
        self.run(|txn, qsys| {
            let id = object(txn, qsys, lib, name, objtype)?;
            let h = ops::describe(txn, id).map_err(|e| store_errno(&e))?;
            Ok(entry_of(txn, &h))
        })
    }

    fn members(&self, lib: &str, file: &str) -> Result<Vec<QsysEntry>> {
        self.run(|txn, qsys| {
            let id = object(txn, qsys, lib, file, "*FILE")?;
            let desc = db::file::describe(txn, id).map_err(|e| db_errno(&e))?;
            if desc.attr.kind == FileKind::Logical {
                return Ok(Vec::new());
            }
            Ok(desc
                .members
                .iter()
                .map(|e| member_entry(txn, &desc, e))
                .collect())
        })
    }

    fn read_member(&self, lib: &str, file: &str, mbr: &str) -> Result<MemberData> {
        self.run(|txn, qsys| {
            let id = object(txn, qsys, lib, file, "*FILE")?;
            let desc = db::file::describe(txn, id).map_err(|e| db_errno(&e))?;
            let spec = MemberSpec::Name(name(mbr)?);
            match desc.attr.kind {
                FileKind::Source => {
                    let width = db::source::data_width(txn, id).map_err(|e| db_errno(&e))?;
                    let records = db::source::records(txn, id, spec)
                        .map_err(|e| db_errno(&e))?
                        .into_iter()
                        .map(|(_, _, d)| d)
                        .collect();
                    Ok(MemberData {
                        records,
                        record_len: width,
                        source: true,
                        ccsid: desc.attr.ccsid,
                    })
                }
                FileKind::Physical => {
                    let m = db::Member::open_by(txn, id, spec).map_err(|e| db_errno(&e))?;
                    let len = m.format().format.record_len as usize;
                    let records = m
                        .scan(txn, 1, usize::MAX)
                        .map_err(|e| db_errno(&e))?
                        .into_iter()
                        .filter(|s| !s.deleted)
                        .map(|s| s.record)
                        .collect();
                    Ok(MemberData {
                        records,
                        record_len: len,
                        source: false,
                        ccsid: desc.attr.ccsid,
                    })
                }
                FileKind::Logical => Err(Errno::ENOTSUP),
            }
        })
    }

    fn write_member(&self, lib: &str, file: &str, mbr: &str, records: Vec<Vec<u8>>) -> Result<()> {
        let job = self.access.job();
        let now = self.access.now();
        self.run(|txn, qsys| {
            let id = object(txn, qsys, lib, file, "*FILE")?;
            let desc = db::file::describe(txn, id).map_err(|e| db_errno(&e))?;
            let spec = MemberSpec::Name(name(mbr)?);
            match desc.attr.kind {
                FileKind::Source => {
                    db::source::put_records(txn, job, id, spec, &records, now)
                        .map_err(|e| db_errno(&e))?;
                    Ok(())
                }
                FileKind::Physical => {
                    let pad = if ccsid::conv::Codec::of(desc.attr.ccsid)
                        .ok()
                        .flatten()
                        .is_some_and(|c| c.is_ebcdic())
                    {
                        ccsid::SPACE
                    } else {
                        0
                    };
                    db::util::put_records(txn, job, id, spec, &records, pad, now)
                        .map_err(|e| db_errno(&e))?;
                    Ok(())
                }
                FileKind::Logical => Err(Errno::ENOTSUP),
            }
        })
    }

    fn read_stream(&self, lib: &str, name: &str, objtype: &str) -> Result<Vec<u8>> {
        match objtype {
            "*USRSPC" => {}
            "*SAVF" => {
                return self.run(|txn, qsys| {
                    let id = object(txn, qsys, lib, name, objtype)?;
                    savf::read(txn, id)
                });
            }
            _ => return Err(Errno::ENOTSUP),
        }
        self.run(|txn, qsys| {
            let id = object(txn, qsys, lib, name, objtype)?;
            let a = usrspc::describe(txn, id).map_err(|e| store_errno(&e))?;
            let len = usize::try_from(a.size).map_err(|_| Errno::EOVERFLOW)?;
            usrspc::read(txn, id, 0, len).map_err(|e| store_errno(&e))
        })
    }

    fn write_stream(&self, lib: &str, name: &str, objtype: &str, data: &[u8]) -> Result<()> {
        match objtype {
            "*USRSPC" => {}
            "*SAVF" => {
                return self.run(|txn, qsys| {
                    let id = object(txn, qsys, lib, name, objtype)?;
                    savf::write(txn, id, data)
                });
            }
            _ => return Err(Errno::ENOTSUP),
        }
        self.run(|txn, qsys| {
            let id = object(txn, qsys, lib, name, objtype)?;
            let a = usrspc::describe(txn, id).map_err(|e| store_errno(&e))?;
            if a.size != data.len() as u64 {
                usrspc::change_size(txn, id, data.len() as u64).map_err(|e| store_errno(&e))?;
            }
            usrspc::write(txn, id, 0, data).map_err(|e| store_errno(&e))
        })
    }

    fn authority(&self, lib: &str, name: &str, objtype: &str, who: &Principal) -> Result<Auth> {
        self.run(|txn, qsys| {
            let id = object(txn, qsys, lib, name, objtype)?;
            let (_, h) = ops::read_header(txn, id).map_err(|e| store_errno(&e))?;
            let found = search::find(txn, qsys, who, &h).map_err(|_| Errno::EIO)?;
            if found.all {
                return Ok(Auth(Auth::MASK));
            }
            let mut have = found.have;
            for a in &who.adopted {
                have = have.union(search::adopted(txn, qsys, a, &h).map_err(|_| Errno::EIO)?);
            }
            Ok(have)
        })
    }

    fn create_library(&self, lib: &str, who: &Principal) -> Result<()> {
        let now = self.access.now();
        self.run(|txn, qsys| {
            ops::create(
                txn,
                &CreateParams {
                    library: qsys,
                    name: name(lib)?,
                    type_code: types::LIB,
                    attribute: "PROD".into(),
                    text: String::new(),
                    owner: who.user.clone(),
                    system: "QS400".into(),
                    now,
                    by_system: false,
                },
            )
            .map(|_| ())
            .map_err(|e| store_errno(&e))
        })
    }

    fn create_member(&self, lib: &str, file: &str, mbr: &str, _who: &Principal) -> Result<()> {
        let now = self.access.now();
        self.run(|txn, qsys| {
            let id = object(txn, qsys, lib, file, "*FILE")?;
            db::file::add_member(txn, id, name(mbr)?, "", None, now)
                .map(|_| ())
                .map_err(|e| db_errno(&e))
        })
    }

    fn delete(&self, node: &QsysNode, _who: &Principal) -> Result<()> {
        let job = self.access.job();
        self.run(|txn, qsys| match node {
            QsysNode::Root => Err(Errno::EPERM),
            QsysNode::Lib(l) => {
                let id = library(txn, qsys, l)?;
                if id == qsys {
                    return Err(Errno::EPERM);
                }
                if !ops::list(txn, id, None)
                    .map_err(|e| store_errno(&e))?
                    .is_empty()
                {
                    return Err(Errno::ENOTEMPTY);
                }
                ops::delete(txn, job, id).map_err(|e| store_errno(&e))
            }
            QsysNode::Obj(l, o, t) => {
                let id = object(txn, qsys, l, o, t)?;
                if t == "*FILE" {
                    db::file::delete_file(txn, job, id).map_err(|e| db_errno(&e))
                } else {
                    ops::delete(txn, job, id).map_err(|e| store_errno(&e))
                }
            }
            QsysNode::Mbr(l, f, m) => {
                let id = object(txn, qsys, l, f, "*FILE")?;
                db::file::remove_member(txn, job, id, name(m)?).map_err(|e| db_errno(&e))
            }
        })
    }
}
