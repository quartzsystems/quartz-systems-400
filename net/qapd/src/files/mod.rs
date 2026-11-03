// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Ifs` service (docs/QAP.md, version 5) over an
//! [`IfsClient`], the one the job's file-system commands use: every
//! operation is the calls those commands make, so QAP's answers,
//! authority checks and audit entries are theirs.
//!
//! - `entries`: `ListDirectory`, `GetInfo` and `Stat` (version 6), as
//!   `WRKLNK` reads them.
//! - `read`: `ReadFile`, as stored or converted, with the shift-state
//!   memo of mixed files.
//! - `write`: `WriteFile`, converted per request, the shift-in and
//!   shift-out joined at the end of a mixed file.
//! - `names`: `CreateDirectory` (`MD`), `Delete` (`RMVLNK`, `RD`),
//!   `Rename` (`MOV`), `Copy` (`CPY`).
//! - [`shift`]: the shift state of mixed CCSIDs.
//!
//! A failure is the command's message: the `CPFA0xx` escape for the
//! `errno` ([`ifscmd::Msg::errno`]), its status by the message.

mod entries;
mod names;
mod read;
pub mod shift;
mod write;

use std::collections::VecDeque;
use std::sync::Mutex;

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::lock::range::RangeKind;
use ifs::ops::flags::{F_GETLK, SEEK_SET};
use ifscmd::Msg;
use ipc::Wire;
use qap_proto::ops::ifs::{CreateDirectory, FromTo, ListDirectory, PathRef, ReadFile, WriteFile};
use qap_proto::{Envelope, Op, Status};

use crate::conn::{Answer, lock};
use crate::host::ThreadHost;
use shift::Shift;

/// The message when data cannot be converted between two CCSIDs.
pub const NO_CONVERSION: &str = "TCP1A5A";

/// Why an operation stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// A value out of range: `InvalidRequest`.
    Invalid,
    /// A command's message, binary substitution data and all.
    Msg(Status, Msg),
    /// A message by its text variables.
    Text(Status, &'static str, Vec<String>),
}

impl Stop {
    /// The command's escape for `e` on `path`.
    #[must_use]
    pub fn errno(e: Errno, path: &str) -> Self {
        let m = Msg::errno(e, path);
        Self::Msg(status_of(&m.id), m)
    }

    /// The command's escape `m`.
    #[must_use]
    pub fn escape(m: Msg) -> Self {
        Self::Msg(status_of(&m.id), m)
    }

    /// `from` cannot be converted to `to`.
    #[must_use]
    pub fn conversion(from: u16, to: u16) -> Self {
        Self::Text(Status::Error, NO_CONVERSION, vec![from.to_string(), to.to_string()])
    }
}

/// The status a file-system command's escape stands for.
#[must_use]
pub fn status_of(id: &str) -> Status {
    match id {
        "CPFA0A9" => Status::NotFound,
        "CPFA09C" => Status::NotAuthorized,
        "CPFA0A0" => Status::Exists,
        "CPFA09E" => Status::InUse,
        _ => Status::Error,
    }
}

/// An operation's result: the encoded reply body.
pub type Done = Result<Vec<u8>, Stop>;

/// One mixed file's shift state at an offset, as the last reply left
/// it: the path, the offset, the file's change time and the state.
type Mark = (String, u64, i64, Shift);

/// Per connection: where each `ReadFile` of a mixed file stopped and in
/// which shift state, so the next read from there needs no scan.
#[derive(Debug, Default)]
pub struct Memo {
    marks: Mutex<VecDeque<Mark>>,
}

/// Marks kept per connection.
const MARKS: usize = 32;

impl Memo {
    /// The state at `offset` of `path` changed at `mtime`, if a reply
    /// stopped there.
    #[must_use]
    pub fn get(&self, path: &str, offset: u64, mtime: i64) -> Option<Shift> {
        lock(&self.marks)
            .iter()
            .find(|(p, o, m, _)| p == path && *o == offset && *m == mtime)
            .map(|m| m.3)
    }

    /// A reply stopped at `offset` in state `s`.
    pub fn put(&self, path: &str, offset: u64, mtime: i64, s: Shift) {
        let mut marks = lock(&self.marks);
        marks.retain(|(p, ..)| p != path);
        if marks.len() == MARKS {
            marks.pop_front();
        }
        marks.push_back((path.to_string(), offset, mtime, s));
    }
}

/// Whether another job holds a byte-range lock over `len` bytes (0: to
/// the end) from `start` that the access `kind` would conflict with:
/// `EBUSY`, which the commands report as the object in use.
pub(crate) fn held(c: &dyn IfsClient, fd: i32, kind: RangeKind, start: u64, len: u64) -> Result<(), Errno> {
    let s = i64::try_from(start).unwrap_or(i64::MAX);
    let l = i64::try_from(len).unwrap_or(0);
    match c.lock_range(fd, F_GETLK, kind, SEEK_SET, s, l)? {
        Some(_) => Err(Errno::EBUSY),
        None => Ok(()),
    }
}

/// Run one operation against `c`.
fn run(c: &dyn IfsClient, memo: &Memo, op: Op, req: &Envelope) -> Done {
    match op {
        Op::ListDirectory => entries::list(c, &decode::<ListDirectory>(req)?),
        Op::GetInfo => entries::info(c, &decode::<PathRef>(req)?.path),
        Op::Stat => entries::stat(c, &decode::<PathRef>(req)?.path),
        Op::ReadFile => read::read(c, memo, &decode::<ReadFile>(req)?),
        Op::WriteFile => write::write(c, &decode::<WriteFile>(req)?),
        Op::CreateDirectory => names::create_directory(c, &decode::<CreateDirectory>(req)?),
        Op::Delete => names::delete(c, &decode::<PathRef>(req)?.path),
        Op::Rename => names::rename(c, &decode::<FromTo>(req)?),
        Op::Copy => names::copy(c, &decode::<FromTo>(req)?),
        _ => Err(Stop::Invalid),
    }
}

fn decode<B: Wire>(req: &Envelope) -> Result<B, Stop> {
    req.body_as::<B>().map_err(|_| Stop::Invalid)
}

/// Answer one `Ifs` operation on a secondary thread, as the job's
/// profile, through the host's file system.
pub(crate) fn serve(host: &mut dyn ThreadHost, memo: &Memo, op: Op, req: &Envelope) -> Answer {
    let mut done: Option<Done> = None;
    if let Err(r) = host.with_ifs(&mut |c| done = Some(run(c, memo, op, req))) {
        return Answer::refused(r);
    }
    match done {
        Some(Ok(body)) => Answer {
            status: Status::Ok,
            message: None,
            body,
        },
        Some(Err(Stop::Msg(status, m))) => Answer {
            status,
            message: Some(host.message_data(&m.id, &m.data)),
            body: Vec::new(),
        },
        Some(Err(Stop::Text(status, id, data))) => {
            let data: Vec<&str> = data.iter().map(String::as_str).collect();
            Answer {
                status,
                message: Some(host.message(id, &data)),
                body: Vec::new(),
            }
        }
        Some(Err(Stop::Invalid)) | None => Answer::status(Status::InvalidRequest),
    }
}
