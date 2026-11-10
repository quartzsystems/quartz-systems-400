// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reads and writes at the negotiated maximum, and the timed copy.

use std::time::Instant;

use futures_util::{StreamExt, stream};
use smb::{ReadAt, WriteAt};

use super::scratch::remove;
use super::{Lines, Scratch};
use crate::conn::{Share, base_config};
use crate::error::check;
use crate::{Error, Target};

/// Requests in flight at once during the copy.
const WINDOW: usize = 8;

/// One chunk of the pattern: a fixed body with its index in the first 8 bytes.
fn chunk(body: &[u8], index: u64) -> Vec<u8> {
    let mut c = body.to_vec();
    let n = c.len().min(8);
    c[..n].copy_from_slice(&index.to_le_bytes()[..n]);
    c
}

/// A pseudo-random body (xorshift), so compression or zero pages hide nothing.
fn body(len: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x.to_le_bytes()[3]
        })
        .collect()
}

/// Writes `total` bytes in `size` requests, `window` at a time; then reads
/// them back the same way and checks each chunk. Returns the seconds each took.
async fn transfer(
    s: &Share,
    path: &str,
    total: u64,
    wsize: usize,
    rsize: usize,
    window: usize,
) -> Result<(f64, f64), Error> {
    let size = wsize.min(rsize).max(1);
    let base = body(size);
    let chunks = total.div_ceil(size as u64);
    let len_of =
        |i: u64| usize::try_from((total - i * size as u64).min(size as u64)).unwrap_or(size);

    let f = s.create_file(path).await?;
    let start = Instant::now();
    let mut writes = stream::iter(0..chunks)
        .map(|i| {
            let data = chunk(&base, i);
            let f = &f;
            async move {
                let len = len_of(i);
                let n = f.write_at(&data[..len], i * size as u64).await?;
                check(n == len, || format!("chunk {i}: wrote {n} of {len}"))
            }
        })
        .buffer_unordered(window);
    while let Some(r) = writes.next().await {
        r?;
    }
    drop(writes);
    f.close().await?;
    let wsecs = start.elapsed().as_secs_f64();

    let f = s.open_file(path, false).await?;
    let got = smb::GetLen::get_len(&f).await?;
    check(got == total, || format!("size {got} after writing {total}"))?;
    let start = Instant::now();
    let mut reads = stream::iter(0..chunks)
        .map(|i| {
            let f = &f;
            let base = &base;
            async move {
                let len = len_of(i);
                let mut buf = vec![0u8; len];
                let mut at = 0;
                while at < len {
                    let n = f
                        .read_at(&mut buf[at..], i * size as u64 + at as u64)
                        .await?;
                    check(n > 0, || format!("chunk {i}: short read at {at}"))?;
                    at += n;
                }
                check(buf == chunk(base, i)[..len], || {
                    format!("chunk {i}: data differs")
                })
            }
        })
        .buffer_unordered(window);
    while let Some(r) = reads.next().await {
        r?;
    }
    drop(reads);
    f.close().await?;
    Ok((wsecs, start.elapsed().as_secs_f64()))
}

fn mbps(bytes: u64, secs: f64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mb = bytes as f64 / 1e6;
    format!("{:.1} MB/s", mb / secs.max(1e-9))
}

/// One request at a time at the negotiated maximum sizes, over several requests.
pub async fn large_io(t: &Target, sc: &Scratch) -> Lines {
    let s = Share::connect(t, base_config(t)).await?;
    let info = s.info().await?;
    let (w, r) = (
        info.negotiation.max_write_size as usize,
        info.negotiation.max_read_size as usize,
    );
    let total = t.large_bytes.max(3 * w.max(r) as u64 + 17);
    let path = sc.file("large.bin");
    transfer(&s, &path, total, w, r, 1).await?;
    remove(&s, &path).await?;
    s.close().await?;
    Ok(vec![
        format!("bytes: {total}"),
        format!("max write/read: {w}/{r}"),
    ])
}

/// `copy_bytes` in and out with requests in flight, timed.
pub async fn copy(t: &Target, sc: &Scratch) -> Lines {
    let s = Share::connect(t, base_config(t)).await?;
    let info = s.info().await?;
    let cap = 1 << 20;
    let w = (info.negotiation.max_write_size as usize).min(cap);
    let r = (info.negotiation.max_read_size as usize).min(cap);
    let path = sc.file("copy.bin");
    let (win, rin) = transfer(&s, &path, t.copy_bytes, w, r, WINDOW).await?;
    remove(&s, &path).await?;
    s.close().await?;
    Ok(vec![
        format!("bytes: {}", t.copy_bytes),
        format!("request: {} bytes, {WINDOW} in flight", w.min(r)),
        format!("in: {} ({win:.2}s)", mbps(t.copy_bytes, win)),
        format!("out: {} ({rin:.2}s)", mbps(t.copy_bytes, rin)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_carry_their_index() {
        let b = body(16);
        assert_ne!(chunk(&b, 1), chunk(&b, 2));
        assert_eq!(chunk(&b, 7)[8..], b[8..]);
        assert_eq!(chunk(&b[..4], 1).len(), 4);
    }

    #[test]
    fn rates_print_in_decimal_megabytes() {
        assert_eq!(mbps(2_000_000, 1.0), "2.0 MB/s");
    }
}
