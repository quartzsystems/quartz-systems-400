// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Requests 1560-1567 (docs/DIRSRV.md §4).

use stg::JobInfo;
use stg::dir::{
    DirAnswer, DirBudget, DirChanges, DirClose, DirConn, DirConns, DirError, DirOpened,
};

use crate::ipl::system::System;

fn unavailable() -> DirError {
    DirError::new(
        "CPF9898",
        Vec::new(),
        "The directory server is not available.",
    )
}

/// 1560.
pub fn open(_: &mut System, _: &JobInfo, _: &str, _: DirConn) -> Result<DirOpened, DirError> {
    Err(unavailable())
}

/// 1561.
pub fn bind(
    _: &mut System,
    _: &JobInfo,
    _: u64,
    _: &[u8],
    _: DirConn,
) -> Result<DirAnswer, DirError> {
    Err(unavailable())
}

/// 1562.
pub fn op(_: &mut System, _: &JobInfo, _: u64, _: &[u8]) -> Result<DirAnswer, DirError> {
    Err(unavailable())
}

/// 1563.
pub fn search(
    _: &mut System,
    _: &JobInfo,
    _: u64,
    _: &[u8],
    _: DirBudget,
) -> Result<DirAnswer, DirError> {
    Err(unavailable())
}

/// 1564.
pub fn next(
    _: &mut System,
    _: &JobInfo,
    _: u64,
    _: u64,
    _: DirBudget,
) -> Result<DirAnswer, DirError> {
    Err(unavailable())
}

/// 1565.
pub fn close(_: &mut System, _: &JobInfo, _: u64, _: DirClose) -> Result<(), DirError> {
    Err(unavailable())
}

/// 1566.
pub fn conns(_: &JobInfo, _: &str, _: DirConns) -> Result<(), DirError> {
    Err(unavailable())
}

/// 1567.
pub fn changes(_: &mut System, _: &JobInfo, _: u64, _: u32) -> Result<DirChanges, DirError> {
    Err(unavailable())
}

/// A job ended.
pub fn job_ended(_: u32) {}
