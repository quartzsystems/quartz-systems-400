// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The side of the next IPL in the record: a record written before the
//! value existed still reads (the golden copies are such records), and
//! the value survives the record's encoding and the partition's write.

use std::io::Cursor;
use std::path::PathBuf;

use licpart::{
    ConsoleMode, IplSource, LicPart, RESUME_OFFSET, Record, SLOT_BYTES, Slot, TAG_IPL_SOURCE,
};

/// A device holding the golden copies, written before the value existed.
fn from_golden() -> Cursor<Vec<u8>> {
    let mut dev = vec![0u8; usize::try_from(RESUME_OFFSET).unwrap() + SLOT_BYTES];
    for (name, at) in [("copy-a.bin", 0), ("copy-b.bin", SLOT_BYTES)] {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(name);
        let bytes = std::fs::read(p).unwrap();
        dev[at..at + bytes.len()].copy_from_slice(&bytes);
    }
    Cursor::new(dev)
}

#[test]
fn a_record_written_before_still_reads_and_means_slot_a() {
    let mut part = LicPart::open(from_golden());
    for old in part.read_slots() {
        let old = old.expect("a golden copy reads");
        assert_eq!(old.tlv(TAG_IPL_SOURCE), None);
        assert_eq!(IplSource::of(&old), IplSource::default());
        assert_eq!(IplSource::of(&old).slot(), Slot::A);
        // Storing the value changes nothing else of the record.
        let mut new = old.clone();
        IplSource {
            next: Some(Slot::B),
            default: Some(Slot::A),
        }
        .store(&mut new)
        .unwrap();
        let back = Record::decode(&new.encode().unwrap()).unwrap();
        assert_eq!(back.header, old.header);
        assert_eq!(ConsoleMode::of(&back), ConsoleMode::of(&old));
        assert_eq!(IplSource::of(&back).next, Some(Slot::B));
        assert_eq!(IplSource::of(&back).default, Some(Slot::A));
    }
}

#[test]
fn the_value_survives_the_partition() {
    let dev = Cursor::new(vec![
        0u8;
        usize::try_from(RESUME_OFFSET).unwrap() + SLOT_BYTES
    ]);
    let (mut part, mut rec) = LicPart::init(dev, 1_700_000_000).unwrap();
    // Without the value: the encoding of a record that never had it.
    let without = rec.encode().unwrap();
    assert_eq!(
        IplSource::of(&Record::decode(&without).unwrap()),
        IplSource::default()
    );
    let src = IplSource {
        next: Some(Slot::B),
        default: Some(Slot::B),
    };
    src.store(&mut rec).unwrap();
    part.write(&mut rec).unwrap();
    let mut part = LicPart::open(part.into_device());
    let mut read = part.read().unwrap();
    assert_eq!(IplSource::of(&read), src);
    // The hypervisor's take: slot B once, then the default.
    assert_eq!(IplSource::take(&mut read).unwrap(), (Slot::B, true));
    part.write(&mut read).unwrap();
    let again = LicPart::open(part.into_device()).read().unwrap();
    assert_eq!(
        IplSource::of(&again),
        IplSource {
            next: None,
            default: Some(Slot::B)
        }
    );
}
