// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DDM requester in the job: an open that reaches a DDM file (the
//! service answers [`DdmRedirect`]) opens the remote file instead, over
//! a connection to the system the file names, and the program's record
//! operations go there. The user and password come from the job user's
//! server authentication entry (`ADDSVRAUTE`) for the RDB, then for
//! `QDDMDRDASERVER`, then for `QDDMSERVER`.

use std::collections::HashMap;
use std::net::TcpStream;
use std::time::Duration;

use ddm::client::file::{FileReply, RemoteFile};
use ddm::client::{ConnectOpts, Connection};
use ddm::file::{Rec, Ufcb, ufcb};
use ddm::objects::{DdmFile, RdbDirectory};
use ddm::sec::Mechanism;
use mi::io::{DdmRedirect, FileInfo, IoFail, IoOp, IoRequest, IoResult, IoSource, KeyRef, OpenSpec};
use stg::{Client, Shared};

/// Handles of remote files start here, above any the service gives.
const FIRST_REMOTE: u32 = 0x4000_0000;

/// One remote file.
struct Remote {
    conn: Connection<TcpStream>,
    file: RemoteFile,
    /// Read since the open or a `SETLL *START` (the next read is the next record).
    positioned: bool,
    keys: u32,
}

/// The job's files: the service's, and DDM files' remote ones.
pub struct DdmIo<I> {
    inner: I,
    link: Shared,
    remote: HashMap<u32, Remote>,
    next: u32,
}

impl<I: IoSource> DdmIo<I> {
    /// Over the service's files.
    pub fn new(inner: I, link: Shared) -> Self {
        Self {
            inner,
            link,
            remote: HashMap::new(),
            next: FIRST_REMOTE,
        }
    }
}

/// A failed DDM open or operation.
fn fail(msgid: &str, what: &str) -> IoFail {
    let mut f = IoFail::new(1299, msgid, &[]);
    f.data = ccsid::to_ebcdic(what).unwrap_or_default();
    f
}

/// Where a DDM file goes: host, port, the RDB it is named by.
pub fn destination(f: &DdmFile, dir: &RdbDirectory) -> Result<(String, u16, String), IoFail> {
    if f.rmtloc.eq_ignore_ascii_case("*RDB") {
        let e = dir
            .find(&f.rdb)
            .ok_or_else(|| fail("CPF9190", &format!("No relational database directory entry {}.", f.rdb)))?;
        let host = if e.is_local() { "127.0.0.1".to_string() } else { e.rmtloc.clone() };
        return Ok((host, e.tcp_port(), e.rdb.clone()));
    }
    if !f.is_ip() {
        return Err(fail("CPF9190", "DDM files over SNA are not supported; use RMTLOCNAME(name *IP)."));
    }
    let host = f.rmtloc.trim().trim_matches('\'').to_string();
    Ok((host.clone(), f.port, host.to_ascii_uppercase()))
}

/// The user and password for `rdb` from the job user's entries.
pub(crate) fn credentials(link: &Shared, rdb: &str) -> Option<(String, String)> {
    for server in [rdb, "QDDMDRDASERVER", "QDDMSERVER"] {
        let got = link.borrow_mut().with(|l| l.svraute_get(server.to_string()));
        if let Some((u, p)) = got
            && !p.is_empty()
        {
            return Some((u, p));
        }
    }
    None
}

impl<I: IoSource> DdmIo<I> {
    fn open_remote(&mut self, spec: &OpenSpec, redirect: &DdmRedirect) -> Result<(u32, FileInfo), IoFail> {
        let desc = DdmFile::decode(&redirect.payload)
            .ok_or_else(|| fail("CPF3204", &format!("DDM file {}/{} is damaged.", redirect.library, redirect.file)))?;
        let dir = crate::command::ddm::directory_of(&self.link);
        let (host, port, rdb) = destination(&desc, &dir)?;
        let (user, password) = credentials(&self.link, &rdb).ok_or_else(|| {
            fail("CPF9190", &format!("No server authentication entry with a password for {rdb} (ADDSVRAUTE)."))
        })?;
        let addrs = crate::work::netsvc::resolve(&self.link, &host, port);
        let stream = addrs
            .iter()
            .find_map(|a| TcpStream::connect_timeout(a, Duration::from_secs(10)).ok())
            .ok_or_else(|| fail("CPF9162", &format!("Cannot reach {host} port {port}.")))?;
        let _ = stream.set_nodelay(true);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
        let mut opts = ConnectOpts::new(&rdb, &user, &password);
        opts.access_rdb = false;
        // The password itself first: it proves the user at any QPWDLVL.
        opts.mechanisms = vec![Mechanism::EUsrIdPwd, Mechanism::UsrEncPwd, Mechanism::UsrSsbPwd, Mechanism::UsrSbsPwd];
        opts.srvnam = crate::work::ddm::system_name(&self.link);
        let mut conn = Connection::connect(stream, &opts).map_err(|e| fail("CPF9190", &format!("{rdb}: {e}")))?;
        let (lib, file) = desc.remote_name().ok_or_else(|| fail("CPF9190", "A nonstandard remote file name is not supported."))?;
        let flags = match spec.usage {
            qir::FileUsage::Input => ufcb::INPUT,
            qir::FileUsage::Output => ufcb::OUTPUT,
            qir::FileUsage::Update => ufcb::INPUT | ufcb::UPDATE | ufcb::DELETE,
            qir::FileUsage::All => ufcb::INPUT | ufcb::OUTPUT | ufcb::UPDATE | ufcb::DELETE,
        };
        let (format, fields, keys) = redirect
            .local
            .as_ref()
            .map_or_else(|| (file.clone(), 0, 0), |l| (l.name.clone(), l.fields as usize, l.keys));
        let block = Ufcb {
            file: file.clone(),
            library: lib.clone(),
            member: spec.member.clone().unwrap_or_else(|| "*FIRST".into()),
            flags,
            arrival: !spec.keyed,
            share: Ufcb::share_for(flags),
            blocking: 1,
            format: format.clone(),
        };
        let h = self.next;
        self.next += 1;
        let rf = conn
            .open_file(&format!("F{h:08X}"), &block, fields)
            .map_err(|e| fail("CPF4101", &format!("{lib}/{file} on {rdb}: {e}")))?;
        let info = FileInfo {
            file: redirect.file.clone(),
            lib: redirect.library.clone(),
            member: spec.member.clone().unwrap_or_default(),
            format,
            rec_len: u32::from(rf.feedback.reclen),
            wait_ms: 0,
        };
        self.remote.insert(
            h,
            Remote {
                conn,
                file: rf,
                positioned: false,
                keys,
            },
        );
        Ok((h, info))
    }
}

fn status(code: u16, msgid: &str) -> IoResult {
    IoResult {
        status: code,
        msgid: Some(msgid.into()),
        ..IoResult::default()
    }
}

/// What a read answered as the program sees it.
fn read_result(reply: Result<FileReply, ddm::Error>, keyed: bool) -> IoResult {
    let reply = match reply {
        Ok(r) => r,
        Err(e) => return status(1299, if e.to_string().contains("CPF") { "CPF5029" } else { "CPF5001" }),
    };
    if let Some(r) = reply.records.into_iter().next() {
        return IoResult {
            found: true,
            equal: keyed,
            rrn: u64::from(r.rrn),
            nulls: Some(r.nulls.iter().map(|n| if *n { 0xF1 } else { 0xF0 }).collect()),
            record: Some(r.data),
            ..IoResult::default()
        };
    }
    if keyed {
        IoResult {
            status: 12,
            ..IoResult::default()
        }
    } else {
        IoResult {
            status: 11,
            eof: true,
            ..IoResult::default()
        }
    }
}

fn done(reply: Result<FileReply, ddm::Error>) -> IoResult {
    match reply {
        Ok(r) => match r.error() {
            Some(m) => status(1299, &m.id),
            None => IoResult {
                rrn: r.iofb.map_or(0, |f| u64::from(f.rrn)),
                ..IoResult::default()
            },
        },
        Err(_) => status(1299, "CPF5029"),
    }
}

fn rec(req: &IoRequest<'_>, fields: usize) -> Rec {
    let mut nulls: Vec<bool> = req.nulls.map(|n| n.iter().map(|b| *b == 0xF1).collect()).unwrap_or_default();
    nulls.resize(fields, false);
    Rec {
        data: req.record.map(<[u8]>::to_vec).unwrap_or_default(),
        rrn: 0,
        flags: 0,
        locked: false,
        nulls,
    }
}

impl<I: IoSource> IoSource for DdmIo<I> {
    fn open(&mut self, spec: &OpenSpec) -> Result<(u32, FileInfo), IoFail> {
        match self.inner.open(spec) {
            Err(f) => match DdmRedirect::of(&f) {
                Some(r) => self.open_remote(spec, &r),
                None => Err(f),
            },
            ok => ok,
        }
    }

    fn io(&mut self, handle: u32, req: &IoRequest<'_>) -> IoResult {
        let Some(r) = self.remote.get_mut(&handle) else {
            return self.inner.io(handle, req);
        };
        let lock = if req.nolock { 0 } else { ddm::file::lock::UPDATE };
        match (req.op, req.key) {
            (IoOp::Read, _) => {
                let op = if r.positioned { ddm::file::op::NEXT } else { ddm::file::op::FIRST };
                r.positioned = true;
                let g = r.conn.get(&r.file, ddm::cp::s38::S38GET, op, lock, &[]);
                read_result(g, false)
            }
            (IoOp::SetLL, KeyRef::Start) => {
                r.positioned = false;
                IoResult::default()
            }
            (IoOp::Chain, KeyRef::Fields(k)) => {
                r.positioned = true;
                let g = r.conn.get_key(&r.file, k, r.keys.max(1), !req.nolock);
                read_result(g, true)
            }
            (IoOp::Write, _) => {
                let one = rec(req, r.file.fields);
                done(r.conn.put(&r.file, &[one]))
            }
            (IoOp::Update, _) => {
                let one = rec(req, r.file.fields);
                done(r.conn.update_rec(&r.file, &one))
            }
            (IoOp::Delete, _) => done(r.conn.delete_rec(&r.file)),
            (IoOp::Unlock | IoOp::Feod, _) => IoResult::default(),
            _ => status(1299, "CPF5149"),
        }
    }

    fn close(&mut self, handle: u32) -> IoResult {
        match self.remote.remove(&handle) {
            Some(mut r) => {
                let out = done(r.conn.close_file(&r.file));
                drop(r.conn.into_inner());
                out
            }
            None => self.inner.close(handle),
        }
    }

    fn commit(&mut self, rollback: bool, actgrp: &str) -> Option<Result<(), IoFail>> {
        self.inner.commit(rollback, actgrp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_a_ddm_file_goes() {
        let mut dir = RdbDirectory::installed("QS400A");
        let mut e = ddm::objects::RdbEntry::new("LOOP", "127.0.0.1", "*IP");
        e.port = "*DRDA".into();
        dir.add(e).unwrap();
        let mut f = DdmFile::new("QGPL", "RCUST", "QGPL/CUST", "*RDB", "*IP");
        f.rdb = "LOOP".into();
        assert_eq!(destination(&f, &dir).unwrap(), ("127.0.0.1".into(), 446, "LOOP".into()));
        let g = DdmFile::new("QGPL", "RCUST", "QGPL/CUST", "other.example.com", "*IP");
        assert_eq!(destination(&g, &dir).unwrap().0, "other.example.com");
        f.rdb = "NOPE".into();
        assert_eq!(destination(&f, &dir).unwrap_err().msgid, "CPF9190");
    }
}
