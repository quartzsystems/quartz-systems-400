// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display files: where a program's `WRITE`, `READ` and `EXFMT` of a
//! display record format go, and its subfile operations (`WRITE` at a
//! relative record number, `CHAIN`, `UPDATE`, `READC`). The job
//! implements the trait over its 5250 device and the compiled display
//! file; the runtime only moves record buffers and feedback between the
//! program and it.

use super::IoFail;

/// An open display file.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DisplayOpen {
    /// The handle later operations name.
    pub handle: u32,
    /// The library the file was found in.
    pub lib: String,
}

/// What a read brought back.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct DisplayInput {
    /// The record format read.
    pub format: String,
    /// The input buffer as the program's record format lays it out
    /// (response indicators included when the file has no `INDARA`).
    pub record: Vec<u8>,
    /// The separate indicator area (`INDARA`): 99 bytes of `0` and `1`.
    pub indicators: Vec<u8>,
    /// The attention identifier of the key pressed.
    pub aid: u8,
    /// Cursor row and column, 1-based.
    pub cursor: (u16, u16),
    /// The subfile relative record number: the record the cursor was
    /// in for a control record's read, the record read for `CHAIN` and
    /// `READC`; 0 when none.
    pub sfl_rrn: u32,
    /// The lowest subfile record number on the display, 0 when none.
    pub sfl_top: u32,
    /// How many records the subfile holds.
    pub sfl_count: u32,
}

/// The status of a display operation the source does not support.
fn unsupported(format: &str) -> IoFail {
    IoFail::new(1299, "CPF5129", &[format])
}

/// Where display files are opened and their formats shown and read.
pub trait DisplaySource {
    /// Open `lib/file` for the activation group.
    fn open(&mut self, file: &str, lib: &str, actgrp: &str) -> Result<DisplayOpen, IoFail>;
    /// Show a record format from the program's output buffer.
    fn write(
        &mut self,
        handle: u32,
        format: &str,
        record: &[u8],
        indicators: &[u8],
    ) -> Result<(), IoFail>;
    /// Wait for the display and read a format; an empty format reads
    /// the one last written that takes input.
    fn read(&mut self, handle: u32, format: &str) -> Result<DisplayInput, IoFail>;
    /// Write, then read the same format.
    fn exfmt(
        &mut self,
        handle: u32,
        format: &str,
        record: &[u8],
        indicators: &[u8],
    ) -> Result<DisplayInput, IoFail> {
        self.write(handle, format, record, indicators)?;
        self.read(handle, format)
    }
    /// Put a subfile record at `rrn` (1-based).
    fn sfl_write(
        &mut self,
        _handle: u32,
        format: &str,
        _rrn: u32,
        _record: &[u8],
        _indicators: &[u8],
    ) -> Result<(), IoFail> {
        Err(unsupported(format))
    }
    /// The subfile record at `rrn`, `None` when there is none.
    fn sfl_chain(
        &mut self,
        _handle: u32,
        format: &str,
        _rrn: u32,
    ) -> Result<Option<DisplayInput>, IoFail> {
        Err(unsupported(format))
    }
    /// Replace the subfile record last read.
    fn sfl_update(
        &mut self,
        _handle: u32,
        format: &str,
        _record: &[u8],
        _indicators: &[u8],
    ) -> Result<(), IoFail> {
        Err(unsupported(format))
    }
    /// The next changed subfile record, `None` at the end.
    fn sfl_readc(&mut self, _handle: u32, format: &str) -> Result<Option<DisplayInput>, IoFail> {
        Err(unsupported(format))
    }
    /// Close.
    fn close(&mut self, handle: u32) -> Result<(), IoFail>;
}
