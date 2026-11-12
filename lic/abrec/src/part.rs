// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading and writing the two copies.

use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Write};

use crate::error::AbError;
use crate::format::slot::{Slot, newest, target};
use crate::format::{decode, encode};
use crate::spec::Spec;

/// A block device or anything standing in for one.
pub trait Device: Read + Write + Seek {
    /// Make earlier writes durable. Files call `sync_all`.
    fn sync(&mut self) -> io::Result<()> {
        self.flush()
    }
}

impl Device for File {
    fn sync(&mut self) -> io::Result<()> {
        self.sync_all()
    }
}

impl Device for Cursor<Vec<u8>> {}

impl<D: Device + ?Sized> Device for Box<D> {
    fn sync(&mut self) -> io::Result<()> {
        (**self).sync()
    }
}

impl<D: Device> Device for &mut D {
    fn sync(&mut self) -> io::Result<()> {
        (**self).sync()
    }
}

/// One copy as read: its sequence and the product's bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    /// Monotonic; the newest copy has the highest.
    pub sequence: u64,
    /// The product's bytes.
    pub payload: Vec<u8>,
}

/// The partition, open on a device.
pub struct AbPart<D> {
    spec: Spec,
    dev: D,
}

impl<D: Device> AbPart<D> {
    /// Wrap a device. No I/O happens until a read or a write.
    pub fn open(spec: Spec, dev: D) -> Self {
        Self { spec, dev }
    }

    /// The record's description.
    pub fn spec(&self) -> &Spec {
        &self.spec
    }

    /// The device, for what the product keeps beside the two copies.
    pub fn device(&mut self) -> &mut D {
        &mut self.dev
    }

    /// Give the device back.
    pub fn into_device(self) -> D {
        self.dev
    }

    fn offset(&self, slot: Slot) -> u64 {
        (slot.index() * self.spec.slot_bytes) as u64
    }

    /// One copy. `None` is an invalid or empty copy.
    pub fn read_slot(&mut self, slot: Slot) -> Option<Stored> {
        self.dev.seek(SeekFrom::Start(self.offset(slot))).ok()?;
        let mut buf = vec![0u8; self.spec.slot_bytes];
        let mut got = 0;
        while got < buf.len() {
            match self.dev.read(&mut buf[got..]) {
                Ok(0) => break,
                Ok(n) => got += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
        let frame = decode(&self.spec, &buf[..got]).ok()?;
        Some(Stored {
            sequence: frame.sequence,
            payload: frame.payload.to_vec(),
        })
    }

    /// Both copies.
    pub fn read_slots(&mut self) -> [Option<Stored>; 2] {
        [self.read_slot(Slot::A), self.read_slot(Slot::B)]
    }

    /// The newest valid copy.
    pub fn read(&mut self) -> Result<Stored, AbError> {
        let mut slots = self.read_slots();
        let seq = [0, 1].map(|i| slots[i].as_ref().map(|s| s.sequence));
        newest(seq)
            .and_then(|s| slots[s.index()].take())
            .ok_or(AbError::NoValidCopy)
    }

    /// Write one copy as given, then sync. A product that judges its
    /// payloads itself picks the slot and sequence with
    /// [`target`](crate::target) over the copies it accepts.
    pub fn write_slot(&mut self, slot: Slot, sequence: u64, payload: &[u8]) -> Result<(), AbError> {
        let bytes = encode(&self.spec, sequence, payload)?;
        self.dev.seek(SeekFrom::Start(self.offset(slot)))?;
        self.dev.write_all(&bytes)?;
        self.dev.sync()?;
        Ok(())
    }

    /// Write `payload` over the older (or invalid) copy with the next
    /// sequence number, then sync. Returns the copy written and its
    /// sequence. The newest good copy is never overwritten, so a torn
    /// write cannot lose it.
    pub fn write(&mut self, payload: &[u8]) -> Result<(Slot, u64), AbError> {
        let slots = self.read_slots();
        let (slot, sequence) = target(slots.map(|s| s.map(|s| s.sequence)));
        self.write_slot(slot, sequence, payload)?;
        Ok((slot, sequence))
    }

    /// First boot: both copies must be invalid. Writes `payload` to copy A.
    pub fn init(&mut self, payload: &[u8]) -> Result<(), AbError> {
        if self.read_slots().iter().any(Option::is_some) {
            return Err(AbError::AlreadyInitialized);
        }
        self.write(payload).map(|_| ())
    }
}
