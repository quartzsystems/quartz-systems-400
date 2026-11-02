// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the device is: whole 4096-byte blocks read, written and flushed,
//! and how one request becomes calls on it.

use crate::abi::{IO_F_FUA, IoDesc, OP_FLUSH, OP_READ, OP_WRITE};

/// Bytes of a block.
pub const BLOCK: usize = 4096;
/// Sectors of a block.
const SECTORS_PER_BLOCK: u64 = (BLOCK / 512) as u64;

/// `EIO`.
pub const EIO: i32 = 5;
/// `EINVAL`.
pub const EINVAL: i32 = 22;
/// `EOPNOTSUPP`.
pub const EOPNOTSUPP: i32 = 95;

/// The device behind the server. Errors are `errno` values.
pub trait Target: Send + Sync {
    /// Blocks of the device now.
    fn blocks(&self) -> u64;
    /// Read whole blocks from `block`.
    fn read(&self, block: u64, buf: &mut [u8]) -> Result<(), i32>;
    /// Write whole blocks from `block`.
    fn write(&self, block: u64, buf: &[u8]) -> Result<(), i32>;
    /// Make every completed write durable.
    fn flush(&self) -> Result<(), i32>;
}

/// Serve one request with `buf` as its data (the tag's buffer): the
/// bytes moved, or a negative `errno`.
pub fn serve(target: &dyn Target, d: &IoDesc, buf: &mut [u8]) -> i32 {
    let r = match d.op {
        OP_FLUSH => target.flush().map(|()| 0),
        OP_READ | OP_WRITE => transfer(target, d, buf),
        _ => Err(EOPNOTSUPP),
    };
    r.unwrap_or_else(|e| -e)
}

fn transfer(target: &dyn Target, d: &IoDesc, buf: &mut [u8]) -> Result<i32, i32> {
    let bytes = usize::try_from(d.sectors).map_err(|_| EINVAL)? * 512;
    if !d.start.is_multiple_of(SECTORS_PER_BLOCK) || !bytes.is_multiple_of(BLOCK) || bytes > buf.len() {
        return Err(EINVAL);
    }
    let block = d.start / SECTORS_PER_BLOCK;
    let count = (bytes / BLOCK) as u64;
    if block.checked_add(count).is_none_or(|end| end > target.blocks()) {
        return Err(EINVAL);
    }
    let data = &mut buf[..bytes];
    if d.op == OP_READ {
        target.read(block, data)?;
    } else {
        target.write(block, data)?;
        if d.flags & IO_F_FUA != 0 {
            target.flush()?;
        }
    }
    i32::try_from(bytes).map_err(|_| EINVAL)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A device in memory that counts its flushes.
    struct Mem {
        data: Mutex<Vec<u8>>,
        flushes: Mutex<u32>,
    }

    impl Target for Mem {
        fn blocks(&self) -> u64 {
            (self.data.lock().unwrap().len() / BLOCK) as u64
        }
        fn read(&self, block: u64, buf: &mut [u8]) -> Result<(), i32> {
            let at = usize::try_from(block).unwrap() * BLOCK;
            buf.copy_from_slice(&self.data.lock().unwrap()[at..at + buf.len()]);
            Ok(())
        }
        fn write(&self, block: u64, buf: &[u8]) -> Result<(), i32> {
            let at = usize::try_from(block).unwrap() * BLOCK;
            self.data.lock().unwrap()[at..at + buf.len()].copy_from_slice(buf);
            Ok(())
        }
        fn flush(&self) -> Result<(), i32> {
            *self.flushes.lock().unwrap() += 1;
            Ok(())
        }
    }

    fn desc(op: u8, flags: u32, start: u64, sectors: u32) -> IoDesc {
        IoDesc {
            op,
            flags,
            sectors,
            start,
        }
    }

    #[test]
    fn requests_become_block_calls() {
        let m = Mem {
            data: Mutex::new(vec![0; 16 * BLOCK]),
            flushes: Mutex::new(0),
        };
        let mut buf = vec![7u8; 4 * BLOCK];
        assert_eq!(serve(&m, &desc(OP_WRITE, 0, 16, 16), &mut buf), 8192);
        assert_eq!(*m.flushes.lock().unwrap(), 0);
        assert_eq!(serve(&m, &desc(OP_WRITE, IO_F_FUA, 32, 8), &mut buf), 4096);
        assert_eq!(*m.flushes.lock().unwrap(), 1);
        let mut back = vec![0u8; 4 * BLOCK];
        assert_eq!(serve(&m, &desc(OP_READ, 0, 16, 24), &mut back), 12288);
        assert!(back[..12288].iter().all(|&b| b == 7));
        assert_eq!(serve(&m, &desc(OP_FLUSH, 0, 0, 0), &mut back), 0);
        assert_eq!(*m.flushes.lock().unwrap(), 2);
        // Past the end, unaligned, unknown.
        assert_eq!(serve(&m, &desc(OP_READ, 0, 15 * 8, 16), &mut back), -EINVAL);
        assert_eq!(serve(&m, &desc(OP_READ, 0, 3, 8), &mut back), -EINVAL);
        assert_eq!(serve(&m, &desc(3, 0, 0, 8), &mut back), -EOPNOTSUPP);
    }
}
