// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the wire tests share.

#![allow(dead_code)]

use std::fmt::Debug;
use std::path::PathBuf;

use nfs::xdr::{self, Xdr};

/// Bytes from hex, spaces and `|` ignored.
pub fn hex(s: &str) -> Vec<u8> {
    let d: Vec<u8> = s.bytes().filter(u8::is_ascii_hexdigit).collect();
    assert_eq!(d.len() % 2, 0, "odd hex");
    d.chunks(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}

/// `v` encodes to `want` and `want` decodes to `v`.
pub fn exact<T: Xdr + PartialEq + Debug>(v: &T, want: &[u8]) {
    let got = xdr::to_bytes(v).unwrap();
    assert_eq!(hexs(&got), hexs(want));
    assert_eq!(&xdr::from_bytes::<T>(want).unwrap(), v);
}

/// Bytes as hex words, for readable failures.
pub fn hexs(b: &[u8]) -> String {
    b.chunks(4)
        .map(|w| w.iter().fold(String::new(), |s, x| s + &format!("{x:02x}")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// With `QS400_WRITE_CORPUS=1`, keeps `data` as a seed of fuzz target `target`.
pub fn seed(target: &str, data: &[u8]) {
    if std::env::var_os("QS400_WRITE_CORPUS").is_none() {
        return;
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/fuzz/corpus")
        .join(target);
    std::fs::create_dir_all(&dir).unwrap();
    // FNV-1a names the file; the fuzzer reads every file in the folder.
    let h = data.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3)
    });
    std::fs::write(dir.join(format!("{h:016x}")), data).unwrap();
}
