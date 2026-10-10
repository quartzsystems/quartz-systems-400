// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Linux backend: a partition, a whole disk or an image file opened
//! with `O_DIRECT`, so every transfer goes straight to the device and
//! `flush` (`fdatasync`) is a real barrier.

#![allow(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use rustix::fs::OFlags;
use rustix::io::Errno;
use rustix::ioctl::{Opcode, Setter, ioctl, opcode};

use crate::{AlignedBuf, BLOCK_SIZE, BlockDev, BlockError, Result, check_range};

/// `_IO(0x12, 119)`: `BLKDISCARD`, with a `[start, length]` byte range.
const BLKDISCARD: Opcode = opcode::none(0x12, 119);

/// A block device or image file on Linux.
pub struct LinuxBlockDev {
    path: PathBuf,
    file: File,
    blocks: u64,
    /// Whether `O_DIRECT` is in effect. An image on a file system that
    /// refuses it (tmpfs on older kernels) falls back to buffered I/O.
    direct: bool,
}

impl LinuxBlockDev {
    /// Open `path` read-write with `O_DIRECT`. The size must be a positive
    /// multiple of the block size.
    pub fn open(path: &Path) -> Result<Self> {
        let open = |direct: bool| {
            let mut o = OpenOptions::new();
            o.read(true).write(true);
            let mut flags = OFlags::CLOEXEC;
            if direct {
                flags |= OFlags::DIRECT;
            }
            o.custom_flags(flags.bits().cast_signed());
            o.open(path)
        };
        let (file, direct) = match open(true) {
            Ok(f) => (f, true),
            Err(e) if e.raw_os_error() == Some(Errno::INVAL.raw_os_error()) => {
                (open(false)?, false)
            }
            Err(e) => {
                return Err(BlockError::Open {
                    path: path.into(),
                    source: e,
                });
            }
        };
        let mut file = file;
        // Works for block devices too, where `st_size` is zero.
        let bytes = file
            .seek(SeekFrom::End(0))
            .map_err(|source| BlockError::Open {
                path: path.into(),
                source,
            })?;
        if bytes == 0 || bytes % u64::from(BLOCK_SIZE) != 0 {
            return Err(BlockError::BadSize { bytes });
        }
        Ok(Self {
            path: path.into(),
            file,
            blocks: bytes / u64::from(BLOCK_SIZE),
            direct,
        })
    }

    /// The path this device was opened from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether `O_DIRECT` is in effect.
    #[must_use]
    pub fn is_direct(&self) -> bool {
        self.direct
    }

    fn pread(&self, lba: u64, buf: &mut [u8]) -> Result<()> {
        let mut done = 0;
        let off = lba * u64::from(BLOCK_SIZE);
        while done < buf.len() {
            let n = rustix::io::pread(&self.file, &mut buf[done..], off + done as u64).map_err(
                |e| BlockError::Io {
                    op: "read",
                    lba,
                    source: e.into(),
                },
            )?;
            if n == 0 {
                return Err(BlockError::Short {
                    op: "read",
                    lba,
                    done,
                    len: buf.len(),
                });
            }
            done += n;
        }
        Ok(())
    }

    fn pwrite(&self, lba: u64, buf: &[u8]) -> Result<()> {
        let mut done = 0;
        let off = lba * u64::from(BLOCK_SIZE);
        while done < buf.len() {
            let n =
                rustix::io::pwrite(&self.file, &buf[done..], off + done as u64).map_err(|e| {
                    BlockError::Io {
                        op: "write",
                        lba,
                        source: e.into(),
                    }
                })?;
            if n == 0 {
                return Err(BlockError::Short {
                    op: "write",
                    lba,
                    done,
                    len: buf.len(),
                });
            }
            done += n;
        }
        Ok(())
    }
}

impl From<BlockError> for io::Error {
    fn from(e: BlockError) -> Self {
        io::Error::other(e)
    }
}

impl From<io::Error> for BlockError {
    fn from(source: io::Error) -> Self {
        BlockError::Io {
            op: "open",
            lba: 0,
            source,
        }
    }
}

impl BlockDev for LinuxBlockDev {
    fn block_size(&self) -> u32 {
        BLOCK_SIZE
    }

    fn blocks(&self) -> u64 {
        self.blocks
    }

    fn read(&self, lba: u64, buf: &mut [u8]) -> Result<()> {
        check_range(self.blocks, lba, buf.len())?;
        if !self.direct || AlignedBuf::is_aligned(buf.as_ptr()) {
            return self.pread(lba, buf);
        }
        // Bounce through an aligned buffer; O_DIRECT rejects the caller's.
        let mut tmp = AlignedBuf::new(buf.len() / BLOCK_SIZE as usize);
        self.pread(lba, &mut tmp)?;
        buf.copy_from_slice(&tmp);
        Ok(())
    }

    fn write(&self, lba: u64, buf: &[u8]) -> Result<()> {
        check_range(self.blocks, lba, buf.len())?;
        if !self.direct || AlignedBuf::is_aligned(buf.as_ptr()) {
            return self.pwrite(lba, buf);
        }
        let tmp = AlignedBuf::from_bytes(buf);
        self.pwrite(lba, &tmp)
    }

    fn flush(&self) -> Result<()> {
        rustix::fs::fdatasync(&self.file).map_err(|e| BlockError::Io {
            op: "flush",
            lba: 0,
            source: e.into(),
        })
    }

    fn discard(&self, lba: u64, count: u64) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        let len = usize::try_from(count * u64::from(BLOCK_SIZE)).expect("count fits");
        check_range(self.blocks, lba, len)?;
        let range = [lba * u64::from(BLOCK_SIZE), len as u64];
        // SAFETY: BLKDISCARD reads exactly one `u64[2]` byte range from
        // user memory and writes nothing back.
        match unsafe { ioctl(&self.file, Setter::<BLKDISCARD, [u64; 2]>::new(range)) } {
            // A regular image file is not a block device: nothing to do.
            Ok(()) | Err(Errno::NOTTY | Errno::OPNOTSUPP) => Ok(()),
            Err(e) => Err(BlockError::Io {
                op: "discard",
                lba,
                source: e.into(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_file_round_trip() {
        let dir = std::env::temp_dir().join(format!("qs400-blockdev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("img");
        std::fs::write(&path, vec![0u8; 16 * 4096]).unwrap();
        let d = LinuxBlockDev::open(&path).unwrap();
        assert_eq!(d.blocks(), 16);
        let mut buf = AlignedBuf::new(2);
        buf.fill(0x5A);
        d.write(3, &buf).unwrap();
        d.flush().unwrap();
        let mut back = AlignedBuf::new(2);
        d.read(3, &mut back).unwrap();
        assert_eq!(&*back, &*buf);
        // Unaligned caller buffers bounce.
        let mut v = vec![0u8; 4096 + 1];
        d.read(4, &mut v[1..]).unwrap();
        assert!(v[1..].iter().all(|&b| b == 0x5A));
        d.write(5, &v[1..]).unwrap();
        d.discard(5, 1).unwrap();
        assert!(matches!(
            d.read(16, &mut back),
            Err(BlockError::OutOfRange { .. })
        ));
        drop(d);
        std::fs::write(&path, vec![0u8; 100]).unwrap();
        assert!(matches!(
            LinuxBlockDev::open(&path),
            Err(BlockError::BadSize { bytes: 100 })
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
