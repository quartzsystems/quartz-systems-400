// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job's descriptor table. Sockets and IFS files share one
//! numbering, as on the machine: a number names an open description,
//! which `dup` and `dup2` share (the non-blocking flag belongs to the
//! description, `FD_CLOEXEC` to the number). A socket is a kernel
//! descriptor of the job process; an IFS file is a handle the storage
//! service gave. Closing the last number of a description releases it
//! through the owner's hook ([`SocketOps`], [`IfsOps`]); the table
//! itself knows nothing of the kernel or the service.
//!
//! Errors are IBM `errno` numbers ([`errno`]).

pub mod errno;

use std::cell::RefCell;
use std::rc::Rc;

pub use errno::ErrnoCell;

/// Descriptors a job may hold at once (`EMFILE` past it).
pub const DEFAULT_MAX: usize = 2500;

/// What an open description is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A socket: the kernel descriptor in the job process.
    Socket(i32),
    /// An IFS file: the storage service's handle.
    Ifs(u32),
}

/// An open description, shared by every number `dup` made of it.
#[derive(Debug)]
pub struct Open {
    /// What it is.
    pub kind: Kind,
    /// `O_NONBLOCK` (`F_SETFL`, `FIONBIO`).
    pub nonblock: bool,
}

/// One number of the table.
#[derive(Debug)]
struct Entry {
    open: Rc<RefCell<Open>>,
    cloexec: bool,
}

/// How sockets are released; the sockets layer sets it.
pub trait SocketOps {
    /// Close a kernel descriptor.
    fn close(&mut self, fd: i32) -> Result<(), i32>;
}

/// The IFS file operations a descriptor reaches; the IFS API layer
/// sets it. Minimal on purpose: what `read`, `write` and `close` on a
/// descriptor need when the number is a file, not a socket.
pub trait IfsOps {
    /// Close a service handle.
    fn close(&mut self, handle: u32) -> Result<(), i32>;
    /// Read into `buf`; the bytes read.
    fn read(&mut self, handle: u32, buf: &mut [u8]) -> Result<usize, i32>;
    /// Write `buf`; the bytes written.
    fn write(&mut self, handle: u32, buf: &[u8]) -> Result<usize, i32>;
}

/// The table.
#[derive(Default)]
pub struct FdTable {
    entries: Vec<Option<Entry>>,
    max: usize,
    socket_ops: Option<Box<dyn SocketOps>>,
    ifs_ops: Option<Box<dyn IfsOps>>,
}

impl std::fmt::Debug for FdTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FdTable")
            .field("open", &self.open_count())
            .field("max", &self.max)
            .finish_non_exhaustive()
    }
}

/// `EBADF`.
pub const EBADF: i32 = 3450;
/// `EMFILE`.
pub const EMFILE: i32 = 3452;
/// `EINVAL`.
pub const EINVAL: i32 = 3021;
/// `ENOTSOCK`.
pub const ENOTSOCK: i32 = 3439;
/// `ENOSYS`: no owner hook is set for the descriptor's kind.
pub const ENOSYS: i32 = 3470;

impl FdTable {
    /// An empty table with the default limit.
    #[must_use]
    pub fn new() -> Self {
        Self::with_max(DEFAULT_MAX)
    }

    /// An empty table holding at most `max` numbers.
    #[must_use]
    pub fn with_max(max: usize) -> Self {
        Self {
            entries: Vec::new(),
            max,
            socket_ops: None,
            ifs_ops: None,
        }
    }

    /// Who releases sockets.
    pub fn set_socket_ops(&mut self, ops: Box<dyn SocketOps>) {
        self.socket_ops = Some(ops);
    }

    /// Who operates IFS files.
    pub fn set_ifs_ops(&mut self, ops: Box<dyn IfsOps>) {
        self.ifs_ops = Some(ops);
    }

    /// The IFS operations, when set.
    pub fn ifs_ops(&mut self) -> Option<&mut (dyn IfsOps + 'static)> {
        self.ifs_ops.as_deref_mut()
    }

    /// The limit.
    #[must_use]
    pub fn max(&self) -> usize {
        self.max
    }

    /// Numbers in use.
    #[must_use]
    pub fn open_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_some()).count()
    }

    /// The lowest free number at or above `min`.
    fn free_from(&self, min: usize) -> Result<usize, i32> {
        let mut n = min;
        while n < self.entries.len() {
            if self.entries[n].is_none() {
                return Ok(n);
            }
            n += 1;
        }
        if n >= self.max {
            return Err(EMFILE);
        }
        Ok(n)
    }

    fn put(&mut self, n: usize, e: Entry) {
        if self.entries.len() <= n {
            self.entries.resize_with(n + 1, || None);
        }
        self.entries[n] = Some(e);
    }

    /// A new description at the lowest free number.
    pub fn open(&mut self, kind: Kind) -> Result<i32, i32> {
        let n = self.free_from(0)?;
        self.put(
            n,
            Entry {
                open: Rc::new(RefCell::new(Open {
                    kind,
                    nonblock: false,
                })),
                cloexec: false,
            },
        );
        Ok(i32::try_from(n).expect("below max"))
    }

    fn entry(&self, n: i32) -> Result<&Entry, i32> {
        usize::try_from(n)
            .ok()
            .and_then(|i| self.entries.get(i))
            .and_then(Option::as_ref)
            .ok_or(EBADF)
    }

    /// Whether the number is open.
    #[must_use]
    pub fn is_open(&self, n: i32) -> bool {
        self.entry(n).is_ok()
    }

    /// What the number names.
    pub fn kind(&self, n: i32) -> Result<Kind, i32> {
        Ok(self.entry(n)?.open.borrow().kind)
    }

    /// The kernel descriptor of a socket number (`ENOTSOCK` for a file).
    pub fn socket(&self, n: i32) -> Result<i32, i32> {
        match self.kind(n)? {
            Kind::Socket(fd) => Ok(fd),
            Kind::Ifs(_) => Err(ENOTSOCK),
        }
    }

    /// `O_NONBLOCK` of the description.
    pub fn nonblock(&self, n: i32) -> Result<bool, i32> {
        Ok(self.entry(n)?.open.borrow().nonblock)
    }

    /// Set `O_NONBLOCK` on the description (every dup sees it).
    pub fn set_nonblock(&mut self, n: i32, on: bool) -> Result<(), i32> {
        self.entry(n)?.open.borrow_mut().nonblock = on;
        Ok(())
    }

    /// `FD_CLOEXEC` of the number.
    pub fn cloexec(&self, n: i32) -> Result<bool, i32> {
        Ok(self.entry(n)?.cloexec)
    }

    /// Set `FD_CLOEXEC` on the number.
    pub fn set_cloexec(&mut self, n: i32, on: bool) -> Result<(), i32> {
        let i = usize::try_from(n).map_err(|_| EBADF)?;
        match self.entries.get_mut(i).and_then(Option::as_mut) {
            Some(e) => {
                e.cloexec = on;
                Ok(())
            }
            None => Err(EBADF),
        }
    }

    /// `dup`: the lowest free number sharing the description.
    pub fn dup(&mut self, n: i32) -> Result<i32, i32> {
        self.dup_from(n, 0)
    }

    /// `F_DUPFD`: the lowest free number at or above `min` sharing the
    /// description; `FD_CLOEXEC` is off on the new number.
    pub fn dup_from(&mut self, n: i32, min: i32) -> Result<i32, i32> {
        let open = self.entry(n)?.open.clone();
        let min = usize::try_from(min).map_err(|_| EINVAL)?;
        if min >= self.max {
            return Err(EINVAL);
        }
        let new = self.free_from(min)?;
        self.put(
            new,
            Entry {
                open,
                cloexec: false,
            },
        );
        Ok(i32::try_from(new).expect("below max"))
    }

    /// `dup2`: `new` names `old`'s description, closing what `new`
    /// named before. `old == new` is a no-op that checks `old`.
    pub fn dup2(&mut self, old: i32, new: i32) -> Result<i32, i32> {
        let open = self.entry(old)?.open.clone();
        let i = usize::try_from(new).map_err(|_| EBADF)?;
        if i >= self.max {
            return Err(EBADF);
        }
        if old == new {
            return Ok(new);
        }
        if self.is_open(new) {
            self.close(new)?;
        }
        self.put(
            i,
            Entry {
                open,
                cloexec: false,
            },
        );
        Ok(new)
    }

    /// `close`: the number goes; the description is released through
    /// the owner's hook when no other number shares it.
    pub fn close(&mut self, n: i32) -> Result<(), i32> {
        let i = usize::try_from(n).map_err(|_| EBADF)?;
        let e = self
            .entries
            .get_mut(i)
            .and_then(Option::take)
            .ok_or(EBADF)?;
        if Rc::strong_count(&e.open) > 1 {
            return Ok(());
        }
        let kind = e.open.borrow().kind;
        self.release(kind)
    }

    fn release(&mut self, kind: Kind) -> Result<(), i32> {
        match kind {
            Kind::Socket(fd) => match self.socket_ops.as_mut() {
                Some(ops) => ops.close(fd),
                None => Err(ENOSYS),
            },
            Kind::Ifs(h) => match self.ifs_ops.as_mut() {
                Some(ops) => ops.close(h),
                None => Err(ENOSYS),
            },
        }
    }

    /// Every open number with what it names, ascending.
    #[must_use]
    pub fn list(&self) -> Vec<(i32, Kind, bool)> {
        self.entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                e.as_ref().map(|e| {
                    let o = e.open.borrow();
                    (i32::try_from(i).expect("below max"), o.kind, o.nonblock)
                })
            })
            .collect()
    }

    /// Close everything, as the end of the job does; the first error.
    pub fn close_all(&mut self) -> Result<(), i32> {
        let mut first = Ok(());
        for n in 0..self.entries.len() {
            if self.entries[n].is_some() {
                let r = self.close(i32::try_from(n).expect("below max"));
                if first.is_ok() {
                    first = r;
                }
            }
        }
        first
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Log(Rc<RefCell<Vec<i32>>>);

    impl SocketOps for Log {
        fn close(&mut self, fd: i32) -> Result<(), i32> {
            self.0.borrow_mut().push(fd);
            Ok(())
        }
    }

    fn table(log: &Rc<RefCell<Vec<i32>>>) -> FdTable {
        let mut t = FdTable::with_max(8);
        t.set_socket_ops(Box::new(Log(log.clone())));
        t
    }

    #[test]
    fn lowest_free_number_and_close() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut t = table(&log);
        assert_eq!(t.open(Kind::Socket(10)), Ok(0));
        assert_eq!(t.open(Kind::Socket(11)), Ok(1));
        assert_eq!(t.open(Kind::Ifs(7)), Ok(2));
        assert_eq!(t.close(1), Ok(()));
        assert_eq!(log.borrow().as_slice(), &[11]);
        assert_eq!(t.open(Kind::Socket(12)), Ok(1), "the gap is reused");
        assert_eq!(t.close(1), Ok(()));
        assert_eq!(t.close(1), Err(EBADF));
        assert_eq!(t.socket(2), Err(ENOTSOCK));
        assert_eq!(t.socket(0), Ok(10));
        assert_eq!(t.kind(2), Ok(Kind::Ifs(7)));
        assert_eq!(t.close(2), Err(ENOSYS), "no IFS hook yet");
    }

    #[test]
    fn dups_share_the_description() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut t = table(&log);
        let a = t.open(Kind::Socket(20)).unwrap();
        let b = t.dup(a).unwrap();
        assert_eq!(b, 1);
        t.set_nonblock(a, true).unwrap();
        assert_eq!(t.nonblock(b), Ok(true), "the flag is the description's");
        t.set_cloexec(a, true).unwrap();
        assert_eq!(t.cloexec(b), Ok(false), "cloexec is the number's");
        assert_eq!(t.dup_from(a, 5), Ok(5));
        t.close(a).unwrap();
        t.close(b).unwrap();
        assert!(log.borrow().is_empty(), "still open at 5");
        t.close(5).unwrap();
        assert_eq!(log.borrow().as_slice(), &[20]);
    }

    #[test]
    fn dup2_closes_the_target() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut t = table(&log);
        let a = t.open(Kind::Socket(30)).unwrap();
        let b = t.open(Kind::Socket(31)).unwrap();
        assert_eq!(t.dup2(a, b), Ok(b));
        assert_eq!(log.borrow().as_slice(), &[31]);
        assert_eq!(t.socket(b), Ok(30));
        assert_eq!(t.dup2(a, a), Ok(a));
        assert_eq!(t.dup2(9, 1), Err(EBADF));
        assert_eq!(t.dup2(a, 8), Err(EBADF), "past the limit");
        assert_eq!(t.dup2(a, 7), Ok(7));
        assert_eq!(t.list().len(), 3);
        t.close_all().unwrap();
        assert_eq!(log.borrow().as_slice(), &[31, 30]);
        assert_eq!(t.open_count(), 0);
    }

    #[test]
    fn the_limit_is_emfile() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut t = table(&log);
        for i in 0..8 {
            assert_eq!(t.open(Kind::Socket(i)), Ok(i));
        }
        assert_eq!(t.open(Kind::Socket(99)), Err(EMFILE));
        assert_eq!(t.dup(0), Err(EMFILE));
        assert_eq!(t.dup_from(0, 8), Err(EINVAL));
    }
}
