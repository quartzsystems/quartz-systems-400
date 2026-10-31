// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The virtual library's moves, the *TAPMLB cartridge model and the
//! resource names.

use std::io::Cursor;

use tape::device::{
    Allocation, Category, DriveResource, LibraryError, Location, MediaLibrary, Move,
    ResourceKind, Scanned, assign, execute, parse_hctl, tape_nodes,
};
use tape::virt::{DRIVE_BASE, SLOT_BASE, STATION_BASE};
use tape::{
    Changer, ChangerError, ElementKind, TapeTransport, TransportError, VirtualLibrary,
    VirtualTape, VolumeOptions,
};

type Lib = VirtualLibrary<Cursor<Vec<u8>>>;

fn cart(tag: &str) -> VirtualTape<Cursor<Vec<u8>>> {
    VirtualTape::create(Cursor::new(Vec::new()), &VolumeOptions::imgsiz(1, tag)).unwrap()
}

fn lib() -> Lib {
    let mut l = VirtualLibrary::new(4, 2, 1);
    for (i, t) in ["CTG001", "CTG002", "CTG003"].iter().enumerate() {
        l.insert(i, cart(t)).unwrap();
    }
    l
}

#[test]
fn moves_between_slots_drives_and_stations() {
    let mut l = lib();
    let elems = l.read_element_status().unwrap();
    assert_eq!(elems.iter().filter(|e| e.kind == ElementKind::Storage).count(), 4);
    assert_eq!(elems.iter().filter(|e| e.kind == ElementKind::Drive).count(), 2);
    assert_eq!(elems.iter().filter(|e| e.kind == ElementKind::ImportExport).count(), 1);
    assert_eq!(elems[0].kind, ElementKind::Transport);
    let s1 = elems.iter().find(|e| e.address == SLOT_BASE).unwrap();
    assert_eq!(s1.volume_tag.as_deref(), Some("CTG001"));

    // Nothing in the drive yet.
    assert!(matches!(l.rewind(), Err(TransportError::NotLoaded)));
    assert!(!l.status().unwrap().loaded);
    l.move_medium(SLOT_BASE + 1, DRIVE_BASE).unwrap();
    assert!(l.status().unwrap().loaded);
    l.write_block(b"hello").unwrap();
    assert_eq!(l.find("CTG002"), Some(DRIVE_BASE));
    // Still loaded: removal prevented.
    assert!(matches!(
        l.move_medium(DRIVE_BASE, SLOT_BASE + 1),
        Err(ChangerError::Check { key: 5, asc: 0x53, ascq: 2 })
    ));
    l.unload().unwrap();
    let e = l.read_element_status().unwrap();
    let d = e.iter().find(|e| e.address == DRIVE_BASE).unwrap();
    assert_eq!((d.full, d.source), (true, Some(SLOT_BASE + 1)));
    // Full destination, empty source, unknown element.
    assert!(matches!(l.move_medium(DRIVE_BASE, SLOT_BASE), Err(ChangerError::DestinationFull(_))));
    assert!(matches!(l.move_medium(SLOT_BASE + 3, SLOT_BASE + 1), Err(ChangerError::SourceEmpty(_))));
    assert!(matches!(l.move_medium(0x999, SLOT_BASE), Err(ChangerError::NoElement(0x999))));
    l.move_medium(DRIVE_BASE, SLOT_BASE + 1).unwrap();

    // Second drive, then out through the station.
    l.select_drive(1).unwrap();
    l.move_medium(SLOT_BASE + 1, DRIVE_BASE + 1).unwrap();
    let mut buf = [0u8; 16];
    assert_eq!(l.read_block(&mut buf).unwrap(), tape::ReadOutcome::Block(5));
    assert_eq!(&buf[..5], b"hello");
    l.unload().unwrap();
    l.move_medium(DRIVE_BASE + 1, STATION_BASE).unwrap();
    assert_eq!(l.take(0).unwrap().tag(), "CTG002");
    l.import(0, cart("NEW001")).unwrap();
    l.move_medium(STATION_BASE, SLOT_BASE + 3).unwrap();
    assert_eq!(l.find("NEW001"), Some(SLOT_BASE + 3));
    assert!(l.select_drive(2).is_err());
}

fn model(l: &mut Lib) -> MediaLibrary {
    let drives = vec![
        DriveResource {
            name: "TAP01".into(),
            address: DRIVE_BASE,
            allocation: Allocation::Allocated,
            cartridge: None,
        },
        DriveResource {
            name: "TAP02".into(),
            address: DRIVE_BASE + 1,
            allocation: Allocation::Deallocated,
            cartridge: None,
        },
    ];
    let mut m = MediaLibrary::new("TAPMLB01", drives, false);
    m.sync(&l.read_element_status().unwrap());
    m
}

#[test]
fn categories_and_their_rules() {
    let mut l = lib();
    let mut m = model(&mut l);
    let all: Vec<_> = m.cartridges().map(|c| (c.id.as_str(), c.category.clone())).collect();
    assert_eq!(
        all,
        [("CTG001", Category::Insert), ("CTG002", Category::Insert), ("CTG003", Category::Insert)]
    );
    // ADDTAPCTG: only from *INSERT, only to an assignable category.
    m.add("CTG001", Category::Share400).unwrap();
    assert!(matches!(m.add("CTG001", Category::NoShare), Err(LibraryError::Category { .. })));
    assert!(matches!(m.add("CTG002", Category::Eject), Err(LibraryError::BadCategory(_))));
    assert!(matches!(m.add("CTG002", Category::SysGen), Err(LibraryError::BadCategory(_))));
    assert!(matches!(m.add("NOSUCH", Category::Nl), Err(LibraryError::NotFound(_))));
    m.add("CTG002", Category::parse("PAYROLL").unwrap()).unwrap();
    m.add("CTG003", Category::Cnv).unwrap();
    // CHGTAPCTG.
    m.change("CTG001", Category::NoShare).unwrap();
    assert_eq!(m.cartridge("CTG001").unwrap().category, Category::NoShare);
    assert!(m.change("CTG001", Category::Insert).is_err());
    assert_eq!(Category::parse("*IPL"), Some(Category::Ipl));
    assert_eq!(Category::parse("*BOGUS"), None);
    assert_eq!(Category::User("PAYROLL".into()).name(), "PAYROLL");
    let e = m.add("CTG001", Category::Ipl).unwrap_err();
    assert_eq!(e.message_id(), "CPF67D2");

    // *SYSGEN libraries take no categories.
    let mut s = MediaLibrary::new("TAPMLB02", Vec::new(), true);
    s.sync(&l.read_element_status().unwrap());
    assert!(s.cartridges().all(|c| c.category == Category::SysGen));
    assert!(matches!(s.add("CTG001", Category::Nl), Err(LibraryError::SysGen(_))));
}

#[test]
fn volume_selection_and_its_moves() {
    let mut l = lib();
    let mut m = model(&mut l);
    // *INSERT cartridges cannot be mounted by VOL.
    assert!(matches!(m.select("CTG001"), Err(LibraryError::Category { .. })));
    m.add("CTG001", Category::NoShare).unwrap();
    m.add("CTG002", Category::Share400).unwrap();
    m.add("CTG003", Category::Cnv).unwrap();

    // Only TAP01 is allocated.
    let s = m.select("CTG001").unwrap();
    assert_eq!(s.drive, "TAP01");
    assert_eq!(s.moves, [Move { from: SLOT_BASE, to: DRIVE_BASE }]);
    execute(&mut l, &s.moves).unwrap();
    m.sync(&l.read_element_status().unwrap());
    assert_eq!(m.cartridge("CTG001").unwrap().location, Location::Drive(DRIVE_BASE));
    assert_eq!(m.select("CTG001").unwrap().moves, []);

    // Another volume: the first goes home, the second comes in.
    let s = m.select("CTG002").unwrap();
    assert_eq!(
        s.moves,
        [Move { from: DRIVE_BASE, to: SLOT_BASE }, Move { from: SLOT_BASE + 1, to: DRIVE_BASE }]
    );
    l.unload().unwrap();
    execute(&mut l, &s.moves).unwrap();
    m.sync(&l.read_element_status().unwrap());
    assert_eq!(m.drives[0].cartridge.as_deref(), Some("CTG002"));

    // *UNPROTECTED drives are used too; *DEALLOCATED never.
    m.drives[1].allocation = Allocation::Unprotected;
    let s = m.select("CTG003").unwrap();
    assert_eq!(s.drive, "TAP02");
    l.unload().unwrap();
    execute(&mut l, &s.moves).unwrap();
    m.sync(&l.read_element_status().unwrap());
    // A *CNV cartridge leaves through the station after use.
    l.select_drive(1).unwrap();
    l.unload().unwrap();
    let back = m.release("TAP02").unwrap();
    assert_eq!(back, [Move { from: DRIVE_BASE + 1, to: STATION_BASE }]);
    assert_eq!(m.cartridge("CTG003").unwrap().category, Category::Eject);
    execute(&mut l, &back).unwrap();
    // Others go home.
    assert_eq!(m.release("TAP01").unwrap(), [Move { from: DRIVE_BASE, to: SLOT_BASE + 1 }]);

    m.drives[0].allocation = Allocation::Deallocated;
    m.drives[1].allocation = Allocation::Deallocated;
    m.sync(&l.read_element_status().unwrap());
    let e = m.select("CTG001").unwrap_err();
    assert!(matches!(e, LibraryError::NoDrive(_)));
    assert_eq!(Allocation::Unprotected.name(), "*UNPROTECTED");
}

#[test]
fn rmvtapctg_ejects() {
    let mut l = lib();
    let mut m = model(&mut l);
    m.add("CTG003", Category::NoShare).unwrap();
    let moves = m.remove("CTG003").unwrap();
    assert_eq!(moves, [Move { from: SLOT_BASE + 2, to: STATION_BASE }]);
    assert_eq!(m.cartridge("CTG003").unwrap().category, Category::Eject);
    execute(&mut l, &moves).unwrap();
    // The station is now full: the next one stays in its slot.
    let moves = m.remove("CTG002").unwrap();
    assert!(moves.is_empty());
    // The operator takes the cartridge out: an *EJECT cartridge stays listed.
    l.take(0).unwrap();
    m.sync(&l.read_element_status().unwrap());
    assert_eq!(m.cartridge("CTG003").unwrap().location, Location::Outside);
    // Mounted cartridges cannot be removed.
    m.add("CTG001", Category::NoShare).unwrap();
    execute(&mut l, &m.select("CTG001").unwrap().moves).unwrap();
    m.sync(&l.read_element_status().unwrap());
    assert!(matches!(m.remove("CTG001"), Err(LibraryError::InUse(_))));
}

fn scanned(kind: ResourceKind, node: &str, hctl: &str, serial: &str) -> Scanned {
    Scanned {
        kind,
        node: node.into(),
        hctl: parse_hctl(hctl).unwrap(),
        serial: Some(serial.into()),
    }
}

#[test]
fn resource_names() {
    let entries = ["nst0", "st1", "st0", "st0a", "st0l", "st10", "nst1a", "st2m", "st2"];
    assert_eq!(tape_nodes(&entries), ["st0", "st1", "st2", "st10"]);
    assert_eq!(parse_hctl("2:0:1:0"), Some([2, 0, 1, 0]));
    assert_eq!(parse_hctl("2:0:1"), None);
    assert_eq!(parse_hctl("2:0:1:0:9"), None);

    let scan = vec![
        scanned(ResourceKind::Tape, "st1", "3:0:2:0", "XYZ002"),
        scanned(ResourceKind::Library, "sg4", "3:0:0:0", "LIB001"),
        scanned(ResourceKind::Tape, "st0", "3:0:1:0", "XYZ001"),
        scanned(ResourceKind::Tape, "st2", "1:0:0:0", "XYZ003"),
    ];
    let names: Vec<_> = assign(&scan, &[]).into_iter().map(|r| (r.name, r.device.node)).collect();
    assert_eq!(
        names,
        [
            ("TAP01".into(), "st2".into()),
            ("TAP02".into(), "st0".into()),
            ("TAP03".into(), "st1".into()),
            ("TAPMLB01".into(), "sg4".into()),
        ]
    );
    // The same input in any order gives the same names.
    let mut rev = scan.clone();
    rev.reverse();
    assert_eq!(assign(&rev, &[]), assign(&scan, &[]));
    // Known serials keep their names; new devices fill the gaps.
    let known = [
        ("XYZ001".to_string(), "TAP01".to_string()),
        ("LIB001".to_string(), "TAPMLB03".to_string()),
    ];
    let names: Vec<_> = assign(&scan, &known).into_iter().map(|r| (r.name, r.device.node)).collect();
    assert_eq!(
        names,
        [
            ("TAP02".into(), "st2".into()),
            ("TAP01".into(), "st0".into()),
            ("TAP03".into(), "st1".into()),
            ("TAPMLB03".into(), "sg4".into()),
        ]
    );
}
