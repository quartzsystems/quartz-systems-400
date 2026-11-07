// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Many connections at once over the test system, each signed on and
//! keeping several requests outstanding: spool reads (page ranges in
//! every format the file reads in), spool and queue lists, pings,
//! commands and the odd cancel, in a seeded random mix. No connection
//! may close before its client does, and every request gets its final
//! reply. `soak_short` runs 10 s; `soak_long` (ignored) runs
//! `QAPD_SOAK_SECS` (300 by default). `QAPD_SOAK_SEED` picks the seed.

mod common;

use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use qap_proto::ops::command::CommandText;
use qap_proto::ops::spool::{
    ListOutputQueues, ListSpooledFiles, OutputQueues, ReadFormat, ReadSpooledFile, SpooledData,
    SpooledFileId, SpooledFiles,
};
use qap_proto::{Envelope, Kind, Op, Status};
use qapd::Closed;

use common::log::{Line, capture};
use common::{Controls, Run, cfg, start_with};

/// Connections at once.
const CONNECTIONS: u64 = 32;
/// The most requests a connection keeps outstanding.
const WINDOW: u64 = 12;

/// `SplitMix64`: small, seeded, the same sequence everywhere.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// In `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// An index into `len` things.
    fn index(&mut self, len: usize) -> usize {
        usize::try_from(self.below(u64::try_from(len).unwrap())).unwrap()
    }

    /// In `lo..=hi`.
    fn between(&mut self, lo: u32, hi: u32) -> u32 {
        lo + u32::try_from(self.below(u64::from(hi - lo) + 1)).unwrap()
    }
}

/// A file read in one format: how many pages it has.
#[derive(Clone)]
struct Readable {
    id: SpooledFileId,
    format: ReadFormat,
    pages: u32,
}

/// What a request's reply must be.
enum Expect {
    Ping,
    Files(u32),
    Queues(u32),
    Read { pages: u32 },
    Command(Status),
}

/// Requests sent, by operation, and cancels.
#[derive(Default)]
struct Tally {
    by_op: HashMap<&'static str, u64>,
    cancels: u64,
}

impl Tally {
    fn add(&mut self, other: &Tally) {
        for (k, v) in &other.by_op {
            *self.by_op.entry(k).or_default() += v;
        }
        self.cancels += other.cancels;
    }
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

/// What BOB may read, learned once per connection.
fn readable(r: &mut Run) -> (Vec<Readable>, u32, u32) {
    let all = ListSpooledFiles {
        user: "*ALL".into(),
        ..ListSpooledFiles::default()
    };
    let files: SpooledFiles = r.client.call(Op::ListSpooledFiles, &all).body_as().unwrap();
    let queues: OutputQueues = r
        .client
        .call(
            Op::ListOutputQueues,
            &ListOutputQueues {
                library: "*ALL".into(),
                ..ListOutputQueues::default()
            },
        )
        .body_as()
        .unwrap();
    let mut out = Vec::new();
    for f in &files.files {
        for format in [ReadFormat::Text, ReadFormat::Scs, ReadFormat::Pdf] {
            let e = r.client.call(
                Op::ReadSpooledFile,
                &ReadSpooledFile {
                    id: f.id.clone(),
                    format,
                    first_page: 1,
                    last_page: 0,
                },
            );
            if e.status == Status::Ok {
                let d: SpooledData = e.body_as().unwrap();
                out.push(Readable {
                    id: f.id.clone(),
                    format,
                    pages: d.pages,
                });
            }
        }
    }
    assert!(!out.is_empty(), "BOB reads something");
    (out, files.total, queues.total)
}

/// Send one random request; what its reply must be.
fn send(
    r: &mut Run,
    rng: &mut Rng,
    files: &[Readable],
    totals: (u32, u32),
    tally: &mut Tally,
) -> (u32, Expect) {
    let pick = rng.below(100);
    let (name, id, expect) = match pick {
        0..=34 => {
            let f = &files[rng.index(files.len())];
            let first = rng.between(1, f.pages.max(1));
            let last = if rng.below(4) == 0 {
                0
            } else {
                rng.between(first, f.pages.max(first))
            };
            let req = ReadSpooledFile {
                id: f.id.clone(),
                format: f.format,
                first_page: first,
                last_page: last,
            };
            let id = r.client.request(Op::ReadSpooledFile, &req);
            ("ReadSpooledFile", id, Expect::Read { pages: f.pages })
        }
        35..=49 => {
            let req = ListSpooledFiles {
                user: if rng.below(2) == 0 {
                    "*ALL"
                } else {
                    "*CURRENT"
                }
                .into(),
                start: rng.between(0, 3),
                max: rng.between(0, 4),
                ..ListSpooledFiles::default()
            };
            let all = req.user == "*ALL";
            let id = r.client.request(Op::ListSpooledFiles, &req);
            (
                "ListSpooledFiles",
                id,
                Expect::Files(if all { totals.0 } else { 0 }),
            )
        }
        50..=62 => {
            let req = ListOutputQueues {
                library: "*ALL".into(),
                start: rng.between(0, 2),
                max: rng.between(0, 3),
            };
            let id = r.client.request(Op::ListOutputQueues, &req);
            ("ListOutputQueues", id, Expect::Queues(totals.1))
        }
        63..=77 => ("Ping", r.client.request(Op::Ping, &()), Expect::Ping),
        _ => {
            let (command, status) = match rng.below(20) {
                0 => ("DLYJOB DLY(1)", Status::Ok),
                1..=7 => ("DLYJOB DLY(0)", Status::Ok),
                8..=12 => ("CHGAUT OBJ('/home/bob') USER(BOB) DTAAUT(*RX)", Status::Ok),
                13..=15 => ("NOSUCHCMD", Status::NotFound),
                16..=17 => (
                    "CPYFRMSTMF FROMSTMF('/tmp/a.txt') TOMBR('/QSYS.LIB/QGPL.LIB/A.FILE/A.MBR')",
                    Status::Ok,
                ),
                _ => ("DLTUSRPRF USRPRF(ANN)", Status::NotAuthorized),
            };
            let body = CommandText {
                command: command.into(),
            };
            (
                "RunCommand",
                r.client.request(Op::RunCommand, &body),
                Expect::Command(status),
            )
        }
    };
    *tally.by_op.entry(name).or_default() += 1;
    (id, expect)
}

/// A reply as `expect` says, or `Cancelled` when it was.
fn check(conn: u64, e: &Envelope, expect: &Expect, cancelled: bool) {
    assert_eq!(e.kind, Kind::Reply, "connection {conn}: {e:?}");
    if cancelled && e.status == Status::Cancelled {
        return;
    }
    let ok = |e: &Envelope| assert_eq!(e.status, Status::Ok, "connection {conn}: {:?}", e.message);
    match expect {
        Expect::Ping => ok(e),
        Expect::Files(total) => {
            ok(e);
            let l: SpooledFiles = e.body_as().unwrap();
            if *total > 0 {
                assert_eq!(l.total, *total, "connection {conn}");
            }
        }
        Expect::Queues(total) => {
            ok(e);
            let l: OutputQueues = e.body_as().unwrap();
            assert_eq!(l.total, *total, "connection {conn}");
        }
        Expect::Read { pages } => {
            ok(e);
            let d: SpooledData = e.body_as().unwrap();
            assert_eq!(d.pages, *pages, "connection {conn}");
        }
        Expect::Command(status) => {
            assert_eq!(e.status, *status, "connection {conn}: {:?}", e.message);
        }
    }
}

/// One connection for `run`: requests until the deadline, then every
/// reply, then the client closes.
fn connection(c: &Arc<Controls>, conn: u64, seed: u64, until: Instant) -> Tally {
    let mut rng = Rng(seed ^ conn.wrapping_mul(0xA24B_AED4_963E_E407));
    let mut r = start_with(cfg(), Arc::clone(c));
    let h: qap_proto::ops::connection::HelloReply = r.client.hello_range(1, 3).body_as().unwrap();
    assert_eq!(h.version, 3);
    r.client.sign_on("BOB", "SECRET1");
    let (files, files_total, queues_total) = readable(&mut r);
    let mut tally = Tally::default();
    let mut waiting: HashMap<u32, (Expect, bool)> = HashMap::new();
    while Instant::now() < until {
        let window = usize::try_from(1 + rng.below(WINDOW)).unwrap();
        while waiting.len() < window {
            let (id, expect) = send(
                &mut r,
                &mut rng,
                &files,
                (files_total, queues_total),
                &mut tally,
            );
            assert!(
                waiting.insert(id, (expect, false)).is_none(),
                "IDs are not reused while outstanding"
            );
        }
        // Now and then, cancel one.
        if rng.below(40) == 0 {
            let ids: Vec<u32> = waiting.keys().copied().collect();
            let id = ids[rng.index(ids.len())];
            r.client.send(&Envelope::cancel(id));
            waiting.get_mut(&id).unwrap().1 = true;
            tally.cancels += 1;
        }
        take_one(&mut r, conn, &mut waiting);
    }
    while !waiting.is_empty() {
        take_one(&mut r, conn, &mut waiting);
    }
    drop(r.client);
    assert_eq!(r.server.join().unwrap(), Closed::Peer, "connection {conn}");
    tally
}

fn take_one(r: &mut Run, conn: u64, waiting: &mut HashMap<u32, (Expect, bool)>) {
    let Some(e) = r.client.recv() else {
        // Why the server closed, when it has.
        let until = Instant::now() + Duration::from_secs(5);
        while !r.server.is_finished() && Instant::now() < until {
            thread::sleep(Duration::from_millis(10));
        }
        let why = if r.server.is_finished() {
            "closed"
        } else {
            "still serving"
        };
        panic!(
            "connection {conn} (job {}) closed with {} requests outstanding; the server {why}",
            r.job,
            waiting.len()
        );
    };
    let Some((expect, cancelled)) = waiting.remove(&e.request_id) else {
        panic!("connection {conn}: a reply to nothing outstanding: {e:?}");
    };
    check(conn, &e, &expect, cancelled);
}

/// A close the client did not ask for, as it is logged.
fn unasked(l: &Line) {
    if l.get("reason").is_some_and(|r| r != "client closed") {
        eprintln!("soak: unasked close: {l:?}");
    }
}

fn soak(secs: u64) {
    capture(Some(unasked));
    let seed = env_u64("QAPD_SOAK_SEED").unwrap_or(0x51_7E_57);
    let c = Controls::new();
    let until = Instant::now() + Duration::from_secs(secs);
    let threads: Vec<_> = (0..CONNECTIONS)
        .map(|conn| {
            let c = Arc::clone(&c);
            thread::spawn(move || connection(&c, conn, seed, until))
        })
        .collect();
    let mut total = Tally::default();
    let mut failed = Vec::new();
    for (i, t) in threads.into_iter().enumerate() {
        match t.join() {
            Ok(tally) => total.add(&tally),
            Err(_) => failed.push(i),
        }
    }
    let mut ops: Vec<_> = total.by_op.iter().collect();
    ops.sort();
    eprintln!(
        "soak: {CONNECTIONS} connections, {secs} s, seed {seed:#x}: {ops:?}, {} cancels",
        total.cancels
    );
    assert!(
        failed.is_empty(),
        "connections that failed: {failed:?} (seed {seed:#x})"
    );
}

#[test]
fn soak_short() {
    soak(10);
}

#[test]
#[ignore = "minutes long: cargo test -p qapd --test soak -- --ignored"]
fn soak_long() {
    soak(env_u64("QAPD_SOAK_SECS").unwrap_or(300));
}
