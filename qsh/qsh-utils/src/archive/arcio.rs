// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading and writing a whole archive: a named file or `-` (standard
//! input or output).

use qsh::sys::UtilCtx;
use qsh::sys::stream::{read_to_end, write_all};

use crate::common::io::{input, open_write};

/// Read all of an archive (`None` or `-` is standard input). The error
/// is the message text.
pub fn read_all(cx: &UtilCtx<'_>, file: Option<&str>) -> Result<Vec<u8>, String> {
    let name = file.unwrap_or("-");
    let fd = input(cx, name).map_err(|e| format!("{name}: {}", e.errno.text()))?;
    read_to_end(&*fd).map_err(|e| format!("{name}: {}", crate::common::io::errno_text(&e)))
}

/// Write all of an archive (`None` or `-` is standard output).
pub fn write_out(cx: &UtilCtx<'_>, file: Option<&str>, bytes: &[u8]) -> Result<(), String> {
    match file {
        None | Some("-") => cx
            .write(1, bytes)
            .map_err(|e| format!("standard output: {}", crate::common::io::errno_text(&e))),
        Some(f) => {
            let fd = open_write(cx, f, false).map_err(|e| format!("{f}: {}", e.errno.text()))?;
            write_all(&*fd, bytes).map_err(|e| format!("{f}: {}", crate::common::io::errno_text(&e)))
        }
    }
}

/// The names to archive from standard input, one a line.
pub fn names_from_stdin(cx: &UtilCtx<'_>) -> Vec<String> {
    let data = read_all(cx, None).unwrap_or_default();
    String::from_utf8_lossy(&data)
        .lines()
        .filter(|l| !l.is_empty())
        .map(ToString::to_string)
        .collect()
}

/// A number of bytes with an optional `b` (512) or `k` (1024) suffix,
/// or a product `n*m`.
pub fn block_size(s: &str) -> Option<usize> {
    if let Some((a, b)) = s.split_once(['*', 'x']) {
        return block_size(a)?.checked_mul(block_size(b)?);
    }
    let (num, mul) = match s.as_bytes().last()? {
        b'b' => (&s[..s.len() - 1], 512),
        b'k' => (&s[..s.len() - 1], 1024),
        b'm' => (&s[..s.len() - 1], 1024 * 1024),
        _ => (s, 1),
    };
    let n: usize = crate::common::number(num)?.try_into().ok()?;
    n.checked_mul(mul).filter(|&v| v > 0)
}
