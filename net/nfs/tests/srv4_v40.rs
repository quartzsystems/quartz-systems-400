// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFSv4.0: a client's life (`SETCLIENTID` to `CLOSE`), seqid replays,
//! the lease running out, `RENEW`, the file operations, share
//! reservations between clients and with a local job, root squash and
//! read-only exports.

#![allow(clippy::too_many_lines)]

mod srv3_common;
mod srv4_common;

use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_RDWR, O_SHARE_NONE, O_WRONLY};
use nfs::server::HostPat;
use nfs::types::nfs4::{
    self as t, OPEN4_RESULT_CONFIRM, OPEN4_SHARE_ACCESS_BOTH, OPEN4_SHARE_ACCESS_READ,
    OPEN4_SHARE_ACCESS_WRITE, OPEN4_SHARE_DENY_NONE, OPEN4_SHARE_DENY_WRITE, nfs_argop4,
    nfs_resop4, nfsstat4,
};
use nfs::v4::attr::Bits;
use nfs::xdr::Decoder;
use srv3_common::World;
use srv4_common::{
    Env, V4c, anon, close, fh_of, getattr, getfh, lookup, opened, putfh, read, rootfh, statuses,
    write,
};

fn env() -> Env {
    let w = World::new();
    let e = w.export(1, "/home/x");
    w.put(e);
    Env::new(w)
}

/// The export's root handle.
fn root(e: &Env, c: &V4c) -> Vec<u8> {
    let r = c.raw(e, 0, vec![rootfh(), lookup("home"), lookup("x"), getfh()]);
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    fh_of(&r)
}

/// Opens `name` (made when `create`), confirms: the handle and stateid.
fn open_confirm(
    e: &Env,
    c: &V4c,
    dir: &[u8],
    owner: &str,
    name: &str,
    create: bool,
    access: u32,
    deny: u32,
) -> Result<(Vec<u8>, t::stateid4), nfsstat4> {
    let r = c.raw(
        e,
        0,
        vec![putfh(dir), c.open(owner, name, create, access, deny), getfh()],
    );
    if r.status != nfsstat4::NFS4_OK {
        return Err(r.status);
    }
    let o = opened(&r);
    let fh = fh_of(&r);
    let mut sid = o.stateid.clone();
    if o.rflags & OPEN4_RESULT_CONFIRM != 0 {
        let r = c.raw(
            e,
            0,
            vec![
                putfh(&fh),
                nfs_argop4::OP_OPEN_CONFIRM(t::OPEN_CONFIRM4args {
                    open_stateid: sid.clone(),
                    seqid: c.next_oseq(),
                }),
            ],
        );
        assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
        let nfs_resop4::OP_OPEN_CONFIRM(t::OPEN_CONFIRM4res::NFS4_OK(ok)) = &r.resarray[1] else {
            panic!()
        };
        sid = ok.open_stateid.clone();
    }
    Ok((fh, sid))
}

#[test]
fn a_client_life() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 0);
    let id = c.setclientid(&e, "linux-a", "127.0.0.1.0.1");
    let dir = root(&e, &c);
    // An open that makes the file needs confirming.
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&dir),
            c.open("o1", "f", true, OPEN4_SHARE_ACCESS_BOTH, OPEN4_SHARE_DENY_NONE),
            getfh(),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let o = opened(&r);
    assert_ne!(o.rflags & OPEN4_RESULT_CONFIRM, 0);
    assert_ne!(o.cinfo.before, o.cinfo.after);
    let f = fh_of(&r);
    // Not yet confirmed: its stateid is not usable.
    let r = c.raw(&e, 0, vec![putfh(&f), write(&o.stateid, 0, b"x")]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_BAD_STATEID);
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&f),
            nfs_argop4::OP_OPEN_CONFIRM(t::OPEN_CONFIRM4args {
                open_stateid: o.stateid.clone(),
                seqid: c.next_oseq(),
            }),
        ],
    );
    let nfs_resop4::OP_OPEN_CONFIRM(t::OPEN_CONFIRM4res::NFS4_OK(ok)) = &r.resarray[1] else {
        panic!("{r:?}");
    };
    let sid = ok.open_stateid.clone();
    assert_eq!(sid.seqid, 2);
    // The old seqid of the stateid is old.
    let r = c.raw(&e, 0, vec![putfh(&f), write(&o.stateid, 0, b"x")]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_OLD_STATEID);
    let r = c.raw(&e, 0, vec![putfh(&f), write(&sid, 0, b"hello, world")]);
    let nfs_resop4::OP_WRITE(t::WRITE4res::NFS4_OK(w)) = &r.resarray[1] else {
        panic!("{r:?}");
    };
    assert_eq!(w.count, 12);
    assert_eq!(w.writeverf, e.w.instance.verf());
    let r = c.raw(&e, 0, vec![putfh(&f), read(&sid, 7, 100)]);
    let nfs_resop4::OP_READ(t::READ4res::NFS4_OK(rd)) = &r.resarray[1] else {
        panic!("{r:?}");
    };
    assert_eq!(rd.data, b"world");
    assert!(rd.eof);
    assert_eq!(e.w.read_file("/home/x/f"), b"hello, world");
    // The file is BOB's.
    let r = c.raw(&e, 0, vec![putfh(&f), getattr(&[t::FATTR4_OWNER, t::FATTR4_MODE])]);
    let a = srv4_common::attrs_of(&r);
    let mut d = Decoder::new(&a.attr_vals.0);
    // No primary group: its bits are 0.
    assert_eq!(d.get_u32().unwrap() & 0o777, 0o604);
    assert_eq!(d.get_opaque(u32::MAX).unwrap(), b"BOB@example.com");
    // A size through the open.
    let mut v = nfs::xdr::Encoder::new();
    v.put_u64(5);
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&f),
            nfs_argop4::OP_SETATTR(t::SETATTR4args {
                stateid: sid.clone(),
                obj_attributes: t::fattr4 {
                    attrmask: Bits::of(&[t::FATTR4_SIZE]).to_bitmap(),
                    attr_vals: t::attrlist4(v.into_bytes()),
                },
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    assert_eq!(e.w.read_file("/home/x/f"), b"hello");
    // RENEW, CLOSE.
    let r = c.raw(&e, 0, vec![nfs_argop4::OP_RENEW(t::RENEW4args { clientid: id })]);
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    let cs = c.next_oseq();
    let r = c.raw(&e, 0, vec![putfh(&f), close(cs, &sid)]);
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    // A retransmitted CLOSE gets the same answer.
    let again = c.raw(&e, 0, vec![putfh(&f), close(cs, &sid)]);
    assert_eq!(again.resarray, r.resarray);
    // A seqid out of order.
    let r = c.raw(&e, 0, vec![putfh(&dir), srv4_common::open_args(99, c.owner("o1"), "f", false, 1, 0)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_BAD_SEQID);
    // An unknown client ID of this start, and one of another start.
    let r = c.raw(&e, 0, vec![nfs_argop4::OP_RENEW(t::RENEW4args { clientid: id + 1 })]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_STALE_CLIENTID);
    let r = c.raw(&e, 0, vec![nfs_argop4::OP_RENEW(t::RENEW4args { clientid: 7 })]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_STALE_CLIENTID);
}

#[test]
fn open_retransmission_gets_its_reply() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 0);
    c.setclientid(&e, "linux-b", "127.0.0.1.0.1");
    let dir = root(&e, &c);
    let op = c.open("o", "g", true, OPEN4_SHARE_ACCESS_READ, OPEN4_SHARE_DENY_NONE);
    let r1 = c.raw(&e, 0, vec![putfh(&dir), op.clone(), getfh()]);
    let r2 = c.raw(&e, 0, vec![putfh(&dir), op, getfh()]);
    assert_eq!(r1, r2, "the same stateid, and the file's handle");
}

#[test]
fn setclientid_rules() {
    let e = env();
    let a = V4c::new("10.0.0.1:900", 100, 0);
    let id = a.setclientid(&e, "shared-id", "127.0.0.1.0.1");
    let dir = root(&e, &a);
    open_confirm(&e, &a, &dir, "o", "h", true, 3, 0).unwrap();
    // Another principal with the same ID while state is held: in use.
    let b = V4c::new("10.0.0.2:900", 102, 0);
    let r = b.raw(
        &e,
        0,
        vec![nfs_argop4::OP_SETCLIENTID(t::SETCLIENTID4args {
            client: t::nfs_client_id4 {
                verifier: [9; 8],
                id: b"shared-id".to_vec(),
            },
            callback: t::cb_client4 {
                cb_program: 1,
                cb_location: t::netaddr4 {
                    na_r_netid: b"tcp".to_vec(),
                    na_r_addr: b"10.0.0.2.0.1".to_vec(),
                },
            },
            callback_ident: 1,
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_CLID_INUSE);
    assert!(matches!(
        &r.resarray[0],
        nfs_resop4::OP_SETCLIENTID(t::SETCLIENTID4res::NFS4ERR_CLID_INUSE(_))
    ));
    // The same client restarting (a new verifier): its old state goes
    // when the new ID is confirmed.
    let a2 = V4c::new("10.0.0.1:901", 100, 0);
    let r = a2.raw(
        &e,
        0,
        vec![nfs_argop4::OP_SETCLIENTID(t::SETCLIENTID4args {
            client: t::nfs_client_id4 {
                verifier: [5; 8],
                id: b"shared-id".to_vec(),
            },
            callback: t::cb_client4 {
                cb_program: 1,
                cb_location: t::netaddr4 {
                    na_r_netid: b"tcp".to_vec(),
                    na_r_addr: b"127.0.0.1.0.1".to_vec(),
                },
            },
            callback_ident: 1,
        })],
    );
    let nfs_resop4::OP_SETCLIENTID(t::SETCLIENTID4res::NFS4_OK(ok)) = &r.resarray[0] else {
        panic!("{r:?}")
    };
    assert_ne!(ok.clientid, id);
    let bad = a2.raw(
        &e,
        0,
        vec![nfs_argop4::OP_SETCLIENTID_CONFIRM(t::SETCLIENTID_CONFIRM4args {
            clientid: ok.clientid,
            setclientid_confirm: [0; 8],
        })],
    );
    assert_eq!(bad.status, nfsstat4::NFS4ERR_STALE_CLIENTID);
    let r = a2.raw(
        &e,
        0,
        vec![nfs_argop4::OP_SETCLIENTID_CONFIRM(t::SETCLIENTID_CONFIRM4args {
            clientid: ok.clientid,
            setclientid_confirm: ok.setclientid_confirm,
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    let r = a.raw(&e, 0, vec![nfs_argop4::OP_RENEW(t::RENEW4args { clientid: id })]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_STALE_CLIENTID);
    // Its file is no longer open: a local job may deny everyone.
    let j = e.w.job(5);
    let fd = j
        .open("/home/x/h", OpenArgs::new(O_RDWR | O_SHARE_NONE))
        .unwrap();
    j.close(fd).unwrap();
}

#[test]
fn the_lease_runs_out() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 0);
    let id = c.setclientid(&e, "lapse", "127.0.0.1.0.1");
    let dir = root(&e, &c);
    let (f, sid) = open_confirm(&e, &c, &dir, "o", "l", true, 3, 0).unwrap();
    // Lapsed, but nothing needs its state: still there (courtesy).
    e.advance(100);
    e.tick();
    let r = c.raw(&e, 0, vec![putfh(&f), read(&sid, 0, 10)]);
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    // Past the courtesy bound it goes.
    e.advance(400);
    e.tick();
    let r = c.raw(&e, 0, vec![putfh(&f), read(&sid, 0, 10)]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_EXPIRED);
    let r = c.raw(&e, 0, vec![nfs_argop4::OP_RENEW(t::RENEW4args { clientid: id })]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_STALE_CLIENTID);
}

#[test]
fn a_lapsed_client_gives_way_to_a_conflict() {
    let e = env();
    let a = V4c::new("10.0.0.1:900", 100, 0);
    a.setclientid(&e, "lapse-a", "127.0.0.1.0.1");
    let dir = root(&e, &a);
    open_confirm(&e, &a, &dir, "o", "c", true, 3, OPEN4_SHARE_DENY_WRITE).unwrap();
    let b = V4c::new("10.0.0.2:900", 100, 0);
    b.setclientid(&e, "lapse-b", "127.0.0.1.0.1");
    assert_eq!(
        open_confirm(&e, &b, &dir, "o", "c", false, OPEN4_SHARE_ACCESS_WRITE, 0).err(),
        Some(nfsstat4::NFS4ERR_SHARE_DENIED)
    );
    e.advance(100);
    // b renews; a does not.
    b.raw(&e, 0, vec![nfs_argop4::OP_RENEW(t::RENEW4args { clientid: b.clientid.get() })]);
    assert!(open_confirm(&e, &b, &dir, "o", "c", false, OPEN4_SHARE_ACCESS_WRITE, 0).is_ok());
}

#[test]
fn share_reservations() {
    let e = env();
    let a = V4c::new("10.0.0.1:900", 100, 0);
    a.setclientid(&e, "share-a", "127.0.0.1.0.1");
    let b = V4c::new("10.0.0.2:900", 100, 0);
    b.setclientid(&e, "share-b", "127.0.0.1.0.1");
    let dir = root(&e, &a);
    let (f, sa) = open_confirm(&e, &a, &dir, "o", "s", true, OPEN4_SHARE_ACCESS_READ, OPEN4_SHARE_DENY_WRITE).unwrap();
    // b may read but not write.
    let (_, sb) =
        open_confirm(&e, &b, &dir, "o", "s", false, OPEN4_SHARE_ACCESS_READ, 0).unwrap();
    assert_eq!(
        open_confirm(&e, &b, &dir, "o2", "s", false, OPEN4_SHARE_ACCESS_WRITE, 0).err(),
        Some(nfsstat4::NFS4ERR_SHARE_DENIED)
    );
    // A write with the anonymous stateid is locked out; one with a read
    // open is the wrong mode.
    let r = b.raw(&e, 0, vec![putfh(&f), write(&anon(), 0, b"x")]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_LOCKED);
    let r = b.raw(&e, 0, vec![putfh(&f), write(&sb, 0, b"x")]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_OPENMODE);
    // A local job may not open it for writing either.
    let j = e.w.job(5);
    assert!(j.open("/home/x/s", OpenArgs::new(O_WRONLY)).is_err());
    // An upgrade of a's open to read and write, then a downgrade back.
    let r = a.raw(&e, 0, vec![putfh(&dir), a.open("o", "s", false, OPEN4_SHARE_ACCESS_BOTH, OPEN4_SHARE_DENY_WRITE)]);
    let up = opened(&r);
    assert_eq!(up.stateid.other, sa.other);
    assert_eq!(up.stateid.seqid, sa.seqid + 1);
    let r = a.raw(
        &e,
        0,
        vec![
            putfh(&f),
            nfs_argop4::OP_OPEN_DOWNGRADE(t::OPEN_DOWNGRADE4args {
                open_stateid: up.stateid.clone(),
                seqid: a.next_oseq(),
                share_access: OPEN4_SHARE_ACCESS_READ,
                share_deny: OPEN4_SHARE_DENY_NONE,
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let nfs_resop4::OP_OPEN_DOWNGRADE(t::OPEN_DOWNGRADE4res::NFS4_OK(dg)) = &r.resarray[1] else {
        panic!()
    };
    // Now b may write; a downgrade to what was never asked is refused.
    assert!(open_confirm(&e, &b, &dir, "o3", "s", false, OPEN4_SHARE_ACCESS_WRITE, 0).is_ok());
    let r = a.raw(
        &e,
        0,
        vec![
            putfh(&f),
            nfs_argop4::OP_OPEN_DOWNGRADE(t::OPEN_DOWNGRADE4args {
                open_stateid: dg.open_stateid.clone(),
                seqid: a.next_oseq(),
                share_access: OPEN4_SHARE_ACCESS_WRITE,
                share_deny: OPEN4_SHARE_DENY_NONE,
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_INVAL);
}

#[test]
fn a_local_job_holding_a_deny() {
    let e = env();
    e.w.write_file("/home/x/busy", b"data");
    let j = e.w.job(5);
    let fd = j
        .open("/home/x/busy", OpenArgs::new(O_RDWR | O_SHARE_NONE))
        .unwrap();
    let c = V4c::new("10.0.0.1:900", 100, 0);
    c.setclientid(&e, "local", "127.0.0.1.0.1");
    let dir = root(&e, &c);
    assert_eq!(
        open_confirm(&e, &c, &dir, "o", "busy", false, OPEN4_SHARE_ACCESS_READ, 0).err(),
        Some(nfsstat4::NFS4ERR_SHARE_DENIED)
    );
    j.close(fd).unwrap();
    assert!(open_confirm(&e, &c, &dir, "o", "busy", false, OPEN4_SHARE_ACCESS_READ, 0).is_ok());
}

#[test]
fn directory_operations() {
    let e = env();
    let c = V4c::new("10.0.0.1:900", 100, 0);
    let dir = root(&e, &c);
    let mk = |name: &str, ty: t::createtype4| {
        c.raw(
            &e,
            0,
            vec![
                putfh(&dir),
                nfs_argop4::OP_CREATE(t::CREATE4args {
                    objtype: ty,
                    objname: t::utf8string(name.as_bytes().to_vec()),
                    createattrs: srv4_common::mode_attrs(0o755),
                }),
                getfh(),
            ],
        )
    };
    let r = mk("d", t::createtype4::NF4DIR);
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let r = mk("ln", t::createtype4::NF4LNK(t::utf8string(b"d".to_vec())));
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let ln = fh_of(&r);
    let r = c.raw(&e, 0, vec![putfh(&ln), nfs_argop4::OP_READLINK]);
    let nfs_resop4::OP_READLINK(t::READLINK4res::NFS4_OK(l)) = &r.resarray[1] else {
        panic!("{r:?}")
    };
    assert_eq!(l.link.0, b"d");
    // LOOKUP through a link is SYMLINK.
    let r = c.raw(&e, 0, vec![putfh(&ln), lookup("x")]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_SYMLINK);
    assert_eq!(mk("r", t::createtype4::default(t::nfs_ftype4::NF4REG)).status, nfsstat4::NFS4ERR_BADTYPE);
    assert_eq!(mk("d", t::createtype4::NF4DIR).status, nfsstat4::NFS4ERR_EXIST);
    assert_eq!(mk("..", t::createtype4::NF4DIR).status, nfsstat4::NFS4ERR_BADNAME);
    assert_eq!(mk("", t::createtype4::NF4DIR).status, nfsstat4::NFS4ERR_INVAL);
    e.w.write_file("/home/x/a", b"1");
    j9(&e).chown("/home/x/a", 100, 0).unwrap();
    // RENAME and LINK between the saved and the current directory.
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&dir),
            nfs_argop4::OP_SAVEFH,
            lookup("d"),
            nfs_argop4::OP_RENAME(t::RENAME4args {
                oldname: t::utf8string(b"a".to_vec()),
                newname: t::utf8string(b"b".to_vec()),
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    assert_eq!(e.w.read_file("/home/x/d/b"), b"1");
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&dir),
            lookup("d"),
            lookup("b"),
            nfs_argop4::OP_SAVEFH,
            putfh(&dir),
            nfs_argop4::OP_LINK(t::LINK4args {
                newname: t::utf8string(b"hard".to_vec()),
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&dir),
            nfs_argop4::OP_REMOVE(t::REMOVE4args {
                target: t::utf8string(b"d".to_vec()),
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOTEMPTY);
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&dir),
            nfs_argop4::OP_REMOVE(t::REMOVE4args {
                target: t::utf8string(b"hard".to_vec()),
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    // RESTOREFH with nothing saved.
    let r = c.raw(&e, 0, vec![nfs_argop4::OP_RESTOREFH]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_RESTOREFH);
    // ACCESS.
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&dir),
            nfs_argop4::OP_ACCESS(t::ACCESS4args { access: 0x3f }),
        ],
    );
    let nfs_resop4::OP_ACCESS(t::ACCESS4res::NFS4_OK(a)) = &r.resarray[1] else {
        panic!("{r:?}")
    };
    assert_eq!(a.supported, 0x1f, "no EXECUTE on a directory");
    assert_ne!(a.access & 2, 0);
}

#[test]
fn readdir_pages_while_the_directory_changes() {
    let e = env();
    let j = e.w.job(9);
    for i in 0..300 {
        e.w.write_file(&format!("/home/x/f{i:04}"), b"");
    }
    let c = V4c::new("10.0.0.1:900", 100, 0);
    let dir = root(&e, &c);
    let mut seen = std::collections::BTreeSet::new();
    let (mut cookie, mut verf) = (0u64, [0u8; 8]);
    let mut round = 0;
    loop {
        let r = c.raw(
            &e,
            0,
            vec![
                putfh(&dir),
                nfs_argop4::OP_READDIR(t::READDIR4args {
                    cookie,
                    cookieverf: verf,
                    dircount: 512,
                    maxcount: 2048,
                    attr_request: Bits::of(&[t::FATTR4_FILEID, t::FATTR4_RDATTR_ERROR]).to_bitmap(),
                }),
            ],
        );
        let nfs_resop4::OP_READDIR(t::READDIR4res::NFS4_OK(ok)) = &r.resarray[1] else {
            panic!("{r:?}")
        };
        for en in &ok.reply.entries {
            assert!(seen.insert(en.name.0.clone()), "seen twice");
            cookie = en.cookie;
        }
        verf = ok.cookieverf;
        if round == 2 {
            j.unlink("/home/x/f0299").ok();
            e.w.write_file("/home/x/new", b"");
        }
        round += 1;
        if ok.reply.eof {
            break;
        }
    }
    assert!(round > 3);
    assert!(seen.len() >= 299);
    // Cookies 1 and 2 are reserved; too small a reply holds nothing.
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&dir),
            nfs_argop4::OP_READDIR(t::READDIR4args {
                cookie: 1,
                cookieverf: verf,
                dircount: 512,
                maxcount: 2048,
                attr_request: t::bitmap4(vec![]),
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_BAD_COOKIE);
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&dir),
            nfs_argop4::OP_READDIR(t::READDIR4args {
                cookie: 0,
                cookieverf: [0; 8],
                dircount: 0,
                maxcount: 30,
                attr_request: t::bitmap4(vec![]),
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_TOOSMALL);
}

#[test]
fn root_squash_anonymous_and_read_only() {
    let w = World::new();
    let mut x = w.export(1, "/home/x");
    x.ro = true;
    w.put(x);
    let e = Env::new(w);
    e.w.write_file("/home/x/r", b"read me");
    let root_user = V4c::new("10.0.0.1:900", 100, 0);
    let dir = root(&e, &root_user);
    let r = root_user.raw(
        &e,
        0,
        vec![putfh(&dir), lookup("r"), getattr(&[t::FATTR4_SIZE])],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK);
    // A read-only export refuses changes.
    let r = root_user.raw(
        &e,
        0,
        vec![
            putfh(&dir),
            nfs_argop4::OP_REMOVE(t::REMOVE4args {
                target: t::utf8string(b"r".to_vec()),
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_ROFS);
    root_user.setclientid(&e, "ro", "127.0.0.1.0.1");
    assert_eq!(
        open_confirm(&e, &root_user, &dir, "o", "r", false, OPEN4_SHARE_ACCESS_WRITE, 0).err(),
        Some(nfsstat4::NFS4ERR_ROFS)
    );
    // ANON=-1 refuses an unknown user.
    let mut x = srv3_common::ById::by_id_mem(&*e.w.exports, 1);
    x.anon = nfs::server::Anon::Refuse;
    x.root_hosts = vec![HostPat::Addr("10.0.0.9".parse().unwrap())];
    e.w.put(x);
    let squashed = V4c::new("10.0.0.1:902", 0, 0);
    let r = squashed.raw(&e, 0, vec![putfh(&dir), getattr(&[t::FATTR4_SIZE])]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_ACCESS);
    assert_eq!(statuses(&r), vec![nfsstat4::NFS4ERR_ACCESS]);
}

fn j9(e: &Env) -> ifs::client::LocalIfs<ifs::backend::mem::MemBackend> {
    e.w.job(9)
}
