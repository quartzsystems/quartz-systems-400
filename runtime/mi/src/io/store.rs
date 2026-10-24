// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! File I/O over the database: a session of open data paths on a
//! store, one transaction per operation, keys decoded from the IR's
//! key bytes with the file's key fields.

#![allow(clippy::single_match_else)]

use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use db::odp::{KeyArg, OdpRef, OpenOpts, Session, Usage};
use db::{DbError, FormatDef, Value};
use qsname::{LibSpec, MemberSpec, Name};
use store::Store;
use store::obj::{JobHandle, ObjectId};

use super::{FileInfo, IoFail, IoOp, IoRequest, IoResult, IoSource, KeyRef, OpenSpec};

/// A resolver from a file reference to the object.
pub type Resolver = Box<dyn FnMut(&dds::FileRef) -> db::Result<ObjectId>>;

/// Whoever owns the store and lends it per operation.
pub trait StoreHandle {
    /// The store, borrowed for one operation.
    fn store(&self) -> RefMut<'_, Store>;
}

impl StoreHandle for Rc<RefCell<Store>> {
    fn store(&self) -> RefMut<'_, Store> {
        self.borrow_mut()
    }
}

/// One open file.
struct Entry {
    odp: OdpRef,
    formats: Vec<FormatDef>,
}

/// The database behind a job's files.
pub struct StoreIo {
    store: Box<dyn StoreHandle>,
    session: Session,
    resolve: Resolver,
    open: Vec<Option<Entry>>,
}

impl StoreIo {
    /// Over a store, as `job`, resolving file names with `resolve`.
    #[must_use]
    pub fn new(store: Box<dyn StoreHandle>, job: JobHandle, resolve: Resolver) -> Self {
        Self::with_locks(store, job, resolve, db::odp::new_shared())
    }

    /// Over a store, with record locks in a table other jobs share.
    #[must_use]
    pub fn with_locks(
        store: Box<dyn StoreHandle>,
        job: JobHandle,
        resolve: Resolver,
        locks: db::odp::SharedLocks,
    ) -> Self {
        Self {
            store,
            session: Session::new(job, locks),
            resolve,
            open: Vec::new(),
        }
    }

    /// The session, for overrides and commitment control.
    pub fn session(&mut self) -> &mut Session {
        &mut self.session
    }

    /// End the job's use of the database: open files dropped, an open
    /// commit cycle rolled back, every lock released.
    pub fn end(&mut self) {
        self.open.clear();
        let mut store = self.store.store();
        match store.begin() {
            Ok(mut txn) => {
                self.session.end(&mut txn);
                if let Err(e) = txn.commit() {
                    tracing::warn!("ending the job's files: {e}");
                }
            }
            Err(e) => tracing::warn!("ending the job's files: {e}"),
        }
    }

    /// The key fields' values from key bytes in record layout.
    fn key_values(fmt: &FormatDef, bytes: &[u8]) -> Vec<Value> {
        let mut at = 0;
        let mut out = Vec::new();
        for k in &fmt.format.keys {
            let Some(f) = fmt.format.fields.get(k.field) else {
                break;
            };
            let n = f.byte_len as usize;
            if at + n > bytes.len() {
                break;
            }
            let mut g = f.clone();
            g.offset = 0;
            match db::record::read_field(&g, &bytes[at..at + n], &[], None) {
                Ok(v) => out.push(v),
                Err(_) => break,
            }
            at += n;
        }
        out
    }

    fn key_arg(fmt: &FormatDef, key: KeyRef<'_>) -> Option<KeyArg> {
        match key {
            KeyRef::Fields(b) => Some(KeyArg::Values(Self::key_values(fmt, b))),
            KeyRef::Rrn(n) => Some(KeyArg::Rrn(n)),
            KeyRef::Start => Some(KeyArg::Start),
            KeyRef::End => Some(KeyArg::End),
            KeyRef::LoVal => Some(KeyArg::LoVal),
            KeyRef::HiVal => Some(KeyArg::HiVal),
            KeyRef::None => None,
        }
    }

    fn result_of(odp: &OdpRef, record: Option<db::odp::Record>, fmt: &db::format::FormatDef) -> IoResult {
        // The null map as a byte per field.
        let nulls = record.as_ref().map(|r| {
            (0..fmt.format.fields.len())
                .map(|i| if db::record::is_null(&r.null_map, fmt.null_index(i)) { 0xF1 } else { 0xF0 })
                .collect()
        });
        let o = odp.borrow();
        IoResult {
            status: o.fb.status,
            msgid: o.fb.msgid.map(str::to_string),
            eof: o.fb.eof,
            found: o.fb.found,
            equal: o.fb.equal,
            rrn: o.fb.rrn,
            key: o.fb.key.clone(),
            format: o.fb.format.to_string(),
            member: o.fb.member.to_string(),
            record: record.map(|r| r.data),
            nulls,
        }
    }
}

fn fail_of(e: &DbError) -> IoFail {
    let blank = Name::parse("QTEMP").expect("a name");
    let mut fb = db::odp::IoFeedback::new(blank, blank);
    fb.fail(e);
    IoFail {
        status: fb.status,
        msgid: e.msgid().unwrap_or("CPF4101").into(),
        data: ccsid::to_ebcdic(&e.to_string()).unwrap_or_default(),
    }
}

impl IoSource for StoreIo {
    fn open(&mut self, spec: &OpenSpec) -> Result<(u32, FileInfo), IoFail> {
        let file = Name::parse(&spec.file)
            .map_err(|_| IoFail::new(1299, "CPF4101", &[&spec.file, &spec.lib]))?;
        let library = match spec.lib.as_str() {
            "*LIBL" | "" => LibSpec::Libl,
            "*CURLIB" => LibSpec::Curlib,
            l => LibSpec::Name(Name::parse(l).map_err(|_| IoFail::new(1299, "CPF9810", &[l]))?),
        };
        let fr = dds::FileRef {
            library,
            file,
            format: None,
        };
        let mut opts = OpenOpts::input(self.session.job());
        opts.usage = match spec.usage {
            qir::FileUsage::Input => Usage::INPUT,
            qir::FileUsage::Output => Usage::OUTPUT,
            qir::FileUsage::Update => Usage::UPDATE,
            qir::FileUsage::All => Usage::ALL,
        };
        opts.keyed = spec.keyed;
        if let Some(m) = &spec.member {
            opts.member =
                MemberSpec::Name(Name::parse(m).map_err(|_| IoFail::new(1299, "CPF9815", &[m]))?);
        }
        if let Some(l) = &spec.lvlchk {
            opts.lvlchk = Some(
                l.iter()
                    .filter_map(|(n, id)| Name::parse(n).ok().map(|n| (n, id.clone())))
                    .collect(),
            );
        }
        self.session.set_opener(if spec.actgrp.is_empty() {
            db::odp::ovrdbf::DFTACTGRP
        } else {
            &spec.actgrp
        });
        // Resolve the file and its override's TOFILE before the store is
        // taken: a resolver may need the store itself.
        let mut wanted = vec![fr.clone()];
        if let Some(to) = self.session.override_for(fr.file).and_then(|o| o.tofile.clone()) {
            wanted.push(to);
        }
        let mut known: Vec<(dds::FileRef, Option<db::Result<ObjectId>>)> = Vec::new();
        for w in wanted {
            let r = (self.resolve)(&w);
            known.push((w, Some(r)));
        }
        let mut lookup = |r: &dds::FileRef| -> db::Result<ObjectId> {
            known
                .iter_mut()
                .find(|(k, _)| k == r)
                .and_then(|(_, v)| v.take())
                .unwrap_or_else(|| {
                    Err(DbError::Definition {
                        what: format!("{} was not resolved before the open", r.file),
                    })
                })
        };
        let mut store = self.store.store();
        let mut txn = store.begin().map_err(|e| fail_of(&DbError::Store(e)))?;
        let odp = match self.session.open(&mut txn, &fr, &mut lookup, opts) {
            Ok(o) => o,
            Err(e) => {
                let _ = txn.commit();
                return Err(fail_of(&e));
            }
        };
        let id = odp.borrow().file;
        let desc = db::file::describe(&mut txn, id).map_err(|e| fail_of(&e))?;
        txn.commit().map_err(|e| fail_of(&DbError::Store(e)))?;
        let info = FileInfo {
            file: desc.name().to_string(),
            lib: desc.library_name.to_string(),
            member: odp.borrow().fb.member.to_string(),
            format: desc
                .formats
                .first()
                .map(|f| f.format.name.to_string())
                .unwrap_or_default(),
            rec_len: desc.formats.first().map_or(0, |f| f.format.record_len),
            wait_ms: match odp.borrow().wait_rcd() {
                db::file::Wait::Immed => 0,
                db::file::Wait::NoMax => -1,
                db::file::Wait::Seconds(n) => i64::from(n) * 1000,
            },
        };
        let entry = Entry {
            odp,
            formats: desc.formats,
        };
        let h = match self.open.iter().position(Option::is_none) {
            Some(i) => {
                self.open[i] = Some(entry);
                i
            }
            None => {
                self.open.push(Some(entry));
                self.open.len() - 1
            }
        };
        Ok((u32::try_from(h).expect("few files"), info))
    }

    fn io(&mut self, handle: u32, req: &IoRequest<'_>) -> IoResult {
        let Some(Some(entry)) = self.open.get(handle as usize) else {
            return IoResult {
                status: db::odp::status::CLOSED,
                msgid: Some("CPF4101".into()),
                ..IoResult::default()
            };
        };
        let fmt = &entry.formats[0];
        let key = Self::key_arg(fmt, req.key);
        let lock = !req.nolock;
        let mut store = self.store.store();
        let Ok(mut txn) = store.begin() else {
            return IoResult {
                status: db::odp::status::OTHER,
                ..IoResult::default()
            };
        };
        let odp = Rc::clone(&entry.odp);
        let mut o = odp.borrow_mut();
        let rec_len = fmt.format.record_len as usize;
        // The request's null map (a byte per field) as the record's bits.
        let mut null_map = vec![0u8; usize::from(fmt.null_map_len)];
        if let Some(n) = req.nulls {
            for (i, b) in n.iter().enumerate() {
                if let Some(x) = fmt.null_index(i) {
                    db::record::set_null(&mut null_map, x, *b == 0xF1);
                }
            }
        }
        let buffer = |r: Option<&[u8]>| {
            let mut v = r.map(<[u8]>::to_vec).unwrap_or_default();
            v.resize(rec_len, 0x40);
            v
        };
        let result: db::Result<Option<db::odp::Record>> = match req.op {
            IoOp::Read => o.read(&mut txn, lock),
            IoOp::ReadP => o.readp(&mut txn, lock),
            IoOp::ReadE => o.reade(&mut txn, key.as_ref(), lock),
            IoOp::ReadPE => o.readpe(&mut txn, key.as_ref(), lock),
            IoOp::Chain => o.chain(&mut txn, key.as_ref().unwrap_or(&KeyArg::Start), lock),
            IoOp::SetLL => o
                .setll(&mut txn, key.as_ref().unwrap_or(&KeyArg::Start))
                .map(|()| None),
            IoOp::SetGT => o
                .setgt(&mut txn, key.as_ref().unwrap_or(&KeyArg::Start))
                .map(|()| None),
            IoOp::Write => o
                .write(&mut txn, &buffer(req.record), &null_map)
                .map(|_| None),
            IoOp::Update => o
                .update(&mut txn, &buffer(req.record), &null_map)
                .map(|()| None),
            IoOp::Delete => o.delete(&mut txn).map(|()| None),
            IoOp::Unlock => o.unlock_op(&mut txn).map(|()| None),
            IoOp::Feod => o.feod(&mut txn).map(|()| None),
        };
        drop(o);
        let record = result.unwrap_or(None);
        let mut out = Self::result_of(&odp, record, fmt);
        if let Err(e) = txn.commit() {
            out.status = db::odp::status::OTHER;
            out.msgid = Some(format!("commit: {e}"));
        }
        out
    }

    fn close(&mut self, handle: u32) -> IoResult {
        let Some(slot) = self.open.get_mut(handle as usize) else {
            return IoResult::default();
        };
        let Some(entry) = slot.take() else {
            return IoResult::default();
        };
        let mut store = self.store.store();
        if let Ok(mut txn) = store.begin() {
            self.session.close(&mut txn, &entry.odp);
            let _ = txn.commit();
        }
        IoResult::default()
    }
}
