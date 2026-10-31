// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A virtual media library over catalog volumes: cartridges arrive in
//! *INSERT, are added, mounted by a save, demounted by ENDOPT(*UNLOAD),
//! ejected to a station and taken out, and come back in *INSERT; the
//! kept state round trips; a save spans two cartridges of the VOL list.

#![allow(clippy::many_single_char_names)]

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use medio::library::{CatalogLibrary, LibraryState, LibraryVolumes, drive_names};
use medio::tapefile::{RestoreFile, SaveFile, TapeReader, TapeWriter};
use medio::{MediaReader, MediaWriter};
use tape::device::{Allocation, Category, Location};
use tape::label::{Date, Expiry};
use tape::virt::{DRIVE_BASE, SLOT_BASE, STATION_BASE};
use tape::volume::{Check, InzOptions, inztap};
use tape::{EndOpt, SeqNbr, TapeError, VirtualTape, VolumeOptions};

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("medio-lib-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn rw(p: &Path) -> File {
    OpenOptions::new().read(true).write(true).open(p).unwrap()
}

fn volume(d: &Path, vol: &str, mb: u64) -> PathBuf {
    let p = d.join(vol);
    let mut t = VirtualTape::create(File::create_new(&p).unwrap(), &VolumeOptions::imgsiz(mb, vol)).unwrap();
    inztap(
        &mut t,
        &InzOptions {
            new_vol: Some(vol.into()),
            check: Check::No,
            ..InzOptions::default()
        },
    )
    .unwrap();
    p
}

/// The library over `vols` (index, path), with `state`.
fn open(vols: &[(u16, PathBuf)], state: &LibraryState) -> CatalogLibrary<File> {
    let tapes = vols
        .iter()
        .map(|(i, p)| (*i, VirtualTape::open(rw(p)).unwrap()))
        .collect();
    CatalogLibrary::open("TAPMLB76", "TAPVRT03", tapes, state).unwrap()
}

fn save_file(label: &str, vols: &[&str], end_opt: EndOpt) -> SaveFile {
    SaveFile {
        vols: vols.iter().map(|v| (*v).to_string()).collect(),
        seq: SeqNbr::End,
        label: label.into(),
        expires: Expiry::Perm,
        end_opt,
        today: Date::from_ymd(2026, 10, 31).unwrap(),
        allow_active: false,
        job: "QPADEV0001".into(),
    }
}

fn stream(n: usize) -> Vec<u8> {
    (0..n).map(|i| u8::try_from(i * 13 % 251).unwrap()).collect()
}

#[test]
fn cartridges_move_through_the_library() {
    let d = dir("moves");
    let vols: Vec<(u16, PathBuf)> = ["CTG001", "CTG002", "CTG003"]
        .iter()
        .enumerate()
        .map(|(i, v)| (u16::try_from(i + 1).unwrap(), volume(&d, v, 4)))
        .collect();
    assert_eq!(drive_names("TAPVRT03"), ["TAPVRT03D1", "TAPVRT03D2"]);

    // Insert: every cartridge arrives in *INSERT in its index's slot.
    let mut lib = open(&vols, &LibraryState::default());
    for (i, id) in ["CTG001", "CTG002", "CTG003"].iter().enumerate() {
        let c = lib.model.cartridge(id).unwrap();
        assert_eq!(c.category, Category::Insert);
        assert_eq!(c.location, Location::Slot(SLOT_BASE + u16::try_from(i).unwrap()));
    }
    // Not usable yet.
    assert_eq!(lib.mount("CTG001").unwrap_err().msgid(), "CPF67D2");
    lib.add("CTG001", Category::Share400).unwrap();
    lib.add("CTG002", Category::Share400).unwrap();
    lib.add("CTG003", Category::Cnv).unwrap();

    // Mount: a save with VOL(CTG001) moves it into the first drive.
    let i = lib.mount("CTG001").unwrap();
    assert_eq!(i, 0);
    assert_eq!(lib.model.cartridge("CTG001").unwrap().location, Location::Drive(DRIVE_BASE));
    let mut prov = LibraryVolumes::new(i, lib.spares(), Some("CTG001".into()), Box::new(|_| false));
    let data = stream(200_000);
    let w = TapeWriter::new(&mut lib.changer, &mut prov, save_file("LIBA", &["CTG001"], EndOpt::Rewind))
        .write_stream(&data)
        .unwrap();
    assert_eq!(w.volumes, ["CTG001"]);
    // ENDOPT(*REWIND): it stays mounted.
    assert!(!lib.after(i).unwrap());
    assert_eq!(lib.mounted(), Some((0, "CTG001".to_string())));

    // Kept and built again: the cartridge is still in the drive.
    let state = lib.state();
    let text = state.to_text();
    assert_eq!(LibraryState::parse(&text).unwrap(), state);
    drop(lib);
    let mut lib = open(&vols, &state);
    assert_eq!(lib.model.cartridge("CTG001").unwrap().location, Location::Drive(DRIVE_BASE));
    assert_eq!(lib.model.cartridge("CTG001").unwrap().category, Category::Share400);

    // A second volume goes to the empty drive; reading the first back
    // with ENDOPT(*UNLOAD) demounts it home.
    let j = lib.mount("CTG002").unwrap();
    assert_eq!(j, 1);
    let i = lib.mount("CTG001").unwrap();
    assert_eq!(i, 0);
    let mut prov = LibraryVolumes::new(i, Vec::new(), Some("CTG001".into()), Box::new(|_| false));
    let (back, _) = TapeReader::new(
        &mut lib.changer,
        &mut prov,
        RestoreFile {
            label: Some("LIBA".into()),
            end_opt: EndOpt::Unload,
            ..RestoreFile::default()
        },
    )
    .read_stream()
    .unwrap();
    assert_eq!(back, data);
    assert!(lib.after(i).unwrap());
    assert_eq!(lib.model.cartridge("CTG001").unwrap().location, Location::Slot(SLOT_BASE));

    // Demount by hand.
    assert!(lib.demount(1).unwrap());
    assert_eq!(lib.mounted(), None);

    // A *CNV cartridge leaves after one use and becomes *EJECT.
    let k = lib.mount("CTG003").unwrap();
    assert!(lib.demount(k).unwrap());
    let c = lib.model.cartridge("CTG003").unwrap();
    assert_eq!(c.category, Category::Eject);
    assert_eq!(c.location, Location::Station(STATION_BASE));

    // Eject: RMVTAPCTG to the next station; then taken out.
    let moves = lib.remove("CTG002").unwrap();
    assert_eq!(moves.len(), 1);
    assert_eq!(lib.model.cartridge("CTG002").unwrap().location, Location::Station(STATION_BASE + 1));
    assert!(lib.take_out("CTG002").is_some());
    let c = lib.model.cartridge("CTG002").unwrap();
    assert_eq!((c.category.clone(), c.location), (Category::Eject, Location::Outside));

    // Back in the catalog later: it arrives in *INSERT again.
    let state = lib.state();
    drop(lib);
    let lib = open(&vols, &state);
    assert_eq!(lib.model.cartridge("CTG002").unwrap().category, Category::Insert);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn a_save_spans_cartridges_of_the_vol_list() {
    let d = dir("span");
    let vols = vec![(1, volume(&d, "BIG001", 2)), (2, volume(&d, "BIG002", 2))];
    let mut lib = open(&vols, &LibraryState::default());
    lib.add("BIG001", Category::Share400).unwrap();
    lib.add("BIG002", Category::Share400).unwrap();
    let i = lib.mount("BIG001").unwrap();
    let mut prov = LibraryVolumes::new(i, lib.spares(), Some("BIG001".into()), Box::new(|_| false));
    let data = stream(1_500_000);
    let w = TapeWriter::new(&mut lib.changer, &mut prov, save_file("BIG", &["BIG001", "BIG002"], EndOpt::Unload))
        .write_stream(&data)
        .unwrap();
    assert_eq!(w.volumes, ["BIG001", "BIG002"]);
    assert_eq!(prov.mounted(), ["BIG002"]);
    assert!(lib.after(i).unwrap());
    assert_eq!(lib.model.cartridge("BIG001").unwrap().location, Location::Slot(SLOT_BASE));
    assert_eq!(lib.model.cartridge("BIG002").unwrap().location, Location::Slot(SLOT_BASE + 1));

    // VOL(*MOUNTED): no next volume without the operator; a retry reply
    // takes the next spare.
    let i = lib.mount("BIG001").unwrap();
    let mut prov = LibraryVolumes::new(i, lib.spares(), Some("BIG001".into()), Box::new(|_| false));
    let mut f = save_file("BIG", &[], EndOpt::Rewind);
    f.seq = SeqNbr::Number(1);
    f.allow_active = true;
    let e = TapeWriter::new(&mut lib.changer, &mut prov, f.clone()).write_stream(&data).unwrap_err();
    assert!(matches!(e, medio::MediaError::Tape(TapeError::Cancelled)), "{e}");
    lib.sync().unwrap();
    let i = lib.mount("BIG001").unwrap();
    let mut asked = 0;
    let mut prov = LibraryVolumes::new(
        i,
        lib.spares(),
        Some("BIG001".into()),
        Box::new(|_| {
            asked += 1;
            true
        }),
    );
    let w = TapeWriter::new(&mut lib.changer, &mut prov, f).write_stream(&data).unwrap();
    drop(prov);
    assert_eq!(asked, 1);
    assert_eq!(w.volumes, ["BIG001", "BIG002"]);

    // Drive allocation: a deallocated drive is never chosen.
    lib.sync().unwrap();
    lib.demount(0).unwrap();
    lib.allocate("TAPVRT03D1", Allocation::Deallocated).unwrap();
    assert_eq!(lib.mount("BIG001").unwrap(), 1);
    std::fs::remove_dir_all(d).unwrap();
}
