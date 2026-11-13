// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `soft`, `hard`, `intr`, `timeo` and `retrans` against a server that
//! stops answering, stops and starts again, or restarts; reconnection
//! after the server closes the connection; calls from many threads.

#![allow(clippy::many_single_char_names, clippy::similar_names)]

mod cli_common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use cli_common::{BlackHole, ServerSys, bob};
use ifs::remote::{CreateMode, MountCcsid, RemoteError, RemoteFs, SetAttr, Stable};
use nfs::client::{self, ClientEnv, Hooks, Mount};
use nfs::exports::mount::parse;

fn mount(s: &ServerSys, opts: &str, env: &ClientEnv) -> Mount {
    let o = parse(&format!("{opts},{}", s.ports_opts())).unwrap();
    client::connect(
        "127.0.0.1:/home/exp",
        &o,
        MountCcsid::default(),
        &bob(),
        env,
    )
    .unwrap()
}

#[test]
fn soft_gives_up_after_one_major_timeout() {
    for proto in ["tcp", "udp"] {
        let s = ServerSys::start(false, false);
        // 0.2 + 0.4 + 0.8 seconds.
        let m = mount(
            &s,
            &format!("soft,timeo=2,retrans=2,proto={proto}"),
            &s.env("c1"),
        );
        let (root, _) = m.root().unwrap();
        s.stop();
        let _hole = BlackHole::on(s.ports.nfs);
        let t = Instant::now();
        assert_eq!(m.getattr(&bob(), &root).unwrap_err(), RemoteError::TimedOut);
        let took = t.elapsed();
        assert!(
            took >= Duration::from_millis(1350) && took < Duration::from_millis(2600),
            "{proto}: {took:?}"
        );
    }
}

#[test]
fn hard_rides_out_a_restart_and_reports_it_once() {
    let s = ServerSys::start(false, false);
    let down = Arc::new(AtomicUsize::new(0));
    let up = Arc::new(AtomicUsize::new(0));
    let (d2, u2) = (Arc::clone(&down), Arc::clone(&up));
    let env = ClientEnv {
        hooks: Hooks {
            not_responding: Some(Arc::new(move || {
                d2.fetch_add(1, Ordering::SeqCst);
            })),
            responding: Some(Arc::new(move || {
                u2.fetch_add(1, Ordering::SeqCst);
            })),
        },
        ..s.env("c1")
    };
    let m = Arc::new(mount(&s, "hard,timeo=1,retrans=1", &env));
    let (root, _) = m.root().unwrap();
    let (f, _) = m
        .create(
            &bob(),
            &root,
            b"f",
            CreateMode::Unchecked(SetAttr::default()),
        )
        .unwrap();
    let w = m
        .write(&bob(), &f, 0, b"unstable", Stable::Unstable)
        .unwrap();
    s.stop();
    // Several calls wait through the outage; the server is reported once.
    let waiting: Vec<_> = (0..3)
        .map(|_| {
            let (m, root) = (Arc::clone(&m), root.clone());
            std::thread::spawn(move || m.getattr(&bob(), &root))
        })
        .collect();
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(down.load(Ordering::SeqCst), 1);
    assert_eq!(up.load(Ordering::SeqCst), 0);
    assert!(!m.responding());
    s.restart();
    for t in waiting {
        t.join().unwrap().unwrap();
    }
    assert_eq!(up.load(Ordering::SeqCst), 1);
    assert_eq!(down.load(Ordering::SeqCst), 1);
    assert!(m.responding());
    // The restart lost the unstable write: the verifier says so.
    let v = m.commit(&bob(), &f, 0, 0).unwrap();
    assert_ne!(v, w.verifier);
}

#[test]
fn intr_takes_a_waiting_call_back() {
    let s = ServerSys::start(false, false);
    let m = Arc::new(mount(&s, "hard,intr,timeo=1,retrans=1", &s.env("c1")));
    let (root, _) = m.root().unwrap();
    s.stop();
    let _hole = BlackHole::on(s.ports.nfs);
    let (m2, r2) = (Arc::clone(&m), root.clone());
    let t = std::thread::spawn(move || m2.getattr(&bob(), &r2));
    std::thread::sleep(Duration::from_millis(500));
    assert!(m.interrupt());
    assert_eq!(t.join().unwrap().unwrap_err(), RemoteError::Interrupted);
}

#[test]
fn without_intr_a_call_waits_for_the_server() {
    let s = ServerSys::start(false, false);
    let m = Arc::new(mount(&s, "hard,timeo=1,retrans=1", &s.env("c1")));
    let (root, _) = m.root().unwrap();
    s.stop();
    let (m2, r2) = (Arc::clone(&m), root.clone());
    let t = std::thread::spawn(move || m2.getattr(&bob(), &r2));
    std::thread::sleep(Duration::from_millis(400));
    assert!(!m.interrupt(), "nointr: nothing is taken back");
    s.run();
    t.join().unwrap().unwrap();
}

#[test]
fn a_closed_connection_is_made_again() {
    let s = ServerSys::start(false, false);
    let m = mount(&s, "soft,timeo=5,retrans=4,proto=tcp", &s.env("c1"));
    let (root, _) = m.root().unwrap();
    for i in 0..3 {
        // The server's daemons end and start: every connection closes.
        s.stop();
        s.run();
        let (_, a) = m
            .mkdir(
                &bob(),
                &root,
                format!("d{i}").as_bytes(),
                &SetAttr::default(),
            )
            .unwrap();
        assert_eq!(a.kind, ifs::remote::RKind::Dir);
    }
}

#[test]
fn many_threads_call_at_once() {
    let s = ServerSys::start(false, false);
    let m = Arc::new(mount(&s, "soft,timeo=10,retrans=3", &s.env("c1")));
    let (root, _) = m.root().unwrap();
    let ts: Vec<_> = (0..8)
        .map(|i| {
            let (m, root) = (Arc::clone(&m), root.clone());
            std::thread::spawn(move || {
                let name = format!("t{i}");
                let (f, _) = m
                    .create(
                        &bob(),
                        &root,
                        name.as_bytes(),
                        CreateMode::Unchecked(SetAttr::default()),
                    )
                    .unwrap();
                for k in 0..50u64 {
                    m.write(
                        &bob(),
                        &f,
                        k * 100,
                        &[u8::try_from(i).unwrap(); 100],
                        Stable::FileSync,
                    )
                    .unwrap();
                }
                m.getattr(&bob(), &f).unwrap().size
            })
        })
        .collect();
    for t in ts {
        assert_eq!(t.join().unwrap(), 5000);
    }
}
