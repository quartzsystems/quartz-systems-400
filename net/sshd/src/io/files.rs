// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Whole stream files read and written through an [`IfsClient`]: the
//! configuration, host keys, `authorized_keys` and the banner.

use ifs::client::{CHUNK, IfsClient};
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};

/// The whole contents of `path`, as bytes.
pub fn read(ifs: &dyn IfsClient, path: &str) -> ifs::Result<Vec<u8>> {
    let fd = ifs.open(path, OpenArgs::new(O_RDONLY))?;
    let mut out = Vec::new();
    let r = loop {
        match ifs.read(fd, CHUNK) {
            Ok(b) if b.is_empty() => break Ok(()),
            Ok(b) => out.extend_from_slice(&b),
            Err(e) => break Err(e),
        }
    };
    let _ = ifs.close(fd);
    r.map(|()| out)
}

/// The whole contents of `path`, as text (bytes not UTF-8 replaced).
pub fn read_text(ifs: &dyn IfsClient, path: &str) -> ifs::Result<String> {
    read(ifs, path).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Replace `path` with `data`, made with `mode` when new.
pub fn write(ifs: &dyn IfsClient, path: &str, data: &[u8], mode: u32) -> ifs::Result<()> {
    let fd = ifs.open(path, OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC).mode(mode))?;
    let mut rest = data;
    let r = loop {
        if rest.is_empty() {
            break Ok(());
        }
        let n = rest.len().min(CHUNK);
        match ifs.write(fd, &rest[..n]) {
            Ok(0) => break Err(ifs::Errno::EIO),
            Ok(k) => rest = &rest[k..],
            Err(e) => break Err(e),
        }
    };
    let _ = ifs.close(fd);
    r
}

/// Whether `path` names anything (a symbolic link is followed).
pub fn exists(ifs: &dyn IfsClient, path: &str) -> bool {
    ifs.stat(path, true).is_ok()
}

/// Make `path` and its parents as directories (`mkdir -p`).
pub fn mkdir_p(ifs: &dyn IfsClient, path: &str, mode: u32) -> ifs::Result<()> {
    let mut cur = String::new();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        cur.push('/');
        cur.push_str(part);
        if !exists(ifs, &cur) {
            match ifs.mkdir(&cur, mode) {
                Ok(()) | Err(ifs::Errno::EEXIST) => {}
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}
