// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An SFTP v3 client over a [`Client`] session channel.

use std::io::{Read, Write};

use super::{Client, Incoming};
use crate::error::{SshError, SshResult};
use crate::sftp::proto::{self, Attrs, pflags, status};
use crate::wire::{Reader, Writer};

/// A failed request: the status code and text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    /// `SSH_FX_*`.
    pub code: u32,
    /// The server's text.
    pub text: String,
}

/// The result of one request.
pub type SftpResult<T> = SshResult<Result<T, Status>>;

/// An SFTP session.
pub struct Sftp<'c, R: Read, W: Write> {
    c: &'c mut Client<R, W>,
    chan: u32,
    buf: Vec<u8>,
    id: u32,
    /// The server's extensions from `VERSION`.
    pub extensions: Vec<(String, String)>,
}

impl<'c, R: Read, W: Write> Sftp<'c, R, W> {
    /// Start the `sftp` subsystem.
    pub fn start(c: &'c mut Client<R, W>) -> SshResult<Self> {
        let chan = c.session()?;
        if !c.subsystem(chan, "sftp")? {
            return Err(SshError::Protocol("the sftp subsystem was refused".into()));
        }
        let mut s = Self {
            c,
            chan,
            buf: Vec::new(),
            id: 0,
            extensions: Vec::new(),
        };
        let mut w = Writer::msg(proto::INIT);
        w.u32(3);
        s.packet(&w.finish())?;
        let v = s.reply()?;
        let mut r = Reader::new(&v);
        if r.u8()? != proto::VERSION || r.u32()? != 3 {
            return Err(SshError::Protocol("unexpected SFTP version".into()));
        }
        while !r.is_empty() {
            let n = r.str()?.to_string();
            let d = r.str()?.to_string();
            s.extensions.push((n, d));
        }
        Ok(s)
    }

    fn packet(&mut self, body: &[u8]) -> SshResult<()> {
        let mut p = u32::try_from(body.len()).unwrap_or(0).to_be_bytes().to_vec();
        p.extend_from_slice(body);
        self.c.send_data(self.chan, &p)
    }

    fn reply(&mut self) -> SshResult<Vec<u8>> {
        loop {
            if self.buf.len() >= 4 {
                let n = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]])
                    as usize;
                if self.buf.len() >= 4 + n {
                    let body = self.buf[4..4 + n].to_vec();
                    self.buf.drain(..4 + n);
                    return Ok(body);
                }
            }
            match self.c.recv()? {
                Incoming::Data(c, d) if c == self.chan => self.buf.extend(d),
                Incoming::Close(c) | Incoming::Eof(c) if c == self.chan => {
                    return Err(SshError::Closed);
                }
                _ => {}
            }
        }
    }

    fn call(&mut self, kind: u8, args: &[u8]) -> SshResult<Vec<u8>> {
        self.id += 1;
        let mut w = Writer::msg(kind);
        w.u32(self.id).raw(args);
        self.packet(&w.finish())?;
        self.reply()
    }

    fn status(r: &mut Reader<'_>) -> SshResult<Status> {
        let code = r.u32()?;
        let text = r.str()?.to_string();
        Ok(Status { code, text })
    }

    fn expect(&mut self, kind: u8, args: &[u8], want: u8) -> SftpResult<Vec<u8>> {
        let rep = self.call(kind, args)?;
        let mut r = Reader::new(&rep);
        let k = r.u8()?;
        r.u32()?;
        if k == proto::STATUS {
            let st = Self::status(&mut r)?;
            if want == proto::STATUS && st.code == status::OK {
                return Ok(Ok(Vec::new()));
            }
            return Ok(Err(st));
        }
        if k != want {
            return Err(SshError::Protocol(format!("unexpected SFTP reply {k}")));
        }
        Ok(Ok(r.rest().to_vec()))
    }

    fn path_args(paths: &[&str]) -> Vec<u8> {
        let mut w = Writer::new();
        for p in paths {
            w.str(p);
        }
        w.finish()
    }

    /// `OPEN`: a handle.
    pub fn open(&mut self, path: &str, flags: u32, attrs: &Attrs) -> SftpResult<Vec<u8>> {
        let mut w = Writer::new();
        w.str(path).u32(flags);
        attrs.encode(&mut w);
        Ok(self
            .expect(proto::OPEN, &w.finish(), proto::HANDLE)?
            .map(|b| Reader::new(&b).bytes().map(<[u8]>::to_vec).unwrap_or_default()))
    }

    /// `CLOSE`.
    pub fn close(&mut self, h: &[u8]) -> SftpResult<()> {
        let mut w = Writer::new();
        w.bytes(h);
        Ok(self.expect(proto::CLOSE, &w.finish(), proto::STATUS)?.map(|_| ()))
    }

    /// Read a whole file.
    pub fn get(&mut self, path: &str) -> SftpResult<Vec<u8>> {
        let h = match self.open(path, pflags::READ, &Attrs::default())? {
            Ok(h) => h,
            Err(e) => return Ok(Err(e)),
        };
        let mut out = Vec::new();
        loop {
            let mut w = Writer::new();
            w.bytes(&h).u64(out.len() as u64).u32(32 * 1024);
            match self.expect(proto::READ, &w.finish(), proto::DATA)? {
                Ok(b) => out.extend_from_slice(Reader::new(&b).bytes()?),
                Err(st) if st.code == status::EOF => break,
                Err(st) => return Ok(Err(st)),
            }
        }
        self.close(&h)?.ok();
        Ok(Ok(out))
    }

    /// Write a whole file (created or truncated).
    pub fn put(&mut self, path: &str, data: &[u8]) -> SftpResult<()> {
        let attrs = Attrs {
            permissions: Some(0o644),
            ..Attrs::default()
        };
        let h = match self.open(
            path,
            pflags::WRITE | pflags::CREAT | pflags::TRUNC,
            &attrs,
        )? {
            Ok(h) => h,
            Err(e) => return Ok(Err(e)),
        };
        for (i, chunk) in data.chunks(32 * 1024).enumerate() {
            let mut w = Writer::new();
            w.bytes(&h).u64((i * 32 * 1024) as u64).bytes(chunk);
            if let Err(st) = self.expect(proto::WRITE, &w.finish(), proto::STATUS)? {
                return Ok(Err(st));
            }
        }
        self.close(&h)
    }

    fn attrs(&mut self, kind: u8, path: &str) -> SftpResult<Attrs> {
        Ok(self
            .expect(kind, &Self::path_args(&[path]), proto::ATTRS)?
            .map(|b| Attrs::decode(&mut Reader::new(&b)).unwrap_or_default()))
    }

    /// `STAT`.
    pub fn stat(&mut self, path: &str) -> SftpResult<Attrs> {
        self.attrs(proto::STAT, path)
    }

    /// `LSTAT`.
    pub fn lstat(&mut self, path: &str) -> SftpResult<Attrs> {
        self.attrs(proto::LSTAT, path)
    }

    /// `SETSTAT`.
    pub fn setstat(&mut self, path: &str, a: &Attrs) -> SftpResult<()> {
        let mut w = Writer::new();
        w.str(path);
        a.encode(&mut w);
        Ok(self.expect(proto::SETSTAT, &w.finish(), proto::STATUS)?.map(|_| ()))
    }

    fn simple(&mut self, kind: u8, paths: &[&str]) -> SftpResult<()> {
        Ok(self
            .expect(kind, &Self::path_args(paths), proto::STATUS)?
            .map(|_| ()))
    }

    /// `MKDIR`.
    pub fn mkdir(&mut self, path: &str) -> SftpResult<()> {
        let mut w = Writer::new();
        w.str(path).u32(0);
        Ok(self.expect(proto::MKDIR, &w.finish(), proto::STATUS)?.map(|_| ()))
    }

    /// `RMDIR`.
    pub fn rmdir(&mut self, path: &str) -> SftpResult<()> {
        self.simple(proto::RMDIR, &[path])
    }

    /// `REMOVE`.
    pub fn remove(&mut self, path: &str) -> SftpResult<()> {
        self.simple(proto::REMOVE, &[path])
    }

    /// `RENAME`.
    pub fn rename(&mut self, from: &str, to: &str) -> SftpResult<()> {
        self.simple(proto::RENAME, &[from, to])
    }

    /// `SYMLINK` in OpenSSH's order: the target, then the link.
    pub fn symlink(&mut self, target: &str, link: &str) -> SftpResult<()> {
        self.simple(proto::SYMLINK, &[target, link])
    }

    fn one_name(&mut self, kind: u8, path: &str) -> SftpResult<String> {
        Ok(self
            .expect(kind, &Self::path_args(&[path]), proto::NAME)?
            .map(|b| {
                let mut r = Reader::new(&b);
                r.u32().ok();
                r.str().unwrap_or("").to_string()
            }))
    }

    /// `REALPATH`.
    pub fn realpath(&mut self, path: &str) -> SftpResult<String> {
        self.one_name(proto::REALPATH, path)
    }

    /// `READLINK`.
    pub fn readlink(&mut self, path: &str) -> SftpResult<String> {
        self.one_name(proto::READLINK, path)
    }

    /// `OPENDIR`, `READDIR` to the end, `CLOSE`: (name, longname, attrs).
    pub fn list(&mut self, path: &str) -> SftpResult<Vec<(String, String, Attrs)>> {
        let h = match self.expect(proto::OPENDIR, &Self::path_args(&[path]), proto::HANDLE)? {
            Ok(b) => Reader::new(&b).bytes()?.to_vec(),
            Err(e) => return Ok(Err(e)),
        };
        let mut out = Vec::new();
        loop {
            let mut w = Writer::new();
            w.bytes(&h);
            match self.expect(proto::READDIR, &w.finish(), proto::NAME)? {
                Ok(b) => {
                    let mut r = Reader::new(&b);
                    let n = r.u32()?;
                    for _ in 0..n {
                        let name = r.str()?.to_string();
                        let long = r.str()?.to_string();
                        let a = Attrs::decode(&mut r)?;
                        out.push((name, long, a));
                    }
                }
                Err(st) if st.code == status::EOF => break,
                Err(st) => return Ok(Err(st)),
            }
        }
        self.close(&h)?.ok();
        Ok(Ok(out))
    }

    /// An `EXTENDED` request expecting a status.
    pub fn extended(&mut self, name: &str, args: &[&str]) -> SftpResult<()> {
        let mut w = Writer::new();
        w.str(name);
        for a in args {
            w.str(a);
        }
        Ok(self.expect(proto::EXTENDED, &w.finish(), proto::STATUS)?.map(|_| ()))
    }

    /// End the session.
    pub fn finish(self) -> SshResult<()> {
        self.c.send_eof(self.chan)?;
        self.c.collect(self.chan).map(|_| ())
    }
}
