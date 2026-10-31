// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Save streams on virtual tape (one volume, two volumes from an image
//! catalog's entries, the operator declining) and on optical volumes.

#![allow(clippy::many_single_char_names)]

use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::Arc;

use medio::optfile::{OpticalReader, OpticalWriter, save_path};
use medio::tapefile::{RestoreFile, SaveFile, TapeReader, TapeWriter};
use medio::{CatalogTapes, MediaReader, MediaWriter};
use optical::volume::layout::SaveTarget;
use optical::{MemImage, Volume, VolumeSet};
use tape::label::{Date, Expiry};
use tape::volume::{Check, InzOptions, NoMoreVolumes, VolumeKind, inztap, labels};
use tape::{EndOpt, SeqNbr, TapeError, VirtualTape, VolumeOptions};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("medio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn rw(p: &str) -> std::io::Result<File> {
    OpenOptions::new().read(true).write(true).open(p)
}

/// A labeled volume file of `mb` MiB.
fn volume(d: &std::path::Path, vol: &str, mb: u64) -> String {
    let p = d.join(vol);
    let f = File::create_new(&p).unwrap();
    let mut t = VirtualTape::create(f, &VolumeOptions::imgsiz(mb, vol)).unwrap();
    inztap(
        &mut t,
        &InzOptions {
            new_vol: Some(vol.into()),
            check: Check::No,
            ..InzOptions::default()
        },
    )
    .unwrap();
    p.to_string_lossy().into_owned()
}

fn today() -> Date {
    Date::from_ymd(2026, 10, 30).unwrap()
}

fn save_file(label: &str) -> SaveFile {
    SaveFile {
        vols: Vec::new(),
        seq: SeqNbr::End,
        label: label.into(),
        expires: Expiry::Perm,
        end_opt: EndOpt::Rewind,
        today: today(),
        allow_active: false,
        job: "QPADEV0001".into(),
    }
}

fn stream(n: usize) -> Vec<u8> {
    (0..n).map(|i| u8::try_from(i * 7 % 251).unwrap()).collect()
}

#[test]
fn one_volume_save_and_restore() {
    let d = dir("one");
    let p = volume(&d, "VOL001", 8);
    let mut t = VirtualTape::open(rw(&p).unwrap()).unwrap();
    let data = stream(300_000);
    let w = TapeWriter::new(&mut t, &mut NoMoreVolumes, save_file("mylib")).write_stream(&data).unwrap();
    assert_eq!((w.seq, w.volumes.as_slice(), w.file.as_str()), (1, &["VOL001".to_string()][..], "MYLIB"));
    let w2 = TapeWriter::new(&mut t, &mut NoMoreVolumes, save_file("OTHER")).write_stream(b"x").unwrap();
    assert_eq!(w2.seq, 2);
    let mut none = NoMoreVolumes;
    let mut r = TapeReader::new(
        &mut t,
        &mut none,
        RestoreFile {
            label: Some("MYLIB".into()),
            ..RestoreFile::default()
        },
    );
    let (back, vols) = r.read_stream().unwrap();
    assert_eq!(back, data);
    assert_eq!(vols, ["VOL001"]);
    let l = labels(&mut t, None, EndOpt::Rewind).unwrap();
    assert_eq!(l.files.len(), 2);
    assert!(matches!(l.kind, VolumeKind::Labeled { .. }));
    // An active file is protected unless the save may clear it.
    let mut again = save_file("MYLIB");
    again.seq = SeqNbr::Number(1);
    let e = TapeWriter::new(&mut t, &mut NoMoreVolumes, again.clone()).write_stream(b"y").unwrap_err();
    assert_eq!(e.msgid(), "CPA4036");
    again.allow_active = true;
    TapeWriter::new(&mut t, &mut NoMoreVolumes, again).write_stream(b"y").unwrap();
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn two_volume_save_from_the_catalog() {
    let d = dir("two");
    let a = volume(&d, "VOL001", 2);
    let b = volume(&d, "VOL002", 2);
    let entries = vec![("VOL001".to_string(), a.clone()), ("VOL002".to_string(), b)];
    let mut cat = CatalogTapes::new(
        entries.clone(),
        Some("VOL001".into()),
        Box::new(|p: &str| rw(p)),
        Box::new(|_| false),
    );
    let mut t = VirtualTape::open(rw(&a).unwrap()).unwrap();
    let data = stream(1_500_000);
    let w = TapeWriter::new(&mut t, &mut cat, save_file("BIG")).write_stream(&data).unwrap();
    assert_eq!(w.volumes, ["VOL001", "VOL002"]);
    assert_eq!(cat.mounted, ["VOL002"]);
    drop(t);

    let mut back_cat = CatalogTapes::new(entries, Some("VOL001".into()), Box::new(|p: &str| rw(p)), Box::new(|_| false));
    let mut t = VirtualTape::open(rw(&a).unwrap()).unwrap();
    let mut r = TapeReader::new(
        &mut t,
        &mut back_cat,
        RestoreFile {
            label: Some("BIG".into()),
            ..RestoreFile::default()
        },
    );
    let (back, vols) = r.read_stream().unwrap();
    assert_eq!(vols, ["VOL001", "VOL002"]);
    assert_eq!(back.len(), data.len());
    assert_eq!(back, data);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn no_next_volume_asks_and_cancels() {
    let d = dir("ask");
    let a = volume(&d, "VOL001", 2);
    let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&asked);
    let mut cat = CatalogTapes::new(
        vec![("VOL001".to_string(), a.clone())],
        Some("VOL001".into()),
        Box::new(|p: &str| rw(p)),
        Box::new(move |r| {
            seen.lock().unwrap().push(r.volume_seq);
            false
        }),
    );
    let mut t = VirtualTape::open(rw(&a).unwrap()).unwrap();
    let e = TapeWriter::new(&mut t, &mut cat, save_file("BIG"))
        .write_stream(&stream(1_500_000))
        .unwrap_err();
    assert!(matches!(e, medio::MediaError::Tape(TapeError::Cancelled)), "{e}");
    assert_eq!(*asked.lock().unwrap(), [2]);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn optical_save_files() {
    assert_eq!(save_path("*", SaveTarget::Library("qgpl")), "/QGPL");
    assert_eq!(save_path("'SAVES/*'", SaveTarget::Library("QGPL")), "/SAVES/QGPL");
    assert_eq!(save_path("/A/B", SaveTarget::Library("QGPL")), "/A/B");
    let set = VolumeSet::default();
    let store = Arc::new(MemImage::new(Vec::new()));
    set.insert(Volume::create(store, "SAVE01", "", 8 << 20, 1).unwrap()).unwrap();
    let data = stream(100_000);
    let w = OpticalWriter::new(&set, "SAVE01", "/SAVES/QGPL", false).write_stream(&data).unwrap();
    assert_eq!(w.volumes, ["SAVE01"]);
    let e = OpticalWriter::new(&set, "SAVE01", "/SAVES/QGPL", false).write_stream(b"x").unwrap_err();
    assert_eq!(e.msgid(), "OPT1210");
    let (back, _) = OpticalReader::new(&set, "SAVE01", "/SAVES/QGPL").read_stream().unwrap();
    assert_eq!(back, data);
    let e = OpticalReader::new(&set, "SAVE01", "/NONE").read_stream().unwrap_err();
    assert_eq!(e.msgid(), "OPT1115");
}

/// The next optical volumes of a test, in order.
struct Spare(Vec<String>, Vec<u32>);

impl medio::optfile::OpticalVolumes for Spare {
    fn next_volume(&mut self, req: &tape::VolumeRequest) -> Option<String> {
        self.1.push(req.volume_seq);
        (!self.0.is_empty()).then(|| self.0.remove(0))
    }
}

#[test]
fn optical_saves_span_volumes() {
    let set = VolumeSet::default();
    for v in ["SPAN01", "SPAN02", "SPAN03"] {
        let store = Arc::new(MemImage::new(Vec::new()));
        set.insert(Volume::create(store, v, "", 1 << 20, 1).unwrap()).unwrap();
    }
    let data = stream(2_500_000);
    // Without further volumes it does not fit.
    let e = OpticalWriter::new(&set, "SPAN01", "/BIG", false).write_stream(&data).unwrap_err();
    assert_eq!(e.msgid(), "OPT1335");
    let mut next = Spare(vec!["SPAN02".into(), "SPAN03".into()], Vec::new());
    let w = OpticalWriter::new(&set, "SPAN01", "/BIG", false)
        .spanning(&mut next)
        .write_stream(&data)
        .unwrap();
    assert_eq!(w.volumes, ["SPAN01", "SPAN02", "SPAN03"]);
    assert_eq!(next.1, [2, 3]);
    let label = set.with("SPAN01", |v| v.read_all("/BIG.QSPART")).unwrap();
    let p = medio::optfile::Part::parse(&String::from_utf8_lossy(&label)).unwrap();
    assert_eq!((p.part, p.more), (1, true));

    // Read back across the three.
    let mut next = Spare(vec!["SPAN02".into(), "SPAN03".into()], Vec::new());
    let (back, vols) = OpticalReader::new(&set, "SPAN01", "/BIG").spanning(&mut next).read_stream().unwrap();
    assert_eq!(vols, ["SPAN01", "SPAN02", "SPAN03"]);
    assert_eq!(back, data);
    // Without the next volumes, or from the second, it refuses.
    assert!(OpticalReader::new(&set, "SPAN01", "/BIG").read_stream().is_err());
    let e = OpticalReader::new(&set, "SPAN02", "/BIG").read_stream().unwrap_err();
    assert!(e.to_string().contains("first volume"), "{e}");
    // Out of volumes: the write ends.
    set.insert(Volume::create(Arc::new(MemImage::new(Vec::new())), "SPAN04", "", 1 << 20, 1).unwrap()).unwrap();
    let mut none = Spare(Vec::new(), Vec::new());
    let e = OpticalWriter::new(&set, "SPAN04", "/BIG2", false)
        .spanning(&mut none)
        .write_stream(&data)
        .unwrap_err();
    assert!(matches!(e, medio::MediaError::Tape(TapeError::Cancelled)), "{e}");
}
