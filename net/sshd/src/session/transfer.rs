// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! File transfer in front of any [`SessionHost`]: the `sftp` subsystem
//! (whatever command `Subsystem sftp` names, as `internal-sftp`), an
//! `exec` of `internal-sftp`, and the legacy `scp -t`/`scp -f`, all
//! answered here over the user's IFS. Everything else goes to the
//! wrapped host.

use std::io::Write;
use std::sync::Arc;

use super::{ChannelIo, Exit, SessionHost, SessionKind, SessionRequest};
use crate::io::FileSystem;
use crate::{scp, sftp};

/// A host with SFTP and scp in front.
pub struct FileTransfer<H: SessionHost> {
    inner: H,
    fs: Arc<dyn FileSystem>,
}

impl<H: SessionHost> FileTransfer<H> {
    /// Wrap `inner`; transfers run over `fs`.
    pub fn new(inner: H, fs: Arc<dyn FileSystem>) -> Self {
        Self { inner, fs }
    }
}

impl<H: SessionHost> SessionHost for FileTransfer<H> {
    fn run(&self, req: SessionRequest, mut io: ChannelIo) -> Exit {
        let sftp_wanted = match &req.kind {
            SessionKind::Subsystem { name, .. } => name == "sftp",
            SessionKind::Exec(cmd) => cmd.trim() == "internal-sftp",
            SessionKind::Shell => false,
        };
        let scp_args = match &req.kind {
            SessionKind::Exec(cmd) => scp::Args::parse(cmd),
            _ => None,
        };
        if !sftp_wanted && scp_args.is_none() {
            return self.inner.run(req, io);
        }
        let Some(ifs) = self.fs.user(&req.account) else {
            let _ = io
                .stderr
                .write_all(b"The file system is not available for this user.\n");
            return Exit::Status(1);
        };
        if let Some(args) = scp_args {
            return Exit::Status(scp::run(&*ifs, &req.account.home, &args, &mut io));
        }
        sftp::serve(&*ifs, &req.account.home, &mut io.stdin, &mut io.stdout);
        Exit::Status(0)
    }
}
