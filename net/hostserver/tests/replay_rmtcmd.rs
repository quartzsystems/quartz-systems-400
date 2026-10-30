// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The remote command server against the `cmdcall` and `pgmcall`
//! traces. The job is scripted with what the oracle's job did (which
//! commands fail with which messages, what the programs return), so
//! the comparison is of the server's framing and marshalling.

mod common;

use std::collections::VecDeque;

use common::{Masks, TraceAuth, accept, replay, trace};
use hostserver::rmtcmd::{CallResult, Commands, JobMessage, RmtCmd};
use hostserver::{Handler, NoExits};
use hostsvr::frame::Frame;
use hostsvr::trace::Dir;

fn msg(id: &str, kind: u8, severity: u16) -> JobMessage {
    JobMessage {
        id: id.into(),
        kind,
        severity,
        file: ("QCPFMSG".into(), "QSYS".into()),
        text: format!("Message {id}."),
        ..JobMessage::default()
    }
}

/// A job that answers as the oracle's did.
struct Scripted {
    outputs: VecDeque<Vec<Vec<u8>>>,
}

impl Commands for Scripted {
    fn run(&mut self, command: &str) -> (bool, Vec<JobMessage>) {
        let c = command.to_ascii_uppercase();
        if c.starts_with("SNDPGMMSG") {
            (false, vec![msg("CPD0031", 2, 30), msg("CPF0001", 15, 30)])
        } else if c.contains("QSNOSUCH1") {
            (false, vec![msg("CPF9801", 15, 40)])
        } else if c.starts_with("NOSUCHCMD1") {
            (false, vec![msg("CPD0030", 2, 30), msg("CPF0001", 15, 30)])
        } else {
            (true, Vec::new())
        }
    }

    fn call(&mut self, program: &str, _library: &str, parms: &mut [Vec<u8>]) -> CallResult {
        if program == "QSNOPGM1" {
            return CallResult::Failed(vec![msg("MCH3401", 15, 40)]);
        }
        if let Some(outs) = self.outputs.pop_front() {
            let mut it = outs.into_iter();
            for p in parms.iter_mut() {
                if p.iter().all(|b| *b == 0)
                    && let Some(o) = it.next()
                {
                    let n = o.len().min(p.len());
                    p[..n].copy_from_slice(&o[..n]);
                }
            }
        }
        CallResult::Ran(Vec::new())
    }
}

/// The oracle's output parameters per call, in order.
fn outputs(t: &hostsvr::trace::Trace) -> VecDeque<Vec<Vec<u8>>> {
    let mut v = VecDeque::new();
    for c in &t.connections {
        for ch in c.chunks().filter(|c| c.dir == Dir::Reply && c.framed) {
            let f = Frame::decode(&ch.bytes).unwrap();
            if f.header.req_rep_id == 0x8003 {
                let ps = hostsvr::rmtcmd::parameters(&f.params).unwrap();
                v.push_back(ps.into_iter().map(|p| p.data).collect());
            }
        }
    }
    v
}

fn masks() -> Masks {
    Masks {
        // Message text is ours; only the layout up to the text compares.
        skip: vec![0x1106],
        ..Masks::default()
    }
}

#[test]
fn cmdcall() {
    let t = trace("rmtcmd", "cmdcall");
    let auth = TraceAuth::of(&t);
    let diffs = replay(&t, &masks(), &mut || {
        Box::new(RmtCmd::new(auth.clone(), Scripted { outputs: VecDeque::new() }, NoExits)) as Box<dyn Handler>
    });
    accept("rmtcmd-cmdcall", &diffs);
}

#[test]
fn pgmcall() {
    let t = trace("rmtcmd", "pgmcall");
    let auth = TraceAuth::of(&t);
    let outs = outputs(&t);
    let diffs = replay(&t, &masks(), &mut || {
        Box::new(RmtCmd::new(auth.clone(), Scripted { outputs: outs.clone() }, NoExits)) as Box<dyn Handler>
    });
    accept("rmtcmd-pgmcall", &diffs);
}

#[test]
fn messages_keep_ids_and_types() {
    let m = msg("CPF9801", 15, 40).to_host();
    assert_eq!(m.id_text(), "CPF9801");
    assert_eq!(m.type_number(), 15);
}

#[test]
fn other_traces_start() {
    for s in ["pwdlevel", "svrmap", "sysval"] {
        let t = trace("rmtcmd", s);
        let auth = TraceAuth::of(&t);
        let diffs = replay(&t, &masks(), &mut || {
            Box::new(RmtCmd::new(auth.clone(), Scripted { outputs: VecDeque::new() }, NoExits)) as Box<dyn Handler>
        });
        accept(&format!("rmtcmd-{s}"), &diffs);
    }
}
