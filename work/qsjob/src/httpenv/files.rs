// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `httpd::files::Files`' operations over the IFS as a job sees it
//! ([`IfsClient`]). New files are tagged CCSID 1208 and written whole
//! to a temporary name beside them, then renamed over the old one.

use std::io;

use httpd::files::Entry;
use ifs::Errno;
use ifs::client::{CHUNK, IfsClient};
use ifs::ops::flags::{O_APPEND, O_CCSID, O_CREAT, O_RDONLY, O_TRUNC, O_WRONLY};
use ifs::ops::{OpenArgs, RenameMode, Stat};

/// The CCSID new files are tagged with.
const UTF8: u32 = 1208;
/// Mode of new files and directories.
const FILE_MODE: u32 = 0o644;
const DIR_MODE: u32 = 0o755;

/// An IFS error as an I/O error of the matching kind.
pub fn io_err(e: Errno) -> io::Error {
    let kind = match e {
        Errno::ENOENT | Errno::ENOTDIR => io::ErrorKind::NotFound,
        Errno::EEXIST => io::ErrorKind::AlreadyExists,
        Errno::EACCES | Errno::EPERM => io::ErrorKind::PermissionDenied,
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, e.to_string())
}

fn is_dir(s: &Stat) -> bool {
    s.objtype == "*DIR" || s.mode & 0o170_000 == 0o040_000
}

fn parent(p: &str) -> &str {
    match p.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) | None => "/",
        Some((d, _)) => d,
    }
}

fn entry(name: &str, s: &Stat) -> Entry {
    let dir = is_dir(s);
    Entry {
        name: name.to_string(),
        dir,
        size: if dir { 0 } else { s.size },
        modified: u64::try_from(s.mtime / 1_000_000_000).unwrap_or(0),
    }
}

/// Write all of `data` to `fd` and close it.
fn write_all(c: &dyn IfsClient, fd: i32, data: &[u8]) -> io::Result<()> {
    let mut at = 0;
    let r = (|| {
        while at < data.len() {
            let end = (at + CHUNK).min(data.len());
            let n = c.write(fd, &data[at..end]).map_err(io_err)?;
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "nothing written"));
            }
            at += n;
        }
        c.fsync(fd, false).map_err(io_err)
    })();
    let closed = c.close(fd).map_err(io_err);
    r.and(closed)
}

/// A whole file.
pub fn read(c: &dyn IfsClient, path: &str) -> io::Result<Vec<u8>> {
    let fd = c.open(path, OpenArgs::new(O_RDONLY)).map_err(io_err)?;
    let mut out = Vec::new();
    let r = loop {
        match c.read(fd, CHUNK) {
            Ok(b) if b.is_empty() => break Ok(()),
            Ok(b) => out.extend_from_slice(&b),
            Err(e) => break Err(io_err(e)),
        }
    };
    let _ = c.close(fd);
    r.map(|()| out)
}

/// Replace a file whole: a temporary file beside it, then a rename.
pub fn write_atomic(c: &dyn IfsClient, path: &str, data: &[u8]) -> io::Result<()> {
    let dir = parent(path);
    if !c.stat(dir, true).is_ok_and(|s| is_dir(&s)) {
        return Err(io::Error::new(io::ErrorKind::NotFound, format!("{dir} not found")));
    }
    let name = path.rsplit('/').next().unwrap_or("");
    let tmp = format!("{}/.{name}.tmp", dir.trim_end_matches('/'));
    let args = OpenArgs::new(O_CREAT | O_WRONLY | O_TRUNC | O_CCSID).mode(FILE_MODE).conv(UTF8);
    let fd = c.open(&tmp, args).map_err(io_err)?;
    let r = write_all(c, fd, data).and_then(|()| c.rename(&tmp, path, RenameMode::Unlink).map_err(io_err));
    if r.is_err() {
        let _ = c.unlink(&tmp);
    }
    r
}

/// Add to the end of a file, creating it.
pub fn append(c: &dyn IfsClient, path: &str, data: &[u8]) -> io::Result<()> {
    let args = OpenArgs::new(O_CREAT | O_WRONLY | O_APPEND | O_CCSID).mode(FILE_MODE).conv(UTF8);
    let fd = c.open(path, args).map_err(io_err)?;
    write_all(c, fd, data)
}

/// Rename, replacing `to`.
pub fn rename(c: &dyn IfsClient, from: &str, to: &str) -> io::Result<()> {
    c.rename(from, to, RenameMode::Unlink).map_err(io_err)
}

/// Create a directory and its parents.
pub fn mkdir_all(c: &dyn IfsClient, path: &str) -> io::Result<()> {
    let mut cur = String::new();
    for part in path.split('/').filter(|p| !p.is_empty()) {
        cur.push('/');
        cur.push_str(part);
        match c.stat(&cur, true) {
            Ok(s) if is_dir(&s) => {}
            Ok(_) => return Err(io::Error::other(format!("{cur} is a file"))),
            Err(_) => match c.mkdir(&cur, DIR_MODE) {
                Ok(()) | Err(Errno::EEXIST) => {}
                Err(e) => return Err(io_err(e)),
            },
        }
    }
    Ok(())
}

/// Remove a file.
pub fn remove(c: &dyn IfsClient, path: &str) -> io::Result<()> {
    if is_dir(&c.stat(path, false).map_err(io_err)?) {
        return Err(io::Error::other(format!("{path} is a directory")));
    }
    c.unlink(path).map_err(io_err)
}

/// Remove a directory and everything under it.
pub fn remove_tree(c: &dyn IfsClient, path: &str) -> io::Result<()> {
    c.stat(path, false).map_err(io_err)?;
    c.remove_tree(path).map_err(io_err)
}

/// A directory's entries, by name.
pub fn list(c: &dyn IfsClient, dir: &str) -> io::Result<Vec<Entry>> {
    let base = dir.trim_end_matches('/');
    let mut out = Vec::new();
    for d in c.read_dir(dir).map_err(io_err)? {
        if d.name == "." || d.name == ".." {
            continue;
        }
        let s = c.stat(&format!("{base}/{}", d.name), true).map_err(io_err)?;
        out.push(entry(&d.name, &s));
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The entry at `path`.
pub fn stat(c: &dyn IfsClient, path: &str) -> io::Result<Entry> {
    let s = c.stat(path, true).map_err(io_err)?;
    Ok(entry(path.trim_end_matches('/').rsplit('/').next().unwrap_or(""), &s))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ifs::auth::MemEnv;
    use ifs::backend::mem::MemBackend;
    use ifs::client::LocalIfs;
    use ifs::ops::audit::NoAudit;
    use ifs::ops::{Config, Ifs, Owner};
    use ifs::vfs::qsys::MemQsys;
    use sec::auth::bits::Auth;
    use sec::auth::principal::Principal;

    use super::*;

    fn local() -> LocalIfs<MemBackend> {
        let env = MemEnv::default().user("QSYS", 0).user("QSECOFR", 1).user("QTMHHTTP", 100);
        let ifs = Ifs::new(MemBackend::new(), Box::new(MemQsys::new(Auth::USE)), Config::default());
        ifs.ensure_layout(&env).unwrap();
        LocalIfs {
            ifs: Arc::new(ifs),
            env: Arc::new(env),
            audit: Arc::new(NoAudit),
            principal: Principal {
                user: "QSECOFR".into(),
                qsecurity: 40,
                objaud: "*NONE".into(),
                spcaut: sec::profile::spcaut::SpecialAuth::ALLOBJ.0,
                ..Principal::default()
            },
            owner: Owner(1),
            job_ccsid: 37,
        }
    }

    #[test]
    fn behaves_as_httpd_files() {
        let l = local();
        let f: &dyn IfsClient = &l;
        mkdir_all(f, "/www/a/conf").unwrap();
        write_atomic(f, "/www/a/conf/httpd.conf", b"one").unwrap();
        append(f, "/www/a/conf/httpd.conf", b"+two").unwrap();
        assert_eq!(read(f, "/www/a/conf/httpd.conf").unwrap(), b"one+two");
        write_atomic(f, "/www/a/conf/httpd.conf", b"three").unwrap();
        assert_eq!(read(f, "/www/a/conf/httpd.conf").unwrap(), b"three");
        let names: Vec<String> = list(f, "/www/a/conf").unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["httpd.conf"], "no temporary file is left");
        assert_eq!(f.stat("/www/a/conf/httpd.conf", true).unwrap().ccsid, 1208);
        let e = stat(f, "/www/a/conf/httpd.conf").unwrap();
        assert!(!e.dir && e.size == 5 && e.name == "httpd.conf");
        assert!(stat(f, "/www/a").unwrap().dir);
        assert_eq!(write_atomic(f, "/www/missing/x", b"").unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(read(f, "/www/none").unwrap_err().kind(), io::ErrorKind::NotFound);
        rename(f, "/www/a", "/www/b").unwrap();
        assert!(stat(f, "/www/b/conf/httpd.conf").is_ok() && stat(f, "/www/a").is_err());
        assert!(remove(f, "/www/b/conf").is_err());
        remove(f, "/www/b/conf/httpd.conf").unwrap();
        remove_tree(f, "/www/b").unwrap();
        assert!(list(f, "/www").unwrap().is_empty());
    }
}
