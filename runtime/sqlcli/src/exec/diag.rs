// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading the diagnostics: `SQLError` takes the first record of the
//! handle it is given (the statement's, else the connection's, else the
//! environment's); `SQLGetDiagRec` and `SQLGetDiagField` read without
//! taking. None of them clears the records.

use super::Cli;
use super::host::{Host, put_i16, put_i32, put_str};
use crate::consts::{diag, htype, rc};
use crate::model::diag::Diag;
use crate::model::handles::Ptr;

/// Write a record's state, native error and text.
#[allow(clippy::too_many_arguments)]
fn put_record(host: &mut dyn Host, d: &Diag, state: Ptr, native: Ptr, msg: Ptr, max: i16, mlen: Ptr) -> bool {
    put_str(host, state, 6, &d.state);
    put_i32(host, native, d.native);
    let (n, cut) = put_str(host, msg, i32::from(max), &d.text);
    put_i16(host, mlen, i16::try_from(n).unwrap_or(i16::MAX));
    cut
}

impl Cli {
    /// `SQLError`.
    #[allow(clippy::too_many_arguments)]
    pub fn error(&mut self, host: &mut dyn Host, henv: i32, hdbc: i32, hstmt: i32, state: Ptr, native: Ptr, msg: Ptr, max: i16, mlen: Ptr) -> i32 {
        let (h, t) = if hstmt != 0 {
            (hstmt, htype::STMT)
        } else if hdbc != 0 {
            (hdbc, htype::DBC)
        } else {
            (henv, htype::ENV)
        };
        if !self.handles.is(h, t) {
            return rc::INVALID_HANDLE;
        }
        let Some(d) = self.handles.diags(h).and_then(crate::model::diag::Diags::take) else { return rc::NO_DATA };
        if put_record(host, &d, state, native, msg, max, mlen) { rc::SUCCESS_WITH_INFO } else { rc::SUCCESS }
    }

    /// `SQLGetDiagRec`.
    #[allow(clippy::too_many_arguments)]
    pub fn get_diag_rec(&mut self, host: &mut dyn Host, t: i16, h: i32, rec: i16, state: Ptr, native: Ptr, msg: Ptr, max: i16, mlen: Ptr) -> i32 {
        if !self.handles.is(h, t) {
            return rc::INVALID_HANDLE;
        }
        if rec < 1 || max < 0 {
            return rc::ERROR;
        }
        let Some(d) = self.handles.diags(h).and_then(|x| x.recs.get(usize::try_from(rec).unwrap_or(0) - 1).cloned()) else {
            return rc::NO_DATA;
        };
        if put_record(host, &d, state, native, msg, max, mlen) { rc::SUCCESS_WITH_INFO } else { rc::SUCCESS }
    }

    /// `SQLGetDiagField`: the header's number of records, return code
    /// and row count; a record's state, native error and text.
    #[allow(clippy::too_many_arguments)]
    pub fn get_diag_field(&mut self, host: &mut dyn Host, t: i16, h: i32, rec: i16, id: i16, val: Ptr, max: i16, vlen: Ptr) -> i32 {
        if !self.handles.is(h, t) {
            return rc::INVALID_HANDLE;
        }
        let Some(ds) = self.handles.diags(h).cloned() else { return rc::INVALID_HANDLE };
        match id {
            diag::NUMBER => put_i32(host, val, i32::try_from(ds.recs.len()).unwrap_or(0)),
            diag::RETURNCODE => put_i32(host, val, ds.rc),
            diag::ROW_COUNT if t == htype::STMT => put_i32(host, val, ds.row_count),
            diag::SQLSTATE | diag::NATIVE | diag::MESSAGE_TEXT => {
                let Some(d) = usize::try_from(rec).ok().filter(|r| *r > 0).and_then(|r| ds.recs.get(r - 1)) else {
                    return rc::NO_DATA;
                };
                if id == diag::NATIVE {
                    put_i32(host, val, d.native);
                } else {
                    let text = if id == diag::SQLSTATE { &d.state } else { &d.text };
                    let (n, cut) = put_str(host, val, i32::from(max), text);
                    // The oracle counts a state's NUL.
                    let n = if id == diag::SQLSTATE { n + 1 } else { n };
                    put_i16(host, vlen, i16::try_from(n).unwrap_or(i16::MAX));
                    if cut {
                        return rc::SUCCESS_WITH_INFO;
                    }
                }
            }
            _ => return rc::ERROR,
        }
        rc::SUCCESS
    }
}
