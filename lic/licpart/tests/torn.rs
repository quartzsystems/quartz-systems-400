// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A/B copy behaviour on an in-memory device, including torn writes.

use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

use licpart::{
    Device, IplHistory, IplMode, IplType, LicError, LicPart, Outcome, Record, SLOT_BYTES, Slot,
};

/// An in-memory device that can stop a write part way through.
struct Torn {
    inner: Cursor<Vec<u8>>,
    fail_after: Option<usize>,
}

impl Torn {
    fn new() -> Self {
        Self {
            inner: Cursor::new(vec![0u8; 2 * SLOT_BYTES]),
            fail_after: None,
        }
    }
}

impl Read for Torn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl Write for Torn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.fail_after {
            Some(n) if n < buf.len() => {
                self.inner.write_all(&buf[..n])?;
                self.fail_after = Some(0);
                Err(io::Error::other("power lost mid-write"))
            }
            Some(n) => {
                self.fail_after = Some(n - buf.len());
                self.inner.write(buf)
            }
            None => self.inner.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl Seek for Torn {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl Device for Torn {}

fn history(start: u64) -> IplHistory {
    IplHistory {
        start_time: start,
        end_time: start + 1,
        ipl_type: IplType::A,
        mode: IplMode::Normal,
        slot: Slot::A,
        final_src: *b"C6001090",
        outcome: Outcome::Normal,
    }
}

#[test]
fn blank_device_initializes_once() {
    let (mut part, rec) = LicPart::init(Torn::new(), 42).unwrap();
    assert_eq!(rec.sequence, 1);
    assert_eq!(rec.header.install_time, 42);
    assert_eq!(part.read().unwrap(), rec);

    let dev = part.into_device();
    assert!(matches!(
        LicPart::init(dev, 99),
        Err(LicError::AlreadyInitialized)
    ));
}

#[test]
fn writes_alternate_and_newest_wins() {
    let (mut part, mut rec) = LicPart::init(Torn::new(), 1).unwrap();
    rec.header.ipl_count = 1;
    assert_eq!(part.write(&mut rec).unwrap(), Slot::B);
    assert_eq!(rec.sequence, 2);
    rec.header.ipl_count = 2;
    assert_eq!(part.write(&mut rec).unwrap(), Slot::A);
    assert_eq!(rec.sequence, 3);
    let back = part.read().unwrap();
    assert_eq!(back.sequence, 3);
    assert_eq!(back.header.ipl_count, 2);
    let slots = part.read_slots();
    assert_eq!(slots[0].as_ref().unwrap().sequence, 3);
    assert_eq!(slots[1].as_ref().unwrap().sequence, 2);
}

#[test]
fn torn_write_keeps_previous_good_record() {
    let (mut part, mut rec) = LicPart::init(Torn::new(), 1).unwrap();
    rec.header.ipl_count = 1;
    rec.push_history(history(10));
    part.write(&mut rec).unwrap(); // seq 2 in B
    let good = part.read().unwrap();

    // Next write targets A (older). Lose power 100 bytes in.
    let mut dev = part.into_device();
    dev.fail_after = Some(100);
    let mut part = LicPart::open(dev);
    rec.header.ipl_count = 2;
    rec.push_history(history(20));
    assert!(part.write(&mut rec).is_err());

    // The torn copy fails its CRC; the previous record is still served.
    let back = part.read().unwrap();
    assert_eq!(back, good);
    assert_eq!(back.header.ipl_count, 1);
    assert_eq!(back.history.len(), 1);

    // And the next successful write goes to the torn slot again.
    let mut dev = part.into_device();
    dev.fail_after = None;
    let mut part = LicPart::open(dev);
    assert_eq!(part.write(&mut rec).unwrap(), Slot::A);
    assert_eq!(part.read().unwrap().header.ipl_count, 2);
}

#[test]
fn corrupted_newest_copy_falls_back_to_older() {
    let (mut part, mut rec) = LicPart::init(Torn::new(), 1).unwrap();
    rec.header.ipl_count = 5;
    part.write(&mut rec).unwrap(); // seq 2 in B
    let mut dev = part.into_device();
    dev.inner.get_mut()[SLOT_BYTES + 40] ^= 0xFF;
    let mut part = LicPart::open(dev);
    let back = part.read().unwrap();
    assert_eq!(back.sequence, 1);
    assert_eq!(back.header.ipl_count, 0);
}

#[test]
fn both_copies_bad_is_an_error() {
    let mut part = LicPart::open(Torn::new());
    assert!(matches!(part.read(), Err(LicError::NoValidCopy)));
}

#[test]
fn tlvs_survive_the_round_trip() {
    let (mut part, mut rec) = LicPart::init(Torn::new(), 1).unwrap();
    rec.set_tlv(0x0100, b"QSECURITY=40".to_vec()).unwrap();
    rec.set_tlv(0x0101, vec![1, 2, 3]).unwrap();
    part.write(&mut rec).unwrap();
    let back = part.read().unwrap();
    assert_eq!(back.tlv(0x0100), Some(&b"QSECURITY=40"[..]));
    assert_eq!(back.tlv(0x0101), Some(&[1u8, 2, 3][..]));
    assert_eq!(back.tlv(0x0102), None);
}

#[test]
fn oversized_record_is_refused() {
    let (mut part, mut rec) = LicPart::init(Torn::new(), 1).unwrap();
    rec.set_tlv(0x0100, vec![0u8; SLOT_BYTES]).unwrap();
    assert!(matches!(
        part.write(&mut rec),
        Err(LicError::TooLarge { .. })
    ));
    assert_eq!(part.read().unwrap().sequence, 1);
    let _ = Record::new(0);
}
