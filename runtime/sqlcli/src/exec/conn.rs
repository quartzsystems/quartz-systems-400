// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Connections: `SQLConnect` to the local database (`*LOCAL`, the
//! system's own name, or none), in the job or, in server mode, in a
//! server job; their attributes; ending units of work. A connection
//! starts without autocommit at `SQL_TXN_READ_UNCOMMITTED` (`*CHG`)
//! with `*SQL` naming, as the oracle's do.

use sql::embed::Op;

use super::Cli;
use super::host::{Host, get_i32, get_str, put_i32, put_str};
use crate::consts::{TRUE, conn_attr, htype, rc, tran, txn};
use crate::model::diag::Diag;
use crate::model::handles::{Ptr, StmtState};
use crate::server::{self, ConnectReq};

impl Cli {
    /// `SQLConnect`.
    #[allow(clippy::too_many_arguments)]
    pub fn connect(&mut self, host: &mut dyn Host, hdbc: i32, dsn: Ptr, dsnl: i16, uid: Ptr, uidl: i16, pwd: Ptr, pwdl: i16) -> i32 {
        if !self.begin(hdbc) || !self.handles.is(hdbc, htype::DBC) {
            return rc::INVALID_HANDLE;
        }
        if self.handles.dbc(hdbc).is_some_and(|d| d.connected) {
            return self.note(hdbc, rc::ERROR, Diag { state: "08002".into(), native: -842, text: "The connection is already connected.".into() });
        }
        let name = get_str(host, dsn, i32::from(dsnl)).unwrap_or_default().trim().to_ascii_uppercase();
        let user = get_str(host, uid, i32::from(uidl)).map(|u| u.trim().to_ascii_uppercase()).filter(|u| !u.is_empty());
        let password = get_str(host, pwd, i32::from(pwdl)).map(|p| p.trim_end().to_string());
        let server_mode = self.handles.env_number().and_then(|e| self.handles.env(e)).is_some_and(|e| e.server_mode);
        if server_mode {
            let req = ConnectReq { user: user.clone(), password };
            match server::connect(&req) {
                Ok(r) => {
                    if let Some(d) = self.handles.dbc(hdbc) {
                        d.server = Some(r);
                    }
                }
                Err(d) => return self.note(hdbc, rc::ERROR, d),
            }
        }
        if !name.is_empty() && name != "*LOCAL" && name != self.register(host, hdbc, "CURRENT SERVER") {
            // BLOCKED(rel): a connection to another relational database
            // needs the DRDA requester (handoff Section 6).
            self.close_server(hdbc);
            let text = format!("Relational database {name} is not this system's; connections to other databases are not supported.");
            return self.note(hdbc, rc::ERROR, Diag { state: "42705".into(), native: -950, text });
        }
        // Outside server mode a connection is the job's own user's.
        if !server_mode
            && let Some(u) = user
            && u != self.register(host, hdbc, "USER")
        {
            let text = format!("Outside server mode a connection runs as the job's user, not {u}.");
            return self.note(hdbc, rc::ERROR, Diag { state: "42977".into(), native: -7022, text });
        }
        if let Some(d) = self.handles.dbc(hdbc) {
            d.connected = true;
        }
        let lib = self.handles.dbc(hdbc).map(|d| d.default_lib.clone()).unwrap_or_default();
        if !lib.is_empty()
            && let Some(code) = self.set_schema(host, hdbc, &lib)
        {
            return code;
        }
        rc::SUCCESS
    }

    /// A special register's value (`CURRENT SERVER`, `USER`), upper case.
    fn register(&mut self, host: &mut dyn Host, hdbc: i32, reg: &str) -> String {
        let mut d = self.desc(hdbc, Op::Query);
        d.commit = None;
        d.text = format!("SELECT {reg} FROM SYSIBM.SYSDUMMY1");
        let (r, _) = self.run(host, hdbc, d, Vec::new());
        r.rows.first().and_then(|row| row.first()).map(|v| v.as_text().trim().to_ascii_uppercase()).unwrap_or_default()
    }

    /// The connection's default library, by `SET SCHEMA`.
    fn set_schema(&mut self, host: &mut dyn Host, hdbc: i32, lib: &str) -> Option<i32> {
        let mut d = self.desc(hdbc, Op::Exec);
        d.commit = None;
        d.text = format!("SET SCHEMA {lib}");
        let (r, _) = self.run(host, hdbc, d, Vec::new());
        r.sqlca.is_error().then(|| self.answer(hdbc, &r.sqlca))
    }

    fn close_server(&mut self, hdbc: i32) {
        if let Some(mut r) = self.handles.dbc(hdbc).and_then(|d| d.server.take()) {
            r.close();
        }
    }

    /// `SQLDisconnect`: its statements are freed with it.
    pub fn disconnect(&mut self, host: &mut dyn Host, hdbc: i32) -> i32 {
        if !self.begin(hdbc) || !self.handles.is(hdbc, htype::DBC) {
            return rc::INVALID_HANDLE;
        }
        if !self.handles.dbc(hdbc).is_some_and(|d| d.connected) {
            return self.note(hdbc, rc::ERROR, Diag { state: "08003".into(), native: -843, text: "The connection is not connected.".into() });
        }
        for s in self.handles.stmts_of(hdbc) {
            self.handles.remove(s);
        }
        let d = self.desc(hdbc, Op::End);
        let _ = self.run(host, hdbc, d, Vec::new());
        self.close_server(hdbc);
        if let Some(d) = self.handles.dbc(hdbc) {
            d.connected = false;
        }
        rc::SUCCESS
    }

    /// `SQLSetConnectAttr` (and `SQLSetConnectOption`): integers are
    /// passed by address, a library as characters.
    pub fn set_connect_attr(&mut self, host: &mut dyn Host, hdbc: i32, attr: i32, val: Ptr, len: i32) -> i32 {
        if !self.begin(hdbc) || !self.handles.is(hdbc, htype::DBC) {
            return rc::INVALID_HANDLE;
        }
        let int = get_i32(host, val);
        match attr {
            conn_attr::AUTOCOMMIT | conn_attr::DBC_SYS_NAMING => {
                let Some(v) = int.filter(|v| *v == 0 || *v == TRUE) else {
                    return self.fail(hdbc, "HY024", "The attribute's value is not valid.");
                };
                if let Some(d) = self.handles.dbc(hdbc) {
                    if attr == conn_attr::AUTOCOMMIT {
                        d.autocommit = v == TRUE;
                    } else {
                        d.sys_naming = v == TRUE;
                    }
                }
                rc::SUCCESS
            }
            conn_attr::COMMIT => {
                let Some(v) = int.filter(|v| (txn::NO_COMMIT..=txn::SERIALIZABLE).contains(v)) else {
                    return self.fail(hdbc, "HY024", "The commitment control level is not valid.");
                };
                if let Some(d) = self.handles.dbc(hdbc) {
                    d.commit = v;
                }
                rc::SUCCESS
            }
            conn_attr::DBC_DEFAULT_LIB => {
                let lib = get_str(host, val, len).unwrap_or_default().trim().to_ascii_uppercase();
                let connected = self.handles.dbc(hdbc).is_some_and(|d| d.connected);
                if connected
                    && !lib.is_empty()
                    && let Some(code) = self.set_schema(host, hdbc, &lib)
                {
                    return code;
                }
                if let Some(d) = self.handles.dbc(hdbc) {
                    d.default_lib = lib;
                }
                rc::SUCCESS
            }
            _ => self.fail(hdbc, "HYC00", &format!("Connection attribute {attr} is not supported.")),
        }
    }

    /// `SQLGetConnectAttr`.
    pub fn get_connect_attr(&mut self, host: &mut dyn Host, hdbc: i32, attr: i32, val: Ptr, max: i32, olen: Ptr) -> i32 {
        if !self.begin(hdbc) || !self.handles.is(hdbc, htype::DBC) {
            return rc::INVALID_HANDLE;
        }
        let Some(d) = self.handles.dbc(hdbc) else { return rc::INVALID_HANDLE };
        let flag = |b: bool| if b { TRUE } else { 0 };
        let v = match attr {
            conn_attr::AUTOCOMMIT => flag(d.autocommit),
            conn_attr::COMMIT => d.commit,
            conn_attr::DBC_SYS_NAMING => flag(d.sys_naming),
            conn_attr::DBC_DEFAULT_LIB => {
                let lib = d.default_lib.clone();
                let (n, cut) = put_str(host, val, max, &lib);
                put_i32(host, olen, i32::try_from(n).unwrap_or(0));
                return if cut { self.note(hdbc, rc::SUCCESS_WITH_INFO, Diag::cli("01004", "The value was cut short.")) } else { rc::SUCCESS };
            }
            _ => return self.fail(hdbc, "HYC00", &format!("Connection attribute {attr} is not supported.")),
        };
        put_i32(host, val, v);
        put_i32(host, olen, 4);
        rc::SUCCESS
    }

    /// `SQLEndTran`: one connection, or each of the environment's.
    pub fn end_tran(&mut self, host: &mut dyn Host, t: i16, h: i32, kind: i16) -> i32 {
        if !self.begin(h) {
            return rc::INVALID_HANDLE;
        }
        let dbcs: Vec<i32> = match t {
            htype::ENV if self.handles.is(h, htype::ENV) => self.handles.dbcs().collect(),
            htype::DBC if self.handles.is(h, htype::DBC) => vec![h],
            _ => return rc::INVALID_HANDLE,
        };
        let text = match kind {
            tran::COMMIT => "COMMIT",
            tran::ROLLBACK => "ROLLBACK",
            tran::COMMIT_HOLD => "COMMIT HOLD",
            tran::ROLLBACK_HOLD => "ROLLBACK HOLD",
            _ => return self.fail(h, "HY012", "The completion type is not valid."),
        };
        for dbc in dbcs {
            if !self.handles.dbc(dbc).is_some_and(|d| d.connected) {
                continue;
            }
            let mut d = self.desc(dbc, Op::Exec);
            d.text = text.into();
            let (r, _) = self.run(host, dbc, d, Vec::new());
            if r.sqlca.is_error() && r.sqlca.sqlcode != -7007 {
                return self.answer(h, &r.sqlca);
            }
            if matches!(kind, tran::COMMIT | tran::ROLLBACK) {
                for s in self.handles.stmts_of(dbc) {
                    if let Some(st) = self.handles.stmt(s)
                        && st.state == StmtState::Open
                        && !st.hold
                    {
                        st.state = StmtState::Prepared;
                        st.row = None;
                    }
                }
            }
        }
        rc::SUCCESS
    }

    /// `SQLTransact`: the connection, or with none every connection.
    pub fn transact(&mut self, host: &mut dyn Host, henv: i32, hdbc: i32, kind: i16) -> i32 {
        if hdbc != 0 { self.end_tran(host, htype::DBC, hdbc, kind) } else { self.end_tran(host, htype::ENV, henv, kind) }
    }
}
