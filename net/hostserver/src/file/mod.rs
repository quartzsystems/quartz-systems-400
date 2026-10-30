// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file server (8473): the IFS over the protocol. Paths come in
//! UTF-16; listings answer one `0x8005` per entry and end with return
//! code 18; files are opened into handles the client reads and writes
//! by offset. Every template starts with a two-byte chain indicator.

pub mod attrs;
pub mod rc;

use std::collections::HashMap;

use hostsvr::file::id;
use hostsvr::frame::{Frame, Param};
use ifs::client::IfsClient;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY};
use ifs::ops::{OpenArgs, RenameMode};
use regfac::ExitRequest;
use regfac::formats::host::{Access, func, pwfs0100, pwfs0200};

use crate::exits::{Exits, who};
use crate::session::{Auth, Session};
use crate::{Handler, Outcome, reply, text};

/// The exit point.
pub const EXIT_POINT: &str = "QIBM_QPWFS_FILE_SERV";

/// Code points of the file server's requests.
pub mod cp {
    /// A name (directory create and delete).
    pub const NAME1: u16 = 0x0001;
    /// A name (open, list, delete file).
    pub const NAME: u16 = 0x0002;
    /// The source of a rename.
    pub const SOURCE: u16 = 0x0003;
    /// The target of a rename.
    pub const TARGET: u16 = 0x0004;
    /// Data read.
    pub const READ_DATA: u16 = 0x0005;
    /// Preferred CCSIDs (exchange attributes).
    pub const CCSIDS: u16 = 0x000A;
    /// Object attributes 2.
    pub const OA2: u16 = 0x000F;
    /// Object attributes 1 (and the flags asking for them).
    pub const OA1: u16 = 0x0010;
    /// Data written.
    pub const WRITE_DATA: u16 = 0x0020;
}

/// An open file.
#[derive(Clone, Debug)]
struct Handle {
    fd: i32,
}

/// The server.
pub struct FileServer<A, I, X> {
    /// Sign-on state.
    pub session: Session,
    /// Signing on.
    pub auth: A,
    /// The IFS as the job sees it.
    pub ifs: I,
    /// Exit programs.
    pub exits: X,
    handles: HashMap<u32, Handle>,
    level: u16,
}

/// Split a list pattern into its directory and the name pattern.
fn split(path: &str) -> (String, String) {
    match path.rfind('/') {
        Some(0) => ("/".into(), path[1..].into()),
        Some(i) => (path[..i].into(), path[i + 1..].into()),
        None => (".".into(), path.into()),
    }
}

/// Whether `name` matches a pattern with `*` and `?` (case sensitive
/// as given; the IFS root folds case itself).
pub fn matches(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b) && go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    go(&p, &n)
}

fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') {
        format!("{dir}{name}")
    } else {
        format!("{dir}/{name}")
    }
}

impl<A: Auth, I: IfsClient, X: Exits> FileServer<A, I, X> {
    /// A server for one connection.
    pub fn new(auth: A, ifs: I, exits: X) -> Self {
        Self {
            session: Session::default(),
            auth,
            ifs,
            exits,
            handles: HashMap::new(),
            level: 16,
        }
    }

    fn user(&self) -> String {
        self.session.user.clone().unwrap_or_default()
    }

    fn name(f: &Frame, c: u16) -> String {
        f.param(c)
            .map(|p| text::from_utf16(&p.data))
            .unwrap_or_default()
    }

    fn allowed(&mut self, function: i32, a: Access, name: &str) -> bool {
        let req = ExitRequest::host("PWFS0100", pwfs0100(&who(&self.user(), function), a, name));
        self.exits.allow(EXIT_POINT, &req)
    }

    fn code(f: &Frame, code: u16) -> Vec<u8> {
        let mut t = 0u16.to_be_bytes().to_vec();
        t.extend(code.to_be_bytes());
        reply::frame(&f.header, id::RETURN_CODE_REPLY, t, Vec::new())
    }

    fn exchange(&mut self, f: &Frame) -> Outcome {
        self.level = reply::be16(&f.template, 2);
        let mut t = 0u16.to_be_bytes().to_vec();
        t.extend(0x0014u16.to_be_bytes());
        t.extend(0u16.to_be_bytes());
        t.extend(0x4000_0000u32.to_be_bytes());
        Outcome::one(reply::frame(
            &f.header,
            id::EXCHANGE_ATTRIBUTES_REPLY,
            t,
            vec![Param::new(cp::CCSIDS, 1200u16.to_be_bytes().to_vec())],
        ))
    }

    fn list(&mut self, f: &Frame) -> Outcome {
        let t = &f.template;
        let handle = reply::be32(t, 2);
        if handle != 0 {
            return self.list_handle(f, handle);
        }
        let path = Self::name(f, cp::NAME);
        if !self.allowed(func::FILE_LIST_ATTR, Access { read: true, ..Access::default() }, &path) {
            return Outcome::one(Self::code(f, rc::ACCESS_DENIED_TO_REQUEST));
        }
        let mut out = Vec::new();
        if path.contains(['*', '?']) {
            let (dir, pat) = split(&path);
            match self.ifs.read_dir(&dir) {
                Ok(ents) => {
                    for (restart, e) in (3u32..).zip(ents.iter().filter(|e| matches(&pat, &e.name))) {
                        if let Ok(st) = self.ifs.stat(&join(&dir, &e.name), false) {
                            out.push(attrs::entry_frame(f, &e.name, &st, restart, true));
                        }
                    }
                }
                Err(e) => return Outcome::one(Self::code(f, rc::of(e, true))),
            }
        } else {
            match self.ifs.stat(&path, false) {
                Ok(st) => {
                    let (_, base) = split(&path);
                    out.push(attrs::entry_frame(f, &base, &st, 0, true));
                }
                Err(e) => return Outcome::one(Self::code(f, rc::of(e, false))),
            }
        }
        out.push(Self::code(f, rc::NO_MORE_FILES));
        Outcome {
            replies: out,
            close: false,
        }
    }

    fn list_handle(&mut self, f: &Frame, handle: u32) -> Outcome {
        let Some(h) = self.handles.get(&handle).cloned() else {
            return Outcome::one(Self::code(f, rc::INVALID_HANDLE));
        };
        match self.ifs.fstat(h.fd) {
            Ok(st) => Outcome::one(attrs::object_attrs_frame(f, &st)),
            Err(e) => Outcome::one(Self::code(f, rc::of(e, false))),
        }
    }

    fn open(&mut self, f: &Frame) -> Outcome {
        let t = &f.template;
        let path = Self::name(f, cp::NAME);
        let access = reply::be16(t, 10);
        let option = reply::be16(t, 16);
        let mut oflag = match access {
            1 => O_RDONLY,
            2 => O_WRONLY,
            _ => O_RDWR,
        };
        let existed = self.ifs.stat(&path, true).is_ok();
        oflag |= match option {
            1 => O_CREAT,
            2 => O_CREAT | O_TRUNC,
            4 => O_CREAT | O_EXCL,
            16 => O_TRUNC,
            _ => 0,
        };
        let acc = Access {
            read: access & 1 != 0,
            write: access & 2 != 0,
            read_write: access == 3,
            delete: false,
        };
        let function = if existed { func::FILE_OPEN } else { func::FILE_CREATE };
        if !self.allowed(function, acc, &path) {
            return Outcome::one(Self::code(f, rc::ACCESS_DENIED_TO_REQUEST));
        }
        // A new file is tagged with the client's data CCSID.
        let data_ccsid = u32::from(reply::be16(t, 8));
        let tag = !existed && oflag & O_CREAT != 0 && data_ccsid != 0 && data_ccsid != 0xFFFF;
        let args = OpenArgs {
            oflag: if tag { oflag | O_CCSID } else { oflag },
            mode: 0o666,
            conversion_id: if tag { data_ccsid } else { 0 },
            text_conversion_id: 0,
        };
        match self.ifs.open(&path, args) {
            Ok(fd) => {
                let st = self.ifs.fstat(fd).unwrap_or_default();
                let h = (1..=u32::MAX).find(|h| !self.handles.contains_key(h)).unwrap_or(1);
                self.handles.insert(h, Handle { fd });
                let action = if existed { 1 } else { 4 };
                Outcome::one(attrs::open_frame(f, h, &st, action))
            }
            Err(e) => Outcome::one(Self::code(f, rc::of(e, false))),
        }
    }

    /// The file offset of a read or write: the 64-bit base and relative
    /// offsets at frame offset `long` when the template has them, else
    /// the 32-bit ones at 26.
    fn offset(t: &[u8], long: usize) -> u64 {
        let be64 = |at: usize| u64::from(reply::be32(t, at)) << 32 | u64::from(reply::be32(t, at + 4));
        let l = long - 20;
        if t.len() >= l + 16 {
            be64(l).wrapping_add(be64(l + 8))
        } else {
            u64::from(reply::be32(t, 6)).wrapping_add(u64::from(reply::be32(t, 10)))
        }
    }

    fn read(&mut self, f: &Frame) -> Outcome {
        let t = &f.template;
        let h = reply::be32(t, 2);
        let off = Self::offset(t, 42);
        let len = reply::be32(t, 14) as usize;
        let Some(hd) = self.handles.get(&h) else {
            return Outcome::one(Self::code(f, rc::INVALID_HANDLE));
        };
        match self.ifs.pread(hd.fd, len.min(ifs::client::CHUNK), off) {
            Ok(data) => {
                let mut tp = 0u16.to_be_bytes().to_vec();
                tp.extend(0xFFFFu16.to_be_bytes());
                Outcome::one(reply::frame(
                    &f.header,
                    id::READ_REPLY,
                    tp,
                    vec![Param::new(cp::READ_DATA, data)],
                ))
            }
            Err(e) => Outcome::one(Self::code(f, rc::of(e, false))),
        }
    }

    fn write(&mut self, f: &Frame) -> Outcome {
        let t = &f.template;
        let h = reply::be32(t, 2);
        let off = Self::offset(t, 38);
        let data = f.param(cp::WRITE_DATA).map(|p| p.data.clone()).unwrap_or_default();
        let Some(hd) = self.handles.get(&h) else {
            return Outcome::one(Self::code(f, rc::INVALID_HANDLE));
        };
        let (code, left) = match self.ifs.pwrite(hd.fd, &data, off) {
            Ok(n) => (rc::SUCCESS, data.len() - n),
            Err(e) => (rc::of(e, false), data.len()),
        };
        let mut tp = 0u16.to_be_bytes().to_vec();
        tp.extend(code.to_be_bytes());
        tp.extend([0; 4]);
        tp.extend(reply::len32(left).to_be_bytes());
        tp.resize(20, 0);
        // The reply is asked for by the data flags (2).
        Outcome::one(reply::frame(&f.header, id::WRITE_REPLY, tp, Vec::new()))
    }

    fn close(&mut self, f: &Frame) -> Outcome {
        let h = reply::be32(&f.template, 2);
        let code = match self.handles.remove(&h) {
            Some(hd) => match self.ifs.close(hd.fd) {
                Ok(()) => rc::SUCCESS,
                Err(e) => rc::of(e, false),
            },
            None => rc::INVALID_HANDLE,
        };
        let mut tp = 0u16.to_be_bytes().to_vec();
        tp.extend(code.to_be_bytes());
        tp.extend([0; 4]);
        Outcome::one(reply::frame(&f.header, id::CLOSE_REPLY, tp, Vec::new()))
    }

    fn simple(&mut self, f: &Frame) -> Outcome {
        let rid = f.header.req_rep_id;
        let del = Access {
            delete: true,
            ..Access::default()
        };
        let write = Access {
            write: true,
            ..Access::default()
        };
        let code = match rid {
            id::CREATE_DIR => {
                let p = Self::name(f, cp::NAME1);
                if self.allowed(func::FILE_CREATE, write, &p) {
                    self.ifs.mkdir(&p, 0o777).map_or_else(|e| rc::of(e, false), |()| rc::SUCCESS)
                } else {
                    rc::ACCESS_DENIED_TO_REQUEST
                }
            }
            id::DELETE_DIR => {
                let p = Self::name(f, cp::NAME1);
                if self.allowed(func::FILE_DELETE, del, &p) {
                    self.ifs.rmdir(&p).map_or_else(|e| rc::of(e, false), |()| rc::SUCCESS)
                } else {
                    rc::ACCESS_DENIED_TO_REQUEST
                }
            }
            id::DELETE_FILE => {
                let p = Self::name(f, cp::NAME);
                if self.allowed(func::FILE_DELETE, del, &p) {
                    self.ifs.unlink(&p).map_or_else(|e| rc::of(e, false), |()| rc::SUCCESS)
                } else {
                    rc::ACCESS_DENIED_TO_REQUEST
                }
            }
            id::RENAME => {
                let s = Self::name(f, cp::SOURCE);
                let d = Self::name(f, cp::TARGET);
                let replace = reply::be16(&f.template, 14) != 0;
                let req = ExitRequest::host(
                    "PWFS0200",
                    pwfs0200(&who(&self.user(), func::FILE_RENAME), write, "", &s, &d),
                );
                if self.exits.allow(EXIT_POINT, &req) {
                    let mode = if replace { RenameMode::Unlink } else { RenameMode::Keep };
                    self.ifs.rename(&s, &d, mode).map_or_else(|e| rc::of(e, false), |()| rc::SUCCESS)
                } else {
                    rc::ACCESS_DENIED_TO_REQUEST
                }
            }
            _ => rc::REQUEST_NOT_SUPPORTED,
        };
        Outcome::one(Self::code(f, code))
    }
}

impl<A: Auth, I: IfsClient, X: Exits> Handler for FileServer<A, I, X> {
    fn handle(&mut self, frame: &[u8]) -> Outcome {
        let Ok(f) = Frame::decode(frame) else {
            return Outcome::close();
        };
        if let Some(o) = self.session.common(&f, &mut self.auth) {
            if o.close {
                for (_, h) in self.handles.drain() {
                    let _ = self.ifs.close(h.fd);
                }
            }
            return o;
        }
        if !self.session.started() {
            return Outcome::close();
        }
        match f.header.req_rep_id {
            id::EXCHANGE_ATTRIBUTES => self.exchange(&f),
            id::LIST_ATTRIBUTES => self.list(&f),
            id::OPEN => self.open(&f),
            id::READ => self.read(&f),
            id::WRITE => self.write(&f),
            id::CLOSE => self.close(&f),
            id::COMMIT => {
                let h = reply::be32(&f.template, 2);
                let code = match self.handles.get(&h) {
                    Some(hd) => self.ifs.fsync(hd.fd, false).map_or_else(|e| rc::of(e, false), |()| rc::SUCCESS),
                    None => rc::INVALID_HANDLE,
                };
                Outcome::one(Self::code(&f, code))
            }
            id::CREATE_DIR | id::DELETE_DIR | id::DELETE_FILE | id::RENAME => self.simple(&f),
            _ => Outcome::one(Self::code(&f, rc::REQUEST_NOT_SUPPORTED)),
        }
    }

    /// Files the client left open close.
    fn finish(&mut self) {
        for (_, h) in self.handles.drain() {
            let _ = self.ifs.close(h.fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        assert!(matches("*", "abc"));
        assert!(matches("b.tx*", "b.txt"));
        assert!(!matches("hstrac*", "data"));
        assert_eq!(split("/home/QSTEST/*"), ("/home/QSTEST".into(), "*".into()));
    }
}
