// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bound procedures: arguments as C passes them (`net::abi`),
//! strings in the job's CCSID (37), integers big-endian in storage.

use std::cell::RefCell;
use std::collections::HashMap;

use mi::{FatPtr, Job};
use sockets::Errno;

use super::{SetMode, SpawnReq, Var, procs, split, system};
use crate::net::Stable;
use crate::net::abi::{bound_c, cstr, fat, get_int, int, is_null, mem, put_int, raw};

/// Longest name or `NAME=value` string read.
const MAX: usize = 64 * 1024;

thread_local! {
    /// The strings `getenv` returned: kept while the job runs, as C
    /// callers may hold them.
    static VALUES: RefCell<HashMap<String, Stable>> = RefCell::new(HashMap::new());
}

fn null() -> u128 {
    raw(FatPtr::null())
}

fn set_errno(j: &mut Job, e: Errno) -> i64 {
    j.errno.set(e.code());
    -1
}

/// A value as job storage, NUL-terminated EBCDIC.
fn value_ptr(j: &mut Job, name: &str, value: &str) -> u128 {
    let mut b = ccsid::to_ebcdic(value).unwrap_or_default();
    b.push(0);
    let key = format!("{name}={value}");
    if let Some(p) = VALUES.with(|m| m.borrow().get(&key).map(Stable::ptr)) {
        return raw(p);
    }
    let mut st = Stable::new(j, b.len());
    st.bytes().copy_from_slice(&b);
    let p = st.ptr();
    VALUES.with(|m| m.borrow_mut().insert(key, st));
    raw(p)
}

fn getenv(j: &mut Job, name: u128) -> u128 {
    let Ok(n) = cstr(j, name, MAX) else {
        set_errno(j, Errno::EFAULT);
        return null();
    };
    match super::job_var(&n) {
        Some(v) => value_ptr(j, &n, v.value.as_deref().unwrap_or("")),
        None => null(),
    }
}

fn qp0zgetenv(j: &mut Job, name: u128, ccsid: u128) -> u128 {
    let Ok(n) = cstr(j, name, MAX) else {
        set_errno(j, Errno::EFAULT);
        return null();
    };
    match super::job_var(&n) {
        Some(v) => {
            if !is_null(ccsid) {
                let _ = put_int(j, ccsid, v.ccsid);
            }
            value_ptr(j, &n, v.value.as_deref().unwrap_or(""))
        }
        None => null(),
    }
}

fn put(j: &mut Job, s: u128, ccsid: i32) -> i64 {
    let Ok(text) = cstr(j, s, MAX) else {
        return set_errno(j, Errno::EFAULT);
    };
    let (name, value) = split(&text);
    if name.is_empty() {
        return set_errno(j, Errno::EINVAL);
    }
    match super::set_job(Var { name, value: Some(value.unwrap_or_default()), ccsid }, SetMode::Put) {
        Ok(()) => 0,
        Err(_) => set_errno(j, Errno::ENOMEM),
    }
}

fn putenv(j: &mut Job, s: u128) -> i64 {
    put(j, s, 0)
}

fn qp0zputenv(j: &mut Job, s: u128, ccsid: i64) -> i64 {
    put(j, s, int(ccsid))
}

fn qp0zdltenv(j: &mut Job, name: u128) -> i64 {
    let Ok(n) = cstr(j, name, MAX) else {
        return set_errno(j, Errno::EFAULT);
    };
    match super::remove_job(&n) {
        Ok(()) => 0,
        Err(_) => set_errno(j, Errno::EINVAL),
    }
}

fn code(e: Errno) -> i64 {
    i64::from(e.code())
}

/// `Qp0zGetAllSysEnv(list, *listSize, ccsids, *ccsidSize, reserved)`:
/// the variables as `NAME=value` strings back to back, each ended by a
/// NUL, and an `int` CCSID for each; `ENOSPC` with the sizes needed
/// when a buffer is short. Returns 0 or the `errno`.
fn qp0zgetallsysenv(j: &mut Job, list: u128, list_size: u128, ccsids: u128, ccsid_size: u128, _r: u128) -> i64 {
    let Some(sys) = system() else {
        return code(Errno::ENOSYS);
    };
    let Ok(vars) = sys.list() else {
        return code(Errno::EPERM);
    };
    let mut text = Vec::new();
    let mut ids = Vec::new();
    for v in &vars {
        let s = match &v.value {
            Some(x) => format!("{}={x}", v.name),
            None => v.name.clone(),
        };
        text.extend(ccsid::to_ebcdic(&s).unwrap_or_default());
        text.push(0);
        ids.extend_from_slice(&v.ccsid.to_be_bytes());
    }
    let (Ok(have), Ok(have_ids)) = (get_int(j, list_size), get_int(j, ccsid_size)) else {
        return code(Errno::EFAULT);
    };
    let need = i32::try_from(text.len()).unwrap_or(i32::MAX);
    let need_ids = i32::try_from(ids.len()).unwrap_or(i32::MAX);
    if have < need || have_ids < need_ids {
        let _ = put_int(j, list_size, need);
        let _ = put_int(j, ccsid_size, need_ids);
        return code(Errno::ENOSPC);
    }
    if !text.is_empty() {
        match mem(j, list, text.len()) {
            Ok(b) => b.copy_from_slice(&text),
            Err(e) => return code(e),
        }
    }
    if !ids.is_empty() {
        match mem(j, ccsids, ids.len()) {
            Ok(b) => b.copy_from_slice(&ids),
            Err(e) => return code(e),
        }
    }
    let _ = put_int(j, list_size, need);
    let _ = put_int(j, ccsid_size, need_ids);
    0
}

/// `Qp0zGetSysEnv(name, value, *size, *ccsid, reserved)`: 0 or the
/// `errno` (`ENOENT`, `ENOSPC` with the size needed).
fn qp0zgetsysenv(j: &mut Job, name: u128, value: u128, size: u128, ccsid: u128, _r: u128) -> i64 {
    let Ok(n) = cstr(j, name, MAX) else {
        return code(Errno::EFAULT);
    };
    let Some(sys) = system() else {
        return code(Errno::ENOSYS);
    };
    let Some(v) = sys.list().ok().and_then(|l| l.into_iter().find(|v| v.name == n)) else {
        return code(Errno::ENOENT);
    };
    let mut b = ccsid::to_ebcdic(v.value.as_deref().unwrap_or("")).unwrap_or_default();
    b.push(0);
    let Ok(have) = get_int(j, size) else {
        return code(Errno::EFAULT);
    };
    let need = i32::try_from(b.len()).unwrap_or(i32::MAX);
    let _ = put_int(j, size, need);
    if have < need {
        return code(Errno::ENOSPC);
    }
    match mem(j, value, b.len()) {
        Ok(m) => m.copy_from_slice(&b),
        Err(e) => return code(e),
    }
    if !is_null(ccsid) {
        let _ = put_int(j, ccsid, v.ccsid);
    }
    0
}

fn sys_result(r: Result<(), super::EnvFail>) -> i64 {
    match r {
        Ok(()) => 0,
        Err(e) if e.msgid == "CPFA98E" => code(Errno::EPERM),
        Err(e) if e.msgid == "CPFA981" => code(Errno::ENOENT),
        Err(_) => code(Errno::EINVAL),
    }
}

/// `Qp0zPutSysEnv(string, ccsid, reserved)`: 0 or the `errno`.
fn qp0zputsysenv(j: &mut Job, s: u128, ccsid: i64, _r: u128) -> i64 {
    let Ok(text) = cstr(j, s, MAX) else {
        return code(Errno::EFAULT);
    };
    let (name, value) = split(&text);
    let Some(sys) = system() else {
        return code(Errno::ENOSYS);
    };
    sys_result(sys.set(
        Var {
            name,
            value: Some(value.unwrap_or_default()),
            ccsid: int(ccsid),
        },
        SetMode::Put,
    ))
}

/// `Qp0zDltSysEnv(name, reserved)`: 0 or the `errno`.
fn qp0zdltsysenv(j: &mut Job, name: u128, _r: u128) -> i64 {
    let Ok(n) = cstr(j, name, MAX) else {
        return code(Errno::EFAULT);
    };
    let Some(sys) = system() else {
        return code(Errno::ENOSYS);
    };
    sys_result(sys.remove(&n))
}

/// A NULL-terminated array of string pointers.
fn strings(j: &mut Job, arr: u128) -> Result<Vec<String>, Errno> {
    let mut out = Vec::new();
    if is_null(arr) {
        return Ok(out);
    }
    for i in 0..ARG_MAX {
        let at = j
            .spaces
            .add(fat(arr), i64::try_from(i * 16).unwrap_or(0))
            .map_err(|_| Errno::EFAULT)?;
        let b = mem(j, raw(at), 16)?;
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(b);
        let p = FatPtr::from_bytes(&bytes);
        if p.is_null() {
            return Ok(out);
        }
        out.push(cstr(j, raw(p), MAX)?);
    }
    Err(Errno::E2BIG)
}

/// `SPAWN_MAX_NUM_ARGS`.
pub(crate) const ARG_MAX: usize = 255;

#[allow(clippy::too_many_arguments)]
fn spawn_any(
    j: &mut Job,
    path: u128,
    fd_count: i64,
    fd_map: u128,
    inherit: u128,
    argv: u128,
    envp: u128,
    search: bool,
) -> i64 {
    let Some(p) = procs() else {
        return set_errno(j, Errno::ENOSYS);
    };
    let Ok(path) = cstr(j, path, MAX) else {
        return set_errno(j, Errno::EFAULT);
    };
    let n = usize::try_from(int(fd_count)).unwrap_or(0).min(1024);
    let mut map = Vec::with_capacity(n);
    if n > 0 {
        match mem(j, fd_map, n * 4) {
            Ok(b) => {
                for c in b.chunks(4) {
                    map.push(i32::from_be_bytes([c[0], c[1], c[2], c[3]]));
                }
            }
            Err(e) => return set_errno(j, e),
        }
    }
    let flags = if is_null(inherit) {
        0
    } else {
        get_int(j, inherit).map_or(0, i32::cast_unsigned)
    };
    let argv = match strings(j, argv) {
        Ok(v) => v,
        Err(e) => return set_errno(j, e),
    };
    let envp = if is_null(envp) {
        None
    } else {
        match strings(j, envp) {
            Ok(v) => Some(v),
            Err(e) => return set_errno(j, e),
        }
    };
    let req = SpawnReq {
        path,
        search,
        argv,
        envp,
        fd_map: map,
        flags,
    };
    match p.spawn(req) {
        Ok(pid) => i64::from(pid),
        Err(e) => {
            j.errno.set(e);
            -1
        }
    }
}

fn spawn(j: &mut Job, path: u128, n: i64, map: u128, inh: u128, argv: u128, envp: u128) -> i64 {
    spawn_any(j, path, n, map, inh, argv, envp, false)
}

fn spawnp(j: &mut Job, path: u128, n: i64, map: u128, inh: u128, argv: u128, envp: u128) -> i64 {
    spawn_any(j, path, n, map, inh, argv, envp, true)
}

/// `WNOHANG`.
const WNOHANG: i32 = 1;

fn waitpid(j: &mut Job, pid: i64, stat: u128, options: i64) -> i64 {
    let Some(p) = procs() else {
        return set_errno(j, Errno::ECHILD);
    };
    match p.wait(int(pid), int(options) & WNOHANG != 0) {
        Ok(Some((pid, status))) => {
            if !is_null(stat) {
                let _ = put_int(j, stat, status);
            }
            i64::from(pid)
        }
        Ok(None) => 0,
        Err(e) => {
            j.errno.set(e);
            -1
        }
    }
}

fn wait(j: &mut Job, stat: u128) -> i64 {
    waitpid(j, -1, stat, 0)
}

fn kill(j: &mut Job, pid: i64, sig: i64) -> i64 {
    let Some(p) = procs() else {
        return set_errno(j, Errno::ESRCH);
    };
    match p.kill(int(pid), int(sig)) {
        Ok(()) => 0,
        Err(e) => {
            j.errno.set(e);
            -1
        }
    }
}

fn getpid(_j: &mut Job) -> i64 {
    procs().map_or_else(|| i64::from(std::process::id()), |p| i64::from(p.ids().0))
}

fn getppid(_j: &mut Job) -> i64 {
    procs().map_or(1, |p| i64::from(p.ids().1))
}

/// `QP2RUNPASE_ERROR`.
const RUNPASE_ERROR: i64 = -1;

/// `Qp2RunPase(path, symbol, symbolData, symbolDataLen, ccsid, argv,
/// envp)`: the program's exit status, or -1 with `errno` set. The
/// symbol is not primed (a PASE runtime would).
#[allow(clippy::too_many_arguments)]
fn qp2runpase(j: &mut Job, path: u128, _sym: u128, _data: u128, _len: i64, _ccsid: i64, argv: u128, envp: u128) -> i64 {
    let Some(p) = procs() else {
        set_errno(j, Errno::ENOSYS);
        return RUNPASE_ERROR;
    };
    let Ok(path) = cstr(j, path, MAX) else {
        set_errno(j, Errno::EFAULT);
        return RUNPASE_ERROR;
    };
    let (argv, envp) = match (strings(j, argv), strings(j, envp)) {
        (Ok(a), Ok(e)) => (a, e),
        (Err(e), _) | (_, Err(e)) => {
            set_errno(j, e);
            return RUNPASE_ERROR;
        }
    };
    match p.run_pase(&path, argv, envp) {
        Ok(code) => i64::from(code),
        Err(e) => {
            j.errno.set(e);
            RUNPASE_ERROR
        }
    }
}

bound_c!(b_qp2runpase(p: u128, s: u128, d: u128, l: i64, c: i64, a: u128, e: u128) -> i64 = qp2runpase);
bound_c!(b_getenv(n: u128) -> u128 = getenv);
bound_c!(b_putenv(s: u128) -> i64 = putenv);
bound_c!(b_qp0zgetenv(n: u128, c: u128) -> u128 = qp0zgetenv);
bound_c!(b_qp0zputenv(s: u128, c: i64) -> i64 = qp0zputenv);
bound_c!(b_qp0zdltenv(n: u128) -> i64 = qp0zdltenv);
bound_c!(b_qp0zgetallsysenv(l: u128, ls: u128, c: u128, cs: u128, r: u128) -> i64 = qp0zgetallsysenv);
bound_c!(b_qp0zgetsysenv(n: u128, v: u128, s: u128, c: u128, r: u128) -> i64 = qp0zgetsysenv);
bound_c!(b_qp0zputsysenv(s: u128, c: i64, r: u128) -> i64 = qp0zputsysenv);
bound_c!(b_qp0zdltsysenv(n: u128, r: u128) -> i64 = qp0zdltsysenv);
bound_c!(b_spawn(p: u128, n: i64, m: u128, i: u128, a: u128, e: u128) -> i64 = spawn);
bound_c!(b_spawnp(p: u128, n: i64, m: u128, i: u128, a: u128, e: u128) -> i64 = spawnp);
bound_c!(b_waitpid(p: i64, s: u128, o: i64) -> i64 = waitpid);
bound_c!(b_wait(s: u128) -> i64 = wait);
bound_c!(b_kill(p: i64, s: i64) -> i64 = kill);
bound_c!(b_getpid() -> i64 = getpid);
bound_c!(b_getppid() -> i64 = getppid);

/// Every procedure with its code address.
pub(crate) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("getenv", at!(b_getenv)),
        ("putenv", at!(b_putenv)),
        ("Qp0zGetEnv", at!(b_qp0zgetenv)),
        ("Qp0zPutEnv", at!(b_qp0zputenv)),
        ("Qp0zDltEnv", at!(b_qp0zdltenv)),
        ("Qp0zGetAllSysEnv", at!(b_qp0zgetallsysenv)),
        ("Qp0zGetSysEnv", at!(b_qp0zgetsysenv)),
        ("Qp0zPutSysEnv", at!(b_qp0zputsysenv)),
        ("Qp0zDltSysEnv", at!(b_qp0zdltsysenv)),
        ("spawn", at!(b_spawn)),
        ("spawnp", at!(b_spawnp)),
        ("Qp0zSpawn", at!(b_spawn)),
        ("Qp0zSpawnp", at!(b_spawnp)),
        ("waitpid", at!(b_waitpid)),
        ("wait", at!(b_wait)),
        ("Qp0zWaitpid", at!(b_waitpid)),
        ("Qp0zWait", at!(b_wait)),
        ("kill", at!(b_kill)),
        ("getpid", at!(b_getpid)),
        ("getppid", at!(b_getppid)),
        ("Qp2RunPase", at!(b_qp2runpase)),
    ]
}
