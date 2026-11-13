// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every call of the remote file system as COMPOUNDs.

use ifs::Errno;
use ifs::remote::{
    CreateMode, Cred, DirPage, FsStat, LockConflict, LockKind, LockOwner, OpenToken, PathConf,
    RAttr, RHandle, RKind, RResult, RemoteError, RemoteFs, SetAttr, Share, Stable, WriteReply,
};

use super::V4;

fn no<T>() -> RResult<T> {
    Err(RemoteError::Errno(Errno::ENOTSUP))
}

#[allow(unused_variables)]
impl RemoteFs for V4 {
    fn root(&self) -> RResult<(RHandle, RAttr)> {
        no()
    }
    fn getattr(&self, cred: &Cred, h: &RHandle) -> RResult<RAttr> {
        no()
    }
    fn setattr(
        &self,
        cred: &Cred,
        h: &RHandle,
        set: &SetAttr,
        guard: Option<i64>,
    ) -> RResult<RAttr> {
        no()
    }
    fn lookup(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<(RHandle, RAttr)> {
        no()
    }
    fn access(&self, cred: &Cred, h: &RHandle, mask: u32) -> RResult<u32> {
        no()
    }
    fn readlink(&self, cred: &Cred, h: &RHandle) -> RResult<Vec<u8>> {
        no()
    }
    fn read(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<(Vec<u8>, bool)> {
        no()
    }
    fn write(
        &self,
        cred: &Cred,
        h: &RHandle,
        off: u64,
        data: &[u8],
        stable: Stable,
    ) -> RResult<WriteReply> {
        no()
    }
    fn commit(&self, cred: &Cred, h: &RHandle, off: u64, len: u32) -> RResult<u64> {
        no()
    }
    fn create(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        how: CreateMode,
    ) -> RResult<(RHandle, RAttr)> {
        no()
    }
    fn mkdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        no()
    }
    fn symlink(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        target: &[u8],
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        no()
    }
    fn mknod(
        &self,
        cred: &Cred,
        dir: &RHandle,
        name: &[u8],
        kind: RKind,
        set: &SetAttr,
    ) -> RResult<(RHandle, RAttr)> {
        no()
    }
    fn remove(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<()> {
        no()
    }
    fn rmdir(&self, cred: &Cred, dir: &RHandle, name: &[u8]) -> RResult<()> {
        no()
    }
    fn rename(
        &self,
        cred: &Cred,
        from_dir: &RHandle,
        from: &[u8],
        to_dir: &RHandle,
        to: &[u8],
    ) -> RResult<()> {
        no()
    }
    fn link(&self, cred: &Cred, h: &RHandle, dir: &RHandle, name: &[u8]) -> RResult<()> {
        no()
    }
    fn readdir(
        &self,
        cred: &Cred,
        dir: &RHandle,
        cookie: u64,
        verifier: u64,
        max: u32,
    ) -> RResult<DirPage> {
        no()
    }
    fn statfs(&self, cred: &Cred, h: &RHandle) -> RResult<FsStat> {
        no()
    }
    fn pathconf(&self, cred: &Cred, h: &RHandle) -> RResult<PathConf> {
        no()
    }
    fn open(&self, cred: &Cred, h: &RHandle, share: Share, owner: LockOwner) -> RResult<OpenToken> {
        no()
    }
    fn close(&self, cred: &Cred, h: &RHandle, token: &OpenToken) -> RResult<()> {
        no()
    }
    fn lock(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
        token: &OpenToken,
    ) -> RResult<()> {
        no()
    }
    fn unlock(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        off: u64,
        len: u64,
    ) -> RResult<()> {
        no()
    }
    fn test(
        &self,
        cred: &Cred,
        h: &RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
    ) -> RResult<Option<LockConflict>> {
        no()
    }
    fn unmount(&self) -> RResult<()> {
        no()
    }
}
