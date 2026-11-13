// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `ipc::Wire` codecs of what a job sends and receives about remote
//! mounts: `MOUNT`'s CCSIDs, the mount table's rows, and the block I/O
//! helpers' work and answers.

use ipc::{Dec, Wire, WireError, wire_struct};

use super::helper::{HelperOp, HelperResult, HelperWork};
use super::types::{CcsidSel, MountCcsid, MountInfo};

impl Wire for CcsidSel {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Binary => out.push(0),
            Self::Ascii => out.push(1),
            Self::Job => out.push(2),
            Self::Ccsid(c) => {
                out.push(3);
                c.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        match u8::get(d)? {
            0 => Ok(Self::Binary),
            1 => Ok(Self::Ascii),
            2 => Ok(Self::Job),
            3 => u16::get(d).map(Self::Ccsid),
            tag => Err(WireError::Tag {
                what: "CcsidSel",
                tag,
            }),
        }
    }
}

impl Wire for HelperOp {
    fn put(&self, out: &mut Vec<u8>) {
        out.push(match self {
            Self::Read => 0,
            Self::Write => 1,
        });
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        match u8::get(d)? {
            0 => Ok(Self::Read),
            1 => Ok(Self::Write),
            tag => Err(WireError::Tag {
                what: "HelperOp",
                tag,
            }),
        }
    }
}

wire_struct!(MountCcsid { data, path });
wire_struct!(MountInfo {
    id,
    kind,
    at,
    source,
    options,
    read_only,
    case_sensitive,
    responding,
    data_ccsid,
    path_ccsid
});
wire_struct!(HelperWork {
    id,
    fs,
    source,
    op,
    handle,
    off,
    len,
    data,
    sync,
    uid,
    gid,
    gids
});
wire_struct!(HelperResult {
    data,
    eof,
    count,
    stable,
    verifier,
    err
});

#[cfg(test)]
mod tests {
    use super::*;

    fn round<T: Wire + PartialEq + std::fmt::Debug>(v: &T) {
        let mut out = Vec::new();
        v.put(&mut out);
        let mut d = Dec::new(&out);
        assert_eq!(&T::get(&mut d).expect("decodes"), v);
    }

    #[test]
    fn codecs_round_trip() {
        round(&MountCcsid {
            data: CcsidSel::Ccsid(819),
            path: CcsidSel::Ascii,
        });
        round(&MountInfo {
            id: 100,
            kind: "*NFS".into(),
            responding: Some(false),
            ..MountInfo::default()
        });
        round(&HelperWork {
            id: 1,
            fs: 100,
            source: "h:/x".into(),
            op: HelperOp::Write,
            handle: vec![1, 2],
            off: 4096,
            len: 8,
            data: vec![9; 8],
            sync: false,
            uid: 1,
            gid: 2,
            gids: vec![2, 3],
        });
        round(&HelperResult {
            err: Some(crate::Errno::EIO),
            ..HelperResult::default()
        });
    }
}
