// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What RPG's file information data structure will show: the status of
//! the last operation and where the file stands.

use qsname::Name;

use crate::error::DbError;

/// RPG file status codes.
pub mod status {
    /// Operation completed.
    pub const OK: u16 = 0;
    /// End or beginning of file on a read.
    pub const EOF: u16 = 11;
    /// Record not found on a `CHAIN`.
    pub const NOT_FOUND: u16 = 12;
    /// Duplicate key on a write or update.
    pub const DUPLICATE_KEY: u16 = 1021;
    /// Operation on a closed file.
    pub const CLOSED: u16 = 1211;
    /// Record locked by another job.
    pub const LOCKED: u16 = 1218;
    /// Update or delete without a prior read for update.
    pub const NO_PRIOR_READ: u16 = 1221;
    /// The file could not be allocated.
    pub const ALLOCATE: u16 = 1222;
    /// Any other I/O error.
    pub const OTHER: u16 = 1299;
}

/// The feedback of the last operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IoFeedback {
    /// Status code, as a number (`1218` for `01218`).
    pub status: u16,
    /// Operation code name.
    pub opcode: &'static str,
    /// Relative record number of the record touched, 0 for none.
    pub rrn: u64,
    /// Encoded key of the record touched, empty for none.
    pub key: Vec<u8>,
    /// Record format of the record touched.
    pub format: Name,
    /// Member.
    pub member: Name,
    /// Message ID when the operation failed.
    pub msgid: Option<&'static str>,
    /// `%EOF`.
    pub eof: bool,
    /// `%EQUAL`.
    pub equal: bool,
    /// `%FOUND`.
    pub found: bool,
    /// Operations on this open data path.
    pub operations: u64,
}

impl IoFeedback {
    /// A fresh feedback for an open.
    #[must_use]
    pub fn new(format: Name, member: Name) -> Self {
        Self {
            status: status::OK,
            opcode: "OPEN",
            rrn: 0,
            key: Vec::new(),
            format,
            member,
            msgid: None,
            eof: false,
            equal: false,
            found: false,
            operations: 0,
        }
    }

    /// The five-digit status.
    #[must_use]
    pub fn status_code(&self) -> String {
        format!("{:05}", self.status)
    }

    /// Record an operation that is starting.
    pub fn begin(&mut self, opcode: &'static str) {
        self.opcode = opcode;
        self.status = status::OK;
        self.msgid = None;
        self.eof = false;
        self.equal = false;
        self.found = false;
        self.operations += 1;
    }

    /// Record a failure, mapping the error to its status.
    pub fn fail(&mut self, e: &DbError) {
        self.status = match e {
            DbError::DuplicateKey { .. } => status::DUPLICATE_KEY,
            DbError::RecordLocked { .. } => status::LOCKED,
            DbError::NoLockedRecord { .. } => status::NO_PRIOR_READ,
            DbError::FileLocked { .. } => status::ALLOCATE,
            DbError::Closed => status::CLOSED,
            _ => status::OTHER,
        };
        self.msgid = e.msgid();
    }
}
