// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The resynchronization copy with a cursor: [`copy_step`]'s chunks,
//! writes, flushes and saves, in the same order, without looking at
//! every chunk of the set on each step. A step of one chunk over a set
//! of millions of chunks (a few terabytes) costs the chunk, not the set.
//!
//! [`copy_step`]: crate::mirror::copy_step

use crate::PAGE_SIZE;
use crate::mirror::{RESUME_CHUNK, ResumeIo, ResumeState, SAVE_EVERY};

/// Where a copy has got to: no chunk before `next` is left to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    next: u64,
    left: u64,
}

/// Chunks of `state` marked done.
#[must_use]
pub fn done_count(state: &ResumeState) -> u64 {
    let full = usize::try_from(state.chunks / 8).expect("fits");
    let mut n: u64 = state.done[..full]
        .iter()
        .map(|b| u64::from(b.count_ones()))
        .sum();
    let rest = state.chunks % 8;
    if rest != 0 {
        n += u64::from((state.done[full] & ((1u8 << rest) - 1)).count_ones());
    }
    n
}

/// Mark chunk `i` not done.
pub fn undo(state: &mut ResumeState, i: u64) {
    state.done[usize::try_from(i / 8).expect("fits")] &= !(1 << (i % 8));
}

impl Cursor {
    /// At the start of `state`.
    #[must_use]
    pub fn new(state: &ResumeState) -> Self {
        Self {
            next: 0,
            left: state.chunks - done_count(state),
        }
    }

    /// Chunks left to copy.
    #[must_use]
    pub fn left(&self) -> u64 {
        self.left
    }

    /// No chunk before this one is left.
    #[must_use]
    pub fn next(&self) -> u64 {
        self.next
    }

    /// The first chunk from `next` not done, passing done bytes whole.
    fn find(&mut self, state: &ResumeState) -> Option<u64> {
        let mut i = self.next;
        while i < state.chunks {
            if i.is_multiple_of(8) && state.done[usize::try_from(i / 8).expect("fits")] == 0xFF {
                i += 8;
                continue;
            }
            if !state.is_done(i) {
                self.next = i;
                return Some(i);
            }
            i += 1;
        }
        self.next = state.chunks;
        None
    }
}

/// Copy up to `budget` chunks not yet done, as
/// [`copy_step`](crate::mirror::copy_step) does. True when the last
/// chunk landed: the target is flushed, and what follows is the
/// caller's.
pub fn copy_next<I: ResumeIo>(
    io: &mut I,
    state: &mut ResumeState,
    cursor: &mut Cursor,
    since_save: &mut u64,
    blocks: u64,
    budget: u64,
) -> Result<bool, I::Error> {
    let mut copied = 0;
    while copied < budget {
        let Some(chunk) = cursor.find(state) else {
            break;
        };
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
        cursor.left -= 1;
        cursor.next = chunk + 1;
        *since_save += 1;
        copied += 1;
        if *since_save >= SAVE_EVERY {
            io.flush()?;
            io.save(state)?;
            *since_save = 0;
        }
    }
    if cursor.left == 0 {
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
    use blockdev::AlignedBuf;

    use super::*;
    use crate::mirror::copy_step;

    /// Records every call.
    #[derive(Default)]
    struct Log(Vec<String>);

    impl ResumeIo for Log {
        type Error = ();
        fn read(&mut self, lba: u64, count: u64) -> Result<AlignedBuf, ()> {
            self.0.push(format!("r{lba}+{count}"));
            Ok(AlignedBuf::new(usize::try_from(count).unwrap()))
        }
        fn wanted(&mut self, lba: u64) -> bool {
            lba % 300 != 7
        }
        fn write(&mut self, lba: u64, bytes: &[u8]) -> Result<(), ()> {
            self.0.push(format!("w{lba}+{}", bytes.len() / PAGE_SIZE));
            Ok(())
        }
        fn flush(&mut self) -> Result<(), ()> {
            self.0.push("f".into());
            Ok(())
        }
        fn save(&mut self, s: &ResumeState) -> Result<(), ()> {
            self.0.push(format!("s{}", done_count(s)));
            Ok(())
        }
    }

    #[test]
    fn the_cursor_copies_what_copy_step_copies_in_the_same_order() {
        let blocks = 40 * RESUME_CHUNK + 17;
        for budget in [1, 3, 16, 100] {
            let mut a = ResumeState::new([1; 16], [2; 16], blocks);
            for i in [0, 1, 2, 5, 9, 10, 11, 12, 13, 14, 15, 16, 30] {
                a.set_done(i);
            }
            let mut b = a.clone();
            let (mut la, mut lb) = (Log::default(), Log::default());
            let (mut sa, mut sb) = (0, 0);
            let mut cur = Cursor::new(&b);
            assert_eq!(cur.left(), 41 - 13);
            loop {
                let da = copy_step(&mut la, &mut a, &mut sa, blocks, budget).unwrap();
                let db = copy_next(&mut lb, &mut b, &mut cur, &mut sb, blocks, budget).unwrap();
                assert_eq!(da, db);
                assert_eq!(la.0, lb.0, "budget {budget}");
                if da {
                    break;
                }
            }
            assert_eq!(a, b);
            assert_eq!(cur.left(), 0);
        }
    }

    #[test]
    fn done_counts_only_chunks() {
        let mut s = ResumeState::new([1; 16], [2; 16], 11 * RESUME_CHUNK);
        s.done[1] = 0xFF;
        assert_eq!(done_count(&s), 3);
        undo(&mut s, 9);
        assert_eq!(done_count(&s), 2);
    }
}
