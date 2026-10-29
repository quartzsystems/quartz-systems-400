// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One partition of a device seen as a device of its own.

use std::io::{self, Read, Seek, SeekFrom, Write};

/// `len` bytes of `dev` from `base`, addressed from zero.
pub struct Window<'a, D> {
    dev: &'a mut D,
    base: u64,
    len: u64,
    pos: u64,
}

impl<'a, D: Seek> Window<'a, D> {
    /// The window `[base, base + len)` of `dev`.
    pub fn new(dev: &'a mut D, base: u64, len: u64) -> Self {
        Self {
            dev,
            base,
            len,
            pos: 0,
        }
    }

    fn room(&self, want: usize) -> usize {
        usize::try_from(self.len.saturating_sub(self.pos))
            .unwrap_or(usize::MAX)
            .min(want)
    }
}

impl<D: Read + Seek> Read for Window<'_, D> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.room(buf.len());
        self.dev.seek(SeekFrom::Start(self.base + self.pos))?;
        let got = self.dev.read(&mut buf[..n])?;
        self.pos += got as u64;
        Ok(got)
    }
}

impl<D: Write + Seek> Write for Window<'_, D> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.room(buf.len());
        if n == 0 && !buf.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "past the partition end",
            ));
        }
        self.dev.seek(SeekFrom::Start(self.base + self.pos))?;
        let put = self.dev.write(&buf[..n])?;
        self.pos += put as u64;
        Ok(put)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.dev.flush()
    }
}

impl<D> Seek for Window<'_, D> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let at = match to {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(d) => i128::from(self.len) + i128::from(d),
            SeekFrom::Current(d) => i128::from(self.pos) + i128::from(d),
        };
        self.pos = u64::try_from(at)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "seek before start"))?;
        Ok(self.pos)
    }
}
