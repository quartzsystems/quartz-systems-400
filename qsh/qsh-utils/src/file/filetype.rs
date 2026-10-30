// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `file [-h] [-m magic] file...`: what kind of data each file holds,
//! as `name: description`. Special files by type, `/QSYS.LIB` objects
//! by their type, regular files by their first 4096 bytes: `empty`,
//! `shell script`, `ASCII text`, `UTF-8 text`, `EBCDIC text` (tried
//! first when the file's CCSID attribute is an EBCDIC one),
//! `ISO-8859 text`, else `data`. `-h` describes a symbolic link
//! instead of what it names; `-m` is accepted and ignored.

use qsh::sys::{FileType, Stat, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::{complain_sys, usage};

const NAME: &str = "file";
const SAMPLE: usize = 4096;

fn ascii_text(b: &[u8]) -> bool {
    b.iter()
        .all(|&c| (0x20..0x7f).contains(&c) || matches!(c, b'\t' | b'\n' | b'\r' | 0x0c | 0x08 | 0x0b))
}

fn latin1_text(b: &[u8]) -> bool {
    b.iter().all(|&c| {
        (0x20..0x7f).contains(&c) || c >= 0xa0 || matches!(c, b'\t' | b'\n' | b'\r' | 0x0c)
    })
}

/// Bytes that occur in EBCDIC text: blank, punctuation, letters,
/// digits, and tab, carriage return, new line and line feed.
fn ebcdic_text(b: &[u8]) -> bool {
    let ok = |c: u8| {
        matches!(c, 0x05 | 0x0d | 0x15 | 0x25 | 0x40)
            || (0x4a..=0x50).contains(&c)
            || (0x5a..=0x61).contains(&c)
            || (0x6a..=0x6f).contains(&c)
            || (0x79..=0x7f).contains(&c)
            || (0x81..=0x89).contains(&c)
            || (0x91..=0x99).contains(&c)
            || (0xa1..=0xa9).contains(&c)
            || (0xc0..=0xc9).contains(&c)
            || (0xd0..=0xd9).contains(&c)
            || c == 0xe0
            || (0xe2..=0xe9).contains(&c)
            || (0xf0..=0xf9).contains(&c)
    };
    // Mostly letters and blanks, not just punctuation.
    let letters = b
        .iter()
        .filter(|&&c| c == 0x40 || (0x81..=0xe9).contains(&c) || (0xf0..=0xf9).contains(&c))
        .count();
    b.iter().all(|&c| ok(c)) && letters * 2 >= b.len()
}

fn ebcdic_ccsid(cx: &UtilCtx<'_>, abs: &str) -> bool {
    cx.sys
        .attributes(abs, true)
        .ok()
        .and_then(|a| a.into_iter().find(|(k, _)| k == "CCSID"))
        .and_then(|(_, v)| v.parse::<u16>().ok())
        .and_then(|c| ccsid::conv::Codec::of(c).ok().flatten())
        .is_some_and(|c| c.is_ebcdic())
}

fn object_kind(abs: &str) -> Option<String> {
    let up = abs.to_ascii_uppercase();
    if !up.starts_with("/QSYS.LIB/") {
        return None;
    }
    let last = up.rsplit('/').next().unwrap_or_default();
    let (_, ty) = last.rsplit_once('.')?;
    Some(match ty {
        "LIB" => "library".to_string(),
        "FILE" => "file object".to_string(),
        "MBR" => "member".to_string(),
        "PGM" => "program object".to_string(),
        "SRVPGM" => "service program object".to_string(),
        "MODULE" => "module object".to_string(),
        "CMD" => "command object".to_string(),
        "DTAARA" => "data area object".to_string(),
        "DTAQ" => "data queue object".to_string(),
        "USRSPC" => "user space object".to_string(),
        "MSGF" => "message file object".to_string(),
        "OUTQ" => "output queue object".to_string(),
        other => format!("*{other} object"),
    })
}

fn content(cx: &UtilCtx<'_>, abs: &str, st: &Stat) -> Result<String, qsh::sys::SysError> {
    if st.size == 0 {
        return Ok("empty".to_string());
    }
    let fd = cx.sys.open(abs, crate::common::io::READ, 0)?;
    let mut buf = vec![0u8; SAMPLE];
    let mut n = 0;
    while n < SAMPLE {
        match fd.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    let b = &buf[..n];
    if b.is_empty() {
        return Ok("empty".to_string());
    }
    if b.starts_with(b"#!") {
        return Ok("shell script".to_string());
    }
    if b.starts_with(b"\x7fELF") {
        return Ok("ELF object".to_string());
    }
    let ebcdic_first = ebcdic_ccsid(cx, abs);
    if ebcdic_first && ebcdic_text(b) {
        return Ok("EBCDIC text".to_string());
    }
    if ascii_text(b) {
        return Ok("ASCII text".to_string());
    }
    // A cut multi-byte character at the end of the sample is fine.
    let utf8 = match std::str::from_utf8(b) {
        Ok(_) => true,
        Err(e) => e.error_len().is_none() && n == SAMPLE,
    };
    if utf8 && b.iter().all(|&c| c >= 0x20 || matches!(c, b'\t' | b'\n' | b'\r' | 0x0c)) {
        return Ok("UTF-8 text".to_string());
    }
    if ebcdic_text(b) {
        return Ok("EBCDIC text".to_string());
    }
    if latin1_text(b) {
        return Ok("ISO-8859 text".to_string());
    }
    Ok("data".to_string())
}

fn describe(cx: &UtilCtx<'_>, f: &str, no_follow: bool) -> Result<String, qsh::sys::SysError> {
    let abs = cx.path(f);
    let lst = cx.sys.lstat(&abs)?;
    if lst.ftype == FileType::Symlink {
        let target = cx.sys.readlink(&abs).unwrap_or_default();
        if no_follow {
            return Ok(format!("symbolic link to {target}"));
        }
        if cx.sys.stat(&abs).is_err() {
            return Ok(format!("broken symbolic link to {target}"));
        }
    }
    let st = cx.sys.stat(&abs)?;
    if let Some(k) = object_kind(&abs)
        && (st.ftype != FileType::File || k != "member")
    {
        return Ok(k);
    }
    Ok(match st.ftype {
        FileType::Dir => "directory".to_string(),
        FileType::Fifo => "fifo".to_string(),
        FileType::Char => "character special".to_string(),
        FileType::Block => "block special".to_string(),
        FileType::Socket => "socket".to_string(),
        FileType::Symlink => "symbolic link".to_string(),
        FileType::Other => "object".to_string(),
        FileType::File => {
            let d = content(cx, &abs, &st)?;
            if object_kind(&abs).is_some() {
                format!("member, {d}")
            } else {
                d
            }
        }
    })
}

/// Run `file`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "hm:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    if p.operands.is_empty() {
        return usage(cx, NAME, "A file name is needed.");
    }
    let mut out = Out::stdout(cx);
    let mut status = 0;
    for f in &p.operands {
        match describe(cx, f, p.has('h')) {
            Ok(d) => {
                out.text(&format!("{f}: {d}\n"));
            }
            Err(e) => {
                out.flush();
                complain_sys(cx, NAME, f, e);
                status = 1;
            }
        }
    }
    out.finish(cx, NAME, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristics() {
        assert!(ascii_text(b"abc\n"));
        assert!(!ascii_text(b"\xe9\n"));
        assert!(ebcdic_text(&[0xc1, 0xc2, 0x25]));
        assert!(!ebcdic_text(b"abc\n"));
        assert!(latin1_text(b"\xe9\n"));
    }
}
