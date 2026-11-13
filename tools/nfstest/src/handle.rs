// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `nfstest handle save|use`: a version 3 file handle kept in a local file
//! and used again later, after the server restarted or its machine had an
//! IPL, with no `MOUNT` and no `LOOKUP`: the handle alone must still name
//! the file.

use std::fmt::Write as _;
use std::path::Path;

use crate::Error;
use crate::ctx::Ctx;
use crate::error::check;
use crate::v3::{Nfs3, Session};

/// What the saved file holds.
const TEXT: &[u8] = b"nfstest handle-survives\n";
/// The file's name in the scratch directory.
const NAME: &[u8] = b"handle-survives.txt";

/// Makes `SCRATCH/handle-<pid>/handle-survives.txt`, writes it stably and
/// saves its handle (hexadecimal) in `file`; what to print.
pub fn save(c: &Ctx, file: &Path) -> Result<String, Error> {
    let mut s = Session::open(c, "handle")?;
    s.keep();
    let d = s.dir.clone();
    let f = s.n.create(&d, NAME, 0o644)?;
    s.n.write(&f, 0, TEXT)?;
    let id = s.n.getattr(&f)?.fileid;
    let hex = f.iter().fold(String::new(), |mut h, b| {
        let _ = write!(h, "{b:02x}");
        h
    });
    std::fs::write(file, format!("{hex}\n"))?;
    Ok(format!("ok handle-save: {} bytes, fileid {id}\n", f.len()))
}

/// Reads the handle saved in `file` and uses it straight away on NFS's
/// port: `GETATTR` and `READ` must answer the saved text; what to print.
pub fn use_saved(c: &Ctx, file: &Path) -> Result<String, Error> {
    let text = std::fs::read_to_string(file)?;
    let h = unhex(text.trim())
        .ok_or_else(|| Error::Usage(format!("{}: not a handle", file.display())))?;
    let mut n = Nfs3 { c: c.client(c.nfs) };
    let a = n.getattr(&h)?;
    let (data, _) = n.read(&h, 0, 4096)?;
    check(data == TEXT, || {
        format!(
            "READ of the saved handle: {:?}",
            String::from_utf8_lossy(&data)
        )
    })?;
    Ok(format!(
        "ok handle-use: fileid {}, {} bytes read, no LOOKUP\n",
        a.fileid,
        data.len()
    ))
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::unhex;

    #[test]
    fn hex_back() {
        assert_eq!(unhex("00ff10"), Some(vec![0, 255, 16]));
        assert_eq!(unhex("0"), None);
        assert_eq!(unhex("zz"), None);
        assert_eq!(unhex(""), None);
    }
}
