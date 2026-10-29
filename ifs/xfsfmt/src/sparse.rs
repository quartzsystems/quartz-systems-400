// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An in-memory sparse device for tests: 64 KiB pages allocated on
//! first write, zeros elsewhere.

use std::collections::BTreeMap;
use std::io::{self, Read, Seek, SeekFrom, Write};

const PAGE_LEN: usize = 64 * 1024;
const PAGE: u64 = PAGE_LEN as u64;

/// A device of a fixed size whose unwritten pages read as zeros.
#[derive(Debug, Default, Clone)]
pub struct SparseDev {
    len: u64,
    pos: u64,
    pages: BTreeMap<u64, Vec<u8>>,
}

impl SparseDev {
    /// A zeroed device of `len` bytes.
    #[must_use]
    pub fn new(len: u64) -> Self {
        Self {
            len,
            pos: 0,
            pages: BTreeMap::new(),
        }
    }

    /// Device size.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Whether the device has no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Written pages as `(offset, bytes)`, in offset order.
    pub fn pages(&self) -> impl Iterator<Item = (u64, &[u8])> {
        self.pages.iter().map(|(n, p)| (n * PAGE, p.as_slice()))
    }

    /// Copy `len` bytes at `at` out.
    #[must_use]
    pub fn read_at(&self, at: u64, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        let mut done = 0;
        while done < len {
            let off = at + done as u64;
            let (page, in_page) = (off / PAGE, usize::try_from(off % PAGE).expect("in a page"));
            let n = (PAGE_LEN - in_page).min(len - done);
            if let Some(p) = self.pages.get(&page) {
                out[done..done + n].copy_from_slice(&p[in_page..in_page + n]);
            }
            done += n;
        }
        out
    }
}

impl Read for SparseDev {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = usize::try_from(self.len.saturating_sub(self.pos))
            .unwrap_or(usize::MAX)
            .min(buf.len());
        buf[..n].copy_from_slice(&self.read_at(self.pos, n));
        self.pos += n as u64;
        Ok(n)
    }
}

impl Write for SparseDev {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.pos + buf.len() as u64 > self.len {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "past the device end",
            ));
        }
        let mut done = 0;
        while done < buf.len() {
            let off = self.pos + done as u64;
            let (page, in_page) = (off / PAGE, usize::try_from(off % PAGE).expect("in a page"));
            let n = (PAGE_LEN - in_page).min(buf.len() - done);
            let p = self
                .pages
                .entry(page)
                .or_insert_with(|| vec![0u8; PAGE_LEN]);
            p[in_page..in_page + n].copy_from_slice(&buf[done..done + n]);
            done += n;
        }
        self.pos += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Seek for SparseDev {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let base = match to {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(d) => i128::from(self.len) + i128::from(d),
            SeekFrom::Current(d) => i128::from(self.pos) + i128::from(d),
        };
        self.pos = u64::try_from(base)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?;
        Ok(self.pos)
    }
}
