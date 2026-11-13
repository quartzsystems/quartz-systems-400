// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A daemon that stops after the binder is gone does not wait out the
//! binder for every registration: one short attempt, then it ends.

#![allow(clippy::many_single_char_names)]

mod srv3_common;

use std::net::{TcpListener, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use nfs::rpc::{Dispatcher, serve_datagram};
use nfs::rpcb::Binder;
use nfs::server::run::{self, Common, Listeners, Registration};
use srv3_common::World;

#[test]
fn a_daemon_ends_quickly_when_the_binder_is_gone() {
    let w = World::new();
    let e = w.export(1, "/home/exp");
    w.put(e);
    let bsock = UdpSocket::bind("127.0.0.1:0").unwrap();
    bsock
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let baddr = bsock.local_addr().unwrap();
    let bstop = Arc::new(AtomicBool::new(false));
    let b = {
        let bstop = Arc::clone(&bstop);
        std::thread::spawn(move || {
            let mut d = Dispatcher::new();
            d.add(Arc::new(Binder::default()));
            while !bstop.load(Ordering::Relaxed) {
                let _ = serve_datagram(&bsock, &d);
            }
        })
    };
    let stop = AtomicBool::new(false);
    let ready = AtomicUsize::new(0);
    let on = || {
        ready.fetch_add(1, Ordering::SeqCst);
    };
    std::thread::scope(|sc| {
        let h = sc.spawn(|| {
            let c = Common {
                listeners: Listeners {
                    tcp: Some(TcpListener::bind("127.0.0.1:0").unwrap()),
                    udp: Some(UdpSocket::bind("127.0.0.1:0").unwrap()),
                },
                binder: Some(Registration {
                    binder: baddr,
                    owner: "QNFSNFSD".into(),
                }),
                stop: &stop,
                ready: Some(&on),
            };
            run::nfsd(c, || w.nfs3()).unwrap();
        });
        let end = Instant::now() + Duration::from_secs(10);
        while ready.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(10));
        }
        // The binder ends first.
        bstop.store(true, Ordering::SeqCst);
        b.join().unwrap();
        let t = Instant::now();
        stop.store(true, Ordering::SeqCst);
        h.join().unwrap();
        // Four registrations (TCP and UDP, version 3): before, each
        // waited 2 + 4 + 8 seconds.
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
    });
}
