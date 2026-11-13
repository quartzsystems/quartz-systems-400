// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Kept descriptors yield at once: after an `NFSv3` `WRITE` a daemon
//! thread keeps the file open for writing; a local job's open that
//! denies everyone else (`O_SHARE_NONE`) is refused while it does, and
//! goes through as soon as the job's news poller carries out the lease
//! break, with no further NFS call.

#![allow(clippy::many_single_char_names)]
mod srv3_common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::lease::News;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_RDWR, O_SHARE_NONE};
use nfs::server::ifsvfs::kept::{self, JobNews};
use nfs::server::news::NewsSource;
use nfs::types::nfs3::NFS_PROGRAM::NFS_V3::{Args, Res};
use nfs::types::nfs3::{self as t3};
use srv3_common::{Client, World, fh};

fn write(w: &World, c: &Client, p: &dyn nfs::rpc::LocalProgram, file: &[u8]) {
    let r = c.v3(
        p,
        &Args::NFSPROC3_WRITE(t3::WRITE3args {
            file: fh(file),
            offset: 0,
            count: 5,
            stable: t3::stable_how::FILE_SYNC,
            data: b"hello".to_vec(),
        }),
    );
    assert!(
        matches!(r, Res::NFSPROC3_WRITE(t3::WRITE3res::NFS3_OK(_))),
        "{r:?}"
    );
    assert_eq!(w.read_file("/home/kept/f"), b"hello");
}

fn deny_all(w: &World) -> Result<i32, Errno> {
    w.job(7)
        .open("/home/kept/f", OpenArgs::new(O_RDWR | O_SHARE_NONE))
}

#[test]
fn a_local_deny_open_waits_only_for_the_poller() {
    let w = World::new();
    let e = w.export(1, "/home/kept");
    w.put(e);
    w.write_file("/home/kept/f", b"");
    w.job(9).chmod("/home/kept/f", 0o666).unwrap();
    let file = w.handle(1, "/home/kept/f");
    let p = w.nfs3();
    let c = Client::new("10.0.0.5:700", 100);
    write(&w, &c, &p, &file);
    assert_eq!(p.vfs.cached_fds(), 1, "the WRITE's descriptor is kept");
    // The job's poller (its own client, the same owner) carries out the
    // break the refused open orders. (Without it the open waits for the
    // break's time-out, 35 seconds, and is refused.)
    let stop = AtomicBool::new(false);
    let fd = std::thread::scope(|s| {
        s.spawn(|| {
            let mut n = JobNews::new(w.job(1), Arc::clone(&w.instance));
            while !stop.load(Ordering::SeqCst) {
                let news = n.poll(50).unwrap();
                assert!(news.breaks.is_empty(), "nothing else to pass on");
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let start = Instant::now();
        let end = start + Duration::from_secs(3);
        let fd = loop {
            match deny_all(&w) {
                Ok(fd) => break fd,
                Err(Errno::EBUSY) if Instant::now() < end => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => panic!("the open still fails: {e:?}"),
            }
        };
        assert!(start.elapsed() < Duration::from_secs(3));
        stop.store(true, Ordering::SeqCst);
        fd
    });
    assert_eq!(p.vfs.cached_fds(), 0, "the kept descriptor was closed");
    w.job(7).close(fd).unwrap();
    // The next WRITE opens and keeps one again.
    write(&w, &c, &p, &file);
    assert_eq!(p.vfs.cached_fds(), 1);
}

#[test]
fn other_break_orders_are_passed_on() {
    let w = World::new();
    let n = JobNews::new(w.job(1), Arc::clone(&w.instance));
    let news = n.sift(News {
        breaks: vec![(5, 0), (kept::tag_of(40), 0), (6, 1)],
        ..News::default()
    });
    // A delegation's tag (no top bit) is never a kept descriptor's.
    assert_eq!(news.breaks, [(5, 0), (6, 1)]);
    assert!(!kept::is_kept(5) && kept::is_kept(kept::tag_of(0)));
}
