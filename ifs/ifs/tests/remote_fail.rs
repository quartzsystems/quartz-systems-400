// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A remote mount when things go wrong or slowly: a change is made
//! once however often its request runs; `intr` takes a waiting job
//! back; a soft mount times out; a hard mount waits out an outage while
//! other jobs go on, and says when its server stops and starts
//! answering; stale handles; injected errors; no livelock when the
//! attributes expire at once.

mod remote_common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ifs::Errno;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::lease::defer::deferring;
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_WRONLY};
use ifs::ops::{OpenArgs, Owner};
use ifs::remote::{AttrTimes, RemoteConfig, RemoteError, RemoteEvent, request};
use remote_common::{drive, info, mounted, mounted_with, until};

fn cfg() -> RemoteConfig {
    RemoteConfig {
        threads: 2,
        ..RemoteConfig::default()
    }
}

#[test]
#[allow(clippy::many_single_char_names)]
fn a_change_is_made_once() {
    let r = mounted();
    r.server.set_latency(Duration::from_millis(30));
    let c = r.client("BOB", 1);
    // Run as the service does: deferred, run again with the same token.
    let (res, runs) = drive(&c, |c| c.mkdir("/mnt/x", 0o777));
    assert_eq!(res, Ok(()));
    assert!(runs > 1);
    assert_eq!(r.server.calls_of("mkdir"), 1);
    // A run again before the work is done changes nothing more.
    let (res, d) = deferring(true, || c.with(|c| c.mkdir("/mnt/y", 0o777)));
    assert_eq!(res, Err(Errno::EAGAIN));
    let d = d.expect("deferred");
    let t = request::last().unwrap();
    for _ in 0..3 {
        request::rerun(t);
        let (_, again) = deferring(true, || c.with(|c| c.mkdir("/mnt/y", 0o777)));
        if again.is_none() {
            break;
        }
    }
    let mut d = d;
    let res = loop {
        r.ifs.wait_deferral(&d);
        request::rerun(t);
        let (res, again) = deferring(true, || c.with(|c| c.mkdir("/mnt/y", 0o777)));
        match again {
            Some(x) => d = x,
            None => break res,
        }
    };
    assert_eq!(res, Ok(()));
    assert_eq!(r.server.calls_of("mkdir"), 2);
    // Another request is another change.
    assert_eq!(c.mkdir("/mnt/y", 0o777), Err(Errno::EEXIST));
    // The same for files, renames, removals.
    let (fd, _) = drive(&c, |c| {
        c.open(
            "/mnt/f",
            OpenArgs::new(O_WRONLY | O_CREAT | ifs::ops::flags::O_EXCL).mode(0o644),
        )
    });
    c.close(fd.unwrap()).unwrap();
    assert_eq!(r.server.calls_of("create"), 1);
    let (res, _) = drive(&c, |c| {
        c.rename("/mnt/f", "/mnt/g", ifs::ops::RenameMode::Keep)
    });
    assert_eq!(res, Ok(()));
    let (res, _) = drive(&c, |c| c.unlink("/mnt/g"));
    assert_eq!(res, Ok(()));
    assert_eq!(
        (r.server.calls_of("rename"), r.server.calls_of("remove")),
        (1, 1)
    );
    // Outside a request that may wait, nothing is queued.
    let (res, d) = deferring(false, || c.with(|c| c.mkdir("/mnt/z", 0o777)));
    assert_eq!((res, d), (Err(Errno::EAGAIN), None));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!r.server.exists("z"));
}

#[test]
fn intr_takes_a_waiting_job_back() {
    let mut i = info();
    i.intr = true;
    let r = Arc::new(mounted_with(i, cfg()));
    r.server.put_file("f", b"x");
    r.server.set_down(true, false);
    let r2 = Arc::clone(&r);
    let t = std::thread::spawn(move || r2.client("BOB", 7).stat("/mnt/f", true));
    // Once it waits, it is taken back.
    until(|| r.ifs.interrupt_owner(Owner(7)));
    assert_eq!(t.join().unwrap(), Err(Errno::EINTR));
    r.server.set_down(false, false);
    assert_eq!(r.client("BOB", 8).stat("/mnt/f", true).unwrap().size, 1);
}

#[test]
fn without_intr_the_wait_goes_on() {
    let r = Arc::new(mounted());
    r.server.put_file("f", b"x");
    r.server.set_down(true, false);
    let r2 = Arc::clone(&r);
    let t = std::thread::spawn(move || r2.client("BOB", 7).stat("/mnt/f", true));
    std::thread::sleep(Duration::from_millis(100));
    r.ifs.interrupt_owner(Owner(7));
    std::thread::sleep(Duration::from_millis(100));
    assert!(!t.is_finished());
    r.server.set_down(false, false);
    assert_eq!(t.join().unwrap().unwrap().size, 1);
}

#[test]
fn a_soft_mount_times_out() {
    let mut i = info();
    i.soft = true;
    let r = mounted_with(i, cfg());
    r.server.put_file("f", b"x");
    r.server.set_down(true, true);
    let c = r.client("BOB", 1);
    assert_eq!(c.stat("/mnt/f", true), Err(Errno::ETIMEDOUT));
    r.server.set_down(false, false);
    assert_eq!(c.stat("/mnt/f", true).unwrap().size, 1);
}

#[test]
fn a_hard_mount_waits_out_an_outage_and_others_go_on() {
    let mut i = info();
    i.slow_after = Duration::from_millis(100);
    let r = remote_common::rig_with(i, cfg());
    let seen: Arc<Mutex<Vec<RemoteEvent>>> = Arc::default();
    let s2 = Arc::clone(&seen);
    r.ifs
        .set_remote_events(Some(Arc::new(move |e: &RemoteEvent| {
            s2.lock().unwrap().push(e.clone());
        })));
    let sec = r.client("QSECOFR", 900);
    sec.mount_remote(
        "*NFS",
        "srv:/exp",
        "/mnt",
        "",
        ifs::remote::MountCcsid::default(),
    )
    .unwrap();
    r.server.put_file("f", b"after the outage");
    r.server.set_down(true, false);
    let r = Arc::new(r);
    let done = Arc::new(AtomicBool::new(false));
    let (r2, d2) = (Arc::clone(&r), Arc::clone(&done));
    let t = std::thread::spawn(move || {
        let c = r2.client("BOB", 7);
        let fd = c.open("/mnt/f", OpenArgs::new(O_RDONLY))?;
        let data = read_all(&c, fd)?;
        c.close(fd)?;
        d2.store(true, Ordering::SeqCst);
        Ok::<_, Errno>(data)
    });
    // Another job's local work goes on meanwhile, at once.
    let local = r.client("ANN", 8);
    let start = Instant::now();
    for n in 0..20 {
        let p = format!("/tmp/n{n}");
        let fd = local
            .open(&p, OpenArgs::new(O_WRONLY | O_CREAT).mode(0o644))
            .unwrap();
        write_all(&local, fd, b"local").unwrap();
        local.close(fd).unwrap();
        assert_eq!(local.stat(&p, true).unwrap().size, 5);
    }
    assert!(start.elapsed() < Duration::from_secs(2));
    // The outage outlives what the mount calls slow: reported once.
    until(|| !r.mount().responding());
    std::thread::sleep(Duration::from_millis(400));
    assert!(!done.load(Ordering::SeqCst));
    let rows = r.client("QSECOFR", 900).mounts().unwrap();
    assert_eq!(
        rows.iter().find(|m| m.kind == "*NFS").unwrap().responding,
        Some(false)
    );
    r.server.set_down(false, false);
    assert_eq!(t.join().unwrap().unwrap(), b"after the outage");
    until(|| r.mount().responding());
    let ev = seen.lock().unwrap().clone();
    assert!(matches!(ev[0], RemoteEvent::NotResponding { .. }), "{ev:?}");
    assert!(matches!(ev[1], RemoteEvent::Responding { .. }), "{ev:?}");
    assert_eq!(ev.len(), 2, "{ev:?}");
}

#[test]
fn stale_handles() {
    let r = mounted();
    let c = r.client("BOB", 1);
    r.server.put_file("f", b"old");
    let fd = c.open("/mnt/f", OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(c.read(fd, 10).unwrap(), b"old");
    r.server.recreate("f");
    r.mount().drop_cache();
    assert_eq!(c.pread(fd, 10, 0), Err(Errno::ESTALE));
    c.close(fd).unwrap();
    // The path names the new object.
    assert_eq!(c.stat("/mnt/f", true).unwrap().size, 0);
}

#[test]
fn errors_from_the_server() {
    let r = mounted();
    let c = r.client("BOB", 1);
    r.server.put_file("f", b"x");
    r.server
        .fail_next(1, RemoteError::Errno(Errno::EIO), Some("lookup"));
    assert_eq!(c.stat("/mnt/f", true), Err(Errno::EIO));
    // A failure is good for its request only: the next one asks again.
    assert_eq!(c.stat("/mnt/f", true).unwrap().size, 1);
    r.server
        .fail_next(1, RemoteError::Errno(Errno::ENOSPC), Some("mkdir"));
    assert_eq!(c.mkdir("/mnt/d", 0o777), Err(Errno::ENOSPC));
    c.mkdir("/mnt/d", 0o777).unwrap();
}

#[test]
fn no_livelock_when_attributes_expire_at_once() {
    let mut i = info();
    i.attr_times = AttrTimes::NONE;
    let r = mounted_with(i, cfg());
    r.server.set_latency(Duration::from_millis(5));
    let mut p = String::new();
    for d in ["a", "b", "c", "d", "e", "f"] {
        p.push_str(d);
        r.server.put_dir(&p);
        p.push('/');
    }
    r.server.put_file(&format!("{p}leaf"), b"deep");
    let c = r.client("BOB", 1);
    let path = format!("/mnt/{p}leaf");
    for _ in 0..3 {
        let (st, runs) = drive(&c, |c| c.stat(&path, true));
        assert_eq!(st.unwrap().size, 4);
        // The walk queues the rest of the path at once: few runs.
        assert!(runs <= 12, "{runs} runs");
    }
    let (fd, runs) = drive(&c, |c| c.open(&path, OpenArgs::new(O_RDONLY)));
    let fd = fd.unwrap();
    assert!(runs <= 12, "{runs} runs");
    let (data, runs) = drive(&c, |c| c.pread(fd, 100, 0));
    c.close(fd).unwrap();
    assert_eq!(data.unwrap(), b"deep");
    assert!(runs <= 16, "{runs} runs");
}

#[test]
fn an_abandoned_mount_ends_on_the_server() {
    let r = remote_common::rig_with(info(), cfg());
    r.server.set_latency(Duration::from_millis(50));
    let c = r.client("QSECOFR", 1);
    let (res, d) = deferring(true, || {
        c.with(|c| {
            c.mount_remote(
                "*NFS",
                "srv:/exp",
                "/mnt",
                "",
                ifs::remote::MountCcsid::default(),
            )
        })
    });
    assert_eq!(res, Err(Errno::EAGAIN));
    // Its job went before the mount was made.
    request::abandon(request::last().unwrap());
    r.ifs.wait_deferral(&d.unwrap());
    until(|| r.server.unmounted());
    assert!(c.mounts().unwrap().iter().all(|m| m.kind != "*NFS"));
}
