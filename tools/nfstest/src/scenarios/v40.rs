// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS version 4.0: the client ID, and the shared scenarios in 4.0.

use super::v4::{self, V4};
use super::{Family, Lines, Scenario, line};
use crate::ctx::Ctx;
use crate::error::check;
use crate::wire::v4::{Op, st, status_name};

/// This file's scenarios.
pub const LIST: [Scenario; 12] = [
    s("v4-null", Family::V4, v4::null),
    s("v4-minor2-refused", Family::V4, v4::minor2),
    s("v4-op-illegal", Family::V4, v4::illegal),
    s("v40-setclientid-renew", Family::V40, setclientid_renew),
    s("v40-lookup-export", Family::V40, v4::lookup_export),
    s("v40-open-write-read", Family::V40, v4::open_write_read),
    s("v40-readdir", Family::V40, v4::readdir),
    s("v40-rename-remove", Family::V40, v4::rename_remove),
    s("v40-setattr", Family::V40, v4::setattr),
    s("v40-locks", Family::V40, v4::locks),
    s("v40-share-deny", Family::V40, v4::share_deny),
    s("v40-sequence-refused", Family::V40, sequence_in_40),
];

const fn s(name: &'static str, family: Family, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario { name, family, run }
}

/// `NFS4ERR_STALE_CLIENTID`.
const STALE_CLIENTID: u32 = 10022;

fn setclientid_renew(c: &Ctx) -> Lines {
    let mut v = V4::new(c, 0, "renew");
    v.setup()?;
    let r = v.raw(0, &[Op::Renew(v.clientid)])?;
    check(r.status == st::OK, || {
        format!("RENEW: {}", status_name(r.status))
    })?;
    let r = v.raw(0, &[Op::Renew(v.clientid ^ 0x5a5a_5a5a_0000_0000)])?;
    check(r.status == STALE_CLIENTID, || {
        format!("RENEW of an unknown client ID: {}", status_name(r.status))
    })?;
    Ok(vec![
        line("client ID", format!("{:016x}", v.clientid)),
        line("RENEW of an unknown client ID", status_name(r.status)),
    ])
}

/// `SEQUENCE` is not a 4.0 operation (RFC 7530 section 15.2: `OP_ILLEGAL`).
fn sequence_in_40(c: &Ctx) -> Lines {
    let mut v = V4::new(c, 0, "seq40");
    let r = v.raw(0, &[Op::Sequence([0; 16], 1, 0, 0, false)])?;
    check(r.status == st::OP_ILLEGAL, || {
        format!("SEQUENCE in minor version 0: {}", status_name(r.status))
    })?;
    Ok(vec![line("SEQUENCE in 4.0", status_name(r.status))])
}
