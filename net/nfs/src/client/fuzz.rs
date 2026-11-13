// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fuzz entry of the client (`nfscli`): what a server answers is
//! untrusted bytes. The first byte picks the decoder: a reply message,
//! NFS version 3, MOUNT, NLM 4 or NSM results (the next byte the
//! procedure), a version 4 `COMPOUND` reply with every attribute list
//! in it, or one `fattr4`; what decodes is turned into the IFS's
//! attributes as the client does. None may panic.

use super::channel::decode_res;
use super::v3::attr::rattr as rattr3;
use super::v4::attr;
use crate::types::mount::MOUNT_PROGRAM::MOUNT_V3;
use crate::types::nfs3::NFS_PROGRAM::NFS_V3;
use crate::types::nfs3::{GETATTR3res, READDIRPLUS3res};
use crate::types::nfs4::{COMPOUND4res, GETATTR4res, READDIR4res, fattr4, nfs_resop4};
use crate::types::nlm::NLM_PROG::NLM4_VERS;
use crate::types::nsm::SM_PROG::SM_VERS;
use crate::xdr::{Decoder, Xdr};

fn v4_attrs(f: &fattr4) {
    if let Ok(a) = attr::decode(f) {
        let _ = attr::rattr(&a, None);
    }
}

/// Decode `data` as a server's answer.
pub fn fuzz(data: &[u8]) {
    let Some((sel, rest)) = data.split_first() else {
        return;
    };
    let (p, body) = match rest.split_first() {
        Some((p, b)) => (u32::from(*p), b),
        None => (0, rest),
    };
    match sel % 7 {
        0 => {
            let _ = crate::rpc::client::results(rest);
        }
        1 => match decode_res::<NFS_V3::Res>(p % 22, body) {
            Ok(NFS_V3::Res::NFSPROC3_GETATTR(GETATTR3res::NFS3_OK(ok))) => {
                let _ = rattr3(&ok.obj_attributes);
            }
            Ok(NFS_V3::Res::NFSPROC3_READDIRPLUS(READDIRPLUS3res::NFS3_OK(ok))) => {
                for e in &ok.reply.entries {
                    let _ = e.name_attributes.as_ref().map(rattr3);
                }
            }
            _ => {}
        },
        2 => {
            let _ = decode_res::<MOUNT_V3::Res>(p % 6, body);
        }
        3 => {
            let _ = decode_res::<NLM4_VERS::Res>(p % 24, body);
        }
        4 => {
            let _ = decode_res::<SM_VERS::Res>(p % 7, body);
        }
        5 => {
            let mut d = Decoder::new(rest);
            if let Ok(c) = COMPOUND4res::decode(&mut d) {
                for r in &c.resarray {
                    match r {
                        nfs_resop4::OP_GETATTR(GETATTR4res::NFS4_OK(ok)) => {
                            v4_attrs(&ok.obj_attributes);
                        }
                        nfs_resop4::OP_READDIR(READDIR4res::NFS4_OK(ok)) => {
                            for e in &ok.reply.entries {
                                v4_attrs(&e.attrs);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {
            if let Ok(f) = fattr4::decode(&mut Decoder::new(rest)) {
                v4_attrs(&f);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn random_bytes_do_not_panic() {
        let mut x: u32 = 0x1234_5678;
        for len in 0..400usize {
            let data: Vec<u8> = (0..len)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    (x & 0xff) as u8
                })
                .collect();
            for sel in 0..7u8 {
                let mut d = vec![sel];
                d.extend_from_slice(&data);
                super::fuzz(&d);
            }
        }
    }
}
