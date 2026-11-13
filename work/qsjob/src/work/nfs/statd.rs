// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where `QNFSNSMD` keeps what outlives a start (`nsm::record::StatdStore`):
//! `/etc/STATD`, the state number as the oracle's file has it (CCSID 37,
//! `-rw-------`, owned by `QSYS`), and the monitored hosts in
//! `/QIBM/UserData/OS400/NFS/QNFSNSMD.MON` (`QSMN` version 1,
//! docs/NFS.md section 6.3), each through a client of the calling
//! thread's own.

use std::io;
use std::sync::Arc;

use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
use nfs::nsm::record::StatdStore;
use stg::nfs::{NfsOp, NfsReply};

use super::host::Host;

/// The state number's file.
pub const STATD: &str = "/etc/STATD";
/// The monitored hosts' file.
pub const MONITORED: &str = "/QIBM/UserData/OS400/NFS/QNFSNSMD.MON";

/// The two files in the IFS.
pub struct IfsStatd<H: Host> {
    host: Arc<H>,
}

impl<H: Host> IfsStatd<H> {
    /// Through `host`'s clients.
    #[must_use]
    pub fn new(host: Arc<H>) -> Self {
        Self { host }
    }

    fn fs(&self) -> io::Result<H::Fs> {
        self.host.fs().map_err(io::Error::other)
    }
}

fn err(path: &str, e: ifs::Errno) -> io::Error {
    io::Error::other(format!("{path}: {}", e.text()))
}

/// A file's bytes, `None` when there is none.
fn read(fs: &dyn IfsClient, path: &str) -> io::Result<Option<Vec<u8>>> {
    let fd = match fs.open(path, OpenArgs::new(O_RDONLY)) {
        Ok(fd) => fd,
        Err(ifs::Errno::ENOENT | ifs::Errno::ENOTDIR) => return Ok(None),
        Err(e) => return Err(err(path, e)),
    };
    let data = ifs::client::read_all(fs, fd);
    let _ = fs.close(fd);
    data.map(Some).map_err(|e| err(path, e))
}

/// Replace a file's bytes, made with `mode` and tagged `ccsid` when new.
fn write(fs: &dyn IfsClient, path: &str, bytes: &[u8], mode: u32, ccsid: u32) -> io::Result<()> {
    let fd = fs
        .open(
            path,
            OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC | O_CCSID)
                .mode(mode)
                .conv(ccsid),
        )
        .map_err(|e| err(path, e))?;
    let r = ifs::client::write_all(fs, fd, bytes);
    let _ = fs.close(fd);
    r.map_err(|e| err(path, e))
}

/// Make `dir` and its parents.
fn make_dirs(fs: &dyn IfsClient, dir: &str) -> io::Result<()> {
    let mut at = String::new();
    for part in dir.split('/').filter(|p| !p.is_empty()) {
        at.push('/');
        at.push_str(part);
        match fs.mkdir(&at, 0o755) {
            Ok(()) | Err(ifs::Errno::EEXIST) => {}
            Err(e) => return Err(err(&at, e)),
        }
    }
    Ok(())
}

impl<H: Host> StatdStore for IfsStatd<H> {
    fn read_state(&self) -> io::Result<Option<Vec<u8>>> {
        read(&self.fs()?, STATD)
    }

    fn write_state(&self, bytes: &[u8]) -> io::Result<()> {
        let fs = self.fs()?;
        write(&fs, STATD, bytes, 0o600, 37)?;
        let _ = fs.chmod(STATD, 0o600);
        // The oracle's is QSYS's.
        if let NfsReply::Id { id: Some(uid) } = self.host.op(NfsOp::UidOfName {
            name: "QSYS".into(),
        }) {
            let _ = fs.chown(STATD, uid, u32::MAX);
        }
        Ok(())
    }

    fn read_monitored(&self) -> io::Result<Option<Vec<u8>>> {
        read(&self.fs()?, MONITORED)
    }

    fn write_monitored(&self, bytes: &[u8]) -> io::Result<()> {
        let fs = self.fs()?;
        make_dirs(&fs, "/QIBM/UserData/OS400/NFS")?;
        write(&fs, MONITORED, bytes, 0o600, 65535)
    }
}
