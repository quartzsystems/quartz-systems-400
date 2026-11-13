// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A server job's `NFSv4` stable records (`v4::state::stable`, format
//! `QSN4`): `/QIBM/UserData/OS400/NFS/QNFSV4CL.nnn` for the job of index
//! `nnn`, through a client of the calling thread's own. Damaged records
//! are reported in the job's output and count as none.

use std::io;
use std::sync::Arc;

use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
use nfs::v4::state::stable::{StableImage, StableStore, decode, encode, file_name};

use super::host::Host;

/// The records of server job `index`.
pub struct IfsStable<H: Host> {
    host: Arc<H>,
    path: String,
}

impl<H: Host> IfsStable<H> {
    /// For the server job of index `index`.
    #[must_use]
    pub fn new(host: Arc<H>, index: u16) -> Self {
        Self {
            host,
            path: file_name(u32::from(index)),
        }
    }
}

fn err(path: &str, e: ifs::Errno) -> io::Error {
    io::Error::other(format!("{path}: {}", e.text()))
}

impl<H: Host> StableStore for IfsStable<H> {
    fn load(&self) -> io::Result<StableImage> {
        let fs = self.host.fs().map_err(io::Error::other)?;
        let fd = match fs.open(&self.path, OpenArgs::new(O_RDONLY)) {
            Ok(fd) => fd,
            Err(ifs::Errno::ENOENT | ifs::Errno::ENOTDIR) => return Ok(StableImage::default()),
            Err(e) => return Err(err(&self.path, e)),
        };
        let data = ifs::client::read_all(&fs, fd);
        let _ = fs.close(fd);
        let data = data.map_err(|e| err(&self.path, e))?;
        Ok(decode(&data).unwrap_or_else(|e| {
            eprintln!("qsjob: QNFSNFSD: {} not used: {e}", self.path);
            StableImage::default()
        }))
    }

    fn store(&self, img: &StableImage) -> io::Result<()> {
        let fs = self.host.fs().map_err(io::Error::other)?;
        let mut at = String::new();
        for part in "/QIBM/UserData/OS400/NFS"
            .split('/')
            .filter(|p| !p.is_empty())
        {
            at.push('/');
            at.push_str(part);
            match fs.mkdir(&at, 0o755) {
                Ok(()) | Err(ifs::Errno::EEXIST) => {}
                Err(e) => return Err(err(&at, e)),
            }
        }
        let fd = fs
            .open(
                &self.path,
                OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC | O_CCSID)
                    .mode(0o600)
                    .conv(65535),
            )
            .map_err(|e| err(&self.path, e))?;
        let r = ifs::client::write_all(&fs, fd, &encode(img));
        let _ = fs.close(fd);
        r.map_err(|e| err(&self.path, e))
    }
}
