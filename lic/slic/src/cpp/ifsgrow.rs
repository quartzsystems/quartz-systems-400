// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSGROWIFS`: the IFS volume grown now (`docs/IFS.md` "The volume").

use cmd::block::Parms;
use stg::proto::CmdReply;

use super::direct::failed;
use super::done;
use crate::ipl::system::System;
use crate::stgsvc::ifsvol::{self, GrowError};

/// The IFS grew.
const GREW: &str = "CPC3E20";

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    if name != "QSGROWIFS" {
        return None;
    }
    // SIZE in MB; *STEP is 0.
    let pages = parms
        .int("SIZE")
        .filter(|&mb| mb > 0)
        .and_then(|mb| u64::try_from(mb).ok())
        .map(|mb| mb * 256);
    let mb = |p: u64| (p / 256).to_string();
    Some(match ifsvol::grow(system, pages) {
        Ok((added, total)) => {
            let (a, t) = (mb(added), mb(total));
            done(
                format!("{GREW} the integrated file system grew by {a} MB to {t} MB\n"),
                GREW,
                &[(&a, 10), (&t, 10)],
            )
        }
        Err(GrowError::NoRoom(room)) => failed(
            name,
            &format!(
                "CPF3E21 ASP 1 has room for {} MB more below its threshold",
                mb(room)
            ),
        ),
        Err(GrowError::InPartition) => failed(name, "CPF3E22 the IFS is in its own partition"),
        Err(GrowError::Failed(why)) => failed(name, &format!("CPF3E23 {why}")),
    })
}
