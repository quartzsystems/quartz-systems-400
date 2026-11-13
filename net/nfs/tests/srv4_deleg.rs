// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFSv4.0 read delegations with a client's callback program on
//! loopback TCP: the path probed with `CB_NULL`, a delegation granted
//! on an open for reading, recalled (`CB_RECALL`) when another client
//! of the job opens the file for writing (`NFS4ERR_DELAY` meanwhile) or
//! a local job writes it (the IFS lease's break order), returned
//! (`DELEGRETURN`), and revoked when not returned in time.

#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    clippy::doc_markdown
)]

mod srv3_common;
mod srv4_common;

use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::flags::O_WRONLY;
use nfs::rpc::{Answer, CallContext, Dispatcher, Program, serve_tcp};
use nfs::rpcb::uaddr::to_uaddr;
use nfs::server::news::{ClientNews, NewsSource};
use nfs::types::nfs4::{
    self as t, CB_COMPOUND4args, CB_COMPOUND4res, CB_RECALL4res, nfs_argop4, nfs_cb_argop4,
    nfs_cb_resop4, nfsstat4,
};
use nfs::xdr::{self, Decoder};
use srv3_common::World;
use srv4_common::{Env, V4c, fh_of, getfh, lookup, opened, putfh, rootfh};

/// A client's callback program: what it was sent.
#[derive(Default)]
struct Callbacks {
    nulls: Mutex<u32>,
    recalls: Mutex<Vec<(u32, t::stateid4, Vec<u8>)>>,
}

impl Program for Callbacks {
    fn program(&self) -> u32 {
        0x4000_0001
    }
    fn versions(&self) -> &[u32] {
        &[1]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        if ctx.procedure == 0 {
            *self.nulls.lock().unwrap() += 1;
            return Ok(Some(Vec::new()));
        }
        let a: CB_COMPOUND4args = xdr::from_bytes(d.rest()).unwrap();
        let mut res = Vec::new();
        for op in a.argarray {
            if let nfs_cb_argop4::OP_CB_RECALL(r) = op {
                self.recalls
                    .lock()
                    .unwrap()
                    .push((a.callback_ident, r.stateid, r.fh.0));
                res.push(nfs_cb_resop4::OP_CB_RECALL(CB_RECALL4res {
                    status: nfsstat4::NFS4_OK,
                }));
            }
        }
        Ok(Some(
            xdr::to_bytes(&CB_COMPOUND4res {
                status: nfsstat4::NFS4_OK,
                tag: a.tag,
                resarray: res,
            })
            .unwrap(),
        ))
    }
}

fn until(what: &str, f: impl Fn() -> bool, mut step: impl FnMut()) {
    let end = Instant::now() + Duration::from_secs(10);
    while !f() {
        assert!(Instant::now() < end, "{what}");
        step();
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn env() -> Env {
    let w = World::new();
    let e = w.export(1, "/home/x");
    w.put(e);
    for f in ["f", "g", "h"] {
        let p = format!("/home/x/{f}");
        w.write_file(&p, b"data");
        w.job(9).chmod(&p, 0o666).unwrap();
    }
    Env::new(w)
}

fn open_read(e: &Env, c: &V4c, name: &str) -> (Vec<u8>, t::OPEN4resok) {
    let r = c.raw(
        e,
        0,
        vec![
            rootfh(),
            lookup("home"),
            lookup("x"),
            c.open("o", name, false, 1, 0),
            getfh(),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let o = opened(&r);
    let fh = fh_of(&r);
    if o.rflags & t::OPEN4_RESULT_CONFIRM != 0 {
        let r = c.raw(
            e,
            0,
            vec![
                putfh(&fh),
                nfs_argop4::OP_OPEN_CONFIRM(t::OPEN_CONFIRM4args {
                    open_stateid: o.stateid.clone(),
                    seqid: c.next_oseq(),
                }),
            ],
        );
        assert_eq!(r.status, nfsstat4::NFS4_OK);
    }
    (fh, o)
}

fn deleg(o: &t::OPEN4resok) -> t::stateid4 {
    match &o.delegation {
        t::open_delegation4::OPEN_DELEGATE_READ(d) => d.stateid.clone(),
        d => panic!("no delegation: {d:?}"),
    }
}

fn delegreturn(e: &Env, c: &V4c, fh: &[u8], s: &t::stateid4) -> nfsstat4 {
    c.raw(
        e,
        0,
        vec![
            putfh(fh),
            nfs_argop4::OP_DELEGRETURN(t::DELEGRETURN4args {
                deleg_stateid: s.clone(),
            }),
        ],
    )
    .status
}

#[test]
fn read_delegations_recalled_returned_and_revoked() {
    let e = env();
    let cbs = Arc::new(Callbacks::default());
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let uaddr = to_uaddr(l.local_addr().unwrap());
    let stop = AtomicBool::new(false);
    let mut d = Dispatcher::new();
    d.add(cbs.clone());
    std::thread::scope(|sc| {
        let (l, d, stop) = (&l, &d, &stop);
        l.set_nonblocking(true).unwrap();
        sc.spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match l.accept() {
                    Ok((s, _)) => {
                        s.set_nonblocking(false).unwrap();
                        let _ = serve_tcp(&s, d, 1 << 20);
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        let body = || {
            let a = V4c::new("10.0.0.1:900", 100, 0);
            a.setclientid(&e, "deleg-a", &uaddr);
            // Until the path is probed nothing is delegated.
            let (_, o) = open_read(&e, &a, "h");
            assert_eq!(o.delegation, t::open_delegation4::OPEN_DELEGATE_NONE);
            until("CB_NULL", || *cbs.nulls.lock().unwrap() > 0, || e.tick());
            std::thread::sleep(Duration::from_millis(100));
            let (f, o) = open_read(&e, &a, "f");
            let ds = deleg(&o);
            assert_eq!(e.srv.delegations(), 1);
            // Another client opening for writing: recalled, DELAY.
            let b = V4c::new("10.0.0.2:900", 100, 0);
            b.setclientid(&e, "deleg-b", "127.0.0.1.0.1");
            let w = |b: &V4c| {
                b.raw(
                    &e,
                    0,
                    vec![
                        rootfh(),
                        lookup("home"),
                        lookup("x"),
                        b.open("o", "f", false, 2, 0),
                    ],
                )
                .status
            };
            assert_eq!(w(&b), nfsstat4::NFS4ERR_DELAY);
            until(
                "CB_RECALL",
                || !cbs.recalls.lock().unwrap().is_empty(),
                || e.tick(),
            );
            let (ident, rs, rfh) = cbs.recalls.lock().unwrap()[0].clone();
            assert_eq!(ident, 7);
            assert_eq!(rs, ds);
            assert_eq!(rfh, f);
            assert_eq!(delegreturn(&e, &a, &f, &ds), nfsstat4::NFS4_OK);
            assert_eq!(w(&b), nfsstat4::NFS4_OK);
            // A local job writes: the IFS orders the lease broken.
            let (g, o) = open_read(&e, &a, "g");
            let dg = deleg(&o);
            let j = e.w.job(5);
            let fd = j.open("/home/x/g", OpenArgs::new(O_WRONLY)).unwrap();
            j.pwrite(fd, b"X", 0).unwrap();
            j.close(fd).unwrap();
            let mut news = ClientNews(e.w.job(1));
            until(
                "the break's CB_RECALL",
                || cbs.recalls.lock().unwrap().len() >= 2,
                || {
                    if let Ok(n) = news.poll(50) {
                        for (tag, keep) in n.breaks {
                            e.srv.lease_break(tag, keep);
                        }
                    }
                    e.tick();
                },
            );
            assert_eq!(cbs.recalls.lock().unwrap()[1].1, dg);
            // Not returned within the lease: revoked.
            e.advance(91);
            e.tick();
            assert_eq!(e.srv.delegations(), 0);
            assert_eq!(delegreturn(&e, &a, &g, &dg), nfsstat4::NFS4ERR_EXPIRED);
            // DELEGPURGE: there is no CLAIM_DELEGATE_PREV.
            let r = a.raw(
                &e,
                0,
                vec![nfs_argop4::OP_DELEGPURGE(t::DELEGPURGE4args {
                    clientid: a.clientid.get(),
                })],
            );
            assert_eq!(r.status, nfsstat4::NFS4ERR_NOTSUPP);
        };
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
        stop.store(true, Ordering::SeqCst);
        if let Err(p) = r {
            std::panic::resume_unwind(p);
        }
    });
}

#[test]
fn no_delegation_without_a_callback_or_with_a_writer() {
    let e = env();
    let a = V4c::new("10.0.0.1:900", 100, 0);
    // A callback address nobody answers.
    a.setclientid(&e, "nocb", "127.0.0.1.0.9");
    e.tick();
    std::thread::sleep(Duration::from_millis(300));
    let (_, o) = open_read(&e, &a, "f");
    assert_eq!(o.delegation, t::open_delegation4::OPEN_DELEGATE_NONE);
}
