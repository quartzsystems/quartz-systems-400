// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `nfstest bench`: a sequential write then read of one file, timed.

use std::time::Instant;

use nfs_v3::wire::stable_how;

use crate::Error;
use crate::ctx::Ctx;
use crate::error::check;
use crate::scenarios::files::{FNV0, fnv, io_sizes, pattern};
use crate::v3::{Session, ok};

/// The result.
#[derive(Debug, Clone, Copy)]
pub struct Bench {
    /// Bytes moved each way.
    pub bytes: u64,
    /// The block size.
    pub bs: u32,
    /// Write rate, MiB/s (to the `COMMIT`).
    pub write: f64,
    /// Read rate, MiB/s.
    pub read: f64,
}

impl std::fmt::Display for Bench {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "bytes: {}", self.bytes)?;
        writeln!(f, "block: {}", self.bs)?;
        writeln!(f, "write MiB/s: {:.1}", self.write)?;
        writeln!(f, "read MiB/s: {:.1}", self.read)
    }
}

/// Writes `mib` MiB in blocks of `bs` (0: the server's preferred size),
/// commits, reads it back and compares the checksums.
#[allow(clippy::cast_precision_loss)]
pub fn run(c: &Ctx, mib: u64, bs: u32) -> Result<Bench, Error> {
    let mut s = Session::open(c, "bench")?;
    let (d, root) = (s.dir.clone(), s.root.clone());
    let (rsize, wsize) = io_sizes(c, &mut s.n, &root)?;
    let wsize = if bs == 0 { wsize } else { bs };
    let rsize = if bs == 0 { rsize } else { bs };
    let f = s.n.create(&d, b"bench", 0o644)?;
    let total = mib << 20;
    let block = pattern(0, wsize as usize);
    let mut sum = FNV0;
    let t = Instant::now();
    let mut off = 0u64;
    while off < total {
        let n = (total - off).min(u64::from(wsize)) as usize;
        sum = fnv(sum, &block[..n]);
        let r = ok(
            s.n.write_res(&f, off, &block[..n], stable_how::UNSTABLE)?,
            "WRITE",
        )?;
        check(r.count as usize == n, || {
            format!("WRITE at {off}: {} of {n}", r.count)
        })?;
        off += n as u64;
    }
    let r: nfs_v3::wire::COMMIT3res = s.n.call(
        nfs_v3::wire::NFS_PROGRAM::NFSPROC3_COMMIT,
        &nfs_v3::wire::COMMIT3args {
            file: crate::v3::fh(&f),
            offset: 0,
            count: 0,
        },
    )?;
    ok(r, "COMMIT")?;
    let w = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let mut back = FNV0;
    let mut off = 0u64;
    loop {
        let (b, eof) = s.n.read(&f, off, rsize)?;
        back = fnv(back, &b);
        off += b.len() as u64;
        if eof || b.is_empty() {
            break;
        }
    }
    let rd = t.elapsed().as_secs_f64();
    check(off == total, || format!("read {off} of {total}"))?;
    check(back == sum, || "checksum differs".into())?;
    s.close()?;
    let mibs = |secs: f64| (total as f64 / f64::from(1 << 20)) / secs.max(1e-9);
    Ok(Bench {
        bytes: total,
        bs: wsize,
        write: mibs(w),
        read: mibs(rd),
    })
}
