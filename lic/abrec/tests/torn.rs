// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A/B copy behaviour on an in-memory device, torn writes among it, for
//! a product that is not QS/400.

use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

use abrec::{AbError, AbPart, Device, Slot, Spec};

const SPEC: Spec = Spec {
    magic: *b"DEMOABRC",
    version: 1,
    type_guid: uuid::uuid!("5d0f6f0e-3a57-4c1b-8a55-515344454d01"),
    slot_bytes: 4096,
    min_payload: 0,
};

/// An in-memory device that can stop a write part way through.
struct Torn {
    inner: Cursor<Vec<u8>>,
    fail_after: Option<usize>,
}

impl Torn {
    fn new() -> Self {
        Self {
            inner: Cursor::new(vec![0u8; 2 * SPEC.slot_bytes]),
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
        match self.fail_after.take() {
            Some(n) if n < buf.len() => {
                self.inner.write_all(&buf[..n])?;
                Err(io::Error::other("power lost mid-write"))
            }
            _ => self.inner.write(buf),
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

#[test]
fn a_blank_device_initializes_once() {
    let mut part = AbPart::open(SPEC, Torn::new());
    assert!(matches!(part.read(), Err(AbError::NoValidCopy)));
    part.init(b"first").unwrap();
    let got = part.read().unwrap();
    assert_eq!((got.sequence, got.payload.as_slice()), (1, &b"first"[..]));
    assert!(matches!(
        part.init(b"again"),
        Err(AbError::AlreadyInitialized)
    ));
    assert_eq!(part.spec().magic, *b"DEMOABRC");
    assert_eq!(&part.device().inner.get_ref()[..8], b"DEMOABRC");
}

#[test]
fn writes_alternate_and_the_newest_is_read() {
    let mut part = AbPart::open(SPEC, Torn::new());
    assert_eq!(part.write(b"one").unwrap(), (Slot::A, 1));
    assert_eq!(part.write(b"two").unwrap(), (Slot::B, 2));
    assert_eq!(part.write(b"three").unwrap(), (Slot::A, 3));
    assert_eq!(part.read().unwrap().payload, b"three");
    let [a, b] = part.read_slots();
    assert_eq!(a.unwrap().sequence, 3);
    assert_eq!(b.unwrap().payload, b"two");
    // Copy B starts one slot in.
    let dev = part.into_device().inner.into_inner();
    assert_eq!(&dev[SPEC.slot_bytes..SPEC.slot_bytes + 8], b"DEMOABRC");
}

#[test]
fn a_torn_write_keeps_the_copy_before() {
    let mut part = AbPart::open(SPEC, Torn::new());
    part.write(b"one").unwrap();
    part.write(b"two").unwrap();
    // The next write is over A, the older copy. Lose power 20 bytes in.
    part.device().fail_after = Some(20);
    assert!(part.write(b"three").is_err());
    let got = part.read().unwrap();
    assert_eq!((got.sequence, got.payload.as_slice()), (2, &b"two"[..]));
    // The torn slot is written again.
    assert_eq!(part.write(b"three").unwrap(), (Slot::A, 3));
    assert_eq!(part.read().unwrap().payload, b"three");
}

#[test]
fn a_damaged_newest_copy_falls_back() {
    let mut part = AbPart::open(SPEC, Torn::new());
    part.write(b"one").unwrap();
    part.write(b"two").unwrap();
    part.device().inner.get_mut()[SPEC.slot_bytes + 29] ^= 0xFF;
    assert_eq!(part.read().unwrap().payload, b"one");
    assert_eq!(part.write(b"three").unwrap(), (Slot::B, 2));
}

#[test]
fn another_product_s_record_is_no_record() {
    let mut part = AbPart::open(SPEC, Torn::new());
    part.write(b"ours").unwrap();
    let other = Spec {
        magic: *b"QS400LIC",
        ..SPEC
    };
    let mut theirs = AbPart::open(other, part.into_device());
    assert!(matches!(theirs.read(), Err(AbError::NoValidCopy)));
    let newer = Spec { version: 2, ..SPEC };
    let mut newer = AbPart::open(newer, theirs.into_device());
    assert!(newer.read_slot(Slot::A).is_none());
}

#[test]
fn an_oversized_payload_is_refused() {
    let mut part = AbPart::open(SPEC, Torn::new());
    part.write(b"one").unwrap();
    assert!(matches!(
        part.write(&vec![0u8; SPEC.slot_bytes]),
        Err(AbError::TooLarge { .. })
    ));
    assert_eq!(part.read().unwrap().sequence, 1);
}
