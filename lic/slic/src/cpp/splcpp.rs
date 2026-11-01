// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Spooled file commands whose every outcome is a message: `RLSSPLF`
//! ends with its completion message or its escape, never silently.

use cmd::block::Parms;
use stg::proto::CmdReply;

use super::direct::failed;
use super::done;
use crate::ipl::system::System;
use crate::stgsvc::splcmd;

/// Run `name` when it is one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    match name {
        "RLSSPLF" => Some(rlssplf(system, parms)),
        _ => None,
    }
}

fn rlssplf(system: &mut System, parms: &Parms) -> CmdReply {
    let line = sys_cpps::bridge::specified_line("rlssplf", parms);
    let me = system.current_job;
    match splcmd::rlssplf(system, me, &line) {
        Ok((file, number, job)) => {
            let n = number.to_string();
            done(
                format!("Spooled file {file} number {number} of job {job} released.\n"),
                splcmd::RELEASED,
                &[(&file, 10), (&n, 6), (&job, 28)],
            )
        }
        Err(e) => failed("RLSSPLF", &e),
    }
}
