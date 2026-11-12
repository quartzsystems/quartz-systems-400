// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The bytes on disk stay what they were. `tests/golden/` holds both
//! copies and the resume area as the code before `abrec` wrote them
//! (at `bf2b492c5`); this code must write the same bytes and read those.
//! `QS400_UPDATE_GOLDEN=1` rewrites them, which only a format version
//! bump may do.

use std::io::Cursor;
use std::path::PathBuf;

use licpart::{
    IplHistory, IplMode, IplType, KeyPurpose, KeySlot, LicPart, Outcome, PalEntry, RESUME_OFFSET,
    Record, SERVICE_TAGS, SLOT_BYTES, Severity, Slot,
};

const RESUME: &[u8] = b"unit 2 of 3: 1184 chunks copied";

fn device() -> Cursor<Vec<u8>> {
    Cursor::new(vec![
        0u8;
        usize::try_from(RESUME_OFFSET).unwrap() + SLOT_BYTES
    ])
}

/// A device after three record writes and one of the resume area, and
/// the record last written.
fn written() -> (Vec<u8>, Record) {
    let (mut part, mut rec) = LicPart::init(device(), 1_700_000_000).unwrap();
    rec.header.ipl_in_progress = true;
    rec.header.last_ipl_type = IplType::B;
    rec.header.last_ipl_mode = IplMode::Manual;
    rec.header.last_slot = Slot::B;
    rec.header.next_mode = Some(IplMode::Normal);
    rec.header.ipl_count = 12;
    rec.header.abnormal_count = 2;
    rec.header.install_stage = 1;
    rec.header.auto_install = true;
    rec.push_history(IplHistory {
        start_time: 1_700_000_100,
        end_time: 1_700_000_160,
        ipl_type: IplType::A,
        mode: IplMode::Normal,
        slot: Slot::A,
        final_src: *b"C6001090",
        outcome: Outcome::Normal,
    });
    rec.push_pal(PalEntry {
        time: 1_700_000_120,
        code: *b"A6001255",
        severity: Severity::Attention,
        resource: "TAP01".into(),
    });
    rec.set_key_slot(KeySlot {
        purpose: KeyPurpose::CertStash,
        generation: 3,
        created: 1_700_000_050,
        key: [0x5A; 32],
    });
    rec.set_tlv(0x0100, b"QSECURITY=40".to_vec()).unwrap();
    assert_eq!(part.write(&mut rec).unwrap(), Slot::B);

    // The owner of the service TLVs writes next, over copy A.
    let mut svc = part.read().unwrap();
    svc.set_tlv(SERVICE_TAGS[0], b"CMN01=0000:00:03.0".to_vec())
        .unwrap();
    svc.header.ipl_in_progress = false;
    svc.header.last_good_slot = Slot::B;
    svc.set_tlv(0x0101, vec![1, 2, 3, 0, 0]).unwrap();
    assert_eq!(part.write_service(&mut svc).unwrap(), Slot::A);
    assert_eq!(svc.sequence, 3);

    part.write_resume(RESUME).unwrap();
    (part.into_device().into_inner(), svc)
}

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name)
}

/// The three areas of a device: copy A, copy B, the resume area.
fn areas(dev: &[u8]) -> [(&'static str, &[u8]); 3] {
    let resume = usize::try_from(RESUME_OFFSET).unwrap();
    [
        ("copy-a.bin", &dev[..SLOT_BYTES]),
        ("copy-b.bin", &dev[SLOT_BYTES..2 * SLOT_BYTES]),
        ("resume.bin", &dev[resume..]),
    ]
}

fn trimmed(area: &[u8]) -> &[u8] {
    let end = area.iter().rposition(|b| *b != 0).map_or(0, |p| p + 1);
    &area[..end]
}

/// A device holding the golden areas.
fn from_golden() -> Vec<u8> {
    let mut dev = device().into_inner();
    let resume = usize::try_from(RESUME_OFFSET).unwrap();
    for (name, at) in [
        ("copy-a.bin", 0),
        ("copy-b.bin", SLOT_BYTES),
        ("resume.bin", resume),
    ] {
        let bytes = std::fs::read(golden(name)).unwrap();
        dev[at..at + bytes.len()].copy_from_slice(&bytes);
    }
    dev
}

#[test]
fn writes_the_bytes_written_before() {
    let (dev, _) = written();
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden("")).unwrap();
        for (name, area) in areas(&dev) {
            std::fs::write(golden(name), trimmed(area)).unwrap();
        }
    }
    // Every byte of the device, the gap between the areas too.
    assert!(dev == from_golden(), "the device differs from the golden");
    for (name, area) in areas(&dev) {
        assert!(!trimmed(area).is_empty(), "{name} is empty");
    }
}

#[test]
fn reads_the_bytes_written_before() {
    let (_, last) = written();
    let mut part = LicPart::open(Cursor::new(from_golden()));
    assert_eq!(part.read().unwrap(), last);
    let [a, b] = part.read_slots();
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!((a.sequence, b.sequence), (3, 2));
    assert_eq!(a, last);
    assert!(b.header.ipl_in_progress);
    assert_eq!(b.tlv(SERVICE_TAGS[0]), None);
    assert_eq!(b.key_slot(KeyPurpose::CertStash).unwrap().generation, 3);
    assert_eq!(part.read_resume().unwrap(), RESUME);
    // The next write goes over the older copy with the next sequence.
    let mut rec = part.read().unwrap();
    assert_eq!(part.write(&mut rec).unwrap(), Slot::B);
    assert_eq!(rec.sequence, 4);
}

#[test]
fn the_magics_are_where_they_were() {
    let dev = from_golden();
    let resume = usize::try_from(RESUME_OFFSET).unwrap();
    assert_eq!(&dev[..8], b"QS400LIC");
    assert_eq!(&dev[SLOT_BYTES..SLOT_BYTES + 8], b"QS400LIC");
    assert_eq!(&dev[resume..resume + 8], b"QS400RSJ");
}
