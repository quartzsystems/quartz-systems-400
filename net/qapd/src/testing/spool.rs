// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's output             .filter(|| any || q.outq.library == lib)ueues and spooled files, with the rules
//! the spooled-file commands follow: a file's owner and `*SPLCTL` may
//! act on it; others may read it only on a queue that shows its data to
//! anyone (`DSPDTA(*YES)`); a printing file is not deleted or moved; a
//! held file goes back to what it was when released.

use std::collections::BTreeMap;
use std::sync::Mutex;

use qap_proto::Status;
use qap_proto::ops::spool::{
    DeviceType, ListOutputQueues, ListSpooledFiles, OutputQueue, OutputQueues, ReadSpooledFile, SpoolStatus,
    SpooledData, SpooledFile, SpooledFileId, SpooledFiles,
};
use qap_proto::types::ObjectName;

use super::lock;
use crate::spool::{self, Unreadable};

/// A spooled file.
#[derive(Clone, Debug, Default)]
pub struct File {
    /// Which file.
    pub id: SpooledFileId,
    /// Its status.
    pub status: SpoolStatus,
    /// What a release puts back.
    pub held_from: SpoolStatus,
    /// The queue it is on.
    pub outq: ObjectName,
    /// `USRDTA`.
    pub user_data: String,
    /// `FORMTYPE`.
    pub form_type: String,
    /// Its data's kind.
    pub device: DeviceType,
    /// Pages; counted from the data for SCS when the file is added.
    pub pages: u32,
    /// Copies.
    pub copies: u16,
    /// Made, milliseconds since 1970.
    pub created: i64,
    /// The data as spooled.
    pub data: Vec<u8>,
}

impl File {
    /// Its owner: its job's user.
    #[must_use]
    pub fn owner(&self) -> &str {
        &self.id.job.user
    }

    /// As lists show it.
    #[must_use]
    pub fn info(&self) -> SpooledFile {
        let pages = self.pages;
        SpooledFile {
            id: self.id.clone(),
            status: self.status,
            outq: self.outq.clone(),
            user: self.owner().to_string(),
            user_data: self.user_data.clone(),
            form_type: self.form_type.clone(),
            device_type: self.device,
            pages,
            copies: self.copies,
            size: self.data.len() as u64,
            created: self.created,
        }
    }
}

/// An output queue.
#[derive(Clone, Debug, Default)]
pub struct Queue {
    /// The queue.
    pub outq: ObjectName,
    /// Its text.
    pub text: String,
    /// The writer attached, empty for none.
    pub writer: String,
    /// Held.
    pub held: bool,
    /// `DSPDTA(*YES)`: anyone may read its files.
    pub show_all: bool,
    /// Profiles that may use it; empty for all.
    pub users: Vec<String>,
    /// Its files by key, in queue order.
    pub files: Vec<u32>,
}

/// Why an operation was refused: the status, the message and its data.
pub type No = (Status, &'static str, Vec<String>);

fn no(status: Status, id: &'static str, data: &[&str]) -> No {
    (status, id, data.iter().map(ToString::to_string).collect())
}

/// Every queue and file.
#[derive(Debug, Default)]
pub struct Spool {
    /// The queues, in library and name order.
    pub queues: Vec<Queue>,
    /// The files by key.
    pub files: BTreeMap<u32, File>,
    /// Profiles with `*SPLCTL`.
    pub splctl: Vec<String>,
    /// Libraries.
    pub libraries: Vec<String>,
    /// Libraries the test user may not read.
    pub private_libs: Vec<String>,
    /// Profiles that exist.
    pub profiles: Vec<String>,
}

/// A page of `v` from `start`, at most `max` (0 for all).
fn page<T: Clone>(v: &[T], start: u32, max: u32) -> Vec<T> {
    let start = usize::try_from(start).unwrap_or(usize::MAX);
    let max = if max == 0 { usize::MAX } else { usize::try_from(max).unwrap_or(usize::MAX) };
    v.iter().skip(start).take(max).cloned().collect()
}

fn same(a: &ObjectName, b: &ObjectName) -> bool {
    a.name.eq_ignore_ascii_case(&b.name) && (b.library == "*LIBL" || a.library.eq_ignore_ascii_case(&b.library))
}

impl Spool {
    /// Add a queue.
    pub fn queue(&mut self, q: Queue) {
        self.queues.push(q);
    }

    /// Add a file at the end of its queue; its key.
    pub fn file(&mut self, mut f: File) -> u32 {
        if f.device == DeviceType::Scs {
            f.pages = u32::try_from(::spool::scs::index(&f.data).len()).unwrap_or(u32::MAX);
        }
        let key = self.files.keys().next_back().map_or(1, |k| k + 1);
        if let Some(q) = self.queues.iter_mut().find(|q| same(&q.outq, &f.outq)) {
            q.files.push(key);
        }
        self.files.insert(key, f);
        key
    }

    fn find_queue(&self, outq: &ObjectName) -> Result<usize, No> {
        self.queues
            .iter()
            .position(|q| same(&q.outq, outq))
            .ok_or_else(|| no(Status::NotFound, "CPF3357", &[&outq.name, &outq.library]))
    }

    fn find(&self, id: &SpooledFileId) -> Result<u32, No> {
        self.files
            .iter()
            .find(|(_, f)| {
                f.id.number == id.number
                    && f.id.file.eq_ignore_ascii_case(&id.file)
                    && f.id.job.name.eq_ignore_ascii_case(&id.job.name)
                    && f.id.job.user.eq_ignore_ascii_case(&id.job.user)
                    && f.id.job.number == id.job.number
            })
            .map(|(k, _)| *k)
            .ok_or_else(|| no(Status::NotFound, "CPF3309", &[&id.file]))
    }

    /// `may_spool`: the owner and `*SPLCTL` always; others to read only,
    /// on a queue that shows its files to anyone.
    fn may(&self, me: &str, key: u32, display: bool) -> Result<(), No> {
        let f = &self.files[&key];
        let show = self.queues.iter().any(|q| same(&q.outq, &f.outq) && q.show_all);
        if f.owner() == me || self.splctl.iter().any(|u| u == me) || (display && show) {
            Ok(())
        } else {
            Err(no(Status::NotAuthorized, "CPF3303", &[me, &f.outq.name]))
        }
    }

    fn may_use(&self, me: &str, q: usize) -> Result<(), No> {
        let q = &self.queues[q];
        if q.users.is_empty() || q.users.iter().any(|u| u == me) || self.splctl.iter().any(|u| u == me) {
            Ok(())
        } else {
            Err(no(Status::NotAuthorized, "CPF3303", &[me, &q.outq.name]))
        }
    }

    /// `ListSpooledFiles` for `me`.
    ///
    /// # Errors
    /// `NotFound` for a queue or profile not there, `NotAuthorized` for
    /// a queue `me` may not use.
    pub fn list(&self, me: &str, req: &ListSpooledFiles) -> Result<SpooledFiles, No> {
        let user = match req.user.trim().to_ascii_uppercase().as_str() {
            "" | "*CURRENT" => Some(me.to_string()),
            "*ALL" => None,
            u if self.profiles.iter().any(|p| p == u) => Some(u.to_string()),
            u => return Err(no(Status::NotFound, "CPF2204", &[u])),
        };
        let keys: Vec<u32> = if let Some(q) = &req.outq {
            let i = self.find_queue(q)?;
            self.may_use(me, i)?;
            self.queues[i].files.clone()
        } else {
            let mut all: Vec<(&i64, &u32)> = self.files.iter().map(|(k, f)| (&f.created, k)).collect();
            all.sort();
            all.into_iter().map(|(_, k)| *k).collect()
        };
        let hits: Vec<SpooledFile> = keys
            .iter()
            .filter_map(|k| self.files.get(k))
            .filter(|f| user.as_deref().is_none_or(|u| f.owner() == u))
            .filter(|f| req.status.is_none_or(|s| f.status == s))
            .filter(|f| req.form_type.as_deref().is_none_or(|t| f.form_type.eq_ignore_ascii_case(t)))
            .filter(|f| req.user_data.as_deref().is_none_or(|d| f.user_data == d))
            .map(File::info)
            .collect();
        Ok(SpooledFiles {
            total: u32::try_from(hits.len()).unwrap_or(u32::MAX),
            files: page(&hits, req.start, req.max),
        })
    }

    /// `ReadSpooledFile` for `me`: checked under the lock, read outside
    /// it, so reads run side by side.
    ///
    /// # Errors
    /// `NotFound`, `NotAuthorized`, `InUse`, `Error` (the format) or
    /// `InvalidRequest` (the pages).
    pub fn read(spool: &Mutex<Spool>, me: &str, req: &ReadSpooledFile) -> Result<SpooledData, No> {
        let f = {
            let s = lock(spool);
            let key = s.find(&req.id)?;
            s.may(me, key, true)?;
            s.files[&key].clone()
        };
        let number = f.id.number.to_string();
        spool::read(f.status, f.device, &f.data, req).map_err(|e| match e {
            Unreadable::InUse => no(Status::InUse, spool::IN_USE, &[&f.id.file, &number]),
            Unreadable::Format => no(
                Status::Error,
                spool::NO_FORMAT,
                &[&f.id.file, &number, spool::format_name(req.format), spool::device_name(f.device)],
            ),
            Unreadable::Range => no(Status::InvalidRequest, "CPD0084", &["FIRST_PAGE"]),
        })
    }

    /// `HoldSpooledFile`.
    ///
    /// # Errors
    /// `NotFound`, `NotAuthorized`.
    pub fn hold(&mut self, me: &str, id: &SpooledFileId) -> Result<(), No> {
        let key = self.find(id)?;
        self.may(me, key, false)?;
        let f = self.files.get_mut(&key).expect("found");
        if f.status != SpoolStatus::Held {
            f.held_from = f.status;
            f.status = SpoolStatus::Held;
        }
        Ok(())
    }

    /// `ReleaseSpooledFile`.
    ///
    /// # Errors
    /// `NotFound`, `NotAuthorized`, `Error` when neither held nor saved.
    pub fn release(&mut self, me: &str, id: &SpooledFileId) -> Result<(), No> {
        let key = self.find(id)?;
        self.may(me, key, false)?;
        let f = self.files.get_mut(&key).expect("found");
        f.status = match (f.status, f.held_from) {
            (SpoolStatus::Held, SpoolStatus::Open | SpoolStatus::Closed) => f.held_from,
            (SpoolStatus::Held | SpoolStatus::Saved, _) => SpoolStatus::Ready,
            _ => return Err(no(Status::Error, "CPF3337", &[])),
        };
        Ok(())
    }

    /// `DeleteSpooledFile`.
    ///
    /// # Errors
    /// `NotFound`, `NotAuthorized`, `InUse` while printing.
    pub fn delete(&mut self, me: &str, id: &SpooledFileId) -> Result<(), No> {
        let key = self.find(id)?;
        self.may(me, key, false)?;
        if self.files[&key].status == SpoolStatus::Printing {
            return Err(no(Status::InUse, "CPF3330", &[&id.file]));
        }
        self.files.remove(&key);
        for q in &mut self.queues {
            q.files.retain(|k| *k != key);
        }
        Ok(())
    }

    /// `MoveSpooledFile`.
    ///
    /// # Errors
    /// `NotFound` (the file or the queue), `NotAuthorized`, `InUse`
    /// while printing.
    pub fn move_to(&mut self, me: &str, id: &SpooledFileId, outq: &ObjectName, first: bool) -> Result<(), No> {
        let key = self.find(id)?;
        self.may(me, key, false)?;
        if self.files[&key].status == SpoolStatus::Printing {
            return Err(no(Status::InUse, "CPF3330", &[&id.file]));
        }
        let to = self.find_queue(outq)?;
        for q in &mut self.queues {
            q.files.retain(|k| *k != key);
        }
        let q = &mut self.queues[to];
        if first {
            q.files.insert(0, key);
        } else {
            q.files.push(key);
        }
        let real = q.outq.clone();
        self.files.get_mut(&key).expect("found").outq = real;
        Ok(())
    }

    /// `ListOutputQueues` for `me`.
    ///
    /// # Errors
    /// `NotFound` for a library not there, `NotAuthorized` for one `me`
    /// may not read.
    pub fn queues(&self, me: &str, req: &ListOutputQueues) -> Result<OutputQueues, No> {
        let lib = req.library.trim().to_ascii_uppercase();
        let any = matches!(lib.as_str(), "*ALL" | "*LIBL" | "");
        if !any && !self.libraries.contains(&lib) {
            return Err(no(Status::NotFound, "CPF2110", &[&lib]));
        }
        if self.private_libs.contains(&lib) && !self.splctl.iter().any(|u| u == me) {
            return Err(no(Status::NotAuthorized, "CPF9802", &["*LIB", &lib]));
        }
        let hits: Vec<OutputQueue> = self
            .queues
            .iter()
            .filter(|q| any || q.outq.library == lib)
            .map(|q| OutputQueue {
                outq: q.outq.clone(),
                text: q.text.clone(),
                files: u32::try_from(q.files.len()).unwrap_or(u32::MAX),
                writer: q.writer.clone(),
                held: q.held,
            })
            .collect();
        Ok(OutputQueues {
            total: u32::try_from(hits.len()).unwrap_or(u32::MAX),
            queues: page(&hits, req.start, req.max),
        })
    }
}
