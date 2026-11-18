// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Console commands run in the service on a job's behalf: the object,
//! storage, database and ILE build commands. `slic` keeps its own; the
//! job keeps calls and the debugger.

use crate::ipl::system::System;
use crate::procs::svcjob::LOCAL;
use crate::svc;

/// Run one line; its output.
pub fn run(system: &mut System, line: &str) -> String {
    if let Some(reply) = svc::db::intercept(system, line) {
        return reply;
    }
    let word = line.split_whitespace().next().unwrap_or("");
    if word == "hw" {
        // The table is the hardware resource service's, here as in slic.
        return match hwrsrc::client::table(true) {
            Ok(t) => t.dump(),
            Err(e) => format!("CPF9898 hardware resource service: {e}\n"),
        };
    }
    if LOCAL.contains(&word) {
        return format!("{word} is answered by the console, not the storage service\n");
    }
    let p = wm::cmd::Params::parse(line);
    let reset = p.upper("RESET").is_some_and(|r| r == "*YES");
    match word {
        "wrkactjob" => {
            let view = p.upper("VIEW").unwrap_or_else(|| "1".into());
            return super::stats::wrkactjob(system, &view);
        }
        "wrksyssts" => return super::stats::wrksyssts(system, reset),
        "wrkdsksts" => return super::stats::wrkdsksts(system, reset),
        "wrksysact" => {
            return match super::stats::performance_tools(system) {
                Ok(()) => super::stats::wrksysact(system),
                Err(e) => format!("{e}\n"),
            };
        }
        "wmstats" => return system.stats.report(),
        "dbgstall" if system.boot.svc => return dbgstall(&p),
        "stgstat" => return super::census::report(system),
        "tcp" => return super::tcp::console(system, line),
        _ => {}
    }
    if word == "dsplog" {
        let p = wm::cmd::Params::parse(line);
        return super::hist::dsplog(system, &p).unwrap_or_else(|e| format!("{e}\n"));
    }
    svc::reply(system, line)
}

/// `dbgstall REQ(name) MS(n) [COUNT(n)]`: the slow request fault point
/// (dev builds). `MS(0)` disarms it.
fn dbgstall(p: &wm::cmd::Params) -> String {
    let req = p.kw("REQ").unwrap_or_default();
    let ms: u64 = p.kw("MS").and_then(|m| m.parse().ok()).unwrap_or(0);
    let count: u32 = p.kw("COUNT").and_then(|c| c.parse().ok()).unwrap_or(1);
    if req.is_empty() {
        return "dbgstall REQ(name|IFSDEV) MS(n) [COUNT(n)]\n".into();
    }
    let count = if ms == 0 { 0 } else { count };
    super::stall::arm(req, std::time::Duration::from_millis(ms), count);
    format!("fault point: {count} {req} request(s) wait {ms} ms\n")
}
