// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The decoder against hostile bytes: random frames and valid frames
//! mutated. Nothing panics, nothing over 1 MiB is accepted, and what
//! decodes encodes back to the same bytes. A connection job feeds
//! every frame a client sends through this decoder.
//!
//! `decoder_smoke` runs a few seconds; `decoder_one_minute` is the
//! minute-long run (`cargo test -p qap-proto --test fuzz -- --ignored`).
//! `QAP_FUZZ_SEED` changes the seed.

#![allow(clippy::cast_possible_truncation)]

use std::time::{Duration, Instant};

use ipc::{Wire, decode, encode};
use qap_proto::ops::connection::{Hello, HelloReply, PingReply};
use qap_proto::ops::signon::{ChangePassword, Resume, SignOn, SignOnReply};
use qap_proto::ops::system::SystemInfo;
use qap_proto::{Envelope, MAX_FRAME, Op, Reassembly, ServerMessage, Status};

/// A small deterministic generator, so a failure reproduces.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() as usize) % n.max(1)
    }

    fn bytes(&mut self, max: usize) -> Vec<u8> {
        let n = self.below(max + 1);
        (0..n).map(|_| self.next() as u8).collect()
    }
}

/// Valid frames to mutate.
fn seeds() -> Vec<Vec<u8>> {
    let req = Envelope::request(3, Op::SignOn, &SignOn::default());
    let mut msg = Envelope::reply(&req, Status::SignOnFailed, None, Vec::new());
    msg.message = Some(ServerMessage {
        id: "CPF1107".into(),
        severity: 40,
        data: vec![1, 2, 3],
        text: "text".into(),
        help: "help".into(),
    });
    [
        Envelope::request(1, Op::Hello, &Hello::default()),
        Envelope::request(2, Op::Ping, &()),
        req,
        msg,
        Envelope::request(4, Op::ChangePassword, &ChangePassword::default()),
        Envelope::request(5, Op::Resume, &Resume::default()),
        Envelope::cancel(5),
    ]
    .iter()
    .map(encode)
    .collect()
}

/// Mutate: flip, overwrite, insert, delete, or set a length-sized run.
fn mutate(g: &mut Lcg, mut b: Vec<u8>) -> Vec<u8> {
    for _ in 0..=g.below(4) {
        let at = g.below(b.len());
        match g.below(6) {
            0 if !b.is_empty() => b[at] ^= 1 << g.below(8),
            1 if !b.is_empty() => b[at] = g.next() as u8,
            2 => b.insert(at.min(b.len()), g.next() as u8),
            3 if !b.is_empty() => {
                b.remove(at);
            }
            4 if b.len() >= at + 4 => b[at..at + 4].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0x7F]),
            _ => b.extend(g.bytes(8)),
        }
    }
    b
}

/// What a decoded value must do: encode back to the input.
fn check<T: Wire>(b: &[u8]) {
    if let Ok(v) = decode::<T>(b) {
        assert_eq!(encode(&v), b, "re-encoding differs");
    }
}

fn one(g: &mut Lcg, seeds: &[Vec<u8>], asm: &mut Reassembly) {
    let b = if g.below(4) == 0 {
        g.bytes(96)
    } else {
        let pick = g.below(seeds.len());
        mutate(g, seeds[pick].clone())
    };
    if let Ok(env) = Envelope::from_frame(&b) {
        assert!(b.len() <= MAX_FRAME);
        assert_eq!(encode(&env), b);
        check::<Hello>(&env.body);
        check::<SignOn>(&env.body);
        check::<ChangePassword>(&env.body);
        check::<Resume>(&env.body);
        check::<HelloReply>(&env.body);
        check::<SignOnReply>(&env.body);
        check::<SystemInfo>(&env.body);
        check::<PingReply>(&env.body);
        let _ = asm.push(env);
    }
    // The same bytes as a stream: a short or bogus prefix is an error.
    let mut r = b.as_slice();
    let _ = Envelope::read_from(&mut r);
}

fn run(time: Duration) -> u64 {
    let seed = std::env::var("QAP_FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(400);
    let mut g = Lcg(seed);
    let seeds = seeds();
    let mut asm = Reassembly::new();
    let end = Instant::now() + time;
    let mut n = 0u64;
    while Instant::now() < end {
        for _ in 0..1000 {
            one(&mut g, &seeds, &mut asm);
        }
        n += 1000;
    }
    n
}

#[test]
fn decoder_smoke() {
    assert!(run(Duration::from_secs(3)) > 0);
}

#[test]
#[ignore = "a minute long; run with --ignored"]
fn decoder_one_minute() {
    let n = run(Duration::from_secs(60));
    eprintln!("{n} inputs");
}
