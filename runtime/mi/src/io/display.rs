// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display files: where a program's `WRITE`, `READ` and `EXFMT` of a
//! display record format go. The job implements the trait over its
//! 5250 device and the compiled display file; the runtime only moves
//! record buffers and feedback between the program and it.

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
    /// The subfile relative record number the cursor was in, 0 when
    /// none.
    pub sfl_rrn: u32,
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
    /// Wait for the display and read a format.
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
    /// Close.
    fn close(&mut self, handle: u32) -> Result<(), IoFail>;
}
