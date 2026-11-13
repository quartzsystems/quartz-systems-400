// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Credentials: `AUTH_NONE` and `AUTH_SYS` (RFC 5531 appendix A). Any
//! other flavor, `RPCSEC_GSS` (6) among them, is not accepted: the
//! server answers `MSG_DENIED`/`AUTH_ERROR` with `AUTH_BADCRED`.

use crate::types::rpc::{AUTH_NONE, AUTH_SYS, MAX_GIDS, auth_stat, authsys_parms, opaque_auth};
use crate::xdr;

/// A caller's credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// `AUTH_NONE`.
    None,
    /// `AUTH_SYS`.
    Sys(authsys_parms),
}

impl Credential {
    /// An `AUTH_SYS` credential; at most 16 GIDs and 255 bytes of name.
    pub fn sys(machine: &[u8], uid: u32, gid: u32, gids: &[u32]) -> Self {
        Self::Sys(authsys_parms {
            stamp: 0,
            machinename: machine.to_vec(),
            uid,
            gid,
            gids: gids.to_vec(),
        })
    }

    /// The credential an `opaque_auth` carries, or why it is refused.
    pub fn from_opaque(a: &opaque_auth) -> Result<Self, auth_stat> {
        match a.flavor {
            AUTH_NONE => Ok(Self::None),
            AUTH_SYS => xdr::from_bytes::<authsys_parms>(&a.body)
                .map(Self::Sys)
                .map_err(|_| auth_stat::AUTH_BADCRED),
            // RPCSEC_GSS and every other flavor.
            _ => Err(auth_stat::AUTH_BADCRED),
        }
    }

    /// The credential as an `opaque_auth`.
    pub fn to_opaque(&self) -> xdr::Result<opaque_auth> {
        Ok(match self {
            Self::None => none(),
            Self::Sys(p) => {
                if p.gids.len() > MAX_GIDS as usize {
                    return Err(xdr::Error::TooLong {
                        len: p.gids.len() as u64,
                        max: MAX_GIDS,
                    });
                }
                opaque_auth {
                    flavor: AUTH_SYS,
                    body: xdr::to_bytes(p)?,
                }
            }
        })
    }

    /// The UID of an `AUTH_SYS` credential.
    pub fn uid(&self) -> Option<u32> {
        match self {
            Self::None => None,
            Self::Sys(p) => Some(p.uid),
        }
    }

    /// The GID of an `AUTH_SYS` credential.
    pub fn gid(&self) -> Option<u32> {
        match self {
            Self::None => None,
            Self::Sys(p) => Some(p.gid),
        }
    }
}

/// An `AUTH_NONE` credential or verifier.
pub fn none() -> opaque_auth {
    opaque_auth {
        flavor: AUTH_NONE,
        body: Vec::new(),
    }
}
