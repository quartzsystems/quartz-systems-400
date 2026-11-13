// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS version 3 and MOUNT calls over `nfs-v3`'s and `nfs-mount`'s wire
//! types, and the scratch directory each scenario works in.

use nfs_mount::wire::{dirpath, exports, mountlist, mountres3, mountres3_ok, mountstat3};
use nfs_v3::wire::{
    CREATE3args, CREATE3res, GETATTR3args, GETATTR3res, LOOKUP3args, LOOKUP3res, MKDIR3args,
    MKDIR3res, NFS_PROGRAM as P, Nfs3Option, Nfs3Result, READ3args, READ3res, READDIRPLUS3args,
    READDIRPLUS3res, REMOVE3args, REMOVE3res, RMDIR3args, RMDIR3res, SETATTR3args, SETATTR3res,
    WRITE3args, WRITE3res, cookieverf3, createhow3, diropargs3, fattr3, filename3, ftype3, nfs_fh3,
    nfsstat3, post_op_fh3, sattr3, sattrguard3, stable_how,
};
use onc_xdr::{Opaque, Pack, Unpack};

use crate::ctx::{Ctx, prog};
use crate::rpc::{Answer, Client};
use crate::{Error, error::fail};

/// A handle's bytes as the wire type.
pub fn fh(b: &[u8]) -> nfs_fh3 {
    nfs_fh3 {
        data: Opaque::owned(b.to_vec()),
    }
}

/// `dir` and `name` as `diropargs3`.
pub fn dirop<'a>(dir: &[u8], name: &'a [u8]) -> diropargs3<'a> {
    diropargs3 {
        dir: fh(dir),
        name: filename3(Opaque::borrowed(name)),
    }
}

/// Attributes that set the mode only.
pub fn mode(m: u32) -> sattr3 {
    sattr3 {
        mode: Nfs3Option::Some(m),
        ..sattr3::default()
    }
}

/// The status of a result.
pub fn status<T, E>(r: &Nfs3Result<T, E>) -> nfsstat3 {
    match r {
        Nfs3Result::Ok(_) => nfsstat3::NFS3_OK,
        Nfs3Result::Err((s, _)) => *s,
        _ => nfsstat3::NFS3ERR_SERVERFAULT,
    }
}

/// A result as `Result`: the value or the status.
pub fn split<T, E>(r: Nfs3Result<T, E>) -> Result<T, nfsstat3> {
    match r {
        Nfs3Result::Ok(v) => Ok(v),
        Nfs3Result::Err((s, _)) => Err(s),
        _ => Err(nfsstat3::NFS3ERR_SERVERFAULT),
    }
}

/// An `Nfs3Option` as `Option`.
pub fn opt<T: Pack + Unpack>(o: &Nfs3Option<T>) -> Option<&T> {
    match o {
        Nfs3Option::Some(v) => Some(v),
        _ => None,
    }
}

/// A `MNT` result as `Result`.
pub fn mnt_split(r: mountres3<'static>) -> Result<mountres3_ok<'static>, mountstat3> {
    match r {
        mountres3::Ok(v) => Ok(v),
        mountres3::Err(s) => Err(s),
        _ => Err(mountstat3::MNT3ERR_SERVERFAULT),
    }
}

/// The value of a result, or a check failure naming `what`.
pub fn ok<T, E>(r: Nfs3Result<T, E>, what: &str) -> Result<T, Error> {
    split(r).or_else(|s| fail(format!("{what}: {s}")))
}

/// Fails unless `r` failed with `want`.
pub fn expect_err<T, E>(r: &Nfs3Result<T, E>, want: nfsstat3, what: &str) -> Result<(), Error> {
    let got = status(r);
    if got == want {
        Ok(())
    } else {
        fail(format!("{what}: expected {want}, got {got}"))
    }
}

/// The handle in a `post_op_fh3`.
pub fn handle_of(p: &post_op_fh3) -> Result<Vec<u8>, Error> {
    match opt(p) {
        Some(h) => Ok(h.data.to_vec()),
        None => fail("no handle in the reply"),
    }
}

/// A directory entry: name, type, handle.
pub type Entry = (Vec<u8>, Option<ftype3>, Vec<u8>);

/// A mount list entry: host, directory.
pub type Mounted = (Vec<u8>, Vec<u8>);

/// An export: directory, groups.
pub type Exported = (Vec<u8>, Vec<Vec<u8>>);

/// An NFS version 3 client.
#[derive(Debug)]
pub struct Nfs3 {
    /// The RPC client.
    pub c: Client,
}

impl Nfs3 {
    /// A call of procedure `p`.
    pub fn call<A: Pack, R: Unpack>(&mut self, p: P, a: &A) -> Result<R, Error> {
        self.c.call(prog::NFS, 3, p as u32, a)
    }

    /// A call of procedure `p` with raw argument bytes.
    pub fn call_bytes(&mut self, p: u32, args: &[u8]) -> Result<Answer, Error> {
        self.c.call_bytes(prog::NFS, 3, p, args)
    }

    /// `GETATTR`.
    pub fn getattr_res(&mut self, f: &[u8]) -> Result<GETATTR3res, Error> {
        self.call(P::NFSPROC3_GETATTR, &GETATTR3args { object: fh(f) })
    }

    /// `GETATTR`, which must succeed.
    pub fn getattr(&mut self, f: &[u8]) -> Result<fattr3, Error> {
        Ok(ok(self.getattr_res(f)?, "GETATTR")?.obj_attributes)
    }

    /// `SETATTR`.
    pub fn setattr(
        &mut self,
        f: &[u8],
        a: sattr3,
        guard: sattrguard3,
    ) -> Result<SETATTR3res, Error> {
        self.call(
            P::NFSPROC3_SETATTR,
            &SETATTR3args {
                object: fh(f),
                new_attributes: a,
                guard,
            },
        )
    }

    /// `LOOKUP`.
    pub fn lookup_res(&mut self, dir: &[u8], name: &[u8]) -> Result<LOOKUP3res, Error> {
        self.call(
            P::NFSPROC3_LOOKUP,
            &LOOKUP3args {
                what: dirop(dir, name),
            },
        )
    }

    /// `LOOKUP`, which must succeed: the handle.
    pub fn lookup(&mut self, dir: &[u8], name: &[u8]) -> Result<Vec<u8>, Error> {
        let r = ok(
            self.lookup_res(dir, name)?,
            &format!("LOOKUP {}", String::from_utf8_lossy(name)),
        )?;
        Ok(r.object.data.to_vec())
    }

    /// `CREATE`.
    pub fn create_res(
        &mut self,
        dir: &[u8],
        name: &[u8],
        how: createhow3,
    ) -> Result<CREATE3res, Error> {
        self.call(
            P::NFSPROC3_CREATE,
            &CREATE3args {
                where_: dirop(dir, name),
                how,
            },
        )
    }

    /// A file made `GUARDED` with `mode`: its handle.
    pub fn create(&mut self, dir: &[u8], name: &[u8], m: u32) -> Result<Vec<u8>, Error> {
        let r = self.create_res(dir, name, createhow3::GUARDED(mode(m)))?;
        let r = ok(r, &format!("CREATE {}", String::from_utf8_lossy(name)))?;
        match handle_of(&r.obj) {
            Ok(h) => Ok(h),
            Err(_) => self.lookup(dir, name),
        }
    }

    /// `MKDIR`.
    pub fn mkdir_res(&mut self, dir: &[u8], name: &[u8], m: u32) -> Result<MKDIR3res, Error> {
        self.call(
            P::NFSPROC3_MKDIR,
            &MKDIR3args {
                where_: dirop(dir, name),
                attributes: mode(m),
            },
        )
    }

    /// A directory: its handle.
    pub fn mkdir(&mut self, dir: &[u8], name: &[u8]) -> Result<Vec<u8>, Error> {
        let r = ok(
            self.mkdir_res(dir, name, 0o755)?,
            &format!("MKDIR {}", String::from_utf8_lossy(name)),
        )?;
        match handle_of(&r.obj) {
            Ok(h) => Ok(h),
            Err(_) => self.lookup(dir, name),
        }
    }

    /// `WRITE`.
    pub fn write_res(
        &mut self,
        f: &[u8],
        off: u64,
        data: &[u8],
        stable: stable_how,
    ) -> Result<WRITE3res, Error> {
        self.call(
            P::NFSPROC3_WRITE,
            &WRITE3args {
                file: fh(f),
                offset: off,
                count: u32::try_from(data.len()).expect("short"),
                stable,
                data: Opaque::borrowed(data),
            },
        )
    }

    /// A `FILE_SYNC` write that must take every byte.
    pub fn write(&mut self, f: &[u8], off: u64, data: &[u8]) -> Result<(), Error> {
        let r = ok(
            self.write_res(f, off, data, stable_how::FILE_SYNC)?,
            "WRITE",
        )?;
        if r.count as usize != data.len() {
            return fail(format!("WRITE took {} of {}", r.count, data.len()));
        }
        Ok(())
    }

    /// `READ`.
    pub fn read_res(&mut self, f: &[u8], off: u64, count: u32) -> Result<READ3res<'static>, Error> {
        self.call(
            P::NFSPROC3_READ,
            &READ3args {
                file: fh(f),
                offset: off,
                count,
            },
        )
    }

    /// `READ`, which must succeed: the data and EOF.
    pub fn read(&mut self, f: &[u8], off: u64, count: u32) -> Result<(Vec<u8>, bool), Error> {
        let r = ok(self.read_res(f, off, count)?, "READ")?;
        Ok((r.data.to_vec(), r.eof))
    }

    /// `REMOVE`.
    pub fn remove_res(&mut self, dir: &[u8], name: &[u8]) -> Result<REMOVE3res, Error> {
        self.call(
            P::NFSPROC3_REMOVE,
            &REMOVE3args {
                object: dirop(dir, name),
            },
        )
    }

    /// `RMDIR`.
    pub fn rmdir_res(&mut self, dir: &[u8], name: &[u8]) -> Result<RMDIR3res, Error> {
        self.call(
            P::NFSPROC3_RMDIR,
            &RMDIR3args {
                object: dirop(dir, name),
            },
        )
    }

    /// `READDIRPLUS` from `cookie`.
    pub fn readdirplus(
        &mut self,
        dir: &[u8],
        cookie: u64,
        verf: [u8; 8],
        dircount: u32,
        maxcount: u32,
    ) -> Result<READDIRPLUS3res<'static>, Error> {
        self.call(
            P::NFSPROC3_READDIRPLUS,
            &READDIRPLUS3args {
                dir: fh(dir),
                cookie,
                cookieverf: cookieverf3(verf),
                dircount,
                maxcount,
            },
        )
    }

    /// Every entry of `dir` (but `.` and `..`): name, type, handle.
    pub fn list(&mut self, dir: &[u8]) -> Result<Vec<Entry>, Error> {
        let mut out = Vec::new();
        let (mut cookie, mut verf) = (0, [0; 8]);
        for _ in 0..100_000 {
            let r = ok(
                self.readdirplus(dir, cookie, verf, 8192, 65536)?,
                "READDIRPLUS",
            )?;
            verf = r.cookieverf.0;
            for e in &r.reply.entries.0 {
                cookie = e.cookie;
                let name = e.name.0.to_vec();
                if name == b"." || name == b".." {
                    continue;
                }
                let ty = opt(&e.name_attributes).map(|a| a.type_);
                let h = opt(&e.name_handle)
                    .map(|h| h.data.to_vec())
                    .unwrap_or_default();
                out.push((name, ty, h));
            }
            if r.reply.eof {
                return Ok(out);
            }
            if r.reply.entries.0.is_empty() {
                return fail("READDIRPLUS: an empty page that is not the end");
            }
        }
        fail("READDIRPLUS never reached the end")
    }

    /// Removes `name` in `dir` and everything under it.
    pub fn remove_tree(&mut self, dir: &[u8], name: &[u8]) -> Result<(), Error> {
        let h = match split(self.lookup_res(dir, name)?) {
            Ok(r) => r.object.data.to_vec(),
            Err(nfsstat3::NFS3ERR_NOENT) => return Ok(()),
            Err(s) => return fail(format!("LOOKUP for removal: {s}")),
        };
        let a = self.getattr(&h)?;
        if a.type_ == ftype3::NF3DIR {
            for (n, ty, _) in self.list(&h)? {
                if ty == Some(ftype3::NF3DIR) {
                    self.remove_tree(&h, &n)?;
                } else {
                    ok(self.remove_res(&h, &n)?, "REMOVE")?;
                }
            }
            ok(self.rmdir_res(dir, name)?, "RMDIR")?;
        } else {
            ok(self.remove_res(dir, name)?, "REMOVE")?;
        }
        Ok(())
    }
}

/// A MOUNT client (version 3).
#[derive(Debug)]
pub struct Mount {
    /// The RPC client.
    pub c: Client,
}

impl Mount {
    /// `MNT`.
    pub fn mnt(&mut self, path: &[u8]) -> Result<mountres3<'static>, Error> {
        self.c
            .call(prog::MOUNT, 3, 1, &dirpath(Opaque::borrowed(path)))
    }

    /// `MNT`, which must succeed: the root handle and the flavors.
    pub fn mount(&mut self, path: &[u8]) -> Result<(Vec<u8>, Vec<u32>), Error> {
        match mnt_split(self.mnt(path)?) {
            Ok(r) => Ok((r.fhandle.0.to_vec(), r.auth_flavors)),
            Err(s) => fail(format!("MNT {}: {s}", String::from_utf8_lossy(path))),
        }
    }

    /// `UMNT`.
    pub fn umnt(&mut self, path: &[u8]) -> Result<(), Error> {
        self.c
            .call::<_, onc_xdr::Void>(prog::MOUNT, 3, 3, &dirpath(Opaque::borrowed(path)))?;
        Ok(())
    }

    /// `DUMP`: host and directory.
    pub fn dump(&mut self) -> Result<Vec<Mounted>, Error> {
        let l: mountlist<'static, 'static> = self.c.call(prog::MOUNT, 3, 2, &onc_xdr::Void)?;
        Ok(l.0
            .into_iter()
            .map(|m| (m.ml_hostname.0.to_vec(), m.ml_directory.0.to_vec()))
            .collect())
    }

    /// `EXPORT`: directory and groups.
    pub fn export(&mut self) -> Result<Vec<Exported>, Error> {
        let l: exports<'static, 'static> = self.c.call(prog::MOUNT, 3, 5, &onc_xdr::Void)?;
        Ok(l.0
            .into_iter()
            .map(|e| {
                (
                    e.ex_dir.0.to_vec(),
                    e.ex_groups.0.into_iter().map(|g| g.0.to_vec()).collect(),
                )
            })
            .collect())
    }
}

/// A scenario's own directory under the scratch directory, removed at the end.
#[derive(Debug)]
pub struct Session {
    /// The NFS client.
    pub n: Nfs3,
    /// The export's root.
    pub root: Vec<u8>,
    /// The scratch directory.
    pub scratch: Vec<u8>,
    /// This scenario's directory.
    pub dir: Vec<u8>,
    name: Vec<u8>,
    kept: bool,
}

impl Session {
    /// Mounts the export (or takes `--port`'s server with MOUNT), makes
    /// `SCRATCH/<name>-<pid>`.
    pub fn open(c: &Ctx, name: &str) -> Result<Session, Error> {
        let root = root_handle(c)?;
        let mut n = Nfs3 { c: c.client(c.nfs) };
        let scratch = walk_or_make(&mut n, &root, &c.t.scratch)?;
        let own = format!("{name}-{}", std::process::id()).into_bytes();
        n.remove_tree(&scratch, &own)?;
        let dir = n.mkdir(&scratch, &own)?;
        Ok(Session {
            n,
            root,
            scratch,
            dir,
            name: own,
            kept: false,
        })
    }

    /// Leaves the directory in place (to look at after a failure).
    pub fn keep(&mut self) {
        self.kept = true;
    }

    /// Removes the directory.
    pub fn close(mut self) -> Result<(), Error> {
        self.kept = true;
        let (s, n) = (self.scratch.clone(), self.name.clone());
        self.n.remove_tree(&s, &n)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if !self.kept {
            let (s, n) = (self.scratch.clone(), self.name.clone());
            let _ = self.n.remove_tree(&s, &n);
        }
    }
}

/// The export's root handle through MOUNT.
pub fn root_handle(c: &Ctx) -> Result<Vec<u8>, Error> {
    let mut m = Mount {
        c: c.client(c.mount_addr()?),
    };
    Ok(m.mount(c.t.export.as_bytes())?.0)
}

/// Looks up each component of `path` under `root`, making the missing ones.
pub fn walk_or_make(n: &mut Nfs3, root: &[u8], path: &str) -> Result<Vec<u8>, Error> {
    let mut h = root.to_vec();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        h = match split(n.lookup_res(&h, part.as_bytes())?) {
            Ok(r) => r.object.data.to_vec(),
            Err(nfsstat3::NFS3ERR_NOENT) => n.mkdir(&h, part.as_bytes())?,
            Err(s) => return fail(format!("LOOKUP {part}: {s}")),
        };
    }
    Ok(h)
}
