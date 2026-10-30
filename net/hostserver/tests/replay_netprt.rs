// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The network print server against the `splf` trace, over spooled
//! files made from the ones the oracle listed.

mod common;

use common::mem::MemSpool;
use common::{Masks, TraceAuth, accept, replay, trace};
use hostserver::netprt::{NetPrint, Splf};
use hostserver::netprt::attrs::{Raw, decode, ids};
use hostserver::{Handler, NoExits};
use hostsvr::frame::Frame;
use hostsvr::trace::{Dir, Trace};

fn files(t: &Trace) -> Vec<Splf> {
    let mut v = Vec::new();
    for c in &t.connections {
        for ch in c.chunks().filter(|c| c.dir == Dir::Reply && c.framed) {
            let f = Frame::decode(&ch.bytes).unwrap();
            if f.header.req_rep_id != 0x8001 {
                continue;
            }
            let mut raws: Vec<Raw> = f.param(0x0001).map(|p| decode(&p.data)).unwrap_or_default();
            raws.extend(f.param(0x0008).map(|p| decode(&p.data)).unwrap_or_default());
            let get = |i: u16| raws.iter().find(|r| r.id == i).map(Raw::text).unwrap_or_default();
            let bin = |i: u16| raws.iter().find(|r| r.id == i).map_or(0, Raw::bin4);
            v.push(Splf {
                job: (get(ids::JOBNAME), get(ids::JOBUSER), get(ids::JOBNUMBER)),
                file: get(ids::SPOOLFILE),
                number: bin(ids::SPLFNUM),
                system: get(ids::JOBSYSTEM),
                date: get(ids::DATE),
                time: get(ids::TIME),
                status: get(ids::SPLFSTATUS),
                pages: bin(ids::PAGES),
                copies: bin(ids::COPIES),
                outq: (get(ids::OUTQUELIB), get(ids::OUTQUE)),
                prtf: (get(ids::PRTFLIB), get(ids::PRTFILE)),
                device_type: get(ids::PRTDEVTYPE),
                priority: get(ids::OUTPTY),
                ..Splf::default()
            });
        }
    }
    v
}

#[test]
fn splf() {
    let t = trace("netprt", "splf");
    let auth = TraceAuth::of(&t);
    let spool = MemSpool(files(&t));
    assert_eq!(spool.0.len(), 139);
    let job = ("QNPSERVS".to_string(), "QUSER".to_string(), "007848".to_string());
    let masks = Masks {
        // The attribute values: we answer the subset QS/400 keeps.
        skip: vec![0x0008],
        ..Masks::default()
    };
    let diffs = replay(&t, &masks, &mut || Box::new(NetPrint::new(auth.clone(), spool.clone(), NoExits, job.clone())) as Box<dyn Handler>);
    accept("netprt-splf", &diffs);
    let t = trace("netprt", "svrmap");
    let auth = TraceAuth::of(&t);
    let diffs = replay(&t, &masks, &mut || Box::new(NetPrint::new(auth.clone(), spool.clone(), NoExits, job.clone())) as Box<dyn Handler>);
    accept("netprt-svrmap", &diffs);
}
