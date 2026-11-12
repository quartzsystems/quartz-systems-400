// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The resume copy that brings a half back in step with its partner,
//! chunk by chunk, and the map of the chunks done, which the caller
//! keeps somewhere durable so a crash continues where it was.

use blockdev::AlignedBuf;

use crate::PAGE_SIZE;
use crate::bytes::{get_u64, put_u64};

/// Pages per resume chunk: 1 MiB.
pub const RESUME_CHUNK: u64 = 256;
/// Chunks copied between saves of the map.
pub const SAVE_EVERY: u64 = 16;

/// Which chunks of a set have been copied onto a resuming half.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeState {
    /// The pool the set belongs to.
    pub asp_uuid: [u8; 16],
    /// The half being resumed.
    pub unit_uuid: [u8; 16],
    /// Pages per chunk.
    pub chunk_pages: u64,
    /// Chunks in the set.
    pub chunks: u64,
    /// Bit `i` set: chunk `i` is copied and durable.
    pub done: Vec<u8>,
    /// A resume asked for with the system up; otherwise the
    /// synchronization start mirrored protection leaves to the start
    /// of the system.
    pub background: bool,
}

impl ResumeState {
    /// Nothing copied yet.
    #[must_use]
    pub fn new(asp_uuid: [u8; 16], unit_uuid: [u8; 16], span_pages: u64) -> Self {
        let chunks = span_pages.div_ceil(RESUME_CHUNK);
        Self {
            asp_uuid,
            unit_uuid,
            chunk_pages: RESUME_CHUNK,
            chunks,
            done: vec![0; usize::try_from(chunks.div_ceil(8)).expect("fits")],
            background: false,
        }
    }

    /// Whether chunk `i` is copied.
    #[must_use]
    pub fn is_done(&self, i: u64) -> bool {
        self.done[usize::try_from(i / 8).expect("fits")] >> (i % 8) & 1 == 1
    }

    /// Mark chunk `i` copied.
    pub fn set_done(&mut self, i: u64) {
        self.done[usize::try_from(i / 8).expect("fits")] |= 1 << (i % 8);
    }

    /// Chunks copied.
    #[must_use]
    pub fn chunks_done(&self) -> u64 {
        (0..self.chunks).filter(|&i| self.is_done(i)).count() as u64
    }

    /// Whether every chunk is copied.
    #[must_use]
    pub fn complete(&self) -> bool {
        self.chunks_done() == self.chunks
    }

    /// Percent complete, 0 to 100.
    #[must_use]
    pub fn percent(&self) -> u8 {
        if self.chunks == 0 {
            return 100;
        }
        u8::try_from(self.chunks_done() * 100 / self.chunks).unwrap_or(100)
    }

    /// Bytes to keep: the caller's magic, UUIDs, sizes, bitmap, then one
    /// byte, 1 for a background resume (absent in older maps: 0).
    #[must_use]
    pub fn encode(&self, magic: &[u8; 8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(56 + self.done.len());
        out.extend_from_slice(magic);
        out.extend_from_slice(&self.asp_uuid);
        out.extend_from_slice(&self.unit_uuid);
        let mut nums = [0u8; 16];
        put_u64(&mut nums, 0, self.chunk_pages);
        put_u64(&mut nums, 8, self.chunks);
        out.extend_from_slice(&nums);
        out.extend_from_slice(&self.done);
        out.push(u8::from(self.background));
        out
    }

    /// From kept bytes under the caller's magic.
    #[must_use]
    pub fn decode(magic: &[u8; 8], bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 56 || &bytes[..8] != magic {
            return None;
        }
        let chunk_pages = get_u64(bytes, 40);
        let chunks = get_u64(bytes, 48);
        let need = usize::try_from(chunks.div_ceil(8)).ok()?;
        if chunk_pages != RESUME_CHUNK || bytes.len() < 56 + need {
            return None;
        }
        Some(Self {
            asp_uuid: bytes[8..24].try_into().ok()?,
            unit_uuid: bytes[24..40].try_into().ok()?,
            chunk_pages,
            chunks,
            done: bytes[56..56 + need].to_vec(),
            background: bytes.get(56 + need) == Some(&1),
        })
    }
}

/// How far a resume has come, for displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumeProgress {
    /// The half being resumed.
    pub unit: u16,
    /// Chunks copied.
    pub chunks_done: u64,
    /// Chunks in all.
    pub chunks: u64,
    /// Percent complete.
    pub percent: u8,
    /// The half is active again.
    pub complete: bool,
}

/// What the copy asks of its caller. Block numbers are the set's own,
/// from 0.
pub trait ResumeIo {
    /// The caller's error.
    type Error;
    /// `count` blocks at `lba` from the half in step.
    fn read(&mut self, lba: u64, count: u64) -> Result<AlignedBuf, Self::Error>;
    /// Whether a block must be copied: one the set holds nothing in
    /// (unallocated, or a member's own label) is skipped.
    fn wanted(&mut self, lba: u64) -> bool;
    /// Write whole blocks at `lba` on the resuming half.
    fn write(&mut self, lba: u64, bytes: &[u8]) -> Result<(), Self::Error>;
    /// Flush the resuming half.
    fn flush(&mut self) -> Result<(), Self::Error>;
    /// Make the map durable.
    fn save(&mut self, state: &ResumeState) -> Result<(), Self::Error>;
}

/// Copy up to `budget` chunks not yet done of a set of `blocks` blocks,
/// every wanted block in runs written whole. The map is saved every
/// [`SAVE_EVERY`] chunks, the half flushed first, and at the end of a
/// step that leaves chunks to do; `since_save` counts the chunks since.
/// True when the last chunk landed: the half is flushed, and making it
/// active and forgetting the map are the caller's.
pub fn copy_step<I: ResumeIo>(
    io: &mut I,
    state: &mut ResumeState,
    since_save: &mut u64,
    blocks: u64,
    budget: u64,
) -> Result<bool, I::Error> {
    let mut copied = 0;
    for chunk in 0..state.chunks {
        if copied >= budget {
            break;
        }
        if state.is_done(chunk) {
            continue;
        }
        let first = chunk * RESUME_CHUNK;
        let count = RESUME_CHUNK.min(blocks - first);
        let buf = io.read(first, count)?;
        let mut run_start: Option<u64> = None;
        for i in 0..=count {
            let wanted = i < count && io.wanted(first + i);
            match (run_start, wanted) {
                (None, true) => run_start = Some(i),
                (Some(s), false) => {
                    let bytes = &buf[usize::try_from(s).expect("fits") * PAGE_SIZE
                        ..usize::try_from(i).expect("fits") * PAGE_SIZE];
                    io.write(first + s, bytes)?;
                    run_start = None;
                }
                _ => {}
            }
        }
        state.set_done(chunk);
        *since_save += 1;
        copied += 1;
        if *since_save >= SAVE_EVERY {
            io.flush()?;
            io.save(state)?;
            *since_save = 0;
        }
    }
    if state.complete() {
        io.flush()?;
        return Ok(true);
    }
    if *since_save > 0 {
        io.flush()?;
        io.save(state)?;
        *since_save = 0;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGIC: &[u8; 8] = b"TESTSRSM";

    #[test]
    fn resume_state_round_trips() {
        let mut s = ResumeState::new([1; 16], [2; 16], 1000);
        assert_eq!(s.chunks, 4);
        assert_eq!(s.percent(), 0);
        s.set_done(1);
        s.set_done(3);
        assert_eq!(s.chunks_done(), 2);
        assert_eq!(s.percent(), 50);
        assert!(!s.complete());
        let back = ResumeState::decode(MAGIC, &s.encode(MAGIC)).unwrap();
        assert_eq!(back, s);
        assert!(ResumeState::decode(b"OTHERRSM", &s.encode(MAGIC)).is_none());
        s.background = true;
        assert!(
            ResumeState::decode(MAGIC, &s.encode(MAGIC))
                .unwrap()
                .background
        );
        let bytes = s.encode(MAGIC);
        assert!(
            !ResumeState::decode(MAGIC, &bytes[..bytes.len() - 1])
                .unwrap()
                .background,
            "an older map"
        );
        s.background = false;
        assert!(ResumeState::decode(MAGIC, b"nothing").is_none());
        s.set_done(0);
        s.set_done(2);
        assert!(s.complete());
        assert_eq!(s.percent(), 100);
    }

    /// A source of numbered blocks; block 5 and the last are not wanted.
    struct Io {
        blocks: u64,
        written: Vec<(u64, usize)>,
        flushes: u32,
        saves: u32,
    }

    impl ResumeIo for Io {
        type Error = ();
        fn read(&mut self, _lba: u64, count: u64) -> Result<AlignedBuf, ()> {
            Ok(AlignedBuf::new(usize::try_from(count).unwrap()))
        }
        fn wanted(&mut self, lba: u64) -> bool {
            lba != 5 && lba != self.blocks - 1
        }
        fn write(&mut self, lba: u64, bytes: &[u8]) -> Result<(), ()> {
            self.written.push((lba, bytes.len() / PAGE_SIZE));
            Ok(())
        }
        fn flush(&mut self) -> Result<(), ()> {
            self.flushes += 1;
            Ok(())
        }
        fn save(&mut self, _state: &ResumeState) -> Result<(), ()> {
            self.saves += 1;
            Ok(())
        }
    }

    #[test]
    fn the_copy_writes_wanted_runs_whole_and_saves_on_the_way() {
        let blocks = 2 * RESUME_CHUNK + 10;
        let mut io = Io {
            blocks,
            written: Vec::new(),
            flushes: 0,
            saves: 0,
        };
        let mut state = ResumeState::new([1; 16], [2; 16], blocks);
        let mut since = 0;
        assert!(!copy_step(&mut io, &mut state, &mut since, blocks, 1).unwrap());
        assert_eq!(io.written, [(0, 5), (6, 250)]);
        assert_eq!((io.flushes, io.saves, since), (1, 1, 0));
        assert!(copy_step(&mut io, &mut state, &mut since, blocks, 10).unwrap());
        assert_eq!(io.written[2..], [(256, 256), (512, 9)]);
        // Complete: flushed, and the map is the caller's to forget.
        assert_eq!((io.flushes, io.saves), (2, 1));
        assert!(state.complete());
    }
}
