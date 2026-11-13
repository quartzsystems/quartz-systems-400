// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A remote mount's data path: read-ahead, write-behind with commits
//! and a server that restarts between, the first write-back error at
//! close, a file removed while open, synchronous writes, truncation of
//! data not yet written, and block I/O helpers taking the queue's work.

mod remote_common;

use std::sync::Arc;
use std::time::Duration;

use ifs::Errno;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_SYNC, O_TRUNC, O_WRONLY};
use ifs::remote::mem::MemRemote;
use ifs::remote::{Cred, HelperOp, HelperResult, RHandle, RemoteConfig, RemoteError, RemoteFs};
use remote_common::{drive, info, mounted, mounted_with, until};

#[test]
fn sequential_reads_read_ahead() {
    let r = mounted_with(
        info(),
        RemoteConfig {
            threads: 2,
            readahead: 4,
            ..RemoteConfig::default()
        },
    );
    let data: Vec<u8> = (0..16 * 1024u32)
        .map(|i| u8::try_from(i % 251).unwrap())
        .collect();
    r.server.put_file("big", &data);
    let c = r.client("BOB", 1);
    let fd = c.open("/mnt/big", OpenArgs::new(O_RDONLY)).unwrap();
    let mut got = Vec::new();
    let mut waits = 0;
    loop {
        let (chunk, runs) = drive(&c, |c| c.read(fd, 1024));
        waits += runs - 1;
        let chunk = chunk.unwrap();
        if chunk.is_empty() {
            break;
        }
        got.extend_from_slice(&chunk);
    }
    assert_eq!(got, data);
    // Each block was read once, and most reads found theirs ahead.
    assert_eq!(r.server.calls_of("read"), 16);
    assert!(waits < 8, "{waits} waits for 16 blocks");
    c.close(fd).unwrap();
}

#[test]
fn write_behind_survives_a_server_restart() {
    let r = mounted();
    let c = r.client("BOB", 1);
    let data: Vec<u8> = (0..8 * 1024u32)
        .map(|i| u8::try_from(i % 253).unwrap())
        .collect();
    let fd = c
        .open("/mnt/w", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o644))
        .unwrap();
    for piece in data.chunks(1024) {
        write_all(&c, fd, piece).unwrap();
    }
    // Full blocks go ahead, unstable, before the close.
    until(|| r.server.calls_of("write") >= 2);
    // The server restarts: what it had not committed is lost.
    r.server.restart();
    c.close(fd).unwrap();
    assert_eq!(r.server.file("w").unwrap(), data);
    // Sent again, stable, after the commit's verifier changed.
    assert!(r.server.calls_of("commit") >= 1);
    assert!(r.server.calls_of("write") > 8);
    // And it survives another restart.
    r.server.restart();
    assert_eq!(r.server.file("w").unwrap(), data);
}

#[test]
fn close_reports_the_first_write_back_error() {
    let r = mounted();
    let c = r.client("BOB", 1);
    let fd = c
        .open("/mnt/e", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o644))
        .unwrap();
    r.server
        .fail_next(1000, RemoteError::Errno(Errno::ENOSPC), Some("write"));
    assert_eq!(c.write(fd, b"no room").unwrap(), 7);
    assert_eq!(c.close(fd), Err(Errno::ENOSPC));
    r.server.fail_next(0, RemoteError::TimedOut, None);
    // The descriptor is gone all the same.
    assert_eq!(c.close(fd), Err(Errno::EBADF));
    let fd = c.open("/mnt/e", OpenArgs::new(O_WRONLY)).unwrap();
    c.write(fd, b"room").unwrap();
    c.fsync(fd, false).unwrap();
    c.close(fd).unwrap();
    assert_eq!(r.server.file("e").unwrap(), b"room");
}

#[test]
fn a_file_removed_while_open_lives_until_closed() {
    let r = mounted();
    let c = r.client("BOB", 1);
    r.server.put_file("s", b"still here");
    let fd = c.open("/mnt/s", OpenArgs::new(O_RDONLY)).unwrap();
    c.unlink("/mnt/s").unwrap();
    assert_eq!(c.stat("/mnt/s", true), Err(Errno::ENOENT));
    let hidden: Vec<String> = r.server.names("");
    assert_eq!(hidden.len(), 1);
    assert!(hidden[0].starts_with(".nfs"), "{hidden:?}");
    r.mount().drop_cache();
    assert_eq!(read_all(&c, fd).unwrap(), b"still here");
    c.close(fd).unwrap();
    until(|| r.server.names("").is_empty());
    // Not open: removed at once.
    r.server.put_file("t", b"t");
    c.unlink("/mnt/t").unwrap();
    assert!(r.server.names("").is_empty());
}

#[test]
fn synchronous_writes_and_truncation() {
    let r = mounted();
    let c = r.client("BOB", 1);
    let fd = c
        .open(
            "/mnt/s",
            OpenArgs::new(O_WRONLY | O_CREAT | O_SYNC).mode(0o644),
        )
        .unwrap();
    c.write(fd, b"at once").unwrap();
    assert_eq!(r.server.file("s").unwrap(), b"at once");
    r.server.restart();
    assert_eq!(r.server.file("s").unwrap(), b"at once");
    c.close(fd).unwrap();
    let fd = c
        .open(
            "/mnt/t",
            OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(0o644),
        )
        .unwrap();
    write_all(&c, fd, &[9u8; 5000]).unwrap();
    c.ftruncate(fd, 100).unwrap();
    assert_eq!(c.fstat(fd).unwrap().size, 100);
    c.close(fd).unwrap();
    assert_eq!(r.server.file("t").unwrap(), vec![9u8; 100]);
}

#[test]
fn block_io_helpers_take_queued_work() {
    let r = mounted_with(
        info(),
        RemoteConfig {
            threads: 1,
            readahead: 4,
            ..RemoteConfig::default()
        },
    );
    let data: Vec<u8> = (0..4096u32).map(|i| u8::try_from(i % 7).unwrap()).collect();
    r.server.put_file("h", &data);
    let r = Arc::new(r);
    let c = r.client("BOB", 1);
    let fd = c.open("/mnt/h", OpenArgs::new(O_RDONLY)).unwrap();
    // The mount's one thread is slow: the reads wait in the queue.
    r.server.set_latency(Duration::from_millis(300));
    let r2 = Arc::clone(&r);
    let reader = std::thread::spawn(move || {
        let c = r2.client("BOB", 1);
        c.pread(fd, 4096, 0)
    });
    let bob = r.client("BOB", 1);
    // A helper (QNFSBIOD) without *IOSYSCFG is refused.
    assert_eq!(bob.remote_take(), Err(Errno::EPERM));
    let helper = r.client("QSECOFR", 3);
    let mut work = None;
    for _ in 0..100 {
        work = helper.remote_take().unwrap();
        if work.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let w = work.expect("work for a helper");
    assert_eq!(w.op, HelperOp::Read);
    assert_eq!(w.source, "srv:/exp");
    // The helper makes the call over its own connection.
    r.server.set_latency(Duration::ZERO);
    let fs = MemRemote(Arc::clone(&r.server));
    let cred = Cred {
        uid: w.uid,
        gid: w.gid,
        gids: w.gids.clone(),
    };
    let (bytes, eof) = fs
        .read(&cred, &RHandle(w.handle.clone()), w.off, w.len)
        .unwrap();
    helper
        .remote_post(
            w.fs,
            w.id,
            &HelperResult {
                data: bytes,
                eof,
                ..HelperResult::default()
            },
        )
        .unwrap();
    assert_eq!(
        helper.remote_post(w.fs, w.id, &HelperResult::default()),
        Err(Errno::ENOENT)
    );
    // The file reads whole, the helper's block among the rest.
    let mut all = reader.join().unwrap().unwrap();
    while all.len() < 4096 {
        let more = r
            .client("BOB", 1)
            .pread(fd, 4096, all.len() as u64)
            .unwrap();
        assert!(!more.is_empty());
        all.extend(more);
    }
    assert_eq!(all, data);
    c.close(fd).unwrap();
}

#[test]
fn small_files_and_holes() {
    let r = mounted();
    let c = r.client("BOB", 1);
    let fd = c
        .open("/mnt/holes", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o644))
        .unwrap();
    c.pwrite(fd, b"end", 3000).unwrap();
    c.pwrite(fd, b"start", 0).unwrap();
    c.close(fd).unwrap();
    let mut want = vec![0u8; 3003];
    want[..5].copy_from_slice(b"start");
    want[3000..].copy_from_slice(b"end");
    assert_eq!(r.server.file("holes").unwrap(), want);
    let fd = c.open("/mnt/holes", OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(read_all(&c, fd).unwrap(), want);
    c.close(fd).unwrap();
}
