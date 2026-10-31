// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Save and restore throughput to virtual tape and virtual optical: a
//! library of user spaces saved (the stream built, then written to a
//! virtual tape volume file or a writable optical image file) and
//! restored (read back from the file, then restored to a new name).
//! `cargo test --release -p saverst --test media_bench -- --ignored --nocapture`.
//! `QS400_SAVE_BENCH_MIB` sets the library's size (default 64).

#![allow(clippy::many_single_char_names, clippy::cast_precision_loss)]

use std::fs::{File, OpenOptions};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use blockdev::{BlockDev, MemBlockDev};
use medio::optfile::{OpticalReader, OpticalWriter};
use medio::tapefile::{RestoreFile, SaveFile, TapeReader, TapeWriter};
use medio::{MediaReader, MediaWriter};
use optical::{FileImage, Volume, VolumeSet};
use qsname::Name;
use savfmt::{Compression, MediaHeader, Reader, SaveKind, Writer};
use saverst::{AllowAll, RestoreOptions, SaveOptions, restore, save_library};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types, usrspc};
use store::{FormatParams, Store, StoreConfig};
use tape::label::{Date, Expiry};
use tape::volume::{Check, InzOptions, NoMoreVolumes, inztap};
use tape::{EndOpt, SeqNbr, VirtualTape, VolumeOptions};

const NOW: u64 = 1_767_225_600;

fn system(mib: u64) -> (Store, ObjectId) {
    let pages = (mib * 7 + 64) * 256;
    let d = Arc::new(MemBlockDev::with_seed(pages, 5));
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

/// A library of `mib` 1 MiB user spaces, half text-like, half noise.
fn library(s: &mut Store, q: ObjectId, mib: u64) {
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
        let id = usrspc::create(&mut t, &params(lib, &format!("US{i:05}"), types::USRSPC), 1 << 20, false, 0).unwrap();
        usrspc::write(&mut t, id, 0, &b).unwrap();
        t.commit().unwrap();
    }
}

fn stream(s: &mut Store, q: ObjectId, mib: u64) -> Vec<u8> {
    let h = MediaHeader {
        release: (1, 0, 0),
        target: (1, 0, 0),
        compression: Compression::None,
        kind: SaveKind::Library,
        created: NOW,
        stream_id: [7; 16],
        system: "QS400".into(),
        serial: String::new(),
        command: "SAVLIB".into(),
        user: "QSYS".into(),
    };
    let mut w = Writer::new(Vec::with_capacity(usize::try_from(mib << 20).unwrap()), &h).unwrap();
    let opts = SaveOptions {
        update_history: false,
        ..SaveOptions::default()
    };
    save_library(s, JobHandle(1), &AllowAll, q, "QSBENCH", &opts, &mut w).unwrap();
    w.finish(0).unwrap().1
}

fn restore_as(s: &mut Store, q: ObjectId, bytes: &[u8], lib: &str) -> usize {
    let mut r = Reader::new(Cursor::new(bytes)).unwrap();
    restore(
        s,
        JobHandle(1),
        &AllowAll,
        q,
        &mut r,
        &RestoreOptions {
            target_library: Some(lib.into()),
            ..RestoreOptions::default()
        },
    )
    .unwrap()
    .restored()
}

fn dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("qs400-media-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
#[ignore = "bench: run by hand with --release"]
fn save_and_restore_to_virtual_media() {
    let mib: u64 = std::env::var("QS400_SAVE_BENCH_MIB").ok().and_then(|v| v.parse().ok()).unwrap_or(64);
    let (mut s, q) = system(mib);
    library(&mut s, q, mib);
    let m = mib as f64;
    let d = dir();

    // Virtual tape: a volume file twice the library's size.
    let path = d.join("BENCH1");
    let mut t = VirtualTape::create(File::create_new(&path).unwrap(), &VolumeOptions::imgsiz(mib * 2 + 16, "BENCH1")).unwrap();
    inztap(
        &mut t,
        &InzOptions {
            new_vol: Some("BENCH1".into()),
            check: Check::No,
            ..InzOptions::default()
        },
    )
    .unwrap();
    drop(t);
    let start = Instant::now();
    let bytes = stream(&mut s, q, mib);
    let mut t = VirtualTape::open(OpenOptions::new().read(true).write(true).open(&path).unwrap()).unwrap();
    let file = SaveFile {
        vols: Vec::new(),
        seq: SeqNbr::End,
        label: "QSBENCH".into(),
        expires: Expiry::None,
        end_opt: EndOpt::Rewind,
        today: Date::from_ymd(2026, 10, 31).unwrap(),
        allow_active: true,
        job: "QPADEV0001".into(),
    };
    TapeWriter::new(&mut t, &mut NoMoreVolumes, file).write_stream(&bytes).unwrap();
    drop(t);
    let save = start.elapsed().as_secs_f64();
    let start = Instant::now();
    let mut t = VirtualTape::open(OpenOptions::new().read(true).write(true).open(&path).unwrap()).unwrap();
    let (back, _) = TapeReader::new(
        &mut t,
        &mut NoMoreVolumes,
        RestoreFile {
            label: Some("QSBENCH".into()),
            ..RestoreFile::default()
        },
    )
    .read_stream()
    .unwrap();
    let n = restore_as(&mut s, q, &back, "QSBTAP");
    let rst = start.elapsed().as_secs_f64();
    assert_eq!(n, usize::try_from(mib).unwrap());
    println!("virtual tape: {mib} MiB; save {:.0} MiB/s, restore {:.0} MiB/s", m / save, m / rst);

    // Virtual optical: a writable image file of twice the size.
    let img = d.join("BENCH.ISO");
    std::fs::write(&img, b"").unwrap();
    let set = VolumeSet::default();
    let store: Arc<dyn optical::ImageStore> = Arc::new(FileImage::open(&img).unwrap());
    set.insert(Volume::create(store, "BENCHO", "", (mib * 2 + 16) << 20, NOW).unwrap()).unwrap();
    let start = Instant::now();
    let bytes = stream(&mut s, q, mib);
    OpticalWriter::new(&set, "BENCHO", "/QSBENCH", true).write_stream(&bytes).unwrap();
    let save = start.elapsed().as_secs_f64();
    drop(set);
    let start = Instant::now();
    let set = VolumeSet::default();
    let store: Arc<dyn optical::ImageStore> = Arc::new(FileImage::open(&img).unwrap());
    set.insert(Volume::open(store, false).unwrap()).unwrap();
    let (back, _) = OpticalReader::new(&set, "BENCHO", "/QSBENCH").read_stream().unwrap();
    let n = restore_as(&mut s, q, &back, "QSBOPT");
    let rst = start.elapsed().as_secs_f64();
    assert_eq!(n, usize::try_from(mib).unwrap());
    println!("virtual optical: {mib} MiB; save {:.0} MiB/s, restore {:.0} MiB/s", m / save, m / rst);
    let _ = std::fs::remove_dir_all(d);
}
