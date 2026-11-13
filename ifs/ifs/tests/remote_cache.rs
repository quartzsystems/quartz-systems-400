// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A remote mount's cache: attributes within their time to live and
//! asked again after it, close-to-open, names known absent, listings,
//! data dropped when the server's change value moves, and one cache
//! for every job.

mod remote_common;

use std::time::Duration;

use ifs::Errno;
use ifs::client::{IfsClient, read_all, write_all};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_RDWR, O_WRONLY};
use remote_common::mounted;

/// Past the longest attribute time of `remote_common::info`.
const PAST: Duration = Duration::from_millis(700);

#[test]
fn attributes_live_their_time() {
    let r = mounted();
    let c = r.client("BOB", 1);
    r.server.put_file("f", b"abc");
    assert_eq!(c.stat("/mnt/f", true).unwrap().size, 3);
    let calls = r.server.calls();
    assert_eq!(c.stat("/mnt/f", true).unwrap().size, 3);
    assert_eq!(r.server.calls(), calls, "answered from the cache");
    // Behind the mount's back: seen once the time is up.
    r.server.put_file("f", b"abcdef");
    assert_eq!(c.stat("/mnt/f", true).unwrap().size, 3);
    std::thread::sleep(PAST);
    assert_eq!(c.stat("/mnt/f", true).unwrap().size, 6);
    assert!(r.server.calls() > calls);
}

#[test]
fn an_open_asks_the_server() {
    let r = mounted();
    let c = r.client("BOB", 1);
    r.server.put_file("f", b"one");
    let fd = c.open("/mnt/f", OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(read_all(&c, fd).unwrap(), b"one");
    c.close(fd).unwrap();
    // Another client wrote and closed: the next open sees it, though
    // the attributes are still within their time.
    r.server.put_file("f", b"two!");
    let fd = c.open("/mnt/f", OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(read_all(&c, fd).unwrap(), b"two!");
    c.close(fd).unwrap();
}

#[test]
fn data_goes_when_the_change_value_moves() {
    let r = mounted();
    let c = r.client("BOB", 1);
    r.server.put_file("f", &[b'a'; 3000]);
    let fd = c.open("/mnt/f", OpenArgs::new(O_RDONLY)).unwrap();
    assert_eq!(c.pread(fd, 10, 0).unwrap(), vec![b'a'; 10]);
    let reads = r.server.calls_of("read");
    assert_eq!(c.pread(fd, 10, 0).unwrap(), vec![b'a'; 10]);
    assert_eq!(r.server.calls_of("read"), reads, "a cached block");
    r.server.put_file("f", &[b'b'; 3000]);
    std::thread::sleep(PAST);
    assert_eq!(c.pread(fd, 10, 0).unwrap(), vec![b'b'; 10]);
    c.close(fd).unwrap();
}

#[test]
fn absent_names_and_listings_are_cached() {
    let r = mounted();
    let c = r.client("BOB", 1);
    assert_eq!(c.stat("/mnt/new", true), Err(Errno::ENOENT));
    assert!(c.read_dir("/mnt").unwrap().is_empty());
    r.server.put_file("new", b"n");
    assert_eq!(c.stat("/mnt/new", true), Err(Errno::ENOENT));
    assert!(c.read_dir("/mnt").unwrap().is_empty());
    std::thread::sleep(PAST);
    assert_eq!(c.stat("/mnt/new", true).unwrap().size, 1);
    let names: Vec<String> = c
        .read_dir("/mnt")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, vec!["new"]);
    // Many entries come in pages.
    r.server.set_page(3);
    for n in 0..10 {
        r.server.put_file(&format!("p{n}"), b"");
    }
    std::thread::sleep(PAST);
    assert_eq!(c.read_dir("/mnt").unwrap().len(), 11);
    // Changes made here show at once.
    c.mkdir("/mnt/here", 0o777).unwrap();
    assert_eq!(c.read_dir("/mnt").unwrap().len(), 12);
}

#[test]
fn every_job_shares_one_cache() {
    let r = mounted();
    let a = r.client("BOB", 1);
    let b = r.client("BOB", 2);
    let fa = a
        .open("/mnt/f", OpenArgs::new(O_RDWR | O_CREAT).mode(0o666))
        .unwrap();
    write_all(&a, fa, b"written by a").unwrap();
    // B sees A's data before it reaches the server.
    let fb = b.open("/mnt/f", OpenArgs::new(O_RDWR)).unwrap();
    assert_eq!(read_all(&b, fb).unwrap(), b"written by a");
    assert_eq!(b.pwrite(fb, b"B", 0).unwrap(), 1);
    assert_eq!(a.pread(fa, 12, 0).unwrap(), b"Britten by a");
    a.close(fa).unwrap();
    b.close(fb).unwrap();
    assert_eq!(r.server.file("f").unwrap(), b"Britten by a");
    let c = r.client("ANN", 3);
    let fd = c.open("/mnt/f", OpenArgs::new(O_WRONLY));
    // The server's mode: rw for everyone (0o666 less BOB's umask 022).
    assert_eq!(fd, Err(Errno::EACCES));
}
