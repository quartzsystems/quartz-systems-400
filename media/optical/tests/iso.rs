// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The writer and the reader together: round trips with and without
//! Joliet and Rock Ridge, deep trees, long names, multi-extent files,
//! El Torito, the hybrid image, and mutated images.

#![allow(clippy::cast_possible_truncation)]

use std::fmt::Write as _;

use optical::iso::hybrid::{self, names};
use optical::iso::{SECTOR, boot};
use optical::{IsoBuilder, IsoImage};

const EPOCH: u64 = 1_767_225_600;

fn sample(joliet: bool, rr: bool) -> Vec<u8> {
    let mut b = IsoBuilder::new("QS400_TEST", EPOCH);
    b.joliet(joliet).rock_ridge(rr);
    b.add_file("EFI.IMG", vec![7u8; 5000]).unwrap();
    b.add_file("QS400.TXT", b"hello".to_vec()).unwrap();
    b.add_file("QS400/MEDIA.MANIFEST", b"manifest".to_vec())
        .unwrap();
    b.add_file("QS400/LIC/qs400.efi", vec![3u8; 3000]).unwrap();
    b.add_file(
        "Mixed Case/a long file name with spaces.text",
        b"x".to_vec(),
    )
    .unwrap();
    b.add_dir("empty").unwrap();
    b.boot_image("EFI.IMG").unwrap();
    b.build().unwrap()
}

#[test]
fn round_trip_with_every_extension() {
    let iso = sample(true, true);
    assert_eq!(iso.len() % SECTOR, 0);
    let img = IsoImage::open(iso.as_slice()).unwrap();
    assert!(img.info().joliet);
    assert!(img.info().rock_ridge);
    assert_eq!(img.info().volume_id, "QS400_TEST");
    assert_eq!(img.info().joliet_volume_id.as_deref(), Some("QS400_TEST"));
    let e = img.lookup("QS400/LIC/qs400.efi").unwrap();
    assert_eq!(img.read_file(&e).unwrap(), vec![3u8; 3000]);
    let e = img
        .lookup("Mixed Case/a long file name with spaces.text")
        .unwrap();
    assert_eq!(img.read_file(&e).unwrap(), b"x");
    assert!(img.lookup("empty").unwrap().is_dir());
    assert!(img.lookup("qs400/media.manifest").is_ok());
    let names: Vec<String> = img.walk().unwrap().into_iter().map(|(p, _)| p).collect();
    assert!(names.contains(&"QS400/LIC".to_string()));
    // Determinism.
    assert_eq!(sample(true, true), iso);
}

#[test]
fn rock_ridge_names_without_joliet() {
    let iso = sample(false, true);
    let img = IsoImage::open(iso.as_slice()).unwrap();
    assert!(!img.info().joliet);
    assert!(img.info().rock_ridge);
    let e = img.lookup("QS400/LIC/qs400.efi").unwrap();
    assert_eq!(e.name, "qs400.efi");
    // Preferring the primary tree over Joliet still reads Rock Ridge.
    let iso = sample(true, true);
    let img = IsoImage::open_with(iso.as_slice(), false).unwrap();
    assert!(
        img.lookup("Mixed Case/a long file name with spaces.text")
            .is_ok()
    );
}

#[test]
fn level2_names_alone() {
    let iso = sample(false, false);
    let img = IsoImage::open(iso.as_slice()).unwrap();
    assert!(!img.info().rock_ridge && !img.info().joliet);
    let e = img.lookup("QS400/LIC/QS400.EFI").unwrap();
    assert_eq!(e.name, "QS400.EFI");
    let dirs = img.read_dir("").unwrap();
    assert!(dirs.iter().any(|d| d.name == "MIXED_CASE"));
    // The old reader's view: the root files under level 2 names.
    let root = img.read_dir("/").unwrap();
    assert!(root.iter().any(|d| d.name == "EFI.IMG"));
}

#[test]
fn deep_trees_and_long_names() {
    let mut b = IsoBuilder::new("DEEP", EPOCH);
    let mut path = String::new();
    for i in 0..12 {
        let _ = write!(path, "level{i}/");
    }
    let long = "n".repeat(64);
    b.add_file(&format!("{path}{long}"), b"deep".to_vec())
        .unwrap();
    // Many names that fold to one level 2 identifier.
    for i in 0..300 {
        b.add_file(
            &format!("crowd/a-file-name-that-is-long-{i:03}.dat"),
            vec![1],
        )
        .unwrap();
    }
    let iso = b.build().unwrap();
    let img = IsoImage::open(iso.as_slice()).unwrap();
    let e = img.lookup(&format!("{path}{long}")).unwrap();
    assert_eq!(img.read_file(&e).unwrap(), b"deep");
    assert_eq!(img.read_dir("crowd").unwrap().len(), 300);
    let p = IsoImage::open_with(iso.as_slice(), false).unwrap();
    assert_eq!(p.read_dir("crowd").unwrap().len(), 300);
    let plain = {
        let mut b2 = b.clone();
        b2.joliet(false).rock_ridge(false);
        b2.build().unwrap()
    };
    let p = IsoImage::open(plain.as_slice()).unwrap();
    let mut ids: Vec<String> = p
        .read_dir("CROWD")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    ids.dedup();
    assert_eq!(ids.len(), 300);
    assert!(
        IsoBuilder::new("X", 0)
            .add_file(&"y".repeat(65), vec![])
            .is_ok()
    );
    assert!(
        IsoBuilder::new("X", 0)
            .add_file(&"y".repeat(65), vec![])
            .unwrap()
            .build()
            .is_err()
    );
}

#[test]
fn multi_extent_files() {
    let mut b = IsoBuilder::new("BIG", EPOCH);
    b.extent_limit(4096);
    let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
    b.add_file("big.bin", data.clone()).unwrap();
    b.add_file("after.txt", b"after".to_vec()).unwrap();
    let iso = b.build().unwrap();
    for joliet in [true, false] {
        let img = IsoImage::open_with(iso.as_slice(), joliet).unwrap();
        let e = img.lookup("big.bin").unwrap();
        assert_eq!(e.extents.len(), 3);
        assert_eq!(e.size, 10_000);
        assert_eq!(img.read_file(&e).unwrap(), data);
        let e = img.lookup("after.txt").unwrap();
        assert_eq!(img.read_file(&e).unwrap(), b"after");
    }
    let lay = b.layout().unwrap();
    let big = lay.files.iter().find(|f| f.path == "big.bin").unwrap();
    assert_eq!(
        big.extents.iter().map(|e| e.1).collect::<Vec<_>>(),
        vec![4096, 4096, 1808]
    );
}

#[test]
fn links_share_extents() {
    let mut b = IsoBuilder::new("LINK", EPOCH);
    b.add_file("EFI/BOOT/BOOTX64.EFI", vec![9u8; 7000]).unwrap();
    b.add_link("QS400/LIC/qs400.efi", "EFI/BOOT/BOOTX64.EFI")
        .unwrap();
    let iso = b.build().unwrap();
    let img = IsoImage::open(iso.as_slice()).unwrap();
    let a = img.lookup("EFI/BOOT/BOOTX64.EFI").unwrap();
    let l = img.lookup("QS400/LIC/qs400.efi").unwrap();
    assert_eq!(a.extents, l.extents);
    assert_eq!(img.read_file(&l).unwrap(), vec![9u8; 7000]);
}

#[test]
fn el_torito_entry() {
    let iso = sample(true, true);
    let img = IsoImage::open(iso.as_slice()).unwrap();
    let boot = img.info().boot.unwrap();
    assert_eq!(boot.platform, boot::PLATFORM_EFI);
    assert!(boot.bootable);
    assert_eq!(boot.sectors512, 10);
    let e = img.lookup("EFI.IMG").unwrap();
    assert_eq!(e.extents[0].0, boot.lba);
    // Descriptors in their sectors: PVD, boot record, SVD, terminator.
    assert_eq!(iso[16 * SECTOR], 1);
    assert_eq!(iso[17 * SECTOR], 0);
    assert_eq!(iso[18 * SECTOR], 2);
    assert_eq!(iso[19 * SECTOR], 255);
    assert_eq!(
        &iso[16 * SECTOR + 813..16 * SECTOR + 829],
        b"2026010100000000"
    );
}

#[test]
fn hybrid_image_partitions_match_the_boot_image() {
    let mut b = IsoBuilder::new("QS400_HYBRID", EPOCH);
    b.add_file("efi.img", vec![5u8; 64 * 1024]).unwrap();
    b.add_file("EFI/BOOT/BOOTX64.EFI", vec![1u8; 9000]).unwrap();
    b.add_file("zz-last-by-name.txt", b"z".to_vec()).unwrap();
    b.boot_image("efi.img").unwrap().hybrid(true);
    let iso = b.build().unwrap();
    let lay = b.layout().unwrap();
    let h = lay.hybrid.clone().unwrap();
    assert_eq!(h.total, iso.len() as u64);
    let gpt = hybrid::verify(&iso).unwrap();
    let (sector, len) = lay.boot.unwrap();
    let esp = &gpt.partitions[0];
    assert_eq!(esp.type_guid, hybrid::EFI_SYSTEM);
    assert_eq!(esp.first_lba, u64::from(sector) * 4);
    assert_eq!(esp.last_lba, u64::from(sector) * 4 + len / 512 - 1);
    let payload = &gpt.partitions[1];
    assert_eq!(payload.type_guid, hybrid::QUARTZ_MEDIA);
    assert_eq!(payload.first_lba, 64);
    assert_eq!(payload.last_lba + 1, esp.first_lba);
    assert_eq!(h.region(names::ESP).sector(), u64::from(sector));
    // Still an ISO with its boot entry.
    let img = IsoImage::open(iso.as_slice()).unwrap();
    assert_eq!(img.info().boot.unwrap().lba, sector);
    assert_eq!(img.info().sectors as usize * SECTOR, iso.len());
    assert_eq!(b.build().unwrap(), iso);
}

/// A small deterministic generator.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn mutated_images_never_panic() {
    let mut b = IsoBuilder::new("FUZZ", EPOCH);
    b.extent_limit(2048);
    b.add_file("a/b/c.txt", vec![1u8; 5000]).unwrap();
    b.add_file("a/d.txt", b"dd".to_vec()).unwrap();
    b.add_file("efi.img", vec![0u8; 2048]).unwrap();
    b.boot_image("efi.img").unwrap();
    let base = b.build().unwrap();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    // Mutate the metadata area, where damage matters.
    let meta = base.len().min(40 * SECTOR);
    for round in 0..3000 {
        let mut img = base.clone();
        let flips = 1 + (rng.next() % 16) as usize;
        for _ in 0..flips {
            let at = (rng.next() as usize) % meta;
            img[at] = rng.next() as u8;
        }
        if round % 50 == 0 {
            img.truncate((rng.next() as usize) % base.len());
        }
        for joliet in [true, false] {
            if let Ok(iso) = IsoImage::open_with(img.as_slice(), joliet)
                && let Ok(all) = iso.walk()
            {
                for (_, e) in all {
                    if !e.is_dir() && e.size < 1 << 20 {
                        let _ = iso.read_file(&e);
                    }
                }
            }
        }
    }
}
