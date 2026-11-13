// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The servers' fuzz entry points (`nfs3`, `mount`, `nlm`, `nsm`)
//! without instrumentation: calls built from seeds that carry the
//! export's root handle, mutated; nothing panics and every reply's
//! results decode.

#![allow(clippy::many_single_char_names)]
use std::time::Instant;

use nfs::fuzzing;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).unwrap_or(0)
    }
}

/// The root handle's marker: `opaque<32>` of `0xAA`.
fn fh() -> Vec<u8> {
    let mut v = vec![0, 0, 0, 32];
    v.extend([0xAA; 32]);
    v
}

fn word(n: u32) -> Vec<u8> {
    n.to_be_bytes().to_vec()
}

fn name(s: &str) -> Vec<u8> {
    let mut v = word(u32::try_from(s.len()).unwrap());
    v.extend(s.as_bytes());
    while !v.len().is_multiple_of(4) {
        v.push(0);
    }
    v
}

#[allow(clippy::too_many_lines)]
fn seeds() -> Vec<(u8, Vec<u8>)> {
    let cat = |parts: &[Vec<u8>]| parts.concat();
    let sattr = cat(&[
        word(1),
        word(0o644),
        word(0),
        word(0),
        word(0),
        word(0),
        word(0),
    ]);
    vec![
        // NFS 3: GETATTR, LOOKUP, CREATE, WRITE-shaped, READDIRPLUS, REMOVE.
        (b'3', cat(&[vec![1, 0x20], fh()])),
        (b'3', cat(&[vec![3, 0x20], fh(), name("f")])),
        (
            b'3',
            cat(&[vec![8, 0x20], fh(), name("new"), word(0), sattr.clone()]),
        ),
        (b'3', cat(&[vec![9, 0x20], fh(), name("dir"), sattr])),
        (
            b'3',
            cat(&[
                vec![17, 0x20],
                fh(),
                word(0),
                word(0),
                word(0),
                word(0),
                word(512),
                word(4096),
            ]),
        ),
        (b'3', cat(&[vec![12, 0x20], fh(), name("f")])),
        (
            b'3',
            cat(&[vec![14, 0x20], fh(), name("f"), fh(), name("g")]),
        ),
        // MOUNT 3 and 1.
        (b'm', cat(&[vec![1, 0x21], name("/home/fz")])),
        (b'm', vec![2, 0x21]),
        (b'm', vec![5, 0x20]),
        // NLM 4 LOCK and TEST, version 3 SHARE.
        (
            b'n',
            cat(&[
                vec![2, 0x23],
                name("c"),
                word(1),
                word(1),
                name("host"),
                fh(),
                name("oh"),
                word(7),
                word(0),
                word(0),
                word(0),
                word(10),
                word(0),
                word(1),
            ]),
        ),
        (
            b'n',
            cat(&[
                vec![1, 0x23],
                name("c"),
                word(1),
                name("host"),
                fh(),
                name("oh"),
                word(7),
                word(0),
                word(0),
                word(0),
                word(10),
            ]),
        ),
        (
            b'n',
            cat(&[
                vec![20, 0x22],
                name("c"),
                name("host"),
                fh(),
                name("oh"),
                word(1),
                word(3),
                word(0),
            ]),
        ),
        // NSM: MON from this host, NOTIFY.
        (
            b's',
            cat(&[
                vec![2, 0x00],
                name("h1"),
                name("127.0.0.1"),
                word(100_021),
                word(2),
                word(1),
                vec![0; 16],
            ]),
        ),
        (b's', cat(&[vec![6, 0x40], name("h1"), word(3)])),
    ]
}

fn run(kind: u8, d: &[u8]) {
    match kind {
        b'3' => fuzzing::nfs3(d),
        b'm' => fuzzing::mount(d),
        b'n' => fuzzing::nlm(d),
        _ => fuzzing::nsm(d),
    }
}

#[test]
fn served_calls_never_panic() {
    let mut r = Rng(0x005e_ed0f_5e7e);
    let seeds = seeds();
    let t = Instant::now();
    for (k, s) in &seeds {
        run(*k, s);
    }
    for _ in 0..6000 {
        let (k, s) = &seeds[r.below(seeds.len())];
        let mut v = s.clone();
        for _ in 0..=r.below(3) {
            if v.is_empty() {
                break;
            }
            let i = r.below(v.len());
            match r.below(4) {
                0 => v[i] ^= 1 << r.below(8),
                1 => v[i] = [0, 0xff, 1, 0x20][r.below(4)],
                2 => v.truncate(i),
                _ => v.insert(i, u8::try_from(r.next() & 0xff).unwrap_or(0)),
            }
        }
        run(*k, &v);
    }
    assert!(t.elapsed().as_secs() < 120);
}
