// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `dirsrv::admin::DirAdmin` as the user: each call is the storage
//! service's `dir_admin` (1568) on the job's thread, the job swapped to
//! the signed-on profile so the service checks that profile's authority.

use dirsrv::admin::{AdminError, ErrorKind};
use stg::Client as _;
use stg::diradmin::{DirAdminOp, DirAdminReply, Sender};

use super::Link;

/// The directory server's administration.
pub struct LiveDir {
    /// The job's thread.
    pub link: Link,
}

impl Sender for LiveDir {
    fn send(&self, user: &str, op: DirAdminOp) -> DirAdminReply {
        self.link
            .read(user, move |m| Some(m.ask(|l| l.dir_admin(op))))
            .unwrap_or_else(|| {
                let g = super::gone();
                DirAdminReply::Refused {
                    error: AdminError {
                        kind: ErrorKind::Unavailable,
                        msgid: g.id,
                        data: Vec::new(),
                        text: g.text,
                    },
                }
            })
    }
}
