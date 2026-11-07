// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 162 and 163: the `Debug` service (version 9) through the real
//! connection jobs, over `HLINV/ASSETS`, an RPG program compiled
//! `DBGVIEW(*SOURCE)` from [`ASSETS`].
//!
//! Step 162: `Start Own`, `AddProgram`, a breakpoint on a comment line
//! landing on the next statement, `CALL` through `RunCommand`: a
//! `Stopped` event at that line while the command waits, `Stack`, the
//! program's fields in `Variables` (and a structure's parts), `Eval`,
//! a step to the next line, `Resume Go` and the command's reply; a
//! version 8 connection has no `Debug` service and gets
//! `InvalidRequest`.
//!
//! Step 163: `SetServiceEntry` on the same program for the profile,
//! then a 5250 session of the profile calls it: its job stops (the
//! display stays locked), the `Stopped` event names that job and the
//! program's first line, a breakpoint set then stops it again, and
//! `Resume Go` lets the display go on.

use std::time::Duration;

use qap_proto::ops::command::{CommandReply, CommandText};
use qap_proto::ops::connection::{Hello, HelloReply};
use qap_proto::ops::debug::{
    AddProgram, BreakpointsSet, DebugTarget, Eval, Evaluated, Frames, LineBreakpoint, ProgramAdded,
    RemoveServiceEntry, Resume, ResumeHow, SetBreakpoints, SetServiceEntry, Start, Started,
    StopReason, Stopped, VariableList, Variables, View,
};
use qap_proto::ops::event;
use qap_proto::types::ObjectName;
use qap_proto::{Envelope, Op, Service, Status};

use super::client::{Conn, brief};

/// `ASSETS`: statements on lines 9, 10, 12, 13 and 14; line 11 is a
/// comment.
pub const ASSETS: &str = "**FREE
ctl-opt dftactgrp(*no);
dcl-s count int(10) inz(0);
dcl-s name char(10) inz('DESK');
dcl-ds asset qualified;
  id int(10);
  tag char(8);
end-ds;
count = count + 1;
asset.id = count;
// the tag
asset.tag = 'A' + %char(count);
name = 'CHAIR';
*inlr = *on;
";

/// How long a stop or a command may take in an emulated guest.
const WAIT: Duration = Duration::from_secs(90);

fn assets() -> ObjectName {
    ObjectName {
        library: "HLINV".into(),
        name: "ASSETS".into(),
    }
}

/// The next `Stopped` event.
pub fn stopped(c: &mut Conn) -> Result<Stopped, String> {
    let e = c.event(WAIT)?;
    if (e.service, e.op) != (Service::Debug, event::STOPPED) {
        return Err(format!("an event other than Stopped: {e:?}"));
    }
    e.body_as().map_err(|e| format!("Stopped's body: {e}"))
}

fn resume(c: &mut Conn, how: ResumeHow) -> Result<(), String> {
    let r = c.call(Op::DebugResume, &Resume { how })?;
    if r.status != Status::Ok {
        return Err(format!("Resume: {}", brief(&r)));
    }
    Ok(())
}

/// A session on `c` with `ASSETS` added; its modules.
pub fn session(c: &mut Conn) -> Result<ProgramAdded, String> {
    let started: Started = c.ok(
        Op::DebugStart,
        &Start {
            target: DebugTarget::Own,
        },
    )?;
    if started.session == 0 {
        return Err("Start: session 0".into());
    }
    let added: ProgramAdded = c.ok(
        Op::AddProgram,
        &AddProgram {
            program: assets(),
            r#type: "*PGM".into(),
        },
    )?;
    if !added
        .modules
        .iter()
        .any(|m| m.name == "ASSETS" && m.views.contains(&View::Source))
    {
        return Err(format!("AddProgram: {:?}", added.modules));
    }
    Ok(added)
}

/// Breakpoints on `lines` of `ASSETS`.
pub fn breakpoints(c: &mut Conn, lines: &[u32]) -> Result<BreakpointsSet, String> {
    c.ok(
        Op::SetBreakpoints,
        &SetBreakpoints {
            program: assets(),
            module: "ASSETS".into(),
            lines: lines
                .iter()
                .map(|l| LineBreakpoint {
                    line: *l,
                    condition: None,
                })
                .collect(),
        },
    )
}

/// Step 162 on `a`, signed on; `old` a version 8 connection, not yet
/// signed on.
pub fn own(a: &mut Conn, old: &mut Conn) -> Result<String, String> {
    session(a)?;
    let set = breakpoints(a, &[11])?;
    if set.set.first().map(|b| b.line) != Some(12) {
        return Err(format!("a breakpoint on line 11 landed on {:?}", set.set));
    }
    let call = a.request(
        Op::RunCommand,
        &CommandText {
            command: "CALL PGM(HLINV/ASSETS)".into(),
        },
    )?;
    let s = stopped(a)?;
    if s.reason != StopReason::Breakpoint || s.frame.line != 12 || s.frame.program != assets() {
        return Err(format!("the stop: {s:?}"));
    }
    if a.poll_reply(call, Duration::from_millis(500))?.is_some() {
        return Err("the command ended while the program was stopped".into());
    }
    let stack: Frames = a.ok(Op::Stack, &())?;
    if stack.frames.first().map(|f| f.line) != Some(12) {
        return Err(format!("Stack: {:?}", stack.frames));
    }
    let vars: VariableList = a.ok(
        Op::Variables,
        &Variables {
            frame: 0,
            parent: None,
        },
    )?;
    let value = |n: &str| {
        vars.variables
            .iter()
            .find(|v| v.name == n)
            .map(|v| (v.value.clone(), v.expandable))
    };
    if value("COUNT") != Some(("1".into(), false))
        || value("NAME").is_none_or(|(v, _)| !v.contains("DESK"))
        || value("ASSET").is_none_or(|(_, e)| !e)
    {
        return Err(format!("Variables: {:?}", vars.variables));
    }
    let parts: VariableList = a.ok(
        Op::Variables,
        &Variables {
            frame: 0,
            parent: Some("ASSET".into()),
        },
    )?;
    if !parts
        .variables
        .iter()
        .any(|v| v.name == "ASSET.ID" && v.value == "1")
    {
        return Err(format!("ASSET's parts: {:?}", parts.variables));
    }
    let tag: Evaluated = a.ok(
        Op::Eval,
        &Eval {
            frame: 0,
            expression: "ASSET.TAG".into(),
        },
    )?;
    resume(a, ResumeHow::StepOver)?;
    let s = stopped(a)?;
    if s.reason != StopReason::Step || s.frame.line != 13 {
        return Err(format!("the step: {s:?}"));
    }
    resume(a, ResumeHow::Go)?;
    let r = a.reply(call, WAIT)?;
    if r.status != Status::Ok {
        return Err(format!("CALL: {}", brief(&r)));
    }
    let _: CommandReply = r.body_as().map_err(|e| e.to_string())?;
    let end = a.call(Op::DebugEnd, &())?;
    if end.status != Status::Ok {
        return Err(format!("End: {}", brief(&end)));
    }
    let gate = gate(old)?;
    Ok(format!(
        "stopped at line 12 with COUNT 1, NAME and ASSET's parts, ASSET.TAG {}, stepped to 13, the CALL completed; {gate}",
        tag.value
    ))
}

/// A version 8 connection: no `Debug` service, and `InvalidRequest`.
fn gate(old: &mut Conn) -> Result<String, String> {
    let h: HelloReply = old.ok(
        Op::Hello,
        &Hello {
            min_version: 1,
            max_version: 8,
            client_name: "conformance".into(),
            client_version: "162".into(),
            capabilities: Vec::new(),
        },
    )?;
    if h.version != 8 || h.services.contains(&Service::Debug) {
        return Err(format!("a version 8 Hello: {h:?}"));
    }
    old.sign_on(super::USER, super::PASSWORD)?;
    let r = old.call(
        Op::DebugStart,
        &Start {
            target: DebugTarget::Own,
        },
    )?;
    if r.status != Status::InvalidRequest {
        return Err(format!("Debug on version 8: {}", brief(&r)));
    }
    Ok("version 8 has no Debug service".into())
}

/// Step 163's QAP side once the display job called the program: the
/// entry's stop, a breakpoint's, then the job let go. The stop's job.
pub fn entry_stops(a: &mut Conn) -> Result<Stopped, String> {
    let s = stopped(a)?;
    if s.reason != StopReason::ServiceEntry || s.frame.line != 9 || s.job.user != super::USER {
        return Err(format!("the entry's stop: {s:?}"));
    }
    breakpoints(a, &[13])?;
    resume(a, ResumeHow::Go)?;
    let b = stopped(a)?;
    if b.reason != StopReason::Breakpoint || b.frame.line != 13 || b.job != s.job {
        return Err(format!("the breakpoint in the display job: {b:?}"));
    }
    let name: Evaluated = a.ok(
        Op::Eval,
        &Eval {
            frame: 0,
            expression: "NAME".into(),
        },
    )?;
    if !name.value.contains("DESK") {
        return Err(format!("NAME in the display job: {}", name.value));
    }
    Ok(s)
}

/// The entry point set on `a`'s session.
pub fn set_entry(a: &mut Conn) -> Result<(), String> {
    session(a)?;
    let r = a.call(
        Op::SetServiceEntry,
        &SetServiceEntry {
            program: assets(),
            user: "*CURRENT".into(),
        },
    )?;
    if r.status != Status::Ok {
        return Err(format!("SetServiceEntry: {}", brief(&r)));
    }
    Ok(())
}

/// The job let go, the entry removed, the session ended.
pub fn finish(a: &mut Conn) -> Result<(), String> {
    resume(a, ResumeHow::Go)?;
    let r = a.call(
        Op::RemoveServiceEntry,
        &RemoveServiceEntry { program: assets() },
    )?;
    if r.status != Status::Ok {
        return Err(format!("RemoveServiceEntry: {}", brief(&r)));
    }
    let end: Envelope = a.call(Op::DebugEnd, &())?;
    if end.status != Status::Ok {
        return Err(format!("End: {}", brief(&end)));
    }
    Ok(())
}
