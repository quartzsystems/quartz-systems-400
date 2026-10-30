// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS APIs as C-style bound procedures under IBM's service
//! program names (`QP0LLIB1`, `QP0LLIB2`, `QP0LLIB3`, `QC2UTIL1`,
//! `QC2IO`; `ile::system_srvpgm` has the table) and the program APIs
//! `QP0LROR` and `QP0LRRO`. Each job reaches the file system through
//! an [`ifs::client::IfsClient`] given by [`configure`]: the job
//! process gives the storage service's (`stg::remote::RemoteIfs`),
//! tests a `LocalIfs`. Descriptors are numbers of the job's table
//! (`mi::fd`), shared with sockets: [`configure`] installs the IFS
//! operations there, so `read()`, `write()` and `close()` serve both.
//! Paths arrive in the job's CCSID (or a `Qlg_Path_Name_T`'s), errors
//! are IBM `errno` values through `__errno()`.

pub mod consts;
pub mod layout;
pub mod memory;

mod attr;
mod dir;
mod file;
mod names;
mod path;
mod qlg;
mod qp0l;
mod refs;

pub use attr::DATA as ATTR_DATA;
pub(crate) use file::fcntl_ifs;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use ifs::Errno;
use ifs::client::{IfsClient, Unavailable};
use mi::Job;
use mi::fd::{IfsOps, Kind};

use crate::net::Stable;

type R<T> = Result<T, Errno>;

/// One open directory stream: the service's handle, the `DIR` the
/// program holds a pointer to, and the entry storage `readdir` returns.
pub(crate) struct Dir {
    handle: u64,
    block: Stable,
    entry: Option<Stable>,
    entry_lg: Option<Stable>,
    ahead: std::collections::VecDeque<ifs::ops::DirEnt>,
}

/// One job's IFS state.
pub struct IfsState {
    /// The file system as the job sees it.
    pub client: Rc<dyn IfsClient>,
    /// The CCSID paths and texts are in when a call names none.
    pub ccsid: u16,
    /// Open directory streams by the native address of their `DIR`.
    pub(crate) dirs: HashMap<u64, Dir>,
}

/// The descriptor table's hook for IFS files.
struct Ops(Rc<dyn IfsClient>);

fn handle(h: u32) -> i32 {
    i32::try_from(h).unwrap_or(i32::MAX)
}

impl IfsOps for Ops {
    fn close(&mut self, h: u32) -> Result<(), i32> {
        self.0.close(handle(h)).map_err(Errno::code)
    }

    fn read(&mut self, h: u32, buf: &mut [u8]) -> Result<usize, i32> {
        let mut done = 0;
        while done < buf.len() {
            let got = self.0.read(handle(h), buf.len() - done).map_err(Errno::code)?;
            if got.is_empty() {
                break;
            }
            buf[done..done + got.len()].copy_from_slice(&got);
            done += got.len();
            // One read answers what one request gave, as a short read may.
            if done < buf.len() && got.len() < ifs::client::CHUNK {
                break;
            }
        }
        Ok(done)
    }

    fn write(&mut self, h: u32, buf: &[u8]) -> Result<usize, i32> {
        let mut done = 0;
        while done < buf.len() {
            let n = self.0.write(handle(h), &buf[done..]).map_err(Errno::code)?;
            if n == 0 {
                break;
            }
            done += n;
        }
        Ok(done)
    }
}

thread_local! {
    static STATES: RefCell<HashMap<usize, Rc<RefCell<IfsState>>>> = RefCell::new(HashMap::new());
}

fn key(j: &Job) -> usize {
    std::ptr::from_ref(j) as usize
}

/// Give a job its file system: `client` answers every IFS call, `ccsid`
/// is the job's CCSID for paths and the APIs' texts. IFS descriptors the
/// job held before are closed.
pub fn configure(j: &mut Job, client: Rc<dyn IfsClient>, ccsid: u16) {
    close_files(j);
    j.fds.set_ifs_ops(Box::new(Ops(client.clone())));
    let st = IfsState {
        client,
        ccsid: if ccsid == 0 || ccsid == 65535 { 37 } else { ccsid },
        dirs: HashMap::new(),
    };
    STATES.with(|m| m.borrow_mut().insert(key(j), Rc::new(RefCell::new(st))));
}

/// A job's IFS state; a job never configured gets a file system that
/// answers `ENOTSUP` to everything.
pub fn state(j: &mut Job) -> Rc<RefCell<IfsState>> {
    if let Some(s) = STATES.with(|m| m.borrow().get(&key(j)).cloned()) {
        return s;
    }
    configure(j, Rc::new(Unavailable(Errno::ENOTSUP)), 37);
    STATES.with(|m| m.borrow().get(&key(j)).cloned()).expect("just configured")
}

/// The job's client.
pub(crate) fn client(j: &mut Job) -> Rc<dyn IfsClient> {
    state(j).borrow().client.clone()
}

/// The job's CCSID for paths and texts.
pub(crate) fn job_ccsid(j: &mut Job) -> u16 {
    state(j).borrow().ccsid
}

fn close_files(j: &mut Job) {
    for (n, kind, _) in j.fds.list() {
        if matches!(kind, Kind::Ifs(_)) {
            let _ = j.fds.close(n);
        }
    }
}

/// Forget a job's IFS state as the job goes away.
pub(crate) fn drop_state(j: &Job) {
    STATES.with(|m| m.borrow_mut().remove(&key(j)));
}

/// The service's descriptor behind a job number; `ENOTSUP` for a socket.
pub(crate) fn service_fd(j: &Job, n: i32) -> R<i32> {
    match j.fds.kind(n).map_err(errno)? {
        Kind::Ifs(h) => Ok(handle(h)),
        Kind::Socket(_) => Err(Errno::ENOTSUP),
    }
}

/// A new job number for a service descriptor; the descriptor closes
/// when the table is full.
pub(crate) fn adopt(j: &mut Job, fd: i32) -> R<i32> {
    let h = u32::try_from(fd).map_err(|_| Errno::EBADF)?;
    match j.fds.open(Kind::Ifs(h)) {
        Ok(n) => Ok(n),
        Err(e) => {
            let _ = client(j).close(fd);
            Err(errno(e))
        }
    }
}

/// A `mi` `errno` number as the `Errno` it names.
pub(crate) fn errno(code: i32) -> Errno {
    Errno::from_code(code).unwrap_or(Errno::EUNKNOWN)
}

/// Every procedure with its code address.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    let mut v = file::procedures();
    v.extend(names::procedures());
    v.extend(dir::procedures());
    v.extend(qlg::procedures());
    v.extend(attr::procedures());
    v.extend(qp0l::procedures());
    v
}

/// The program APIs (`QP0LROR`, `QP0LRRO`).
pub const APIS: &[(&str, crate::ApiFn)] = refs::APIS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unconfigured_job_answers_enotsup() {
        let mut jb = Job::boxed(Box::new(mi::MemSource::default()));
        let j = &mut *jb;
        let c = client(j);
        assert_eq!(c.getcwd(), Err(Errno::ENOTSUP));
        assert_eq!(job_ccsid(j), 37);
        drop_state(j);
    }
}
