// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`Files`] for threads when only one thread may reach the files: a
//! job's link to the storage service is not shared between threads,
//! so connection threads send each operation to the job's main thread,
//! which answers it through files only it may use ([`Local`],
//! [`Pump::serve`]). [`Pump::run`] runs work that needs [`Files`] on a
//! helper thread while the owner answers for it.

use std::io;
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crate::files::{Entry, Files};

/// Files one thread may use (not `Send`): the operations the bridge
/// carries. Every [`Files`] is one.
#[allow(clippy::missing_errors_doc)]
pub trait Local {
    /// A whole file.
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
    /// Replace a file whole.
    fn write_atomic(&self, path: &str, data: &[u8]) -> io::Result<()>;
    /// Add to the end of a file, creating it.
    fn append(&self, path: &str, data: &[u8]) -> io::Result<()>;
    /// Rename, replacing `to`.
    fn rename(&self, from: &str, to: &str) -> io::Result<()>;
    /// Create a directory and its parents.
    fn mkdir_all(&self, path: &str) -> io::Result<()>;
    /// Remove a file.
    fn remove(&self, path: &str) -> io::Result<()>;
    /// Remove a directory and everything under it.
    fn remove_tree(&self, path: &str) -> io::Result<()>;
    /// A directory's entries.
    fn list(&self, dir: &str) -> io::Result<Vec<Entry>>;
    /// The entry at `path`.
    fn stat(&self, path: &str) -> io::Result<Entry>;
}

impl<T: Files + ?Sized> Local for T {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        Files::read(self, path)
    }
    fn write_atomic(&self, path: &str, data: &[u8]) -> io::Result<()> {
        Files::write_atomic(self, path, data)
    }
    fn append(&self, path: &str, data: &[u8]) -> io::Result<()> {
        Files::append(self, path, data)
    }
    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        Files::rename(self, from, to)
    }
    fn mkdir_all(&self, path: &str) -> io::Result<()> {
        Files::mkdir_all(self, path)
    }
    fn remove(&self, path: &str) -> io::Result<()> {
        Files::remove(self, path)
    }
    fn remove_tree(&self, path: &str) -> io::Result<()> {
        Files::remove_tree(self, path)
    }
    fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        Files::list(self, dir)
    }
    fn stat(&self, path: &str) -> io::Result<Entry> {
        Files::stat(self, path)
    }
}

/// One operation and where its answer goes.
enum Op {
    Read(String, Sender<io::Result<Vec<u8>>>),
    Write(String, Vec<u8>, Sender<io::Result<()>>),
    Append(String, Vec<u8>, Sender<io::Result<()>>),
    Rename(String, String, Sender<io::Result<()>>),
    Mkdir(String, Sender<io::Result<()>>),
    Remove(String, Sender<io::Result<()>>),
    RemoveTree(String, Sender<io::Result<()>>),
    List(String, Sender<io::Result<Vec<Entry>>>),
    Stat(String, Sender<io::Result<Entry>>),
}

/// The [`Files`] the threads use.
pub struct Remote {
    tx: Mutex<Sender<Op>>,
}

/// The owning thread's end.
pub struct Pump {
    rx: Receiver<Op>,
}

/// A connected pair.
pub fn pair() -> (Remote, Pump) {
    let (tx, rx) = mpsc::channel();
    (Remote { tx: Mutex::new(tx) }, Pump { rx })
}

fn gone() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "the job is ending")
}

impl Remote {
    fn ask<T>(&self, make: impl FnOnce(Sender<io::Result<T>>) -> Op) -> io::Result<T> {
        let (tx, rx) = mpsc::channel();
        self.tx.lock().map_err(|_| gone())?.send(make(tx)).map_err(|_| gone())?;
        rx.recv().map_err(|_| gone())?
    }
}

impl Files for Remote {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        self.ask(|t| Op::Read(path.into(), t))
    }
    fn write_atomic(&self, path: &str, data: &[u8]) -> io::Result<()> {
        self.ask(|t| Op::Write(path.into(), data.to_vec(), t))
    }
    fn append(&self, path: &str, data: &[u8]) -> io::Result<()> {
        self.ask(|t| Op::Append(path.into(), data.to_vec(), t))
    }
    fn rename(&self, from: &str, to: &str) -> io::Result<()> {
        self.ask(|t| Op::Rename(from.into(), to.into(), t))
    }
    fn mkdir_all(&self, path: &str) -> io::Result<()> {
        self.ask(|t| Op::Mkdir(path.into(), t))
    }
    fn remove(&self, path: &str) -> io::Result<()> {
        self.ask(|t| Op::Remove(path.into(), t))
    }
    fn remove_tree(&self, path: &str) -> io::Result<()> {
        self.ask(|t| Op::RemoveTree(path.into(), t))
    }
    fn list(&self, dir: &str) -> io::Result<Vec<Entry>> {
        self.ask(|t| Op::List(dir.into(), t))
    }
    fn stat(&self, path: &str) -> io::Result<Entry> {
        self.ask(|t| Op::Stat(path.into(), t))
    }
}

impl Pump {
    /// Run `work` over `remote` on a helper thread, answering its
    /// operations through `files` here until it is done.
    pub fn run<T: Send>(&self, remote: &Remote, files: &dyn Local, work: impl FnOnce(&dyn Files) -> T + Send) -> T {
        std::thread::scope(|s| {
            let h = s.spawn(|| work(remote));
            while !h.is_finished() {
                self.serve(files, Duration::from_millis(10));
            }
            match h.join() {
                Ok(v) => v,
                Err(p) => std::panic::resume_unwind(p),
            }
        })
    }

    /// Answer operations through `files` for up to `wait`; how many.
    pub fn serve(&self, files: &dyn Local, wait: Duration) -> usize {
        let until = Instant::now() + wait;
        let mut n = 0;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(op) => {
                    n += 1;
                    answer(files, op);
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return n,
            }
            if Instant::now() >= until {
                // Whatever is queued still gets its answer.
                while let Ok(op) = self.rx.try_recv() {
                    n += 1;
                    answer(files, op);
                }
                return n;
            }
        }
    }
}

fn answer(f: &dyn Local, op: Op) {
    match op {
        Op::Read(p, t) => drop(t.send(f.read(&p))),
        Op::Write(p, d, t) => drop(t.send(f.write_atomic(&p, &d))),
        Op::Append(p, d, t) => drop(t.send(f.append(&p, &d))),
        Op::Rename(a, b, t) => drop(t.send(f.rename(&a, &b))),
        Op::Mkdir(p, t) => drop(t.send(f.mkdir_all(&p))),
        Op::Remove(p, t) => drop(t.send(f.remove(&p))),
        Op::RemoveTree(p, t) => drop(t.send(f.remove_tree(&p))),
        Op::List(p, t) => drop(t.send(f.list(&p))),
        Op::Stat(p, t) => drop(t.send(f.stat(&p))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::MemFiles;
    use std::sync::Arc;

    #[test]
    fn threads_reach_the_owner() {
        let (remote, pump) = pair();
        let remote = Arc::new(remote);
        let mem = MemFiles::new();
        mem.put("/a/b.txt", b"hello");
        let r2 = Arc::clone(&remote);
        let t = std::thread::spawn(move || {
            let f: &dyn Files = r2.as_ref();
            Files::append(f, "/a/b.txt", b"!").unwrap();
            let e = Files::stat(f, "/a/b.txt").unwrap();
            (Files::read(f, "/a/b.txt").unwrap(), e.size, f.exists("/nope"))
        });
        while !t.is_finished() {
            pump.serve(&mem, Duration::from_millis(20));
        }
        assert_eq!(t.join().unwrap(), (b"hello!".to_vec(), 6, false));
        let names = pump.run(&remote, &mem, |f| crate::registry::Registry::load(f).map(|r| r.instances.len()));
        assert_eq!(names, Ok(0));
        drop(pump);
        assert!(Files::read(remote.as_ref(), "/a/b.txt").is_err());
    }
}
