// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writable virtual volumes: sessions, capacity, protection, the
//! command operations, devices and the volume set.

#![allow(clippy::cast_possible_truncation, clippy::many_single_char_names)]

use std::fmt::Write as _;
use std::sync::Arc;

use optical::device::open_media;
use optical::iso::hybrid::{self, Guids, HybridLayout};
use optical::volume::layout::LABEL_FILE;
use optical::volume::ops::{self, DupOpt, InzOpt};
use optical::{
    Error, FileImage, ImageStore, IsoBuilder, IsoImage, MemImage, ReadAt, Volume, VolumeSet,
};

const MIB: u64 = 1024 * 1024;

fn store() -> Arc<dyn ImageStore> {
    Arc::new(MemImage::default())
}

fn fresh(cap: u64) -> (Arc<dyn ImageStore>, Volume) {
    let s = store();
    let v = Volume::create(s.clone(), "save01", "nightly saves", cap, 100).unwrap();
    (s, v)
}

#[test]
fn a_session_adds_replaces_deletes_and_reopens() {
    let (s, mut v) = fresh(16 * MIB);
    assert_eq!(v.id(), "SAVE01");
    assert!(v.is_writable());
    assert!(v.list("/").unwrap().is_empty(), "the label is hidden");
    v.mkdir("/QGPL").unwrap();
    v.put_file("/QGPL/SAVF1", vec![1u8; 5000], false).unwrap();
    v.put_file("/MYLIB", b"saved library".to_vec(), false)
        .unwrap();
    assert!(matches!(
        v.put_file("/mylib", vec![], false),
        Err(Error::Exists(_))
    ));
    v.put_file("/mylib", b"replaced".to_vec(), true).unwrap();
    v.create_file("/notes.txt").unwrap();
    v.write("/notes.txt", 3, b"abc").unwrap();
    assert_eq!(v.read_all("/NOTES.TXT").unwrap(), b"\0\0\0abc");
    v.truncate("/notes.txt", 4).unwrap();
    let mut deep = String::new();
    for i in 0..10 {
        let _ = write!(deep, "/d{i}");
        v.mkdir(&deep).unwrap();
    }
    v.put_file(
        &format!("{deep}/{}", "x".repeat(64)),
        b"deep".to_vec(),
        false,
    )
    .unwrap();
    assert!(matches!(v.mkdir("/a/b"), Err(Error::NotFound(_))));
    assert!(matches!(v.remove("/QGPL"), Err(Error::NotEmpty(_))));
    v.remove("/QGPL/SAVF1").unwrap();
    v.remove("/QGPL").unwrap();
    assert!(matches!(
        v.put_file("/QS400VOL.LBL", vec![], false),
        Err(Error::Exists(_))
    ));
    assert!(v.is_dirty());
    assert!(v.flush(200).unwrap());
    assert!(!v.flush(201).unwrap());

    let first = s.size().unwrap();
    let v2 = Volume::open(s.clone(), false).unwrap();
    assert_eq!(v2.read_all("/MYLIB").unwrap(), b"replaced");
    assert_eq!(v2.read_all("/notes.txt").unwrap(), b"\0\0\0a");
    assert!(v2.stat("/QGPL").is_err());
    assert_eq!(v2.label().unwrap().changed, 200);
    assert_eq!(v2.label().unwrap().text, "nightly saves");
    let names: Vec<String> = v2.list("/").unwrap().into_iter().map(|e| e.name).collect();
    assert_eq!(names, ["MYLIB", "d0", "notes.txt"]);
    // Any ISO reader sees the volume, label and all.
    let img = IsoImage::open(s.clone()).unwrap();
    assert!(img.lookup(LABEL_FILE).is_ok());
    assert_eq!(img.info().volume_id, "SAVE01");
    assert!(first > 0);
}

#[test]
fn the_same_session_writes_the_same_bytes() {
    let run = || {
        let (s, mut v) = fresh(4 * MIB);
        v.mkdir("/B").unwrap();
        v.put_file("/B/one", vec![9; 3000], false).unwrap();
        v.put_file("/A", vec![8; 10], false).unwrap();
        v.flush(500).unwrap();
        let mut b = vec![0u8; usize::try_from(s.size().unwrap()).unwrap()];
        s.read_exact_at(0, &mut b).unwrap();
        b
    };
    assert_eq!(run(), run());
}

#[test]
fn capacity_and_protection() {
    let (s, mut v) = fresh(MIB);
    let err = v
        .put_file("/big", vec![0; 2 * MIB as usize], false)
        .unwrap_err();
    assert!(matches!(err, Error::VolumeFull { .. }), "{err}");
    assert_eq!(err.msgid(), "OPT1335");
    assert!(v.stat("/big").is_err(), "a refused file leaves nothing");
    v.put_file("/small", vec![1; 1000], false).unwrap();
    assert!(matches!(
        v.write("/small", 0, &vec![0; 2 * MIB as usize]),
        Err(Error::VolumeFull { .. })
    ));
    assert_eq!(v.stat("/small").unwrap().size, 1000);
    assert!(matches!(
        v.truncate("/small", 3 * MIB),
        Err(Error::VolumeFull { .. })
    ));
    v.flush(1).unwrap();

    let mut p = Volume::open(s.clone(), true).unwrap();
    assert!(!p.is_writable());
    assert!(matches!(p.mkdir("/x"), Err(Error::WriteProtected)));
    assert!(matches!(
        p.put_file("/x", vec![], false),
        Err(Error::WriteProtected)
    ));
    p.set_write_protect(false);
    p.mkdir("/x").unwrap();

    // A plain ISO is read only.
    let mut b = IsoBuilder::new("PLAIN", 0);
    b.add_file("README", b"hi".to_vec()).unwrap();
    let iso: Arc<dyn ImageStore> = Arc::new(MemImage::new(b.build().unwrap()));
    let mut r = Volume::open(iso, false).unwrap();
    assert_eq!(r.id(), "PLAIN");
    assert!(matches!(r.mkdir("/x"), Err(Error::ReadOnly)));
    assert_eq!(r.read_all("/readme").unwrap(), b"hi");
    assert_eq!(r.list("").unwrap().len(), 1);
}

#[test]
fn multi_extent_files_on_a_volume_survive_a_rewrite() {
    // Files written as several extents by another writer read and copy.
    let mut b = IsoBuilder::new("PIECES", 0);
    b.extent_limit(2048);
    let data: Vec<u8> = (0..9000u32).map(|i| (i % 7) as u8).collect();
    b.add_file("pieces.bin", data.clone()).unwrap();
    let src: Arc<dyn ImageStore> = Arc::new(MemImage::new(b.build().unwrap()));
    let from = Volume::open(src, false).unwrap();
    let to = store();
    let copy = ops::dupopt(
        &from,
        to,
        &DupOpt {
            new_volume: Some("COPY".into()),
            capacity: Some(MIB),
            ..DupOpt::default()
        },
    )
    .unwrap();
    assert_eq!(copy.read_all("/pieces.bin").unwrap(), data);
}

#[test]
fn inzopt_dspopt_chkopt_dupopt() {
    let (s, mut v) = fresh(8 * MIB);
    v.mkdir("/LIB1").unwrap();
    v.put_file("/LIB1/OBJ", vec![5; 7000], false).unwrap();
    v.put_file("/top", vec![6; 10], false).unwrap();
    v.set_text("changed text").unwrap();
    v.flush(300).unwrap();

    let a = ops::volume_attrs(&v).unwrap();
    assert_eq!(a.volume_id, "SAVE01");
    assert_eq!(a.text, "changed text");
    assert_eq!(a.capacity, 8 * MIB);
    assert_eq!(a.used + a.free, a.capacity);
    assert_eq!(a.media_format, "*ISO9660");
    assert!(a.writable_layout && !a.write_protected && a.joliet);
    let dirs = ops::dir_attrs(&v, "/", true).unwrap();
    assert_eq!(dirs[0].path, "/");
    assert_eq!((dirs[0].files, dirs[0].dirs), (1, 1));
    assert_eq!(dirs[1].path, "/LIB1");
    assert_eq!(dirs[1].bytes, 7000);
    let files = ops::file_attrs(&v, "/LIB1").unwrap();
    assert_eq!(files[0].path, "/LIB1/OBJ");

    let r = ops::chkopt(&v);
    assert_eq!((r.files, r.dirs, r.bytes), (2, 2, 7010));
    assert!(r.errors.is_empty());

    let to = store();
    let copy = ops::dupopt(
        &v,
        to.clone(),
        &DupOpt {
            new_volume: Some("SAVE02".into()),
            epoch: 400,
            ..DupOpt::default()
        },
    )
    .unwrap();
    assert_eq!(copy.id(), "SAVE02");
    assert_eq!(copy.read_all("/LIB1/OBJ").unwrap(), vec![5; 7000]);
    assert_eq!(copy.label().unwrap().capacity, 8 * MIB);
    assert!(ops::dupopt(&v, s.clone(), &DupOpt::default()).is_err());

    let inz = InzOpt {
        new_volume: "FRESH".into(),
        check: true,
        clear: false,
        text: "new".into(),
        capacity: None,
        epoch: 500,
    };
    assert!(matches!(
        ops::inzopt(s.clone(), &inz),
        Err(Error::ActiveFiles)
    ));
    let v = ops::inzopt(
        s.clone(),
        &InzOpt {
            check: false,
            clear: true,
            ..inz.clone()
        },
    )
    .unwrap();
    assert_eq!(v.id(), "FRESH");
    assert!(v.list("/").unwrap().is_empty());
    assert_eq!(v.label().unwrap().capacity, 8 * MIB);
    assert!(matches!(
        ops::inzopt(
            s,
            &InzOpt {
                new_volume: "-bad".into(),
                ..inz.clone()
            }
        ),
        Err(Error::BadVolumeId(_))
    ));
    // An empty store needs a capacity.
    assert!(ops::inzopt(store(), &inz).is_err());
    let v = ops::inzopt(
        store(),
        &InzOpt {
            capacity: Some(MIB),
            ..inz
        },
    )
    .unwrap();
    assert_eq!(v.id(), "FRESH");
}

#[test]
fn a_damaged_volume_fails_chkopt_without_panicking() {
    let (s, mut v) = fresh(4 * MIB);
    v.put_file("/f", vec![1; 4096], false).unwrap();
    v.flush(1).unwrap();
    let mut bytes = vec![0u8; usize::try_from(s.size().unwrap()).unwrap()];
    s.read_exact_at(0, &mut bytes).unwrap();
    // Cut the image short of the file's data.
    let e = v.image().lookup("/f").unwrap();
    bytes.truncate(e.extents[0].0 as usize * 2048 + 10);
    let cut: Arc<dyn ImageStore> = Arc::new(MemImage::new(bytes));
    if let Ok(v) = Volume::open(cut, false) {
        let r = ops::chkopt(&v);
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].1.starts_with("OPT1605"));
    }
}

#[test]
fn file_images_on_the_real_file_system() {
    let dir = std::env::temp_dir().join(format!("optical-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("vol.iso");
    let _ = std::fs::remove_file(&path);
    let s: Arc<dyn ImageStore> = Arc::new(FileImage::create(&path, &[]).unwrap());
    let mut v = ops::inzopt(
        s,
        &InzOpt {
            new_volume: "DISK1".into(),
            check: true,
            clear: false,
            text: String::new(),
            capacity: Some(4 * MIB),
            epoch: 7,
        },
    )
    .unwrap();
    v.put_file("/a.txt", b"on disk".to_vec(), false).unwrap();
    v.flush(8).unwrap();
    drop(v);
    let s: Arc<dyn ImageStore> = Arc::new(FileImage::open(&path).unwrap());
    let v = Volume::open(s, false).unwrap();
    assert_eq!(v.read_all("/a.txt").unwrap(), b"on disk");
    drop(v);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn media_on_devices() {
    let mut b = IsoBuilder::new("STICK", 0);
    b.add_file("QS400/MEDIA.MANIFEST", b"m".to_vec()).unwrap();
    let iso = b.build().unwrap();
    // At the device's start.
    let img = open_media(iso.clone()).unwrap();
    assert_eq!(img.info().volume_id, "STICK");
    // Inside a GPT partition at LBA 2048.
    let start = 2048 * 512;
    let total = (start + iso.len() + 64 * 2048) as u64;
    let mut disk = vec![0u8; usize::try_from(total).unwrap()];
    disk[start..start + iso.len()].copy_from_slice(&iso);
    // Borrow the hybrid writer's GPT for a disk whose partition 1 holds the ISO.
    let l = HybridLayout::new(total, (start / 2048) as u32, iso.len() as u64).unwrap();
    let g = Guids::derived(b"t");
    let sa = l.system_area(&g);
    disk[..1024 + 16384].copy_from_slice(&sa[..1024 + 16384]);
    let tail = l.tail(&g);
    let off = usize::try_from(l.tail_offset()).unwrap();
    disk[off..].copy_from_slice(&tail);
    assert!(hybrid::verify(&disk).is_ok());
    let img = open_media(disk).unwrap();
    assert!(img.lookup("QS400/MEDIA.MANIFEST").is_ok());
    assert!(open_media(vec![0u8; 100_000]).is_err());
}

#[test]
fn the_volume_set() {
    let set = VolumeSet::new(Arc::new(|| 900));
    let (s, v) = fresh(4 * MIB);
    set.insert(v).unwrap();
    let (_, dup) = fresh(MIB);
    assert!(matches!(set.insert(dup), Err(Error::Exists(_))));
    assert_eq!(set.names(), ["SAVE01"]);
    set.with("save01", |v| v.put_file("/x", b"x".to_vec(), false))
        .unwrap();
    assert!(set.sync("SAVE01").unwrap());
    let v = set.remove("SAVE01").unwrap().unwrap();
    assert_eq!(v.label().unwrap().changed, 900);
    assert!(set.names().is_empty());
    assert!(set.with("SAVE01", |_| Ok(())).is_err());
    assert_eq!(
        Volume::open(s, false).unwrap().read_all("/x").unwrap(),
        b"x"
    );
}
