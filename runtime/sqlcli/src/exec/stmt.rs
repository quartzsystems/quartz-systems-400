// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Statements: preparing and describing, binding parameters and
//! columns, running, fetching into the bound storage, closing. The
//! bound buffers are the program's: parameters are read from them at
//! each `SQLExecute`, columns written at each `SQLFetch`, long after
//! the calls that named them.

use sql::Value;
use sql::embed::Op;

use super::host::{Host, get_i32, get_str, put_i16, put_i32, put_str};
use super::{Cli, outcome};
use crate::consts::{TRUE, fetch, free, htype, len, null, param, rc, stmt_attr, ty};
use crate::data::convert::{self, Image};
use crate::data::describe::{self, default_ctype};
use crate::model::diag::Diag;
use crate::model::handles::{ColBind, ParamBind, Ptr, StmtState};

/// The prepared statement's name in its connection's scope.
fn stmt_name(h: i32) -> String {
    format!("S{h}")
}

/// The cursor's name.
fn cursor_name(h: i32) -> String {
    format!("CLICUR{h}")
}

impl Cli {
    /// The connection of statement `h`; `None` when `h` is not a
    /// statement (its records cleared otherwise).
    fn stmt_dbc(&mut self, h: i32) -> Option<i32> {
        if !self.handles.is(h, htype::STMT) || !self.begin(h) {
            return None;
        }
        self.handles.stmt(h).map(|s| s.dbc)
    }

    /// `SQLPrepare`.
    pub fn prepare(&mut self, host: &mut dyn Host, h: i32, text: Ptr, tlen: i32) -> i32 {
        let Some(dbc) = self.stmt_dbc(h) else { return rc::INVALID_HANDLE };
        if self.handles.stmt(h).is_some_and(|s| s.state == StmtState::Open) {
            return self.fail(h, "HY010", "The statement has a cursor open.");
        }
        let Some(text) = get_str(host, text, tlen) else { return self.fail(h, "HY009", "No statement text was given.") };
        self.prepare_text(host, h, dbc, &text)
    }

    /// Prepare `text` as statement `h` and describe it.
    pub(crate) fn prepare_text(&mut self, host: &mut dyn Host, h: i32, dbc: i32, text: &str) -> i32 {
        if let Some(s) = self.handles.stmt(h) {
            s.state = StmtState::Allocated;
            s.cols.clear();
            s.params.clear();
            s.row = None;
            s.row_count = -1;
        }
        let mut d = self.desc(dbc, Op::Prepare);
        d.name = stmt_name(h);
        d.text = text.trim().to_string();
        let (r, _) = self.run(host, dbc, d, Vec::new());
        if r.sqlca.is_error() {
            return self.answer(h, &r.sqlca);
        }
        let mut d = self.desc(dbc, Op::Describe);
        d.name = stmt_name(h);
        d.orient = "OUTPUT NAMES".into();
        let (out, _) = self.run(host, dbc, d.clone(), Vec::new());
        d.orient = "INPUT NAMES".into();
        let (inp, _) = self.run(host, dbc, d, Vec::new());
        for ca in [&out.sqlca, &inp.sqlca] {
            if ca.is_error() {
                return self.answer(h, ca);
            }
        }
        if let Some(s) = self.handles.stmt(h) {
            s.cols = describe::columns(&out.rows);
            s.params = describe::params(&inp.rows);
            s.state = StmtState::Prepared;
        }
        if r.sqlca.sqlcode > 0 { self.answer(h, &r.sqlca) } else { rc::SUCCESS }
    }

    /// `SQLExecDirect`: prepared, then run.
    pub fn exec_direct(&mut self, host: &mut dyn Host, h: i32, text: Ptr, tlen: i32) -> i32 {
        let code = self.prepare(host, h, text, tlen);
        if code != rc::SUCCESS && code != rc::SUCCESS_WITH_INFO {
            return code;
        }
        self.execute(host, h)
    }

    /// The input values from the bound parameters.
    fn inputs(&mut self, host: &dyn Host, h: i32) -> Result<Vec<Value>, Diag> {
        let Some(s) = self.handles.stmt(h) else { return Ok(Vec::new()) };
        let mut out = Vec::with_capacity(s.params.len());
        for (i, p) in s.params.iter().enumerate() {
            let Some(b) = u16::try_from(i + 1).ok().and_then(|k| s.param_binds.get(&k)).copied() else {
                return Err(Diag::cli("07001", &format!("Parameter marker {} is not bound.", i + 1)));
            };
            if b.io == param::OUTPUT {
                out.push(Value::Null);
                continue;
            }
            let ind = if b.ind.is_some() { get_i32(host, b.ind) } else { None };
            if ind == Some(len::NULL_DATA) {
                out.push(Value::Null);
                continue;
            }
            let ctype = if b.ctype == ty::DEFAULT { default_ctype(if b.stype == 0 { p.sql_type } else { b.stype }) } else { b.ctype };
            let Some(at) = b.data else { return Err(Diag::cli("HY009", &format!("Parameter {} has no buffer.", i + 1))) };
            let avail = host.avail(at);
            let want = match ctype {
                ty::CHAR | ty::DATE | ty::TIME | ty::TIMESTAMP | ty::BINARY => {
                    let cap = if b.max > 0 {
                        b.max
                    } else if let Some(n) = ind.filter(|n| *n >= 0) {
                        n
                    } else if b.len > 0 {
                        b.len
                    } else {
                        i32::try_from(avail.min(32_766)).unwrap_or(0)
                    };
                    usize::try_from(cap).unwrap_or(0)
                }
                ty::VARCHAR | ty::VARBINARY => {
                    let head = host.read(at, 2).unwrap_or_default();
                    2 + head.get(..2).map_or(0, |x| usize::from(u16::from_be_bytes([x[0], x[1]])))
                }
                ty::DECIMAL => usize::try_from(b.len).unwrap_or(0) / 2 + 1,
                ty::NUMERIC => usize::try_from(b.len).unwrap_or(0),
                other => convert::buffer_len(other, 0),
            };
            let bytes = host.read(at, want.min(avail)).unwrap_or_default();
            out.push(convert::value_of(ctype, &bytes, ind, b.len, b.scale)?);
        }
        Ok(out)
    }

    /// `SQLExecute`.
    pub fn execute(&mut self, host: &mut dyn Host, h: i32) -> i32 {
        let Some(dbc) = self.stmt_dbc(h) else { return rc::INVALID_HANDLE };
        match self.handles.stmt(h).map(|s| s.state) {
            Some(StmtState::Allocated) => return self.fail(h, "HY010", "No statement is prepared."),
            Some(StmtState::Open) => return self.fail(h, "HY010", "The statement has a cursor open."),
            _ => {}
        }
        let inputs = match self.inputs(host, h) {
            Ok(v) => v,
            Err(d) => return self.note(h, rc::ERROR, d),
        };
        let Some(s) = self.handles.stmt(h) else { return rc::INVALID_HANDLE };
        let query = !s.cols.is_empty();
        let (scroll, hold) = (s.scrollable, s.hold);
        if query {
            let mut d = self.desc(dbc, Op::Open);
            d.name = cursor_name(h);
            d.stmt = stmt_name(h);
            d.scroll = scroll;
            d.hold = hold;
            let (r, _) = self.run(host, dbc, d, inputs);
            if r.sqlca.is_error() {
                return self.answer(h, &r.sqlca);
            }
            if let Some(s) = self.handles.stmt(h) {
                s.state = StmtState::Open;
                s.cursor = cursor_name(h);
                s.row = None;
                s.row_count = -1;
            }
            return if r.sqlca.sqlcode > 0 && r.sqlca.sqlcode != 100 { self.answer(h, &r.sqlca) } else { rc::SUCCESS };
        }
        let mut d = self.desc(dbc, Op::Execute);
        d.name = stmt_name(h);
        let (r, outs) = self.run(host, dbc, d, inputs);
        let (code, rec) = outcome(&r.sqlca);
        if code == rc::ERROR {
            return self.answer(h, &r.sqlca);
        }
        let count = if r.sqlca.sqlcode == 100 { 0 } else { r.sqlca.sqlerrd[2] };
        if let Some(s) = self.handles.stmt(h) {
            s.row_count = count;
            s.diags.row_count = count;
        }
        if let Err(d) = self.write_outs(host, h, &outs) {
            return self.note(h, rc::ERROR, d);
        }
        // A procedure's first result set is the statement's cursor.
        if r.sqlca.sqlcode == 466
            && let Some(name) = r.rows.first().and_then(|row| row.first()).map(Value::as_text)
        {
            let mut d = self.desc(dbc, Op::Describe);
            d.name.clone_from(&name);
            d.orient = "CURSOR".into();
            let (cols, _) = self.run(host, dbc, d, Vec::new());
            if let Some(s) = self.handles.stmt(h) {
                s.cols = describe::columns(&cols.rows);
                s.state = StmtState::Open;
                s.cursor = name;
                s.row = None;
            }
        }
        if let Some(ca) = self.autocommit(host, dbc) {
            return self.answer(h, &ca);
        }
        match rec {
            Some(d) => self.note(h, code, d),
            None => code,
        }
    }

    /// A `CALL`'s output values into the bound output parameters.
    fn write_outs(&mut self, host: &mut dyn Host, h: i32, outs: &[(usize, Value)]) -> Result<(), Diag> {
        let nts = self.nts();
        let Some(s) = self.handles.stmt(h) else { return Ok(()) };
        let binds = s.param_binds.clone();
        let types: Vec<i16> = s.params.iter().map(|p| p.sql_type).collect();
        for (m, v) in outs {
            let Some(b) = u16::try_from(m + 1).ok().and_then(|k| binds.get(&k)) else { continue };
            if b.io == param::INPUT {
                continue;
            }
            let ctype = if b.ctype == ty::DEFAULT { default_ctype(types.get(*m).copied().unwrap_or(ty::CHAR)) } else { b.ctype };
            let max = if b.max > 0 { b.max } else { b.len };
            let max = if matches!(ctype, ty::DECIMAL | ty::NUMERIC) { b.len * 256 + i32::from(b.scale) } else { max };
            put_value(host, v, ctype, max, b.data, b.ind, nts)?;
        }
        Ok(())
    }

    /// Whether character output ends with a NUL.
    fn nts(&mut self) -> bool {
        self.handles.env_number().and_then(|e| self.handles.env(e)).is_none_or(|e| e.output_nts)
    }

    /// `SQLBindParam` and `SQLBindParameter`; a null buffer unbinds.
    #[allow(clippy::too_many_arguments)]
    pub fn bind_param(&mut self, h: i32, ipar: i16, io: i16, ctype: i16, stype: i16, plen: i32, scale: i16, data: Ptr, max: i32, ind: Ptr) -> i32 {
        if self.stmt_dbc(h).is_none() {
            return rc::INVALID_HANDLE;
        }
        let Ok(k) = u16::try_from(ipar) else { return self.fail(h, "07009", "The parameter number is not valid.") };
        if k == 0 {
            return self.fail(h, "07009", "The parameter number is not valid.");
        }
        if !matches!(io, param::INPUT | param::OUTPUT | param::INPUT_OUTPUT) {
            return self.fail(h, "HY105", "The parameter type is not valid.");
        }
        if ctype != ty::DEFAULT && !convert::supported(ctype) {
            return self.note(h, rc::ERROR, convert::unsupported(ctype));
        }
        let Some(s) = self.handles.stmt(h) else { return rc::INVALID_HANDLE };
        if data.is_none() && ind.is_none() {
            s.param_binds.remove(&k);
        } else {
            s.param_binds.insert(k, ParamBind { io, ctype, stype, len: plen, scale, data, max, ind });
        }
        rc::SUCCESS
    }

    /// `SQLNumParams`.
    pub fn num_params(&mut self, host: &mut dyn Host, h: i32, out: Ptr) -> i32 {
        if self.stmt_dbc(h).is_none() {
            return rc::INVALID_HANDLE;
        }
        let n = self.handles.stmt(h).map_or(0, |s| s.params.len());
        put_i16(host, out, i16::try_from(n).unwrap_or(i16::MAX));
        rc::SUCCESS
    }

    /// `SQLNumResultCols`.
    pub fn num_result_cols(&mut self, host: &mut dyn Host, h: i32, out: Ptr) -> i32 {
        if self.stmt_dbc(h).is_none() {
            return rc::INVALID_HANDLE;
        }
        let n = self.handles.stmt(h).map_or(0, |s| s.cols.len());
        put_i16(host, out, i16::try_from(n).unwrap_or(i16::MAX));
        rc::SUCCESS
    }

    /// `SQLDescribeParam`: a marker is always nullable.
    pub fn describe_param(&mut self, host: &mut dyn Host, h: i32, ipar: i16, t: Ptr, def: Ptr, scale: Ptr, nullable: Ptr) -> i32 {
        if self.stmt_dbc(h).is_none() {
            return rc::INVALID_HANDLE;
        }
        let p = usize::try_from(ipar).ok().filter(|i| *i > 0).and_then(|i| self.handles.stmt(h).and_then(|s| s.params.get(i - 1).cloned()));
        let Some(p) = p else { return self.fail(h, "07009", "The parameter number is not valid.") };
        put_i16(host, t, p.sql_type);
        put_i32(host, def, p.size);
        put_i16(host, scale, p.scale);
        put_i16(host, nullable, null::NULLABLE);
        rc::SUCCESS
    }

    /// `SQLDescribeCol`.
    #[allow(clippy::too_many_arguments)]
    pub fn describe_col(&mut self, host: &mut dyn Host, h: i32, icol: i16, name: Ptr, namemax: i16, namelen: Ptr, t: Ptr, def: Ptr, scale: Ptr, nullable: Ptr) -> i32 {
        if self.stmt_dbc(h).is_none() {
            return rc::INVALID_HANDLE;
        }
        if self.handles.stmt(h).is_some_and(|s| s.state == StmtState::Allocated) {
            return self.fail(h, "HY010", "No statement is prepared.");
        }
        let c = usize::try_from(icol).ok().filter(|i| *i > 0).and_then(|i| self.handles.stmt(h).and_then(|s| s.cols.get(i - 1).cloned()));
        let Some(c) = c else { return self.fail(h, "07009", "The column number is not valid.") };
        let (n, cut) = put_str(host, name, i32::from(namemax), &c.name);
        put_i16(host, namelen, i16::try_from(n).unwrap_or(i16::MAX));
        put_i16(host, t, c.sql_type);
        put_i32(host, def, c.size);
        put_i16(host, scale, c.scale);
        put_i16(host, nullable, if c.nullable { null::NULLABLE } else { null::NO_NULLS });
        if cut { self.note(h, rc::SUCCESS_WITH_INFO, Diag::cli("01004", "The column name was cut short.")) } else { rc::SUCCESS }
    }

    /// `SQLBindCol`; a null buffer unbinds the column.
    pub fn bind_col(&mut self, h: i32, icol: i16, ctype: i16, data: Ptr, max: i32, ind: Ptr) -> i32 {
        if self.stmt_dbc(h).is_none() {
            return rc::INVALID_HANDLE;
        }
        let Ok(k) = u16::try_from(icol) else { return self.fail(h, "07009", "The column number is not valid.") };
        if k == 0 {
            return self.fail(h, "07009", "Bookmark columns are not supported.");
        }
        if ctype != ty::DEFAULT && !convert::supported(ctype) {
            return self.note(h, rc::ERROR, convert::unsupported(ctype));
        }
        if max < 0 {
            return self.fail(h, "HY090", "The buffer length is not valid.");
        }
        let Some(s) = self.handles.stmt(h) else { return rc::INVALID_HANDLE };
        if data.is_none() {
            s.col_binds.remove(&k);
        } else {
            s.col_binds.insert(k, ColBind { ctype, data, max, ind });
        }
        rc::SUCCESS
    }

    /// `SQLFetch`.
    pub fn fetch(&mut self, host: &mut dyn Host, h: i32) -> i32 {
        self.fetch_scroll(host, h, fetch::NEXT, 0)
    }

    /// `SQLFetchScroll`: other than `SQL_FETCH_NEXT` needs a scrollable cursor.
    pub fn fetch_scroll(&mut self, host: &mut dyn Host, h: i32, orient: i16, offset: i32) -> i32 {
        let Some(dbc) = self.stmt_dbc(h) else { return rc::INVALID_HANDLE };
        let Some(s) = self.handles.stmt(h) else { return rc::INVALID_HANDLE };
        if s.state != StmtState::Open {
            return self.fail(h, "HY010", "The statement has no cursor open.");
        }
        let words = match orient {
            fetch::NEXT => "NEXT".to_string(),
            fetch::FIRST => "FIRST".into(),
            fetch::LAST => "LAST".into(),
            fetch::PRIOR => "PRIOR".into(),
            fetch::ABSOLUTE => format!("ABSOLUTE {offset}"),
            fetch::RELATIVE => format!("RELATIVE {offset}"),
            _ => return self.fail(h, "HY106", "The fetch orientation is not valid."),
        };
        if orient != fetch::NEXT && !s.scrollable {
            return self.fail(h, "HY106", "The cursor is not scrollable.");
        }
        let cursor = s.cursor.clone();
        let scroll = s.scrollable;
        let mut d = self.desc(dbc, Op::Fetch);
        d.name = cursor;
        d.orient = words;
        d.scroll = scroll;
        let (r, _) = self.run(host, dbc, d, Vec::new());
        if r.sqlca.sqlcode == 100 || (r.sqlca.sqlcode >= 0 && r.rows.is_empty()) {
            if let Some(s) = self.handles.stmt(h) {
                s.row = None;
            }
            return rc::NO_DATA;
        }
        if r.sqlca.is_error() {
            return self.answer(h, &r.sqlca);
        }
        let row = r.rows.into_iter().next().unwrap_or_default();
        let nts = self.nts();
        let Some(s) = self.handles.stmt(h) else { return rc::INVALID_HANDLE };
        s.got.clear();
        let binds: Vec<(u16, ColBind)> = s.col_binds.iter().map(|(k, b)| (*k, *b)).collect();
        let types: Vec<i16> = s.cols.iter().map(|c| c.sql_type).collect();
        s.row = Some(row.clone());
        for (k, b) in binds {
            let Some(v) = row.get(usize::from(k) - 1) else { continue };
            let ctype = if b.ctype == ty::DEFAULT { default_ctype(types[usize::from(k) - 1]) } else { b.ctype };
            if let Err(d) = put_value(host, v, ctype, b.max, b.data, b.ind, nts) {
                return self.note(h, rc::ERROR, d);
            }
        }
        if r.sqlca.sqlcode > 0 { self.answer(h, &r.sqlca) } else { rc::SUCCESS }
    }

    /// `SQLGetData`: a column of the current row, each once, in order.
    pub fn get_data(&mut self, host: &mut dyn Host, h: i32, icol: i16, ctype: i16, data: Ptr, max: i32, ind: Ptr) -> i32 {
        if self.stmt_dbc(h).is_none() {
            return rc::INVALID_HANDLE;
        }
        let nts = self.nts();
        let Some(s) = self.handles.stmt(h) else { return rc::INVALID_HANDLE };
        let Some(row) = s.row.clone() else { return self.fail(h, "HY010", "No row is current.") };
        let Some(k) = u16::try_from(icol).ok().filter(|k| *k > 0 && usize::from(*k) <= row.len()) else {
            return self.fail(h, "07009", "The column number is not valid.");
        };
        if s.got.contains(&k) {
            return rc::NO_DATA;
        }
        if s.got.iter().any(|g| *g > k) {
            return self.fail(h, "HY010", "Columns are retrieved in order.");
        }
        s.got.push(k);
        let ctype = if ctype == ty::DEFAULT { default_ctype(s.cols[usize::from(k) - 1].sql_type) } else { ctype };
        if !convert::supported(ctype) {
            return self.note(h, rc::ERROR, convert::unsupported(ctype));
        }
        match put_value(host, &row[usize::from(k) - 1], ctype, max, data, ind, nts) {
            Ok(()) => rc::SUCCESS,
            Err(d) => self.note(h, rc::ERROR, d),
        }
    }

    /// `SQLRowCount`.
    pub fn row_count(&mut self, host: &mut dyn Host, h: i32, out: Ptr) -> i32 {
        if self.stmt_dbc(h).is_none() {
            return rc::INVALID_HANDLE;
        }
        let n = self.handles.stmt(h).map_or(-1, |s| s.row_count);
        put_i32(host, out, n);
        rc::SUCCESS
    }

    /// `SQLCloseCursor`: nothing to do when none is open.
    pub fn close_cursor(&mut self, host: &mut dyn Host, h: i32) -> i32 {
        let Some(dbc) = self.stmt_dbc(h) else { return rc::INVALID_HANDLE };
        self.close(host, h, dbc)
    }

    fn close(&mut self, host: &mut dyn Host, h: i32, dbc: i32) -> i32 {
        let Some(s) = self.handles.stmt(h) else { return rc::INVALID_HANDLE };
        if s.state != StmtState::Open {
            return rc::SUCCESS;
        }
        let cursor = s.cursor.clone();
        s.state = StmtState::Prepared;
        s.row = None;
        let mut d = self.desc(dbc, Op::Close);
        d.name = cursor;
        let (r, _) = self.run(host, dbc, d, Vec::new());
        if r.sqlca.is_error() && r.sqlca.sqlcode != -501 {
            return self.answer(h, &r.sqlca);
        }
        if let Some(ca) = self.autocommit(host, dbc) {
            return self.answer(h, &ca);
        }
        rc::SUCCESS
    }

    /// `SQLFreeStmt`.
    pub fn free_stmt(&mut self, host: &mut dyn Host, h: i32, opt: i16) -> i32 {
        let Some(dbc) = self.stmt_dbc(h) else { return rc::INVALID_HANDLE };
        match opt {
            free::CLOSE => self.close(host, h, dbc),
            free::DROP => {
                let code = self.close(host, h, dbc);
                if self.handles.stmt(h).is_some_and(|s| s.state != StmtState::Allocated) {
                    let mut d = self.desc(dbc, Op::Deallocate);
                    d.name = stmt_name(h);
                    let _ = self.run(host, dbc, d, Vec::new());
                }
                self.handles.remove(h);
                if code == rc::ERROR { rc::SUCCESS_WITH_INFO } else { rc::SUCCESS }
            }
            free::UNBIND => {
                if let Some(s) = self.handles.stmt(h) {
                    s.col_binds.clear();
                }
                rc::SUCCESS
            }
            free::RESET_PARAMS => {
                if let Some(s) = self.handles.stmt(h) {
                    s.param_binds.clear();
                }
                rc::SUCCESS
            }
            _ => self.fail(h, "HY092", "The option is not valid."),
        }
    }

    /// `SQLSetStmtAttr`: integers are passed by address.
    pub fn set_stmt_attr(&mut self, host: &mut dyn Host, h: i32, attr: i32, val: Ptr) -> i32 {
        if self.stmt_dbc(h).is_none() {
            return rc::INVALID_HANDLE;
        }
        if self.handles.stmt(h).is_some_and(|s| s.state == StmtState::Open) {
            return self.fail(h, "HY010", "The statement has a cursor open.");
        }
        let Some(v) = get_i32(host, val) else { return self.fail(h, "HY009", "The attribute's value is missing.") };
        let flag = v == TRUE;
        let ok_flag = v == 0 || v == TRUE;
        let Some(s) = self.handles.stmt(h) else { return rc::INVALID_HANDLE };
        match attr {
            stmt_attr::CURSOR_SCROLLABLE if ok_flag => s.scrollable = flag,
            stmt_attr::CURSOR_HOLD if ok_flag => s.hold = flag,
            stmt_attr::FOR_FETCH_ONLY if ok_flag => s.fetch_only = flag,
            // Forward only (0) or static (1).
            stmt_attr::CURSOR_TYPE if ok_flag => s.scrollable = flag,
            stmt_attr::CURSOR_SCROLLABLE | stmt_attr::CURSOR_HOLD | stmt_attr::FOR_FETCH_ONLY | stmt_attr::CURSOR_TYPE => {
                return self.fail(h, "HY024", "The attribute's value is not valid.");
            }
            _ => return self.fail(h, "HYC00", &format!("Statement attribute {attr} is not supported.")),
        }
        rc::SUCCESS
    }

    /// `SQLGetStmtAttr`.
    pub fn get_stmt_attr(&mut self, host: &mut dyn Host, h: i32, attr: i32, val: Ptr, olen: Ptr) -> i32 {
        if self.stmt_dbc(h).is_none() {
            return rc::INVALID_HANDLE;
        }
        let Some(s) = self.handles.stmt(h) else { return rc::INVALID_HANDLE };
        let flag = |b: bool| if b { TRUE } else { 0 };
        let v = match attr {
            stmt_attr::CURSOR_SCROLLABLE | stmt_attr::CURSOR_TYPE => flag(s.scrollable),
            stmt_attr::CURSOR_HOLD => flag(s.hold),
            stmt_attr::FOR_FETCH_ONLY => flag(s.fetch_only),
            _ => return self.fail(h, "HYC00", &format!("Statement attribute {attr} is not supported.")),
        };
        put_i32(host, val, v);
        put_i32(host, olen, 4);
        rc::SUCCESS
    }
}

/// A value into a buffer and its length word: a null value needs the word.
fn put_value(host: &mut dyn Host, v: &Value, ctype: i16, max: i32, data: Ptr, ind: Ptr, nts: bool) -> Result<(), Diag> {
    let Image { bytes, ind: word } = if v.is_null() {
        if ind.is_none() {
            return Err(Diag { state: "22002".into(), native: -305, text: "A null value has no indicator to go to.".into() });
        }
        Image { bytes: convert::null_image(ctype, max), ind: len::NULL_DATA }
    } else {
        convert::image(v, ctype, max, nts)?
    };
    if let Some(p) = data
        && !host.write(p, &bytes)
    {
        return Err(Diag::cli("HY090", "The buffer is too small for the value."));
    }
    put_i32(host, ind, word);
    Ok(())
}
