// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Allocating and freeing handles, and the environment's attributes.
//! A job has one environment: allocating it again gives the same
//! handle. Its attributes are set before any connection exists.

use super::Cli;
use super::host::{Host, get_i32, get_str, put_i32, put_str};
use crate::consts::{TRUE, env_attr, htype, rc};
use crate::model::handles::{Dbc, Env, Handle, Ptr, Stmt};

impl Cli {
    /// `SQLAllocEnv`.
    pub fn alloc_env(&mut self, host: &mut dyn Host, out: Ptr) -> i32 {
        let n = match self.handles.env_number() {
            Some(n) => {
                if let Some(e) = self.handles.env(n) {
                    e.count += 1;
                }
                n
            }
            None => self.handles.add(Handle::Env(Env::default())),
        };
        put_i32(host, out, n);
        rc::SUCCESS
    }

    /// `SQLAllocConnect`.
    pub fn alloc_connect(&mut self, host: &mut dyn Host, henv: i32, out: Ptr) -> i32 {
        if !self.begin(henv) || !self.handles.is(henv, htype::ENV) {
            put_i32(host, out, 0);
            return rc::INVALID_HANDLE;
        }
        let number = self.handles.peek();
        let Some(env) = self.handles.env(henv) else {
            return rc::INVALID_HANDLE;
        };
        let d = Dbc::new(env, number);
        let n = self.handles.add(Handle::Dbc(d));
        put_i32(host, out, n);
        rc::SUCCESS
    }

    /// `SQLAllocStmt`: only on a connected connection.
    pub fn alloc_stmt(&mut self, host: &mut dyn Host, hdbc: i32, out: Ptr) -> i32 {
        if !self.begin(hdbc) || !self.handles.is(hdbc, htype::DBC) {
            put_i32(host, out, 0);
            return rc::INVALID_HANDLE;
        }
        if !self.handles.dbc(hdbc).is_some_and(|d| d.connected) {
            put_i32(host, out, 0);
            return self.note(
                hdbc,
                rc::ERROR,
                crate::model::diag::Diag {
                    state: "08003".into(),
                    native: -843,
                    text: "The connection is not connected.".into(),
                },
            );
        }
        let n = self.handles.add(Handle::Stmt(Stmt {
            dbc: hdbc,
            row_count: -1,
            ..Stmt::default()
        }));
        put_i32(host, out, n);
        rc::SUCCESS
    }

    /// `SQLAllocHandle`.
    pub fn alloc_handle(&mut self, host: &mut dyn Host, t: i16, input: i32, out: Ptr) -> i32 {
        match t {
            htype::ENV => self.alloc_env(host, out),
            htype::DBC => self.alloc_connect(host, input, out),
            htype::STMT => self.alloc_stmt(host, input, out),
            htype::DESC => {
                if !self.begin(input) {
                    return rc::INVALID_HANDLE;
                }
                self.fail(input, "HYC00", "Descriptor handles are not supported.")
            }
            _ => {
                if self.begin(input) {
                    self.fail(input, "HY092", "The handle type is not valid.")
                } else {
                    rc::INVALID_HANDLE
                }
            }
        }
    }

    /// `SQLFreeEnv`: not while connections exist.
    pub fn free_env(&mut self, henv: i32) -> i32 {
        if !self.begin(henv) || !self.handles.is(henv, htype::ENV) {
            return rc::INVALID_HANDLE;
        }
        if self.handles.dbcs().next().is_some() {
            return self.fail(henv, "HY010", "The environment still has connections.");
        }
        self.handles.remove(henv);
        rc::SUCCESS
    }

    /// `SQLFreeConnect`: not while connected.
    pub fn free_connect(&mut self, hdbc: i32) -> i32 {
        if !self.begin(hdbc) || !self.handles.is(hdbc, htype::DBC) {
            return rc::INVALID_HANDLE;
        }
        if self.handles.dbc(hdbc).is_some_and(|d| d.connected) {
            return self.fail(hdbc, "HY010", "The connection is still connected.");
        }
        self.handles.remove(hdbc);
        rc::SUCCESS
    }

    /// `SQLFreeHandle`.
    pub fn free_handle(&mut self, host: &mut dyn Host, t: i16, h: i32) -> i32 {
        match t {
            htype::ENV => self.free_env(h),
            htype::DBC => self.free_connect(h),
            htype::STMT => self.free_stmt(host, h, crate::consts::free::DROP),
            _ => {
                if self.begin(h) {
                    self.fail(h, "HY092", "The handle type is not valid.")
                } else {
                    rc::INVALID_HANDLE
                }
            }
        }
    }

    /// `SQLSetEnvAttr`: integers are passed by address.
    pub fn set_env_attr(
        &mut self,
        host: &mut dyn Host,
        henv: i32,
        attr: i32,
        val: Ptr,
        len: i32,
    ) -> i32 {
        if !self.begin(henv) || !self.handles.is(henv, htype::ENV) {
            return rc::INVALID_HANDLE;
        }
        if self.handles.dbcs().next().is_some() {
            return self.fail(
                henv,
                "HY010",
                "Environment attributes are set before any connection is allocated.",
            );
        }
        let int = get_i32(host, val);
        let text = get_str(host, val, len);
        let Some(env) = self.handles.env(henv) else {
            return rc::INVALID_HANDLE;
        };
        match attr {
            env_attr::SERVER_MODE | env_attr::OUTPUT_NTS | env_attr::SYS_NAMING => {
                let Some(v) = int else {
                    return self.fail(henv, "HY009", "The attribute's value is missing.");
                };
                let on = v == TRUE;
                if v != TRUE && v != 0 {
                    return self.fail(henv, "HY024", "The attribute's value is not valid.");
                }
                match attr {
                    env_attr::SERVER_MODE => env.server_mode = on,
                    env_attr::OUTPUT_NTS => env.output_nts = on,
                    _ => env.sys_naming = on,
                }
                rc::SUCCESS
            }
            env_attr::DEFAULT_LIB => {
                env.default_lib = text.unwrap_or_default().trim().to_ascii_uppercase();
                rc::SUCCESS
            }
            _ => self.fail(
                henv,
                "HYC00",
                &format!("Environment attribute {attr} is not supported."),
            ),
        }
    }

    /// `SQLGetEnvAttr`.
    pub fn get_env_attr(
        &mut self,
        host: &mut dyn Host,
        henv: i32,
        attr: i32,
        val: Ptr,
        max: i32,
        olen: Ptr,
    ) -> i32 {
        if !self.begin(henv) || !self.handles.is(henv, htype::ENV) {
            return rc::INVALID_HANDLE;
        }
        let Some(env) = self.handles.env(henv) else {
            return rc::INVALID_HANDLE;
        };
        let flag = |b: bool| if b { TRUE } else { 0 };
        let v = match attr {
            env_attr::SERVER_MODE => flag(env.server_mode),
            env_attr::OUTPUT_NTS => flag(env.output_nts),
            env_attr::SYS_NAMING => flag(env.sys_naming),
            env_attr::ENVHNDL_COUNTER => env.count,
            env_attr::DEFAULT_LIB => {
                let lib = env.default_lib.clone();
                let (n, cut) = put_str(host, val, max, &lib);
                put_i32(host, olen, i32::try_from(n).unwrap_or(0));
                return if cut {
                    self.note(
                        henv,
                        rc::SUCCESS_WITH_INFO,
                        crate::model::diag::Diag::cli("01004", "The value was cut short."),
                    )
                } else {
                    rc::SUCCESS
                };
            }
            _ => {
                return self.fail(
                    henv,
                    "HYC00",
                    &format!("Environment attribute {attr} is not supported."),
                );
            }
        };
        put_i32(host, val, v);
        put_i32(host, olen, 4);
        rc::SUCCESS
    }
}
