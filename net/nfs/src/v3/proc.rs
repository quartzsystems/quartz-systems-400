// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The procedures but the directory reads ([`super::dir`]).

use super::conv::{fattr, fh3, no_wcc, node, post, sattr, status, time_of, wcc};
use super::{MAX_IO, PREF_IO};
use crate::rpc::CallContext;
use crate::server::gate::writable;
use crate::server::vfs::{CreateHow, Special, Stable};
use crate::server::{FileHandle, Gate, Req, Vfs, VfsError};
use crate::types::nfs3::{
    ACCESS3args, ACCESS3res, ACCESS3resfail, ACCESS3resok, COMMIT3args, COMMIT3res,
    COMMIT3resfail, COMMIT3resok, CREATE3args, CREATE3res, CREATE3resfail, CREATE3resok,
    FSF3_CANSETTIME, FSF3_HOMOGENEOUS, FSF3_LINK, FSF3_SYMLINK, FSINFO3args, FSINFO3res,
    FSINFO3resfail, FSINFO3resok, FSSTAT3args, FSSTAT3res, FSSTAT3resfail, FSSTAT3resok,
    GETATTR3args, GETATTR3res, GETATTR3resok, LINK3args, LINK3res, LINK3resfail, LINK3resok,
    LOOKUP3args, LOOKUP3res, LOOKUP3resfail, LOOKUP3resok, MKDIR3args, MKDIR3res, MKDIR3resfail,
    MKDIR3resok, MKNOD3args, MKNOD3res, MKNOD3resfail, MKNOD3resok, PATHCONF3args, PATHCONF3res,
    PATHCONF3resfail, PATHCONF3resok, READ3args, READ3res, READ3resfail, READ3resok,
    READLINK3args, READLINK3res, READLINK3resfail, READLINK3resok, REMOVE3args, REMOVE3res,
    REMOVE3resfail, REMOVE3resok, RENAME3args, RENAME3res, RENAME3resfail, RENAME3resok,
    RMDIR3args, RMDIR3res, RMDIR3resfail, RMDIR3resok, SETATTR3args, SETATTR3res,
    SETATTR3resfail, SETATTR3resok, SYMLINK3args, SYMLINK3res, SYMLINK3resfail, SYMLINK3resok,
    WRITE3args, WRITE3res, WRITE3resfail, WRITE3resok, createhow3, mknoddata3, nfspath3,
    nfstime3, post_op_attr, stable_how,
};

/// A call's context.
pub(super) struct Procs<'a, V: Vfs + ?Sized> {
    pub vfs: &'a V,
    pub gate: &'a Gate,
    pub ctx: &'a CallContext,
}

impl<V: Vfs + ?Sized> Procs<'_, V> {
    pub(super) fn open(&self, fh: &[u8]) -> Result<(Req, FileHandle), VfsError> {
        self.gate.open(self.ctx, fh, 3)
    }

    /// Like [`Self::open`], and the export must be writable for the client.
    fn open_w(&self, fh: &[u8]) -> Result<(Req, FileHandle), VfsError> {
        let (r, h) = self.open(fh)?;
        if !writable(&r) {
            return Err(VfsError::RoFs);
        }
        Ok((r, h))
    }

    /// The attributes after a failure, when they can be had.
    fn after(&self, r: Option<&Req>, h: Option<&FileHandle>) -> post_op_attr {
        match (r, h) {
            (Some(r), Some(h)) => self.vfs.getattr(r, h).ok().as_ref().map(fattr),
            _ => None,
        }
    }

    /// The directory's `wcc_data` after a failure.
    fn dir_wcc(&self, fh: &[u8]) -> crate::types::nfs3::wcc_data {
        match self.open(fh) {
            Ok((r, h)) => crate::types::nfs3::wcc_data {
                before: None,
                after: self.after(Some(&r), Some(&h)),
            },
            Err(_) => no_wcc(),
        }
    }

    pub(super) fn getattr(&self, a: &GETATTR3args) -> GETATTR3res {
        match self.open(&a.object.data).and_then(|(r, h)| self.vfs.getattr(&r, &h)) {
            Ok(at) => GETATTR3res::NFS3_OK(GETATTR3resok {
                obj_attributes: fattr(&at),
            }),
            Err(e) => GETATTR3res::default(status(e)),
        }
    }

    pub(super) fn setattr(&self, a: &SETATTR3args) -> SETATTR3res {
        let s = sattr(&a.new_attributes);
        let guard = a.guard.as_ref().map(time_of);
        match self
            .open_w(&a.object.data)
            .and_then(|(r, h)| self.vfs.setattr(&r, &h, &s, guard))
        {
            Ok(w) => SETATTR3res::NFS3_OK(SETATTR3resok { obj_wcc: wcc(&w) }),
            Err(e) => SETATTR3res::default(
                status(e),
                SETATTR3resfail {
                    obj_wcc: self.dir_wcc(&a.object.data),
                },
            ),
        }
    }

    pub(super) fn lookup(&self, a: &LOOKUP3args) -> LOOKUP3res {
        let o = self.open(&a.what.dir.data);
        match o.as_ref().map_err(|e| *e).and_then(|(r, h)| self.vfs.lookup(r, h, &a.what.name.0)) {
            Ok((n, d)) => LOOKUP3res::NFS3_OK(LOOKUP3resok {
                object: fh3(&n.fh),
                obj_attributes: Some(fattr(&n.attr)),
                dir_attributes: post(d.as_ref()),
            }),
            Err(e) => LOOKUP3res::default(
                status(e),
                LOOKUP3resfail {
                    dir_attributes: o.ok().and_then(|(r, h)| self.after(Some(&r), Some(&h))),
                },
            ),
        }
    }

    pub(super) fn access(&self, a: &ACCESS3args) -> ACCESS3res {
        let o = self.open(&a.object.data);
        match o.as_ref().map_err(|e| *e).and_then(|(r, h)| self.vfs.access(r, h, a.access)) {
            Ok((got, at)) => ACCESS3res::NFS3_OK(ACCESS3resok {
                obj_attributes: post(at.as_ref()),
                access: got,
            }),
            Err(e) => ACCESS3res::default(
                status(e),
                ACCESS3resfail {
                    obj_attributes: o.ok().and_then(|(r, h)| self.after(Some(&r), Some(&h))),
                },
            ),
        }
    }

    pub(super) fn readlink(&self, a: &READLINK3args) -> READLINK3res {
        let o = self.open(&a.symlink.data);
        match o.as_ref().map_err(|e| *e).and_then(|(r, h)| self.vfs.readlink(r, h)) {
            Ok((t, at)) => READLINK3res::NFS3_OK(READLINK3resok {
                symlink_attributes: post(at.as_ref()),
                data: nfspath3(t),
            }),
            Err(e) => READLINK3res::default(
                status(e),
                READLINK3resfail {
                    symlink_attributes: o.ok().and_then(|(r, h)| self.after(Some(&r), Some(&h))),
                },
            ),
        }
    }

    pub(super) fn read(&self, a: &READ3args) -> READ3res {
        let o = self.open(&a.file.data);
        let count = a.count.min(MAX_IO);
        match o.as_ref().map_err(|e| *e).and_then(|(r, h)| self.vfs.read(r, h, a.offset, count)) {
            Ok(d) => READ3res::NFS3_OK(READ3resok {
                file_attributes: post(d.attr.as_ref()),
                count: u32::try_from(d.data.len()).unwrap_or(u32::MAX),
                eof: d.eof,
                data: d.data,
            }),
            Err(e) => READ3res::default(
                status(e),
                READ3resfail {
                    file_attributes: o.ok().and_then(|(r, h)| self.after(Some(&r), Some(&h))),
                },
            ),
        }
    }

    pub(super) fn write(&self, a: &WRITE3args) -> WRITE3res {
        let n = usize::try_from(a.count).unwrap_or(usize::MAX);
        let stable = match a.stable {
            stable_how::UNSTABLE => Stable::Unstable,
            stable_how::DATA_SYNC => Stable::DataSync,
            stable_how::FILE_SYNC => Stable::FileSync,
        };
        let res = if a.count > MAX_IO {
            Err(VfsError::FBig)
        } else if a.data.len() < n {
            Err(VfsError::Inval)
        } else {
            self.open_w(&a.file.data)
        };
        let res = res.and_then(|(r, h)| self.vfs.write(&r, &h, a.offset, &a.data[..n], stable));
        match res {
            Ok(w) => WRITE3res::NFS3_OK(WRITE3resok {
                file_wcc: wcc(&w.wcc),
                count: w.count,
                committed: match w.committed {
                    Stable::Unstable => stable_how::UNSTABLE,
                    Stable::DataSync => stable_how::DATA_SYNC,
                    Stable::FileSync => stable_how::FILE_SYNC,
                },
                verf: w.verf,
            }),
            Err(e) => WRITE3res::default(
                status(e),
                WRITE3resfail {
                    file_wcc: self.dir_wcc(&a.file.data),
                },
            ),
        }
    }

    pub(super) fn create(&self, a: &CREATE3args) -> CREATE3res {
        let how = match &a.how {
            createhow3::UNCHECKED(s) => CreateHow::Unchecked(sattr(s)),
            createhow3::GUARDED(s) => CreateHow::Guarded(sattr(s)),
            createhow3::EXCLUSIVE(v) => CreateHow::Exclusive(*v),
        };
        match self
            .open_w(&a.r#where.dir.data)
            .and_then(|(r, h)| self.vfs.create(&r, &h, &a.r#where.name.0, how))
        {
            Ok(m) => {
                let (obj, obj_attributes) = node(m.node.as_ref());
                CREATE3res::NFS3_OK(CREATE3resok {
                    obj,
                    obj_attributes,
                    dir_wcc: wcc(&m.dir),
                })
            }
            Err(e) => CREATE3res::default(
                status(e),
                CREATE3resfail {
                    dir_wcc: self.dir_wcc(&a.r#where.dir.data),
                },
            ),
        }
    }

    pub(super) fn mkdir(&self, a: &MKDIR3args) -> MKDIR3res {
        let s = sattr(&a.attributes);
        match self
            .open_w(&a.r#where.dir.data)
            .and_then(|(r, h)| self.vfs.mkdir(&r, &h, &a.r#where.name.0, &s))
        {
            Ok(m) => {
                let (obj, obj_attributes) = node(m.node.as_ref());
                MKDIR3res::NFS3_OK(MKDIR3resok {
                    obj,
                    obj_attributes,
                    dir_wcc: wcc(&m.dir),
                })
            }
            Err(e) => MKDIR3res::default(
                status(e),
                MKDIR3resfail {
                    dir_wcc: self.dir_wcc(&a.r#where.dir.data),
                },
            ),
        }
    }

    pub(super) fn symlink(&self, a: &SYMLINK3args) -> SYMLINK3res {
        let s = sattr(&a.symlink.symlink_attributes);
        match self.open_w(&a.r#where.dir.data).and_then(|(r, h)| {
            self.vfs
                .symlink(&r, &h, &a.r#where.name.0, &a.symlink.symlink_data.0, &s)
        }) {
            Ok(m) => {
                let (obj, obj_attributes) = node(m.node.as_ref());
                SYMLINK3res::NFS3_OK(SYMLINK3resok {
                    obj,
                    obj_attributes,
                    dir_wcc: wcc(&m.dir),
                })
            }
            Err(e) => SYMLINK3res::default(
                status(e),
                SYMLINK3resfail {
                    dir_wcc: self.dir_wcc(&a.r#where.dir.data),
                },
            ),
        }
    }

    pub(super) fn mknod(&self, a: &MKNOD3args) -> MKNOD3res {
        let what = match &a.what {
            mknoddata3::NF3FIFO(s) => Ok((Special::Fifo, sattr(s))),
            mknoddata3::NF3SOCK(s) => Ok((Special::Sock, sattr(s))),
            mknoddata3::NF3CHR(d) => Ok((
                Special::Chr(d.spec.specdata1, d.spec.specdata2),
                sattr(&d.dev_attributes),
            )),
            mknoddata3::NF3BLK(d) => Ok((
                Special::Blk(d.spec.specdata1, d.spec.specdata2),
                sattr(&d.dev_attributes),
            )),
            mknoddata3::default(_) => Err(VfsError::BadType),
        };
        let res = what.and_then(|(w, s)| {
            self.open_w(&a.r#where.dir.data)
                .and_then(|(r, h)| self.vfs.mknod(&r, &h, &a.r#where.name.0, w, &s))
        });
        match res {
            Ok(m) => {
                let (obj, obj_attributes) = node(m.node.as_ref());
                MKNOD3res::NFS3_OK(MKNOD3resok {
                    obj,
                    obj_attributes,
                    dir_wcc: wcc(&m.dir),
                })
            }
            Err(e) => MKNOD3res::default(
                status(e),
                MKNOD3resfail {
                    dir_wcc: self.dir_wcc(&a.r#where.dir.data),
                },
            ),
        }
    }

    pub(super) fn remove(&self, a: &REMOVE3args) -> REMOVE3res {
        match self
            .open_w(&a.object.dir.data)
            .and_then(|(r, h)| self.vfs.remove(&r, &h, &a.object.name.0))
        {
            Ok(w) => REMOVE3res::NFS3_OK(REMOVE3resok { dir_wcc: wcc(&w) }),
            Err(e) => REMOVE3res::default(
                status(e),
                REMOVE3resfail {
                    dir_wcc: self.dir_wcc(&a.object.dir.data),
                },
            ),
        }
    }

    pub(super) fn rmdir(&self, a: &RMDIR3args) -> RMDIR3res {
        match self
            .open_w(&a.object.dir.data)
            .and_then(|(r, h)| self.vfs.rmdir(&r, &h, &a.object.name.0))
        {
            Ok(w) => RMDIR3res::NFS3_OK(RMDIR3resok { dir_wcc: wcc(&w) }),
            Err(e) => RMDIR3res::default(
                status(e),
                RMDIR3resfail {
                    dir_wcc: self.dir_wcc(&a.object.dir.data),
                },
            ),
        }
    }

    pub(super) fn rename(&self, a: &RENAME3args) -> RENAME3res {
        let res = self.open_w(&a.from.dir.data).and_then(|(r, from)| {
            let to = self.gate.second(&r, &a.to.dir.data)?;
            self.vfs
                .rename(&r, &from, &a.from.name.0, &to, &a.to.name.0)
        });
        match res {
            Ok((f, t)) => RENAME3res::NFS3_OK(RENAME3resok {
                fromdir_wcc: wcc(&f),
                todir_wcc: wcc(&t),
            }),
            Err(e) => RENAME3res::default(
                status(e),
                RENAME3resfail {
                    fromdir_wcc: self.dir_wcc(&a.from.dir.data),
                    todir_wcc: self.dir_wcc(&a.to.dir.data),
                },
            ),
        }
    }

    pub(super) fn link(&self, a: &LINK3args) -> LINK3res {
        let res = self.open_w(&a.file.data).and_then(|(r, file)| {
            let dir = self.gate.second(&r, &a.link.dir.data)?;
            self.vfs.link(&r, &file, &dir, &a.link.name.0)
        });
        match res {
            Ok((fa, w)) => LINK3res::NFS3_OK(LINK3resok {
                file_attributes: post(fa.as_ref()),
                linkdir_wcc: wcc(&w),
            }),
            Err(e) => {
                let fa = self
                    .open(&a.file.data)
                    .ok()
                    .and_then(|(r, h)| self.after(Some(&r), Some(&h)));
                LINK3res::default(
                    status(e),
                    LINK3resfail {
                        file_attributes: fa,
                        linkdir_wcc: self.dir_wcc(&a.link.dir.data),
                    },
                )
            }
        }
    }

    pub(super) fn fsstat(&self, a: &FSSTAT3args) -> FSSTAT3res {
        let o = self.open(&a.fsroot.data);
        match o.as_ref().map_err(|e| *e).and_then(|(r, h)| self.vfs.fsstat(r, h)) {
            Ok((s, at)) => FSSTAT3res::NFS3_OK(FSSTAT3resok {
                obj_attributes: post(at.as_ref()),
                tbytes: s.tbytes,
                fbytes: s.fbytes,
                abytes: s.abytes,
                tfiles: s.tfiles,
                ffiles: s.ffiles,
                afiles: s.afiles,
                invarsec: 0,
            }),
            Err(e) => FSSTAT3res::default(
                status(e),
                FSSTAT3resfail {
                    obj_attributes: o.ok().and_then(|(r, h)| self.after(Some(&r), Some(&h))),
                },
            ),
        }
    }

    pub(super) fn fsinfo(&self, a: &FSINFO3args) -> FSINFO3res {
        match self.open(&a.fsroot.data).and_then(|(r, h)| self.vfs.getattr(&r, &h)) {
            Ok(at) => FSINFO3res::NFS3_OK(FSINFO3resok {
                obj_attributes: Some(fattr(&at)),
                rtmax: MAX_IO,
                rtpref: PREF_IO,
                rtmult: 4096,
                wtmax: MAX_IO,
                wtpref: PREF_IO,
                wtmult: 4096,
                dtpref: PREF_IO,
                maxfilesize: i64::MAX.unsigned_abs(),
                time_delta: nfstime3 {
                    seconds: 0,
                    nseconds: 1,
                },
                properties: FSF3_LINK | FSF3_SYMLINK | FSF3_HOMOGENEOUS | FSF3_CANSETTIME,
            }),
            Err(e) => FSINFO3res::default(status(e), FSINFO3resfail { obj_attributes: None }),
        }
    }

    pub(super) fn pathconf(&self, a: &PATHCONF3args) -> PATHCONF3res {
        let o = self.open(&a.object.data);
        match o.as_ref().map_err(|e| *e).and_then(|(r, h)| self.vfs.pathconf(r, h)) {
            Ok((p, at)) => PATHCONF3res::NFS3_OK(PATHCONF3resok {
                obj_attributes: post(at.as_ref()),
                linkmax: p.linkmax,
                name_max: p.name_max,
                no_trunc: p.no_trunc,
                chown_restricted: p.chown_restricted,
                case_insensitive: p.case_insensitive,
                case_preserving: p.case_preserving,
            }),
            Err(e) => PATHCONF3res::default(
                status(e),
                PATHCONF3resfail {
                    obj_attributes: o.ok().and_then(|(r, h)| self.after(Some(&r), Some(&h))),
                },
            ),
        }
    }

    pub(super) fn commit(&self, a: &COMMIT3args) -> COMMIT3res {
        match self
            .open(&a.file.data)
            .and_then(|(r, h)| self.vfs.commit(&r, &h, a.offset, a.count))
        {
            Ok((verf, w)) => COMMIT3res::NFS3_OK(COMMIT3resok {
                file_wcc: wcc(&w),
                verf,
            }),
            Err(e) => COMMIT3res::default(
                status(e),
                COMMIT3resfail {
                    file_wcc: self.dir_wcc(&a.file.data),
                },
            ),
        }
    }
}
