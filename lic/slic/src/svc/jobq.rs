// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Job queue and job control commands of the service console. `SBMJOB`
//! runs in the job, which copies its `*LDA` into the submission.

use wm::cmd::Params;

use crate::ipl::system::System;
use crate::stgsvc::{jobqs, jobscd};

/// Help for these commands.
pub const HELP: &str = "\
sbmjob CMD('..') [JOB() JOBD() JOBQ() JOBPTY() USER() RTGDTA() HOLD() SCDDATE() SCDTIME() MSGQ() INLLIBL() CURLIB() LOG() ...]
wrkjobq [LIB/Q] | crtjobq LIB/Q [TEXT()] | dltjobq Q | hldjobq Q | rlsjobq Q | clrjobq Q
hldjob JOB(n/u/name) | rlsjob JOB() | endjob JOB() [OPTION(*CNTRLD|*IMMED)] | dlyjob DLY(s)
addjobscde JOB() CMD('..') FRQ(*ONCE|*WEEKLY|*MONTHLY) SCDDATE() SCDDAY() SCDTIME(hhmmss) [RELDAYMON() RCYACN() JOBD() JOBQ() USER() MSGQ() OMITDATE() SAVE() TEXT()]
chgjobscde JOB() [ENTRYNBR()] ... | rmvjobscde JOB() | hldjobscde JOB() | rlsjobscde JOB() | wrkjobscde
";

/// Dispatch; `None` when `cmd` is not one of these.
pub fn command(system: &mut System, cmd: &str, line: &str) -> Option<String> {
    let p = Params::parse(line);
    let me = system.current_job;
    let job = || p.pos_or(0, "JOB").map(|j| j.to_ascii_uppercase());
    let queue = || p.pos_or(0, "JOBQ").map(|q| q.to_ascii_uppercase());
    let r = match cmd {
        "wrkjobq" => {
            let q = p.pos_or(0, "JOBQ").ok().map(|q| q.to_ascii_uppercase());
            jobqs::wrkjobq(system, q.as_deref())
        }
        "crtjobq" => {
            queue().and_then(|q| jobqs::create_delete(system, &q, &p.text("TEXT"), true, me))
        }
        "dltjobq" => queue().and_then(|q| jobqs::create_delete(system, &q, "", false, me)),
        "chgjobq" => queue().and_then(|q| jobqs::chgjobq(system, &q, &p)),
        "hldjobq" => queue().and_then(|q| jobqs::queue_op(system, &q, "hold")),
        "rlsjobq" => queue().and_then(|q| jobqs::queue_op(system, &q, "release")),
        "clrjobq" => queue().and_then(|q| jobqs::queue_op(system, &q, "clear")),
        "hldjob" => job().and_then(|j| jobqs::hold(system, me, &j, true)),
        "rlsjob" => job().and_then(|j| jobqs::hold(system, me, &j, false)),
        "endjob" => job().and_then(|j| {
            let immediate = matches!(p.upper("OPTION").as_deref(), Some("*IMMED"));
            jobqs::endjob(system, me, &j, immediate)
        }),
        "addjobscde" | "chgjobscde" | "rmvjobscde" | "hldjobscde" | "rlsjobscde" | "wrkjobscde" => {
            jobscd::command(system, me, cmd, line)
        }
        _ => return None,
    };
    Some(match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    })
}
