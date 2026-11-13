// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A scripted NFS version 4 responder, written from RFC 7530 and 8881 for
//! these tests only: client IDs, sessions with one slot and its reply
//! cache, a pseudo file system where every name exists as a directory,
//! and refusals (minor version 2, `OP_ILLEGAL`, `SEQUENCE` rules, pNFS,
//! the ACL attribute). No file data: the file scenarios wait for the real
//! server.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use nfstest::rpc::transport::read_record;
use nfstest::wire::v4::{op, st};
use nfstest::wire::v4attr::{self, a};
use nfstest::wire::xdr::{R, W};
use onc_rpc_client::rpc::{
    accept_stat_data, accepted_reply, msg_body, opaque_auth, reply_body, rpc_msg,
};
use onc_xdr::{Pack, Unpack};

#[derive(Default)]
struct State {
    next: u64,
    clients: HashMap<u64, bool>,
    sessions: HashMap<[u8; 16], Slot>,
}

struct Slot {
    clientid: u64,
    seq: u32,
    cached: Vec<u8>,
}

/// The responder.
pub struct Fake {
    /// Its TCP port.
    pub port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Starts it on a loopback port.
pub fn start() -> Fake {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    l.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let st = Arc::new(Mutex::new(State {
        next: 0x1000,
        ..State::default()
    }));
    let s2 = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        while !s2.load(Ordering::Relaxed) {
            match l.accept() {
                Ok((c, _)) => {
                    let st = Arc::clone(&st);
                    std::thread::spawn(move || conn(c, &st));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
    });
    Fake {
        port,
        stop,
        thread: Some(thread),
    }
}

fn conn(mut c: TcpStream, st: &Mutex<State>) {
    let _ = c.set_nonblocking(false);
    while let Ok(m) = read_record(&mut c) {
        let Some(reply) = answer(&m, st) else {
            continue;
        };
        let mut rec = (reply.len() as u32 | 0x8000_0000).to_be_bytes().to_vec();
        rec.extend_from_slice(&reply);
        if c.write_all(&rec).is_err() {
            return;
        }
    }
}

fn answer(m: &[u8], st: &Mutex<State>) -> Option<Vec<u8>> {
    let mut c = Cursor::new(m);
    let (msg, _) = rpc_msg::unpack(&mut c).ok()?;
    let msg_body::CALL(call) = msg.body else {
        return None;
    };
    let args = &m[c.position() as usize..];
    let (data, results) = match (call.prog, call.vers, call.proc) {
        (100_003, 4, 0) => (accept_stat_data::SUCCESS, Vec::new()),
        (100_003, 4, 1) => match compound(args, st) {
            Some(r) => (accept_stat_data::SUCCESS, r),
            None => (accept_stat_data::GARBAGE_ARGS, Vec::new()),
        },
        (100_003, 4, _) => (accept_stat_data::PROC_UNAVAIL, Vec::new()),
        (100_003, _, _) => (
            accept_stat_data::PROG_MISMATCH { low: 4, high: 4 },
            Vec::new(),
        ),
        _ => (accept_stat_data::PROG_UNAVAIL, Vec::new()),
    };
    let r = rpc_msg {
        xid: msg.xid,
        body: msg_body::REPLY(reply_body::MSG_ACCEPTED(accepted_reply {
            verf: opaque_auth::default(),
            reply_data: data,
        })),
    };
    let mut out = Vec::new();
    r.pack(&mut out).ok()?;
    out.extend_from_slice(&results);
    Some(out)
}

/// The supported attributes: the required ones, `fileid` and `mode`; not `acl`.
fn supported() -> Vec<u32> {
    let mut v = v4attr::REQUIRED.to_vec();
    v.extend([a::FILEID, a::MODE]);
    v
}

fn attrs(asked: &[u32], fh: &[u8]) -> Vec<u8> {
    let sup = supported();
    let give: Vec<u32> = v4attr::numbers(&v4attr::bitmap(asked))
        .into_iter()
        .filter(|n| sup.contains(n))
        .collect();
    let mut vals = W::new();
    for n in &give {
        match *n {
            a::SUPPORTED_ATTRS => v4attr::put_bitmap(&mut vals, &v4attr::bitmap(&sup)),
            a::TYPE => {
                vals.u32(2);
            }
            a::FH_EXPIRE_TYPE | a::RDATTR_ERROR => {
                vals.u32(0);
            }
            a::CHANGE | a::SIZE => {
                vals.u64(4096);
            }
            a::LINK_SUPPORT | a::SYMLINK_SUPPORT | a::UNIQUE_HANDLES => {
                vals.bool(true);
            }
            a::NAMED_ATTR => {
                vals.bool(false);
            }
            a::FSID => {
                vals.u64(u64::from(fh.len() > 4)).u64(0);
            }
            a::LEASE_TIME => {
                vals.u32(90);
            }
            a::FILEHANDLE => {
                vals.opaque(fh);
            }
            a::FILEID => {
                vals.u64(fh.len() as u64);
            }
            a::MODE => {
                vals.u32(0o755);
            }
            _ => unreachable!(),
        }
    }
    let mut w = W::new();
    v4attr::put_bitmap(&mut w, &v4attr::bitmap(&give));
    w.opaque(&vals.done());
    w.done()
}

fn skip_channel(r: &mut R<'_>) -> Option<()> {
    for _ in 0..6 {
        r.u32().ok()?;
    }
    let n = r.u32().ok()?;
    for _ in 0..n {
        r.u32().ok()?;
    }
    Some(())
}

fn put_channel(w: &mut W) {
    w.u32(0)
        .u32(1 << 20)
        .u32(1 << 20)
        .u32(8192)
        .u32(16)
        .u32(1)
        .u32(0);
}

#[allow(clippy::too_many_lines)]
fn compound(args: &[u8], st: &Mutex<State>) -> Option<Vec<u8>> {
    let mut r = R::new(args);
    let tag = r.opaque(1024).ok()?.to_vec();
    let minor = r.u32().ok()?;
    let n = r.u32().ok()?;
    let head = |status: u32, results: &[Vec<u8>]| {
        let mut w = W::new();
        w.u32(status).opaque(&tag).u32(results.len() as u32);
        for x in results {
            w.raw(x);
        }
        w.done()
    };
    if minor > 1 {
        return Some(head(st::MINOR_VERS_MISMATCH, &[]));
    }
    let mut s = st.lock().unwrap();
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut status = st::OK;
    let mut fh: Option<Vec<u8>> = None;
    let mut session: Option<[u8; 16]> = None;
    for i in 0..n {
        let opn = r.u32().ok()?;
        let mut w = W::new();
        let res = |w: &mut W, op: u32, s: u32| {
            w.u32(op).u32(s);
            s
        };
        // 4.1: SEQUENCE first, but for the operations that make or end a session.
        if minor == 1
            && i == 0
            && !matches!(
                opn,
                op::SEQUENCE
                    | op::EXCHANGE_ID
                    | op::CREATE_SESSION
                    | op::DESTROY_SESSION
                    | op::DESTROY_CLIENTID
            )
        {
            status = res(&mut w, opn, st::OP_NOT_IN_SESSION);
            out.push(w.done());
            break;
        }
        match (opn, minor) {
            (op::PUTROOTFH, _) => {
                fh = Some(b"/".to_vec());
                status = res(&mut w, opn, st::OK);
            }
            (op::PUTFH, _) => {
                fh = Some(r.opaque(128).ok()?.to_vec());
                status = res(&mut w, opn, st::OK);
            }
            (op::LOOKUP, _) => {
                let name = r.opaque(1024).ok()?.to_vec();
                let Some(cur) = fh.as_mut() else {
                    status = res(&mut w, opn, 10020);
                    out.push(w.done());
                    break;
                };
                if cur.last() != Some(&b'/') {
                    cur.push(b'/');
                }
                cur.extend_from_slice(&name);
                status = res(&mut w, opn, st::OK);
            }
            (op::GETFH, _) => {
                status = res(&mut w, opn, st::OK);
                w.opaque(fh.as_deref().unwrap_or_default());
            }
            (op::GETATTR, _) => {
                let bm = v4attr::get_bitmap(&mut r).ok()?;
                status = res(&mut w, opn, st::OK);
                w.raw(&attrs(
                    &v4attr::numbers(&bm),
                    fh.as_deref().unwrap_or_default(),
                ));
            }
            (op::SETCLIENTID, 0) => {
                r.fixed(8).ok()?;
                r.opaque(1024).ok()?;
                r.u32().ok()?;
                r.opaque(1024).ok()?;
                r.opaque(1024).ok()?;
                r.u32().ok()?;
                s.next += 1;
                let id = s.next;
                s.clients.insert(id, false);
                status = res(&mut w, opn, st::OK);
                w.u64(id).fixed(b"confirm!");
            }
            (op::SETCLIENTID_CONFIRM, 0) => {
                let id = r.u64().ok()?;
                let v = r.fixed(8).ok()?;
                let ok = v == b"confirm!" && s.clients.contains_key(&id);
                if ok {
                    s.clients.insert(id, true);
                }
                status = res(&mut w, opn, if ok { st::OK } else { 10022 });
            }
            (op::RENEW, 0) => {
                let id = r.u64().ok()?;
                let known = s.clients.get(&id) == Some(&true);
                status = res(&mut w, opn, if known { st::OK } else { 10022 });
            }
            (op::EXCHANGE_ID, 1) => {
                r.fixed(8).ok()?;
                r.opaque(1024).ok()?;
                r.u32().ok()?;
                if r.u32().ok()? != 0 {
                    return None;
                }
                let impls = r.u32().ok()?;
                for _ in 0..impls {
                    r.opaque(1024).ok()?;
                    r.opaque(1024).ok()?;
                    r.u64().ok()?;
                    r.u32().ok()?;
                }
                s.next += 1;
                let id = s.next;
                s.clients.insert(id, false);
                status = res(&mut w, opn, st::OK);
                // clientid, sequenceid, USE_NON_PNFS, SP4_NONE, server owner, scope, no impl ID.
                w.u64(id)
                    .u32(1)
                    .u32(0x0001_0000)
                    .u32(0)
                    .u64(1)
                    .opaque(b"fake")
                    .opaque(b"fake")
                    .u32(0);
            }
            (op::CREATE_SESSION, 1) => {
                let id = r.u64().ok()?;
                let seq = r.u32().ok()?;
                r.u32().ok()?;
                skip_channel(&mut r)?;
                skip_channel(&mut r)?;
                r.u32().ok()?;
                let k = r.u32().ok()?;
                for _ in 0..k {
                    if r.u32().ok()? != 0 {
                        return None;
                    }
                }
                if !s.clients.contains_key(&id) {
                    status = res(&mut w, opn, 10022);
                    out.push(w.done());
                    break;
                }
                s.clients.insert(id, true);
                let mut sid = [0u8; 16];
                sid[..8].copy_from_slice(&id.to_be_bytes());
                sid[8..12].copy_from_slice(&seq.to_be_bytes());
                s.sessions.insert(
                    sid,
                    Slot {
                        clientid: id,
                        seq: 0,
                        cached: Vec::new(),
                    },
                );
                status = res(&mut w, opn, st::OK);
                w.fixed(&sid).u32(seq).u32(0);
                put_channel(&mut w);
                put_channel(&mut w);
            }
            (op::SEQUENCE, 1) => {
                let sid: [u8; 16] = r.fixed(16).ok()?.try_into().ok()?;
                let seq = r.u32().ok()?;
                let slot = r.u32().ok()?;
                r.u32().ok()?;
                r.bool().ok()?;
                let Some(sl) = s.sessions.get(&sid) else {
                    status = res(&mut w, opn, 10052);
                    out.push(w.done());
                    break;
                };
                if slot != 0 {
                    status = res(&mut w, opn, 10053);
                    out.push(w.done());
                    break;
                }
                if seq == sl.seq && !sl.cached.is_empty() {
                    return Some(sl.cached.clone());
                }
                if seq != sl.seq.wrapping_add(1) {
                    status = res(&mut w, opn, st::SEQ_MISORDERED);
                    out.push(w.done());
                    break;
                }
                session = Some(sid);
                status = res(&mut w, opn, st::OK);
                w.fixed(&sid).u32(seq).u32(0).u32(0).u32(0).u32(0);
            }
            (op::RECLAIM_COMPLETE, 1) => {
                r.bool().ok()?;
                status = res(&mut w, opn, st::OK);
            }
            (op::DESTROY_SESSION, 1) => {
                let sid: [u8; 16] = r.fixed(16).ok()?.try_into().ok()?;
                let found = s.sessions.remove(&sid).is_some();
                status = res(&mut w, opn, if found { st::OK } else { 10052 });
            }
            (op::DESTROY_CLIENTID, 1) => {
                let id = r.u64().ok()?;
                let found = s.clients.remove(&id).is_some();
                s.sessions.retain(|_, v| v.clientid != id);
                status = res(&mut w, opn, if found { st::OK } else { 10022 });
            }
            (op::LAYOUTGET, 1) => {
                // Arguments read to the end; pNFS is refused.
                r.bool().ok()?;
                for _ in 0..2 {
                    r.u32().ok()?;
                }
                for _ in 0..3 {
                    r.u64().ok()?;
                }
                r.u32().ok()?;
                r.fixed(12).ok()?;
                r.u32().ok()?;
                status = res(&mut w, opn, st::NOTSUPP);
            }
            _ => {
                // Not an operation of this minor version, or not one at all.
                status = res(&mut w, op::ILLEGAL, st::OP_ILLEGAL);
            }
        }
        out.push(w.done());
        if status != st::OK {
            break;
        }
    }
    let reply = head(status, &out);
    if let Some(sid) = session
        && let Some(sl) = s.sessions.get_mut(&sid)
    {
        sl.seq = sl.seq.wrapping_add(1);
        sl.cached.clone_from(&reply);
    }
    Some(reply)
}
