// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One `sysv64` entry per exported function. A pointer that would
//! start in `r9` takes an `i64` pad before it (`ABI.md` §1).

use super::{int, ptr, short, with};
use crate::consts::param;

/// An entry: the job's CLI and storage, then `$body`.
macro_rules! entry {
    ($name:ident($($a:ident: $t:ty),*) => |$c:ident, $h:ident| $body:expr) => {
        #[allow(clippy::too_many_arguments, clippy::used_underscore_binding)]
        extern "sysv64" fn $name(env: *mut codegen::abi::Env, _argc: i64, _od: i64, $($a: $t),*) -> i64 {
            let job = mi::storage::job_of(env);
            i64::from(with(job, |$c, $h| $body))
        }
    };
}

entry!(alloc_env(p: u128) => |c, h| c.alloc_env(h, ptr(p)));
entry!(alloc_connect(env: i64, p: u128) => |c, h| c.alloc_connect(h, int(env), ptr(p)));
entry!(alloc_stmt(dbc: i64, p: u128) => |c, h| c.alloc_stmt(h, int(dbc), ptr(p)));
entry!(alloc_handle(t: i64, i: i64, _pad: i64, p: u128) => |c, h| c.alloc_handle(h, short(t), int(i), ptr(p)));
entry!(free_env(e: i64) => |c, _h| c.free_env(int(e)));
entry!(free_connect(d: i64) => |c, _h| c.free_connect(int(d)));
entry!(free_stmt(s: i64, opt: i64) => |c, h| c.free_stmt(h, int(s), short(opt)));
entry!(free_handle(t: i64, x: i64) => |c, h| c.free_handle(h, short(t), int(x)));
entry!(set_env_attr(e: i64, a: i64, _pad: i64, v: u128, n: i64) => |c, h| c.set_env_attr(h, int(e), int(a), ptr(v), int(n)));
entry!(get_env_attr(e: i64, a: i64, _pad: i64, v: u128, m: i64, o: u128) => |c, h| c.get_env_attr(h, int(e), int(a), ptr(v), int(m), ptr(o)));
entry!(set_connect_attr(d: i64, a: i64, _pad: i64, v: u128, n: i64) => |c, h| c.set_connect_attr(h, int(d), int(a), ptr(v), int(n)));
entry!(get_connect_attr(d: i64, a: i64, _pad: i64, v: u128, m: i64, o: u128) => |c, h| c.get_connect_attr(h, int(d), int(a), ptr(v), int(m), ptr(o)));
entry!(set_connect_option(d: i64, a: i64, _pad: i64, v: u128) => |c, h| c.set_connect_attr(h, int(d), i32::from(short(a)), ptr(v), crate::consts::len::NTS));
entry!(set_stmt_attr(s: i64, a: i64, _pad: i64, v: u128, _n: i64) => |c, h| c.set_stmt_attr(h, int(s), int(a), ptr(v)));
entry!(get_stmt_attr(s: i64, a: i64, _pad: i64, v: u128, _m: i64, o: u128) => |c, h| c.get_stmt_attr(h, int(s), int(a), ptr(v), ptr(o)));
entry!(connect(d: i64, dsn: u128, dl: i64, uid: u128, ul: i64, pwd: u128, pl: i64) => |c, h| c.connect(h, int(d), ptr(dsn), short(dl), ptr(uid), short(ul), ptr(pwd), short(pl)));
entry!(disconnect(d: i64) => |c, h| c.disconnect(h, int(d)));
entry!(prepare(s: i64, t: u128, n: i64) => |c, h| c.prepare(h, int(s), ptr(t), int(n)));
entry!(execute(s: i64) => |c, h| c.execute(h, int(s)));
entry!(exec_direct(s: i64, t: u128, n: i64) => |c, h| c.exec_direct(h, int(s), ptr(t), int(n)));
entry!(num_params(s: i64, p: u128) => |c, h| c.num_params(h, int(s), ptr(p)));
entry!(num_result_cols(s: i64, p: u128) => |c, h| c.num_result_cols(h, int(s), ptr(p)));
entry!(describe_param(s: i64, i: i64, _pad: i64, t: u128, d: u128, sc: u128, nl: u128) => |c, h| c.describe_param(h, int(s), short(i), ptr(t), ptr(d), ptr(sc), ptr(nl)));
entry!(bind_param(s: i64, i: i64, ct: i64, st: i64, l: i64, sc: i64, d: u128, ind: u128) => |c, _h| c.bind_param(int(s), short(i), param::INPUT, short(ct), short(st), int(l), short(sc), ptr(d), 0, ptr(ind)));
entry!(bind_parameter(s: i64, i: i64, io: i64, ct: i64, st: i64, l: i64, sc: i64, d: u128, m: i64, ind: u128) => |c, _h| c.bind_param(int(s), short(i), short(io), short(ct), short(st), int(l), short(sc), ptr(d), int(m), ptr(ind)));
entry!(describe_col(s: i64, i: i64, _pad: i64, n: u128, nm: i64, nl: u128, t: u128, d: u128, sc: u128, nu: u128) => |c, h| c.describe_col(h, int(s), short(i), ptr(n), short(nm), ptr(nl), ptr(t), ptr(d), ptr(sc), ptr(nu)));
entry!(bind_col(s: i64, i: i64, ct: i64, d: u128, m: i64, ind: u128) => |c, _h| c.bind_col(int(s), short(i), short(ct), ptr(d), int(m), ptr(ind)));
entry!(fetch(s: i64) => |c, h| c.fetch(h, int(s)));
entry!(fetch_scroll(s: i64, o: i64, off: i64) => |c, h| c.fetch_scroll(h, int(s), short(o), int(off)));
entry!(get_data(s: i64, i: i64, ct: i64, d: u128, m: i64, ind: u128) => |c, h| c.get_data(h, int(s), short(i), short(ct), ptr(d), int(m), ptr(ind)));
entry!(row_count(s: i64, p: u128) => |c, h| c.row_count(h, int(s), ptr(p)));
entry!(close_cursor(s: i64) => |c, h| c.close_cursor(h, int(s)));
entry!(error(e: i64, d: i64, s: i64, st: u128, nat: u128, m: u128, mx: i64, ml: u128) => |c, h| c.error(h, int(e), int(d), int(s), ptr(st), ptr(nat), ptr(m), short(mx), ptr(ml)));
entry!(get_diag_rec(t: i64, x: i64, r: i64, st: u128, nat: u128, m: u128, mx: i64, ml: u128) => |c, h| c.get_diag_rec(h, short(t), int(x), short(r), ptr(st), ptr(nat), ptr(m), short(mx), ptr(ml)));
entry!(get_diag_field(t: i64, x: i64, r: i64, id: i64, v: u128, mx: i64, l: u128) => |c, h| c.get_diag_field(h, short(t), int(x), short(r), short(id), ptr(v), short(mx), ptr(l)));
entry!(end_tran(t: i64, x: i64, k: i64) => |c, h| c.end_tran(h, short(t), int(x), short(k)));
entry!(transact(e: i64, d: i64, k: i64) => |c, h| c.transact(h, int(e), int(d), short(k)));
entry!(tables(s: i64, ca: u128, cal: i64, sc: u128, scl: i64, t: u128, tl: i64, ty: u128, tyl: i64) => |c, h| c.tables(h, int(s), ptr(ca), short(cal), ptr(sc), short(scl), ptr(t), short(tl), ptr(ty), short(tyl)));
entry!(columns(s: i64, ca: u128, cal: i64, sc: u128, scl: i64, t: u128, tl: i64, co: u128, col: i64) => |c, h| c.columns(h, int(s), ptr(ca), short(cal), ptr(sc), short(scl), ptr(t), short(tl), ptr(co), short(col)));
entry!(primary_keys(s: i64, ca: u128, cal: i64, sc: u128, scl: i64, t: u128, tl: i64) => |c, h| c.primary_keys(h, int(s), ptr(ca), short(cal), ptr(sc), short(scl), ptr(t), short(tl)));

/// Every exported function with its entry's address, by IBM's name.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    vec![
        ("SQLAllocEnv", alloc_env as *const () as usize),
        ("SQLAllocConnect", alloc_connect as *const () as usize),
        ("SQLAllocStmt", alloc_stmt as *const () as usize),
        ("SQLAllocHandle", alloc_handle as *const () as usize),
        ("SQLFreeEnv", free_env as *const () as usize),
        ("SQLFreeConnect", free_connect as *const () as usize),
        ("SQLFreeStmt", free_stmt as *const () as usize),
        ("SQLFreeHandle", free_handle as *const () as usize),
        ("SQLSetEnvAttr", set_env_attr as *const () as usize),
        ("SQLGetEnvAttr", get_env_attr as *const () as usize),
        ("SQLSetConnectAttr", set_connect_attr as *const () as usize),
        ("SQLGetConnectAttr", get_connect_attr as *const () as usize),
        ("SQLSetConnectOption", set_connect_option as *const () as usize),
        ("SQLSetStmtAttr", set_stmt_attr as *const () as usize),
        ("SQLGetStmtAttr", get_stmt_attr as *const () as usize),
        ("SQLConnect", connect as *const () as usize),
        ("SQLDisconnect", disconnect as *const () as usize),
        ("SQLPrepare", prepare as *const () as usize),
        ("SQLExecute", execute as *const () as usize),
        ("SQLExecDirect", exec_direct as *const () as usize),
        ("SQLNumParams", num_params as *const () as usize),
        ("SQLNumResultCols", num_result_cols as *const () as usize),
        ("SQLDescribeParam", describe_param as *const () as usize),
        ("SQLBindParam", bind_param as *const () as usize),
        ("SQLBindParameter", bind_parameter as *const () as usize),
        ("SQLDescribeCol", describe_col as *const () as usize),
        ("SQLBindCol", bind_col as *const () as usize),
        ("SQLFetch", fetch as *const () as usize),
        ("SQLFetchScroll", fetch_scroll as *const () as usize),
        ("SQLGetData", get_data as *const () as usize),
        ("SQLRowCount", row_count as *const () as usize),
        ("SQLCloseCursor", close_cursor as *const () as usize),
        ("SQLError", error as *const () as usize),
        ("SQLGetDiagRec", get_diag_rec as *const () as usize),
        ("SQLGetDiagField", get_diag_field as *const () as usize),
        ("SQLEndTran", end_tran as *const () as usize),
        ("SQLTransact", transact as *const () as usize),
        ("SQLTables", tables as *const () as usize),
        ("SQLColumns", columns as *const () as usize),
        ("SQLPrimaryKeys", primary_keys as *const () as usize),
    ]
}
