// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The RPC layer: `NULL` of every program, refusals, credentials and the
//! duplicate request cache.

use std::time::Duration;

use nfs_v3::wire::{NFS_PROGRAM as P, RENAME3args, createhow3, nfsstat3};
use onc_rpc_client::rpc::auth_flavor;

use super::{Family, Lines, Scenario, line};
use crate::ctx::{Ctx, prog};
use crate::error::{check, fail};
use crate::rpc::{self, Cred, Proto, Refusal};
use crate::v3::{Nfs3, Session, dirop, mode, ok, status};
use crate::wire::xdr::W;

/// This file's scenarios.
pub const LIST: [Scenario; 16] = [
    s("null-pmap", null_pmap),
    s("null-nfs", null_nfs),
    s("null-mount", null_mount),
    s("null-nlm", null_nlm),
    s("null-nsm", null_nsm),
    s("rpc-prog-unavail", prog_unavail),
    s("rpc-nfs-v2-mismatch", nfs_v2),
    s("rpc-proc-unavail", proc_unavail),
    s("rpc-garbage-args", garbage),
    s("rpc-version-mismatch", rpc_version),
    s("rpc-oversize-record", oversize),
    s("rpc-gss-refused", gss),
    s("auth-none", auth_none),
    s("auth-root-squash", root_squash),
    s("auth-unknown-uid", unknown_uid),
    s("dup-request", dup_request),
];

const fn s(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        family: Family::V3,
        run,
    }
}

fn null(c: &Ctx, addr: std::net::SocketAddr, p: u32, versions: &[u32]) -> Lines {
    let mut out = Vec::new();
    for v in versions {
        let mut cl = c.client(addr);
        cl.call_raw(p, *v, 0, &[])?;
        out.push(format!("{p} version {v}: answered"));
    }
    Ok(out)
}

fn null_pmap(c: &Ctx) -> Lines {
    null(c, c.pmap, prog::PMAP, &[2, 3, 4])
}

fn null_nfs(c: &Ctx) -> Lines {
    null(c, c.nfs, prog::NFS, &[3])
}

fn null_mount(c: &Ctx) -> Lines {
    null(c, c.mount_addr()?, prog::MOUNT, &[1, 3])
}

fn null_nlm(c: &Ctx) -> Lines {
    null(c, c.nlm_addr()?, prog::NLM, &[1, 3, 4])
}

fn null_nsm(c: &Ctx) -> Lines {
    null(c, c.nsm_addr()?, prog::NSM, &[1])
}

fn refusal(c: &Ctx, p: u32, v: u32, proc: u32, args: &[u8]) -> Result<Refusal, crate::Error> {
    match c.client(c.nfs).call_bytes(p, v, proc, args)? {
        Ok(_) => fail(format!("{p}/{v}/{proc} was answered")),
        Err(r) => Ok(r),
    }
}

fn prog_unavail(c: &Ctx) -> Lines {
    let r = refusal(c, 100_099, 1, 0, &[])?;
    check(r == Refusal::ProgUnavail, || format!("got {r}"))?;
    Ok(vec![line("program 100099", r)])
}

fn nfs_v2(c: &Ctx) -> Lines {
    let r = refusal(c, prog::NFS, 2, 0, &[])?;
    match r {
        Refusal::ProgMismatch(3, h @ (3 | 4)) => Ok(vec![
            line("NFS version 2", r),
            format!("highest version: {h}"),
        ]),
        r => fail(format!("expected PROG_MISMATCH 3..3 or 3..4, got {r}")),
    }
}

fn proc_unavail(c: &Ctx) -> Lines {
    let r = refusal(c, prog::NFS, 3, 22, &[])?;
    check(r == Refusal::ProcUnavail, || format!("got {r}"))?;
    Ok(vec![line("procedure 22", r)])
}

fn garbage(c: &Ctx) -> Lines {
    let mut out = Vec::new();
    // A handle longer than NFS3_FHSIZE, a truncated body, and a bad bool.
    let cases: [(&str, u32, Vec<u8>); 3] = [
        ("GETATTR handle of 255 bytes", 1, W::new().u32(255).done()),
        ("LOOKUP cut short", 3, W::new().u32(8).raw(&[1, 2]).done()),
        (
            "SETATTR guard bool 7",
            2,
            W::new()
                .opaque(&[0; 8])
                .u32(0)
                .u32(0)
                .u32(0)
                .u32(0)
                .u32(0)
                .u32(0)
                .u32(7)
                .done(),
        ),
    ];
    for (what, proc, args) in cases {
        let r = refusal(c, prog::NFS, 3, proc, &args)?;
        check(r == Refusal::GarbageArgs, || format!("{what}: got {r}"))?;
        out.push(line(what, r));
    }
    // Still serving.
    c.client(c.nfs).call_raw(prog::NFS, 3, 0, &[])?;
    Ok(out)
}

fn rpc_version(c: &Ctx) -> Lines {
    let mut cl = c.client(c.nfs);
    let xid = cl.next_xid();
    let msg = rpc::message(xid, 3, prog::NFS, 3, 0, &cl.cred, &[]);
    match cl.exchange(xid, &msg)? {
        Err(r @ Refusal::RpcMismatch(2, 2)) => Ok(vec![line("RPC version 3", r)]),
        Err(r) => fail(format!("expected RPC_MISMATCH 2..2, got {r}")),
        Ok(_) => fail("RPC version 3 was answered"),
    }
}

fn oversize(c: &Ctx) -> Lines {
    if c.t.proto == Proto::Udp {
        return Err(crate::Error::Skip("a TCP record only".into()));
    }
    let mut cl = c.client(c.nfs);
    // 2 MiB in 64 KiB fragments, none marked last: more than any server takes.
    let frag = vec![0u8; 64 * 1024];
    let mut head = (64u32 * 1024).to_be_bytes().to_vec();
    head.extend_from_slice(&frag);
    let mut closed = false;
    for _ in 0..32 {
        if cl.conn.send_raw(&head).is_err() {
            closed = true;
            break;
        }
    }
    let mut lines = Vec::new();
    if closed {
        lines.push("the server closed the connection while it was sent".into());
    } else {
        // The server must not answer; it closes the connection.
        cl.conn.timeout = Duration::from_secs(3);
        match cl.conn.recv() {
            Ok(b) => return fail(format!("{} bytes came back", b.len())),
            Err(e) => lines.push(line("after 2 MiB", e.kind())),
        }
    }
    let mut fresh = c.client(c.nfs);
    fresh.call_raw(prog::NFS, 3, 0, &[])?;
    lines.push("a new connection is served".into());
    Ok(lines)
}

fn gss(c: &Ctx) -> Lines {
    // An RPCSEC_GSS credential (RFC 2203 rpc_gss_cred_vers_1_t: INIT, seq 0, svc none, no handle).
    let body = W::new().u32(1).u32(1).u32(0).u32(1).opaque(&[]).done();
    let mut cl = c.client_as(c.nfs, c.t.proto, Cred::Raw(auth_flavor::RPCSEC_GSS, body));
    match cl.call_bytes(prog::NFS, 3, 0, &[])? {
        Err(r @ Refusal::Auth(_)) => Ok(vec![line("RPCSEC_GSS NULL", r)]),
        Err(r) => fail(format!("expected AUTH_ERROR, got {r}")),
        Ok(_) => fail("an RPCSEC_GSS credential was taken"),
    }
}

/// Creates a file as `cred` in a session's directory: the owner or the status.
fn create_as(
    c: &Ctx,
    s: &mut Session,
    cred: Cred,
    name: &[u8],
) -> Result<Result<u32, nfsstat3>, crate::Error> {
    // Anyone may write in the session's directory.
    let d = s.dir.clone();
    ok(
        s.n.setattr(&d, mode(0o777), nfs_v3::wire::Nfs3Option::None)?,
        "SETATTR 0777",
    )?;
    let mut n = Nfs3 {
        c: c.client_as(c.nfs, c.t.proto, cred),
    };
    let r = n.create_res(&s.dir, name, createhow3::UNCHECKED(mode(0o666)))?;
    if status(&r) != nfsstat3::NFS3_OK {
        return Ok(Err(status(&r)));
    }
    let h = n.lookup(&s.dir, name)?;
    let uid = n.getattr(&h)?.uid;
    // Its owner removes it (IFS: removing a link needs *OBJEXIST on the object).
    ok(n.remove_res(&s.dir, name)?, "REMOVE by the creator")?;
    Ok(Ok(uid))
}

fn auth_none(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "auth-none")?;
    let r = create_as(c, &mut s, Cred::None, b"by-none")?;
    s.close()?;
    match r {
        Ok(uid) => {
            check(uid != 0, || {
                "a file made with AUTH_NONE is owned by 0".into()
            })?;
            Ok(vec![line("AUTH_NONE created a file owned by", uid)])
        }
        Err(st) => Ok(vec![line("AUTH_NONE create", st)]),
    }
}

fn root_squash(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "auth-root")?;
    let r = create_as(c, &mut s, Cred::sys(0, 0), b"by-root")?;
    s.close()?;
    match r {
        Ok(uid) if c.t.expect_root => {
            check(uid == 0, || {
                format!("root not squashed expected, owner {uid}")
            })?;
            Ok(vec![line("owner", uid)])
        }
        Ok(uid) => {
            check(uid != 0, || "UID 0 was not squashed".into())?;
            Ok(vec![line("UID 0 squashed to", uid)])
        }
        Err(st) => fail(format!("CREATE as UID 0: {st}")),
    }
}

fn unknown_uid(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "auth-unknown")?;
    let r = create_as(c, &mut s, Cred::sys(3_999_999, 3_999_999), b"by-unknown")?;
    s.close()?;
    Ok(vec![match r {
        Ok(uid) => line("UID 3999999 created a file owned by", uid),
        Err(st) => line("UID 3999999 create", st),
    }])
}

fn dup_request(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "dup-request")?;
    let dir = s.dir.clone();
    s.n.create(&dir, b"a", 0o644)?;
    let args = crate::rpc::pack(&RENAME3args {
        from: dirop(&dir, b"a"),
        to: dirop(&dir, b"b"),
    })?;
    let mut lines = Vec::new();
    let cl = &mut s.n.c;
    let xid = cl.next_xid();
    let msg = cl.message(xid, prog::NFS, 3, P::NFSPROC3_RENAME as u32, &args);
    let first = cl.exchange(xid, &msg)?.map_err(crate::Error::Refused)?;
    let second = cl.exchange(xid, &msg)?.map_err(crate::Error::Refused)?;
    let st = |b: &[u8]| u32::from_be_bytes(b[..4].try_into().expect("status"));
    check(st(&first) == 0, || format!("RENAME: status {}", st(&first)))?;
    check(st(&second) == 0, || {
        format!(
            "RENAME sent again with the same XID: status {} (not the kept reply)",
            st(&second)
        )
    })?;
    lines.push(line("RENAME twice, same XID", "both NFS3_OK"));
    lines.push(line("replies identical", first == second));
    let args = crate::rpc::pack(&nfs_v3::wire::REMOVE3args {
        object: dirop(&dir, b"b"),
    })?;
    let xid = cl.next_xid();
    let msg = cl.message(xid, prog::NFS, 3, P::NFSPROC3_REMOVE as u32, &args);
    let first = cl.exchange(xid, &msg)?.map_err(crate::Error::Refused)?;
    let second = cl.exchange(xid, &msg)?.map_err(crate::Error::Refused)?;
    check(st(&first) == 0 && st(&second) == 0, || {
        format!("REMOVE twice: statuses {} and {}", st(&first), st(&second))
    })?;
    lines.push(line("REMOVE twice, same XID", "both NFS3_OK"));
    // A new XID is a new request.
    let r = s.n.remove_res(&dir, b"b")?;
    check(status(&r) == nfsstat3::NFS3ERR_NOENT, || {
        format!("REMOVE with a new XID: {}", status(&r))
    })?;
    ok(s.n.lookup_res(&dir, b".")?, "LOOKUP .").map(|_| ())?;
    s.close()?;
    Ok(lines)
}
