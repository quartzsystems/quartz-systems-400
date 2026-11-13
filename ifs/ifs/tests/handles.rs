// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! File IDs through a backend that opens nodes by handle (as xfs does,
//! `Backend::path_of`), and the fallback when its answer is missing or
//! wrong: the remembered paths, then the walk.

mod common;

use std::sync::{Arc, Mutex};

use common::{env, user};
use ifs::backend::mem::MemBackend;
use ifs::backend::{BStat, Backend, DirEntry, Kind, Nanos, StatFs};
use ifs::ops::audit::{FileId, MemAudit};
use ifs::ops::{Config, Ifs, Owner};
use ifs::vfs::qsys::MemQsys;
use ifs::{Ctx, Errno, Result};
use sec::auth::bits::Auth;

/// `MemBackend` with handles: every node's generation is derived from
/// its number, and `path_of` answers what the test sets.
#[derive(Clone)]
struct Handles {
    b: MemBackend,
    answer: Arc<Mutex<Option<String>>>,
    asked: Arc<Mutex<u32>>,
}

fn gen_of(ino: u64) -> u32 {
    u32::try_from(ino & 0xFFFF).unwrap() ^ 0x5A00_0000
}

impl Backend for Handles {
    type File = <MemBackend as Backend>::File;
    fn stat(&self, p: &str) -> Result<BStat> {
        self.b.stat(p)
    }
    fn readdir(&self, p: &str) -> Result<Vec<DirEntry>> {
        self.b.readdir(p)
    }
    fn mkdir(&self, p: &str, mode: u32) -> Result<()> {
        self.b.mkdir(p, mode)
    }
    fn rmdir(&self, p: &str) -> Result<()> {
        self.b.rmdir(p)
    }
    fn create(&self, p: &str, mode: u32) -> Result<Self::File> {
        self.b.create(p, mode)
    }
    fn open(&self, p: &str, write: bool) -> Result<Self::File> {
        self.b.open(p, write)
    }
    fn mknod(&self, p: &str, kind: Kind, mode: u32) -> Result<()> {
        self.b.mknod(p, kind, mode)
    }
    fn unlink(&self, p: &str) -> Result<()> {
        self.b.unlink(p)
    }
    fn rename(&self, from: &str, to: &str, replace: bool) -> Result<()> {
        self.b.rename(from, to, replace)
    }
    fn link(&self, existing: &str, new: &str) -> Result<()> {
        self.b.link(existing, new)
    }
    fn symlink(&self, target: &str, p: &str) -> Result<()> {
        self.b.symlink(target, p)
    }
    fn readlink(&self, p: &str) -> Result<String> {
        self.b.readlink(p)
    }
    fn set_times(&self, p: &str, a: Option<Nanos>, m: Option<Nanos>) -> Result<()> {
        self.b.set_times(p, a, m)
    }
    fn set_mode(&self, p: &str, mode: u32) -> Result<()> {
        self.b.set_mode(p, mode)
    }
    fn set_owner(&self, p: &str, uid: u32, gid: u32) -> Result<()> {
        self.b.set_owner(p, uid, gid)
    }
    fn xattr_get(&self, p: &str, name: &str) -> Result<Option<Vec<u8>>> {
        self.b.xattr_get(p, name)
    }
    fn xattr_set(&self, p: &str, name: &str, value: &[u8]) -> Result<()> {
        self.b.xattr_set(p, name, value)
    }
    fn xattr_list(&self, p: &str) -> Result<Vec<String>> {
        self.b.xattr_list(p)
    }
    fn xattr_remove(&self, p: &str, name: &str) -> Result<()> {
        self.b.xattr_remove(p, name)
    }
    fn statfs(&self) -> Result<StatFs> {
        self.b.statfs()
    }
    fn generation(&self, p: &str) -> Result<Option<u32>> {
        Ok(Some(gen_of(self.b.stat(p)?.ino)))
    }
    fn path_of(&self, _ino: u64, _generation: u32) -> Result<Option<String>> {
        *self.asked.lock().unwrap() += 1;
        Ok(self.answer.lock().unwrap().clone())
    }
}

#[test]
fn the_handle_first_then_the_cache_then_the_walk() {
    let h = Handles {
        b: MemBackend::new(),
        answer: Arc::default(),
        asked: Arc::default(),
    };
    let e = env();
    let ifs = Ifs::new(
        h.clone(),
        Box::new(MemQsys::new(Auth::USE)),
        Config::default(),
    );
    ifs.ensure_layout(&e).unwrap();
    let p = user("QSECOFR");
    let audit = MemAudit::default();
    let cx = Ctx {
        owner: Owner(1),
        principal: &p,
        env: &e,
        audit: &audit,
        job_ccsid: 37,
    };
    ifs.mkdir(&cx, "/home/a", 0o755).unwrap();
    let fd = ifs.creat(&cx, "/home/a/f", 0o644).unwrap();
    ifs.close(&cx, fd).unwrap();
    ifs.mkdir(&cx, "/home/b", 0o755).unwrap();
    let s = ifs.stat(&cx, "/home/a/f").unwrap();
    // A new object's generation is the backend's.
    assert_eq!(s.ino_gen, gen_of(s.ino));
    let id = FileId {
        fs: s.vfs,
        ino: s.ino,
        generation: s.ino_gen,
    };
    // The backend names it: no walk, nothing remembered.
    *h.answer.lock().unwrap() = Some("root/home/a/f".into());
    ifs.forget_ids();
    assert_eq!(ifs.stat_by_id(&cx, id).unwrap().0, "/home/a/f");
    assert_eq!(ifs.id_cache_len(), 1, "only the object itself");
    // It names another object, or nothing: the walk finds it.
    for wrong in [
        Some("root/home/b".to_string()),
        None,
        Some("elsewhere/x".into()),
    ] {
        *h.answer.lock().unwrap() = wrong;
        ifs.forget_ids();
        assert_eq!(ifs.stat_by_id(&cx, id).unwrap().0, "/home/a/f");
        assert!(ifs.id_cache_len() > 1, "the walk passed others");
    }
    // Another generation never opens through the handle.
    *h.answer.lock().unwrap() = Some("root/home/a/f".into());
    let other = FileId {
        generation: id.generation ^ 1,
        ..id
    };
    assert_eq!(ifs.stat_by_id(&cx, other).err(), Some(Errno::ESTALE));
    // Generation 0 (no generation) does not ask the backend.
    let asked = *h.asked.lock().unwrap();
    let _ = ifs.stat_by_id(
        &cx,
        FileId {
            generation: 0,
            ..id
        },
    );
    assert_eq!(*h.asked.lock().unwrap(), asked);
}
