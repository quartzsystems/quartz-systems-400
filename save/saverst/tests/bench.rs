// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Save and restore throughput over a store in memory:
//! `cargo test --release -p saverst --test bench -- --ignored --nocapture`.
//! `QS400_SAVE_BENCH_MIB` sets the library's size (default 64).

#![allow(clippy::many_single_char_names, clippy::cast_precision_loss)]

use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use savfmt::{Compression, MediaHeader, Reader, SaveKind, Writer};
use saverst::{AllowAll, RestoreOptions, SaveOptions, restore, save_library};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types, usrspc};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn system(mib: u64) -> (Store, ObjectId) {
    let pages = (mib * 5 + 64) * 256;
    let d = Arc::new(MemBlockDev::with_seed(pages, 5));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 4096,
        now: NOW,
    };
    let mut s = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &fp, StoreConfig::default())
        .unwrap();
    let mut t = s.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    t.commit().unwrap();
    (s, qsys)
}

fn params(lib: ObjectId, n: &str, ty: u16) -> CreateParams {
    CreateParams {
        library: lib,
        name: Name::parse(n).unwrap(),
        type_code: ty,
        attribute: String::new(),
        text: String::new(),
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: NOW,
        by_system: true,
    }
}

#[test]
#[ignore = "bench: run by hand with --release"]
fn save_and_restore_throughput() {
    let mib: u64 = std::env::var("QS400_SAVE_BENCH_MIB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64);
    let (mut s, q) = system(mib);
    // 1 MiB user spaces of mixed data: half text-like, half noise.
    let mut t = s.begin().unwrap();
    let lib = ops::create(&mut t, &params(q, "QSBENCH", types::LIB)).unwrap();
    t.commit().unwrap();
    let mut x = 0x1234_5678_u64;
    for i in 0..mib {
        let mut b = vec![0u8; 1 << 20];
        for (j, c) in b.iter_mut().enumerate() {
            *c = if j % 2048 < 1024 {
                b"QS/400 save bench "[j % 18]
            } else {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x & 0xFF) as u8
            };
        }
        let mut t = s.begin().unwrap();
        let id = usrspc::create(&mut t, &params(lib, &format!("US{i:05}"), types::USRSPC), 1 << 20, false, 0)
            .unwrap();
        usrspc::write(&mut t, id, 0, &b).unwrap();
        t.commit().unwrap();
    }
    for c in [Compression::None, Compression::Low, Compression::Medium] {
        let h = MediaHeader {
            release: (1, 0, 0),
            target: (1, 0, 0),
            compression: c,
            kind: SaveKind::Library,
            created: NOW,
            stream_id: [7; 16],
            system: "QS400".into(),
            serial: String::new(),
            command: "SAVLIB".into(),
            user: "QSYS".into(),
        };
        let start = Instant::now();
        let mut w = Writer::new(Vec::with_capacity(usize::try_from(mib << 20).unwrap()), &h).unwrap();
        let opts = SaveOptions {
            update_history: false,
            ..SaveOptions::default()
        };
        save_library(&mut s, JobHandle(1), &AllowAll, q, "QSBENCH", &opts, &mut w).unwrap();
        let (_, bytes) = w.finish(0).unwrap();
        let save = start.elapsed().as_secs_f64();
        let start = Instant::now();
        let mut r = Reader::new(Cursor::new(bytes.as_slice())).unwrap();
        let rep = restore(
            &mut s,
            JobHandle(1),
            &AllowAll,
            q,
            &mut r,
            &RestoreOptions {
                target_library: Some(format!("QSB{}", c.code())),
                ..RestoreOptions::default()
            },
        )
        .unwrap();
        let rst = start.elapsed().as_secs_f64();
        assert_eq!(rep.restored(), usize::try_from(mib).unwrap());
        let m = mib as f64;
        println!(
            "{c:?}: {mib} MiB, stream {:.1} MiB; save {:.0} MiB/s, restore {:.0} MiB/s",
            bytes.len() as f64 / 1_048_576.0,
            m / save,
            m / rst
        );
    }
}
