// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Archive torture: random data through LZW, deflate and inflate, every
//! archive format and the zip container, random trees through `pax`,
//! `tar` and `ajar` on the seeded `MemSys`, and random bytes into every
//! decoder (which must answer, never panic).

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::many_single_char_names,
    clippy::range_plus_one
)]

mod common;

use qsh_utils::archive::codec::{self, Format};
use qsh_utils::archive::cpio::Cpio;
use qsh_utils::archive::lzw;
use qsh_utils::archive::member::{Kind, Member};
use qsh_utils::archive::zip::{self, Entry, deflate::deflate, inflate::inflate};

/// A small deterministic generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    /// Bytes with a chosen alphabet size and some repeated runs.
    fn data(&mut self, len: usize, alphabet: u64) -> Vec<u8> {
        let mut v = Vec::with_capacity(len);
        while v.len() < len {
            if !v.is_empty() && self.below(4) == 0 {
                let start = self.below(v.len() as u64) as usize;
                let n = (self.below(300) as usize).min(len - v.len());
                for k in 0..n {
                    v.push(v[start + k % (v.len() - start)]);
                }
            } else {
                v.push(self.below(alphabet) as u8);
            }
        }
        v
    }
}

const SIZES: &[usize] = &[0, 1, 2, 3, 10, 255, 256, 257, 1000, 4096, 40_000, 140_000];

#[test]
fn lzw_round_trips() {
    let mut r = Rng(0x5eed_0001);
    for &len in SIZES {
        for alphabet in [2, 16, 256] {
            let d = r.data(len, alphabet);
            let bits = 9 + r.below(8) as u32;
            let z = lzw::compress(&d, bits);
            assert_eq!(lzw::decompress(&z).unwrap(), d, "len {len} alphabet {alphabet} bits {bits}");
        }
    }
    // Enough distinct strings to fill a 9-bit table many times over.
    let d = r.data(300_000, 256);
    assert_eq!(lzw::decompress(&lzw::compress(&d, 9)).unwrap(), d);
}

#[test]
fn lzw_garbage_never_panics() {
    let mut r = Rng(0x5eed_0002);
    for _ in 0..3000 {
        let mut d = vec![0x1f, 0x9d, (0x80 | (9 + r.below(9))) as u8];
        let n = r.below(200) as usize;
        d.extend((0..n).map(|_| r.next() as u8));
        let _ = lzw::decompress(&d);
    }
}

#[test]
fn deflate_round_trips() {
    let mut r = Rng(0x5eed_0003);
    for &len in SIZES {
        for alphabet in [1, 4, 64, 256] {
            let d = r.data(len, alphabet);
            assert_eq!(inflate(&deflate(&d)).unwrap(), d, "len {len} alphabet {alphabet}");
        }
    }
    let d = r.data(500_000, 20);
    let z = deflate(&d);
    assert!(z.len() < d.len() / 2);
    assert_eq!(inflate(&z).unwrap(), d);
}

#[test]
fn inflate_garbage_never_panics() {
    let mut r = Rng(0x5eed_0004);
    for _ in 0..5000 {
        let n = r.below(120) as usize;
        let d: Vec<u8> = (0..n).map(|_| r.next() as u8).collect();
        let _ = inflate(&d);
    }
    // Flipped bits in real streams.
    let base = deflate(&r.data(3000, 30));
    for _ in 0..2000 {
        let mut d = base.clone();
        let i = r.below(d.len() as u64) as usize;
        d[i] ^= 1 << r.below(8);
        let _ = inflate(&d);
    }
}

fn name(r: &mut Rng) -> String {
    let parts = 1 + r.below(4);
    (0..parts)
        .map(|_| {
            let n = 1 + r.below(12) as usize;
            (0..n)
                .map(|_| char::from(b"abcdefghijklmnopqrstuvwxyz0123456789._-"[r.below(39) as usize]))
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[test]
fn formats_round_trip() {
    let mut r = Rng(0x5eed_0005);
    let formats = [
        Format::Ustar,
        Format::Tar,
        Format::Cpio(Cpio::Odc),
        Format::Cpio(Cpio::Newc),
        Format::Cpio(Cpio::Crc),
    ];
    for f in formats {
        for _ in 0..30 {
            let mut ms = Vec::new();
            for i in 0..1 + r.below(12) {
                let kind = match r.below(3) {
                    0 => Kind::Dir,
                    1 => Kind::Symlink,
                    _ => Kind::File,
                };
                let mut m = Member::new(&format!("{}{i}", name(&mut r)), kind);
                m.mode = r.below(0o1000) as u32;
                m.mtime = r.below(2_000_000_000) as i64;
                m.uid = r.below(60000) as u32;
                m.gid = r.below(60000) as u32;
                m.ino = i + 1;
                match kind {
                    Kind::File => {
                        let n = r.below(3000) as usize;
                        m.data = r.data(n, 256);
                    }
                    Kind::Symlink => m.link = name(&mut r),
                    _ => {}
                }
                ms.push(m);
            }
            let mut bytes = Vec::new();
            for m in &ms {
                bytes.extend(codec::encode(m, f).unwrap());
            }
            let bytes = codec::finish(bytes, f, f.default_block());
            assert_eq!(bytes.len() % f.default_block(), 0);
            let (got_f, back) = codec::read(&bytes).unwrap();
            assert_eq!(got_f, f);
            assert_eq!(back.len(), ms.len());
            for (a, b) in ms.iter().zip(&back) {
                assert_eq!((&a.name, a.kind, a.mode, a.mtime), (&b.name, b.kind, b.mode, b.mtime));
                assert_eq!((&a.data, &a.link, a.uid, a.gid), (&b.data, &b.link, b.uid, b.gid));
            }
        }
    }
}

#[test]
fn decoders_never_panic() {
    let mut r = Rng(0x5eed_0006);
    let mut m = Member::new("some/file", Kind::File);
    m.data = b"payload".to_vec();
    for f in [Format::Ustar, Format::Cpio(Cpio::Odc), Format::Cpio(Cpio::Newc)] {
        let good = codec::finish(codec::encode(&m, f).unwrap(), f, 512);
        for _ in 0..2000 {
            let mut d = good.clone();
            for _ in 0..1 + r.below(4) {
                let i = r.below(d.len() as u64) as usize;
                d[i] = r.next() as u8;
            }
            let cut = r.below(d.len() as u64 + 1) as usize;
            let _ = codec::read(&d[..cut]);
        }
    }
    for _ in 0..2000 {
        let n = r.below(600) as usize;
        let d: Vec<u8> = (0..n).map(|_| r.next() as u8).collect();
        let _ = codec::read(&d);
        let _ = zip::read(&d);
    }
}

#[test]
fn zip_round_trips() {
    let mut r = Rng(0x5eed_0007);
    for _ in 0..40 {
        let es: Vec<Entry> = (0..1 + r.below(10))
            .map(|i| {
                let (n, a) = (r.below(20_000) as usize, 1 + r.below(255));
                let d = r.data(n, a);
                let deflated = r.below(2) == 0;
                Entry::new(&format!("{}{i}", name(&mut r)), &d, deflated, (0x6000, 0x5a5c), Some(0o644))
            })
            .collect();
        let back = zip::read(&zip::write(&es)).unwrap();
        assert_eq!(back, es);
        for (a, b) in es.iter().zip(&back) {
            assert_eq!(a.plain().unwrap(), b.plain().unwrap());
        }
    }
}

/// A random tree under `/home/QSHUSER/tree`; the files and their bytes.
fn tree(s: &qsh::sys::MemSys, r: &mut Rng) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for i in 0..20 {
        // Directories start with d and files with f, so none collide.
        let dirs: Vec<String> = name(r).split('/').map(|c| format!("d{c}")).collect();
        let p = format!("tree/{}/f{i}", dirs.join("/"));
        let (n, a) = (r.below(5000) as usize, 1 + r.below(255));
        let d = r.data(n, a);
        s.write_file(&format!("/home/QSHUSER/{p}"), &d);
        files.push((p, d));
    }
    files
}

#[test]
fn utilities_round_trip_trees() {
    let mut r = Rng(0x5eed_0008);
    let scripts = [
        "pax -w -f /tmp/a tree && mkdir /tmp/out && cd /tmp/out && pax -r -f /tmp/a",
        "pax -w -x cpio -f /tmp/a tree && mkdir /tmp/out && cd /tmp/out && pax -r -f /tmp/a",
        "pax -w -x sv4crc -f /tmp/a tree && mkdir /tmp/out && cd /tmp/out && pax -r -f /tmp/a",
        "mkdir /tmp/out && pax -rw tree /tmp/out",
        "tar cf /tmp/a tree && mkdir /tmp/out && cd /tmp/out && tar xf /tmp/a",
        "tar cf - tree | compress | uncompress > /tmp/a && mkdir /tmp/out && cd /tmp/out && tar xf /tmp/a",
        "ajar cMf /tmp/a tree && mkdir /tmp/out && cd /tmp/out && ajar xf /tmp/a",
    ];
    for script in scripts {
        for _ in 0..3 {
            let s = common::sys();
            let files = tree(&s, &mut r);
            let o = common::run_on(&s, script, "");
            assert_eq!((o.status, o.err.as_str()), (0, ""), "{script}");
            for (p, d) in &files {
                let got = s.read_file(&format!("/tmp/out/{p}"));
                assert_eq!(got.as_ref(), Some(d), "{script}: {p}");
            }
        }
    }
}
