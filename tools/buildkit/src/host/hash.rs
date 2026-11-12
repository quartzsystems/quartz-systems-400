// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SHA-256 helpers.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Hex SHA-256 of a byte slice.
pub fn sha256_bytes(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Hex SHA-256 of a file, streamed.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// First twelve hex digits, for file names.
pub fn short(hex: &str) -> &str {
    &hex[..hex.len().min(12)]
}

/// The `sha256sum` line for a file: `<hex>  <file name>`.
fn sum_line(hex: &str, name: &str) -> String {
    format!("{hex}  {name}\n")
}

/// Write `<file>.sha256` beside `file`, whose hash is `hex`, in the
/// form `sha256sum -c` reads. Its path.
pub fn write_sum(file: &Path, hex: &str) -> Result<PathBuf> {
    let name = file
        .file_name()
        .with_context(|| format!("{} has no file name", file.display()))?
        .to_string_lossy()
        .into_owned();
    let out = file.with_file_name(format!("{name}.sha256"));
    fs::write(&out, sum_line(hex, &name))
        .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_file_beside() {
        let dir = std::env::temp_dir().join(format!("qs400-sum-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let iso = dir.join("QS400-V1R0M0-0123456789ab.iso");
        fs::write(&iso, b"abc").unwrap();
        let hex = sha256_file(&iso).unwrap();
        assert_eq!(hex, sha256_bytes(b"abc"));
        let out = write_sum(&iso, &hex).unwrap();
        assert_eq!(out, dir.join("QS400-V1R0M0-0123456789ab.iso.sha256"));
        assert_eq!(
            fs::read_to_string(&out).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  QS400-V1R0M0-0123456789ab.iso\n"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
