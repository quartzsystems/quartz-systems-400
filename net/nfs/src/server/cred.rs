// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The caller of a request (docs/NFS.md section 9): an `AUTH_SYS` UID
//! is the profile with that UID; UID 0 is `QSECOFR` only for a host in
//! `ROOT=`, else the anonymous user (root squash); `AUTH_NONE` is the
//! anonymous user. The anonymous user is `ANON=`: the system's
//! anonymous profile by default, the profile of a UID, or a refusal
//! (`ANON=-1`). A UID no profile has comes back from the IFS as
//! `ENOTENROLL`; [`fallback`] says what the call is then retried as.
//! The wire's GIDs are never used.

use std::net::IpAddr;

use ifs::client::Caller;

use super::error::VfsError;
use super::export::{Anon, ExportView};
use crate::rpc::Credential;

/// The anonymous user of `export`, or `None` when it refuses (`ANON=-1`).
#[must_use]
pub fn anonymous(export: &dyn ExportView) -> Option<Caller> {
    match export.anon() {
        Anon::Default => Some(Caller::Anonymous),
        Anon::Uid(uid) => Some(Caller::Uid { uid }),
        Anon::Refuse => None,
    }
}

/// Whom a call with `cred` from `peer` is made for on `export`.
///
/// # Errors
///
/// [`VfsError::Acces`] when the caller is anonymous and the export
/// refuses anonymous callers.
pub fn caller(cred: &Credential, peer: IpAddr, export: &dyn ExportView) -> Result<Caller, VfsError> {
    match cred {
        Credential::Sys(p) if p.uid == 0 && export.root_access(peer) => Ok(Caller::Uid { uid: 0 }),
        Credential::Sys(p) if p.uid != 0 => Ok(Caller::Uid { uid: p.uid }),
        _ => anonymous(export).ok_or(VfsError::Acces),
    }
}

/// What a call refused with `ENOTENROLL` as `who` is made again as:
/// the export's anonymous user, unless `who` already was it.
#[must_use]
pub fn fallback(who: &Caller, export: &dyn ExportView) -> Option<Caller> {
    anonymous(export).filter(|a| a != who)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::export::{HostPat, MemExport};
    use ifs::ops::audit::FileId;

    #[test]
    fn root_is_squashed_unless_listed() {
        let peer: IpAddr = "10.0.0.5".parse().unwrap();
        let mut e = MemExport::new(1, "/home", FileId::default());
        let root = Credential::sys(b"h", 0, 0, &[]);
        assert_eq!(caller(&root, peer, &e), Ok(Caller::Anonymous));
        e.root_hosts = vec![HostPat::Addr(peer)];
        assert_eq!(caller(&root, peer, &e), Ok(Caller::Uid { uid: 0 }));
        assert_eq!(
            caller(&Credential::sys(b"h", 100, 5, &[1]), peer, &e),
            Ok(Caller::Uid { uid: 100 })
        );
        e.anon = Anon::Refuse;
        assert_eq!(caller(&Credential::None, peer, &e), Err(VfsError::Acces));
        assert_eq!(fallback(&Caller::Uid { uid: 7 }, &e), None);
        e.anon = Anon::Uid(9);
        assert_eq!(caller(&Credential::None, peer, &e), Ok(Caller::Uid { uid: 9 }));
        assert_eq!(fallback(&Caller::Uid { uid: 7 }, &e), Some(Caller::Uid { uid: 9 }));
        assert_eq!(fallback(&Caller::Uid { uid: 9 }, &e), None);
    }
}
