// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fuzz targets' entry points under random and mutated input,
//! without instrumentation (the instrumented runs are `build fuzz` in
//! Linux): nothing panics, nothing hangs, and lengths that claim more
//! than the bytes hold fail at once.

mod wire_common;

use std::time::Instant;

use nfs::fuzzing;
use nfs::types::{mount, nfs3, nfs4, nlm, rpcb};
use nfs::xdr::{self, Error};
use wire_common::hex;

/// xorshift64*.
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

fn seeds() -> Vec<Vec<u8>> {
    vec![
        hex("80000028 00000001 00000000 00000002 00061ae4 00000001 00000000 00000000 00000000 00000000 00000000"),
        hex("00000000 00000000 00000003 00000018 0000000a 00000009 00000002 0000001a 00000002"),
        hex("00000000 00000004 74657374 00000003 00000018 00000000 0000000a 00000000 00000004 01020304"),
        hex("01 00000005 2f686f6d 65000000"),
        hex("02 00000004 01020304 00000001 00000001 00000004 686f7374 00000004 aaaaaaaa 00000001 6f000000
             0000004d 00000000 00000000 00000000 00000064 00000000 00000003"),
        hex("11 00000008 61626364 65666768 00000000 00000000 00000000 00000000 00000000 00000010 00000020"),
        hex("0203 000186a3 00000003 00000003 74637000 00000000 00000000"),
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/conformance/oracle/nfs/etc/RPCBTAB.bin"))
            .unwrap(),
    ]
}

fn mutate(r: &mut Rng, s: &[u8]) -> Vec<u8> {
    let mut v = s.to_vec();
    for _ in 0..=r.below(4) {
        match r.below(6) {
            0 if !v.is_empty() => {
                let i = r.below(v.len());
                v[i] ^= 1 << r.below(8);
            }
            1 if !v.is_empty() => {
                let i = r.below(v.len());
                v[i] = [0x00, 0xff, 0x7f, 0x80, 0x01][r.below(5)];
            }
            2 => v.truncate(r.below(v.len() + 1)),
            3 => {
                let i = r.below(v.len() + 1);
                v.splice(i..i, [0xff, 0xff, 0xff, 0xff]);
            }
            4 if v.len() >= 4 => {
                // A word set to a large count.
                let i = r.below(v.len() - 3) & !3;
                v[i..i + 4].copy_from_slice(&[0x00, 0x10, 0x00, 0x00]);
            }
            _ => v.extend((0..r.below(16)).map(|_| u8::try_from(r.next() & 0xff).unwrap_or(0))),
        }
    }
    v
}

fn run_all(data: &[u8]) {
    fuzzing::rpc(data);
    fuzzing::rpcb(data);
    fuzzing::mount(data);
    fuzzing::nlm(data);
    fuzzing::nsm(data);
    fuzzing::nfs3(data);
    fuzzing::nfs4(data);
}

#[test]
fn seeds_and_mutations_never_panic() {
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    let seeds = seeds();
    let t = Instant::now();
    for s in &seeds {
        run_all(s);
    }
    for _ in 0..20_000 {
        let s = &seeds[r.below(seeds.len())];
        run_all(&mutate(&mut r, s));
    }
    // A hang would show here long before a test timeout.
    assert!(t.elapsed().as_secs() < 120);
}

#[test]
fn random_bytes_never_panic() {
    let mut r = Rng(42);
    for _ in 0..20_000 {
        let n = r.below(256);
        let v: Vec<u8> = (0..n)
            .map(|_| u8::try_from(r.next() & 0xff).unwrap_or(0))
            .collect();
        run_all(&v);
    }
}

#[test]
fn claimed_lengths_fail_before_they_are_allocated() {
    // Four billion operations in a twelve-byte compound.
    let b = hex("00000000 00000000 ffffffff");
    assert!(matches!(
        xdr::from_bytes::<nfs4::COMPOUND4args>(&b),
        Err(Error::Short { .. })
    ));
    // A READ of 4 GiB of data.
    let b = hex("00000000 00000000 00000005 00000001 ffffffff");
    assert!(matches!(
        xdr::from_bytes::<nfs3::READ3res>(&b),
        Err(Error::Short { .. })
    ));
    // A file handle over its bound.
    assert!(matches!(
        xdr::from_bytes::<nfs3::nfs_fh3>(&hex("00000041")),
        Err(Error::TooLong { len: 65, max: 64 })
    ));
    // A netobj over 1024.
    assert!(matches!(
        xdr::from_bytes::<nlm::netobj>(&hex("00000401")),
        Err(Error::TooLong { .. })
    ));
    // A long export list is a loop, not recursion: 200000 entries.
    let mut b = Vec::new();
    for _ in 0..200_000 {
        b.extend_from_slice(&hex("00000001 00000001 2f000000 00000000"));
    }
    b.extend_from_slice(&hex("00000000"));
    assert_eq!(
        xdr::from_bytes::<mount::exports>(&b).unwrap().0.len(),
        200_000
    );
    // The same for a portmap dump.
    let mut b = Vec::new();
    for _ in 0..100_000 {
        b.extend_from_slice(&hex("00000001 000186a3 00000003 00000011 00000801"));
    }
    b.extend_from_slice(&hex("00000000"));
    assert_eq!(
        xdr::from_bytes::<rpcb::pmaplist_ptr>(&b).unwrap().0.len(),
        100_000
    );
}

#[test]
fn bad_padding_bools_and_discriminants() {
    // A filename with a byte of padding set.
    assert_eq!(
        xdr::from_bytes::<nfs3::filename3>(&hex("00000001 61000100")),
        Err(Error::Padding)
    );
    // A bool of 2 in post_op_attr.
    assert_eq!(
        xdr::from_bytes::<nfs3::post_op_attr>(&hex("00000002")),
        Err(Error::Bool(2))
    );
    // An ftype3 of 0.
    assert!(matches!(
        xdr::from_bytes::<nfs3::ftype3>(&hex("00000000")),
        Err(Error::Discriminant {
            ty: "ftype3",
            value: 0
        })
    ));
    // A callback operation of 99: the union has no arm for it.
    assert!(matches!(
        xdr::from_bytes::<nfs4::nfs_cb_argop4>(&hex("00000063")),
        Err(Error::Discriminant {
            ty: "nfs_cb_argop4",
            value: 99
        })
    ));
}
