// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every `unsafe` block of the crate: an `io_uring` of 128-byte entries
//! (its rings mapped from the kernel), the map of a queue's request
//! descriptors, and xfs's grow ioctl.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::ptr::{NonNull, null_mut};
use std::sync::atomic::{AtomicU32, Ordering};

use rustix::io_uring::{
    IORING_OFF_CQ_RING, IORING_OFF_SQ_RING, IORING_OFF_SQES, IoringEnterFlags, IoringFeatureFlags,
    IoringSetupFlags, io_uring_enter, io_uring_params, io_uring_setup,
};
use rustix::mm::{MapFlags, ProtFlags, mmap, munmap};

use crate::abi::{CQE_LEN, IO_DESC_LEN, SQE_LEN};

/// One shared mapping, unmapped on drop.
struct Map {
    ptr: NonNull<u8>,
    len: usize,
}

impl Map {
    fn new(fd: impl AsFd, len: usize, prot: ProtFlags, offset: u64) -> io::Result<Self> {
        // SAFETY: a fresh shared mapping of the kernel's object at no
        // fixed address; nothing else aliases it until it is returned.
        let p = unsafe {
            mmap(
                null_mut(),
                len,
                prot,
                MapFlags::SHARED | MapFlags::POPULATE,
                fd,
                offset,
            )?
        };
        let ptr =
            NonNull::new(p.cast::<u8>()).ok_or_else(|| io::Error::other("mmap returned null"))?;
        Ok(Self { ptr, len })
    }

    /// The `u32` the kernel shares at `off`.
    fn atomic(&self, off: u32) -> &AtomicU32 {
        let off = off as usize;
        assert!(
            off + 4 <= self.len && off.is_multiple_of(4),
            "ring offset outside its map"
        );
        // SAFETY: in bounds and aligned (checked: the map is page aligned
        // and `off` a multiple of 4); the kernel updates the word
        // concurrently, so it is only reached as an atomic.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            &*self.ptr.as_ptr().add(off).cast::<AtomicU32>()
        }
    }

    fn read_u32(&self, off: u32) -> u32 {
        self.atomic(off).load(Ordering::Relaxed)
    }
}

impl Drop for Map {
    fn drop(&mut self) {
        // SAFETY: the mapping made in `new`, unmapped once; no reference
        // into it outlives `self`.
        let _ = unsafe { munmap(self.ptr.as_ptr().cast(), self.len) };
    }
}

// SAFETY: the maps are plain shared memory; the ring that owns them is
// used by one thread at a time (it is moved, never shared).
unsafe impl Send for Map {}

/// An `io_uring` of 128-byte submission entries.
pub struct Ring {
    fd: OwnedFd,
    sq: Map,
    /// The completion ring's own map, unless the kernel shares one.
    cq: Option<Map>,
    sqes: Map,
    sq_head: u32,
    sq_tail: u32,
    sq_mask: u32,
    sq_entries: u32,
    sq_array: u32,
    cq_head: u32,
    cq_tail: u32,
    cq_mask: u32,
    cqes: u32,
    pending: u32,
}

impl Ring {
    /// A ring of `entries` submissions.
    pub fn new(entries: u32) -> io::Result<Self> {
        let mut p = io_uring_params::default();
        p.flags = IoringSetupFlags::SQE128;
        // SAFETY: no `ATTACH_WQ`, so `wq_fd` is not read.
        let fd = unsafe { io_uring_setup(entries, &mut p)? };
        let sq_len = p.sq_off.array as usize + p.sq_entries as usize * 4;
        let cq_len = p.cq_off.cqes as usize + p.cq_entries as usize * CQE_LEN;
        let rw = ProtFlags::READ | ProtFlags::WRITE;
        let single = p.features.contains(IoringFeatureFlags::SINGLE_MMAP);
        let sq = Map::new(
            &fd,
            if single { sq_len.max(cq_len) } else { sq_len },
            rw,
            IORING_OFF_SQ_RING,
        )?;
        let cq = if single {
            None
        } else {
            Some(Map::new(&fd, cq_len, rw, IORING_OFF_CQ_RING)?)
        };
        let sqes = Map::new(&fd, p.sq_entries as usize * SQE_LEN, rw, IORING_OFF_SQES)?;
        let cq_map = cq.as_ref().unwrap_or(&sq);
        let sq_mask = sq.read_u32(p.sq_off.ring_mask);
        let cq_mask = cq_map.read_u32(p.cq_off.ring_mask);
        Ok(Self {
            fd,
            sq_head: p.sq_off.head,
            sq_tail: p.sq_off.tail,
            sq_mask,
            sq_entries: p.sq_entries,
            sq_array: p.sq_off.array,
            cq_head: p.cq_off.head,
            cq_tail: p.cq_off.tail,
            cq_mask,
            cqes: p.cq_off.cqes,
            sq,
            cq,
            sqes,
            pending: 0,
        })
    }

    fn cq_map(&self) -> &Map {
        self.cq.as_ref().unwrap_or(&self.sq)
    }

    /// Queue one entry; `false` when the ring is full.
    pub fn push(&mut self, sqe: &[u8; SQE_LEN]) -> bool {
        let head = self.sq.atomic(self.sq_head).load(Ordering::Acquire);
        let tail = self.sq.atomic(self.sq_tail).load(Ordering::Relaxed);
        if tail.wrapping_sub(head) >= self.sq_entries {
            return false;
        }
        let idx = tail & self.sq_mask;
        let at = idx as usize * SQE_LEN;
        assert!(at + SQE_LEN <= self.sqes.len, "entry outside its map");
        // SAFETY: entry `idx` is within the entries map (checked) and the
        // kernel does not read it until the tail below passes it.
        unsafe {
            std::ptr::copy_nonoverlapping(sqe.as_ptr(), self.sqes.ptr.as_ptr().add(at), SQE_LEN);
        }
        self.sq
            .atomic(self.sq_array + idx * 4)
            .store(idx, Ordering::Relaxed);
        self.sq
            .atomic(self.sq_tail)
            .store(tail.wrapping_add(1), Ordering::Release);
        self.pending += 1;
        true
    }

    /// Submit what was queued and wait for at least `want` completions.
    pub fn submit_and_wait(&mut self, want: u32) -> io::Result<()> {
        loop {
            // SAFETY: every queued entry points only at memory its owner
            // keeps alive until the entry completes (the queue's buffers
            // and the control commands' arguments, held across the wait).
            match unsafe {
                io_uring_enter(&self.fd, self.pending, want, IoringEnterFlags::GETEVENTS)
            } {
                Ok(n) => {
                    self.pending -= n.min(self.pending);
                    return Ok(());
                }
                Err(rustix::io::Errno::INTR) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// The next completion, if any.
    pub fn pop(&mut self) -> Option<[u8; CQE_LEN]> {
        let cq = self.cq_map();
        let head = cq.atomic(self.cq_head).load(Ordering::Relaxed);
        let tail = cq.atomic(self.cq_tail).load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let at = self.cqes as usize + (head & self.cq_mask) as usize * CQE_LEN;
        assert!(at + CQE_LEN <= cq.len, "completion outside its map");
        let mut out = [0u8; CQE_LEN];
        // SAFETY: the entry is in bounds (checked) and the kernel does not
        // reuse it until the head below passes it.
        unsafe {
            std::ptr::copy_nonoverlapping(cq.ptr.as_ptr().add(at), out.as_mut_ptr(), CQE_LEN);
        }
        cq.atomic(self.cq_head)
            .store(head.wrapping_add(1), Ordering::Release);
        Some(out)
    }
}

/// A queue's request descriptors, mapped read-only from `/dev/ublkcN`.
pub struct Descs {
    map: Map,
    depth: usize,
}

impl Descs {
    /// The descriptors of queue 0 of `depth` tags.
    pub fn new(fd: impl AsFd, depth: usize) -> io::Result<Self> {
        let len = (depth * IO_DESC_LEN).next_multiple_of(4096);
        Ok(Self {
            map: Map::new(fd, len, ProtFlags::READ, 0)?,
            depth,
        })
    }

    /// Tag `tag`'s descriptor.
    pub fn get(&self, tag: usize) -> [u8; IO_DESC_LEN] {
        assert!(tag < self.depth, "no such tag");
        let mut out = [0u8; IO_DESC_LEN];
        // SAFETY: in bounds (tag < depth, the map covers depth entries);
        // the kernel wrote it before completing the tag's fetch.
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.map.ptr.as_ptr().add(tag * IO_DESC_LEN),
                out.as_mut_ptr(),
                IO_DESC_LEN,
            );
        }
        out
    }
}

/// Grow the xfs that `fd` (a file on it) belongs to: `XFS_IOC_FSGROWFSDATA`.
pub fn xfs_growfs(fd: impl AsFd, newblocks: u64, imaxpct: u32) -> io::Result<()> {
    use rustix::ioctl::{Setter, ioctl, opcode};
    let data = crate::abi::growfs_data(newblocks, imaxpct);
    // SAFETY: the opcode is `_IOW('X', 110, struct xfs_growfs_data)` and
    // `data` is that structure's 16 bytes.
    unsafe {
        let set = Setter::<{ opcode::write::<[u8; 16]>(b'X', 110) }, [u8; 16]>::new(data);
        ioctl(fd, set)?;
    }
    Ok(())
}
