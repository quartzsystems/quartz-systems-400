// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Open Access: a file whose operations a handler program does instead
//! of the database, the printer or the display (RPG's `HANDLER`). The
//! language runtime makes the handler (`Job::oa_next`) just before the
//! open; every operation on the open file then goes to it.

use std::cell::RefCell;
use std::rc::Rc;

use super::{FileInfo, IoFail, IoRequest, IoResult};
use crate::job::Job;

/// A device operation's request: a printer's line or a display's
/// record, with the carriage control of a program-described printer.
#[derive(Clone, Debug, Default)]
pub struct DevRequest {
    /// 0 a printer's write; a display's 1 write, 2 read, anything else
    /// `EXFMT` (`dev_io`), or past 10 `dsp_io`'s: 11 write, 12 read, 13
    /// `EXFMT`, 14 `CHAIN`, 15 `UPDATE`, 16 `READC` of a subfile.
    pub op: i64,
    /// The record format, blank for a program-described file.
    pub format: String,
    /// The record.
    pub record: Vec<u8>,
    /// Space before, space after, skip before, skip after.
    pub control: Option<[u16; 4]>,
}

/// What a device operation gave back.
#[derive(Clone, Debug, Default)]
pub struct DevResult {
    /// RPG status: 0 when it worked.
    pub status: u16,
    /// A printer reached its overflow line.
    pub overflow: bool,
    /// The record a display read.
    pub input: Option<Vec<u8>>,
}

/// A handler file.
pub trait OpenAccess {
    /// Open: the file's standing, or why the open failed.
    fn open(&mut self, j: &mut Job, file: &qir::FileRef) -> Result<FileInfo, IoFail>;
    /// A record operation.
    fn io(&mut self, j: &mut Job, req: &IoRequest<'_>) -> IoResult;
    /// A device operation.
    fn dev(&mut self, j: &mut Job, req: &DevRequest) -> DevResult;
    /// Close.
    fn close(&mut self, j: &mut Job) -> IoResult;
}

/// A handler file as the job holds it.
pub type OaFile = Rc<RefCell<dyn OpenAccess>>;
