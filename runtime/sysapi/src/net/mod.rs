// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The socket, descriptor and resolver APIs as C-style bound
//! procedures under IBM's service program names (`QSOSRV1`, `QSOSRV2`,
//! `QP0LLIB1`, `QC2UTIL1`; `ile::system_srvpgm` has the table). Each
//! job has a [`Net`]: its sockets (the `sockets` crate, kernel
//! descriptors keyed by kernel number), the resolver and the storage
//! the `netdb` calls return pointers into. Job descriptor numbers come
//! from the job's table (`mi::fd`), shared with IFS files.

pub(crate) mod abi;
mod calls;
mod descr;
mod netdb;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mi::fd::SocketOps;
use mi::{FatPtr, Job};
use sockets::resolver::{DefaultServices, HostConfig, Resolver, ServicesTable, StaticHostConfig};
use sockets::{
    AllowAll, DescriptorPassing, IfsPathMap, InProcess, JobId, PortRules, RootedPaths, Sockets,
};

/// What a job process gives its jobs' network calls; [`NetSetup::default`]
/// has no port restrictions, IFS paths as host paths, in-process
/// descriptor passing and the shipped host and services tables.
pub struct NetSetup {
    /// Port restrictions.
    pub rules: Box<dyn PortRules>,
    /// `AF_UNIX` paths.
    pub paths: Box<dyn IfsPathMap>,
    /// `givedescriptor`/`takedescriptor`.
    pub passing: Box<dyn DescriptorPassing>,
    /// The host table and domain attributes.
    pub hosts: Box<dyn HostConfig>,
    /// The services table.
    pub services: Box<dyn ServicesTable>,
    /// The mark bound sockets get; `None` when the process lacks
    /// `CAP_NET_ADMIN`.
    pub mark: Option<u32>,
}

impl Default for NetSetup {
    fn default() -> Self {
        Self {
            rules: Box::new(AllowAll),
            paths: Box::new(RootedPaths::default()),
            passing: Box::new(InProcess::new([0; 16])),
            hosts: Box::new(StaticHostConfig::default()),
            services: Box::new(DefaultServices),
            mark: Some(netlink::nft::SOCKET_MARK),
        }
    }
}

/// Storage a pointer handed to the program keeps addressing: owned
/// here, a space of the job's table.
pub(crate) struct Stable {
    data: Box<[u8]>,
    ptr: FatPtr,
}

impl Stable {
    /// `n` zero bytes, addressable from the job.
    pub(crate) fn new(j: &mut Job, n: usize) -> Self {
        let data = vec![0u8; n].into_boxed_slice();
        let ptr = j.spaces.add_region(data.as_ptr() as u64, n as u64);
        Self { data, ptr }
    }

    /// The bytes.
    pub(crate) fn bytes(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// The pointer to the first byte.
    pub(crate) fn ptr(&self) -> FatPtr {
        self.ptr
    }

    /// The native address of the first byte.
    pub(crate) fn native(&self) -> u64 {
        self.data.as_ptr() as u64
    }
}

/// One job's network state.
pub struct Net {
    /// The sockets, shared with the descriptor table's release hook.
    pub sockets: Rc<RefCell<Sockets>>,
    /// The resolver.
    pub resolver: Resolver,
    /// Buffers by purpose (`hostent`, `servent`, `ntoa`, ...).
    pub(crate) fixed: HashMap<&'static str, Stable>,
    /// Texts returned by `strerror` and its kin, by table and number.
    pub(crate) texts: HashMap<(&'static str, i32), Stable>,
    /// `addrinfo` lists `freeaddrinfo` releases, by native address.
    pub(crate) lists: HashMap<u64, Stable>,
}

impl Net {
    /// A fixed buffer of at least `n` bytes for one purpose.
    pub(crate) fn fixed(&mut self, j: &mut Job, what: &'static str, n: usize) -> &mut Stable {
        let fits = self.fixed.get(what).is_some_and(|s| s.data.len() >= n);
        if !fits {
            if let Some(old) = self.fixed.remove(what) {
                j.spaces.kill_at(old.native());
            }
            self.fixed.insert(what, Stable::new(j, n));
        }
        self.fixed.get_mut(what).expect("just made")
    }

    /// A NUL-terminated EBCDIC text kept for good; its pointer.
    pub(crate) fn text(&mut self, j: &mut Job, table: &'static str, code: i32, s: &str) -> FatPtr {
        if let Some(t) = self.texts.get(&(table, code)) {
            return t.ptr();
        }
        let mut b = ccsid::to_ebcdic(s).unwrap_or_default();
        b.push(0);
        let mut st = Stable::new(j, b.len());
        st.bytes().copy_from_slice(&b);
        let p = st.ptr();
        self.texts.insert((table, code), st);
        p
    }

    /// Keep a list's storage until `freeaddrinfo`; its pointer.
    pub(crate) fn keep_list(j: &mut Job, bytes: &[u8]) -> Stable {
        let mut st = Stable::new(j, bytes.len().max(1));
        st.bytes()[..bytes.len()].copy_from_slice(bytes);
        st
    }

    /// Release a list.
    pub(crate) fn free_list(&mut self, j: &mut Job, native: u64) -> bool {
        match self.lists.remove(&native) {
            Some(_) => {
                j.spaces.kill_at(native);
                true
            }
            None => false,
        }
    }
}

/// The descriptor table's release hook for sockets.
struct Release(Rc<RefCell<Sockets>>);

impl SocketOps for Release {
    fn close(&mut self, fd: i32) -> Result<(), i32> {
        self.0.borrow_mut().close(fd).map_err(sockets::Errno::code)
    }
}

thread_local! {
    static NETS: RefCell<HashMap<usize, Rc<RefCell<Net>>>> = RefCell::new(HashMap::new());
}

fn key(j: &Job) -> usize {
    std::ptr::from_ref(j) as usize
}

/// A job's internal identifier, as `QUSRJOBI` returns it and
/// `givedescriptor` takes it: the job number and name in EBCDIC.
#[must_use]
pub fn internal_job_id(j: &Job) -> JobId {
    let s = format!("{:0>6}{:<10}", j.ident.number, j.ident.name);
    let mut b = ccsid::to_ebcdic(&s).unwrap_or_default();
    b.resize(16, 0x40);
    let mut id = [0u8; 16];
    id.copy_from_slice(&b[..16]);
    id
}

/// Give a job its network setup, replacing any it had (its open sockets
/// are closed first).
pub fn configure(j: &mut Job, setup: NetSetup) {
    close_sockets(j);
    let mut s = Sockets::new(setup.rules, setup.paths, setup.passing);
    s.set_user(&j.ident.user);
    s.set_mark(setup.mark);
    let sockets = Rc::new(RefCell::new(s));
    j.fds.set_socket_ops(Box::new(Release(sockets.clone())));
    let net = Net {
        sockets,
        resolver: Resolver::new(setup.hosts, setup.services),
        fixed: HashMap::new(),
        texts: HashMap::new(),
        lists: HashMap::new(),
    };
    NETS.with(|m| m.borrow_mut().insert(key(j), Rc::new(RefCell::new(net))));
}

/// A job's network state, made with [`NetSetup::default`] on first use.
pub fn net(j: &mut Job) -> Rc<RefCell<Net>> {
    if let Some(n) = NETS.with(|m| m.borrow().get(&key(j)).cloned()) {
        return n;
    }
    configure(j, NetSetup::default());
    NETS.with(|m| m.borrow().get(&key(j)).cloned()).expect("just configured")
}

/// Close every socket descriptor of the job; IFS files stay open.
fn close_sockets(j: &mut Job) {
    for (n, kind, _) in j.fds.list() {
        if matches!(kind, mi::fd::Kind::Socket(_)) {
            let _ = j.fds.close(n);
        }
    }
}

/// Forget a job's network state; its sockets close.
pub fn forget(j: &mut Job) {
    if NETS.with(|m| m.borrow_mut().remove(&key(j))).is_some() {
        close_sockets(j);
    }
}

/// Forget a job's network state as the job goes away; its sockets
/// close with the job's descriptor table.
pub(crate) fn drop_state(j: &Job) {
    NETS.with(|m| m.borrow_mut().remove(&key(j)));
}

/// Every procedure with its code address.
#[must_use]
pub fn procedures() -> Vec<(&'static str, usize)> {
    let mut v = calls::procedures();
    v.extend(descr::procedures());
    v.extend(netdb::procedures());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_procedure_has_its_service_program_and_back() {
        let ours: Vec<&str> = crate::c_procedures().iter().map(|(n, _)| *n).collect();
        let mut seen = std::collections::HashSet::new();
        for n in &ours {
            assert!(seen.insert(*n), "{n} registered twice");
        }
        for n in &ours {
            assert!(ile::is_system_srvpgm_proc(n), "{n} has no service program");
        }
        for (n, _) in ile::SYSTEM_SRVPGM_PROCS {
            assert!(ours.contains(n), "{n} has no procedure");
        }
    }

    #[test]
    fn errno_and_internal_ids_are_the_job_s() {
        let mut jb = Job::boxed(Box::new(mi::MemSource::default()));
        let j = &mut *jb;
        j.ident.number = "123".into();
        j.ident.name = "QZDASOINIT".into();
        assert_eq!(ccsid::from_ebcdic(&internal_job_id(j)), "000123QZDASOINIT");
        let n = net(j);
        let p = n.borrow_mut().text(j, "strerror", 3425, &sockets::strerror(3425));
        assert_eq!(n.borrow_mut().text(j, "strerror", 3425, "x"), p, "kept");
        let at = j.spaces.deref(p).unwrap();
        let b = mi::storage::view(at, 4);
        assert_ne!(b[0], 0);
        forget(j);
    }
}
