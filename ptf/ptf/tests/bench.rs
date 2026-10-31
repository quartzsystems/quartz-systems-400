// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PTF build, open, load, apply and remove times over stores in memory:
//! `cargo test --release -p ptf --test bench -- --ignored --nocapture`.
//! `QS400_PTF_BENCH_OBJECTS` sets the replacement objects (default 200,
//! 64 KiB user spaces each).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::many_single_char_names
)]

use std::sync::Arc;
use std::time::Instant;

use blockdev::{BlockDev, MemBlockDev};
use ed25519_dalek::SigningKey;
use ptf::apply::{self, Ctx};
use ptf::package::{self, Build};
use ptf::{PtfDb, PtfHeader, PtfKind};
use qsname::Name;
use saverst::AllowAll;
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types, usrspc};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;
const SIZE: usize = 64 << 10;

fn system(n: usize, seed: u8) -> (Store, ObjectId) {
    let pages = (n * SIZE / 4096 * 4 + 8192) as u64;
    let d = Arc::new(MemBlockDev::with_seed(pages, u64::from(seed)));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 4096,
        now: NOW,
    };
    let mut s = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &fp, StoreConfig::default()).unwrap();
    let mut t = s.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let p = |lib, name: &str, ty| CreateParams {
        library: lib,
        name: Name::parse(name).unwrap(),
        type_code: ty,
        attribute: String::new(),
        text: String::new(),
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: true,
    };
    let lib = ops::create(&mut t, &p(qsys, "QSVCLIB", types::LIB)).unwrap();
    t.commit().unwrap();
    for i in 0..n {
        let mut t = s.begin().unwrap();
        let id = usrspc::create(&mut t, &p(lib, &format!("OBJ{i:05}"), types::USRSPC), SIZE as u64, false, 0).unwrap();
        // Half text-like, half noise, as a program's pages might be.
        let mut x = 0x9E37_79B9_7F4A_7C15_u64 ^ (i as u64) ^ u64::from(seed);
        let fill: Vec<u8> = (0..SIZE)
            .map(|j| {
                if j % 2048 < 1024 {
                    b"QS/400 PTF bench "[j % 17]
                } else {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    x as u8
                }
            })
            .collect();
        usrspc::write(&mut t, id, 0, &fill).unwrap();
        t.commit().unwrap();
    }
    (s, qsys)
}

#[test]
#[ignore = "bench: run by hand with --release"]
fn ptf_times() {
    let n: usize = std::env::var("QS400_PTF_BENCH_OBJECTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let key = SigningKey::from_bytes(&[9u8; 32]);
    let trusted = [key.verifying_key().to_bytes()];
    let (mut build_sys, bq) = system(n, 1);
    let header = PtfHeader {
        ptf: "QS00900".into(),
        product: "QS00SS1".into(),
        option: "0000".into(),
        release: "V1R0M0".into(),
        load: "5050".into(),
        target_release: "V1R0M0".into(),
        kind: PtfKind::Immediate,
        lic: false,
        migration: false,
        cover_only: false,
        created: NOW,
        prereqs: Vec::new(),
        coreqs: Vec::new(),
        supersedes: Vec::new(),
        exits: Vec::new(),
        object_count: 0,
        store_format: 0,
    };
    let b = Build {
        header,
        cover: "Bench PTF.".into(),
        objects: (0..n).map(|i| ("QSVCLIB".into(), format!("OBJ{i:05}"), types::USRSPC)).collect(),
        lic: None,
        lic_b: None,
        key: &key,
        system: "QSBUILD".into(),
    };
    let t0 = Instant::now();
    let bytes = package::build(&mut build_sys, bq, &b, NOW).unwrap();
    let built = t0.elapsed().as_secs_f64();

    let (mut s, q) = system(n, 2);
    let t0 = Instant::now();
    let pkg = package::open(&bytes, &trusted).unwrap();
    let mut db = PtfDb::default();
    apply::load(&mut db, &pkg, "QGPL/QS00900", "V1R0M0", NOW).unwrap();
    let loaded = t0.elapsed().as_secs_f64();
    let mut cx = Ctx {
        store: &mut s,
        job: JobHandle(1),
        qsys: q,
        hooks: &AllowAll,
        now: NOW + 1,
    };
    let t0 = Instant::now();
    apply::apply_temp(&mut cx, &mut db, &pkg, &bytes, &[], false).unwrap();
    let applied = t0.elapsed().as_secs_f64();
    let t0 = Instant::now();
    apply::remove(&mut cx, &mut db, "QS00900", false).unwrap();
    let removed = t0.elapsed().as_secs_f64();
    apply::apply_temp(&mut cx, &mut db, &pkg, &bytes, &[], false).unwrap();
    let t0 = Instant::now();
    apply::apply_perm(&mut cx, &mut db, "QS00900").unwrap();
    let perm = t0.elapsed().as_secs_f64();
    let mib = (n * SIZE) as f64 / 1_048_576.0;
    println!(
        "{n} objects, {mib:.1} MiB, stream {:.1} MiB: build {built:.2}s, open+load {loaded:.2}s, \
         apply *TEMP {applied:.2}s, remove {removed:.2}s, apply *PERM {perm:.2}s",
        bytes.len() as f64 / 1_048_576.0
    );
}
