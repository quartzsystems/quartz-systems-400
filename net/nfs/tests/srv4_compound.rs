// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFSv4 `COMPOUND` rules: the header, `OP_ILLEGAL`, minor versions,
//! the 4.1 position rules, refusals; the pseudo file system with two
//! exports and a host allowed one; attributes.

#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    clippy::doc_markdown
)]

mod srv3_common;
mod srv4_common;

use nfs::server::HostPat;
use nfs::types::nfs4::NFS4_PROGRAM::NFS_V4;
use nfs::types::nfs4::{self as t, nfs_argop4, nfs_resop4, nfsstat4};
use nfs::v4::attr::Bits;
use nfs::xdr::{Decoder, Encoder, Xdr};
use srv3_common::World;
use srv4_common::{Env, V4c, getattr, getfh, lookup, putfh, rootfh, statuses};

fn two_exports() -> Env {
    let w = World::new();
    let a = w.export(1, "/home/ann");
    w.put(a);
    let mut b = w.export(2, "/data");
    b.access = Some(vec![HostPat::Addr("10.0.0.1".parse().unwrap())]);
    w.put(b);
    Env::new(w)
}

/// A compound's raw bytes: tag, minor version, then each op's number
/// and its arguments as bytes.
fn raw_compound(minor: u32, ops: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut e = Encoder::new();
    e.put_opaque(b"x", u32::MAX).unwrap();
    e.put_u32(minor);
    e.put_u32(u32::try_from(ops.len()).unwrap());
    for (op, a) in ops {
        e.put_u32(*op);
        e.put_raw(a);
    }
    e.into_bytes()
}

fn send_raw(e: &Env, c: &V4c, body: &[u8]) -> t::COMPOUND4res {
    let call = nfs::rpc::msg::encode_call(c.c.next_xid(), &t_call_body(&c.c), body).unwrap();
    let reply = c.c.send(&e.prog, &call).unwrap();
    let res = nfs::rpc::client::results(&reply).unwrap();
    nfs::xdr::from_bytes(&res).unwrap()
}

fn t_call_body(c: &srv3_common::Client) -> nfs::types::rpc::call_body {
    nfs::types::rpc::call_body {
        rpcvers: 2,
        prog: 100_003,
        vers: 4,
        proc: 1,
        cred: c.cred.to_opaque().unwrap(),
        verf: nfs::rpc::auth::none(),
    }
}

#[test]
fn null_and_minor_versions() {
    let e = two_exports();
    let c = V4c::new("10.0.0.1:900", 100, 0);
    let r: NFS_V4::Res = c.c.call(&e.prog, &NFS_V4::Args::NFSPROC4_NULL);
    assert_eq!(r, NFS_V4::Res::NFSPROC4_NULL);
    let r = c.raw(&e, 2, vec![rootfh()]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_MINOR_VERS_MISMATCH);
    assert!(r.resarray.is_empty());
    assert_eq!(r.tag.0, b"test");
}

#[test]
fn illegal_and_undecodable_operations() {
    let e = two_exports();
    let c = V4c::new("10.0.0.1:900", 100, 0);
    // An operation number the protocol lacks, after one that works.
    let r = send_raw(
        &e,
        &c,
        &raw_compound(0, &[(24, vec![]), (99, vec![1, 2, 3, 4])]),
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_OP_ILLEGAL);
    assert_eq!(
        statuses(&r),
        vec![nfsstat4::NFS4_OK, nfsstat4::NFS4ERR_OP_ILLEGAL]
    );
    assert!(matches!(r.resarray[1], nfs_resop4::OP_ILLEGAL(_)));
    // A 4.1 operation in a 4.0 compound is not 4.0's.
    let r = send_raw(&e, &c, &raw_compound(0, &[(53, vec![0; 32])]));
    assert_eq!(r.status, nfsstat4::NFS4ERR_OP_ILLEGAL);
    // OP_ILLEGAL itself.
    let r = send_raw(&e, &c, &raw_compound(0, &[(10_044, vec![])]));
    assert_eq!(r.status, nfsstat4::NFS4ERR_OP_ILLEGAL);
    // Arguments that do not decode: BADXDR for that operation.
    let r = send_raw(
        &e,
        &c,
        &raw_compound(0, &[(24, vec![]), (15, vec![0, 0, 0, 9])]),
    );
    assert_eq!(
        statuses(&r),
        vec![nfsstat4::NFS4_OK, nfsstat4::NFS4ERR_BADXDR]
    );
}

#[test]
fn the_first_error_ends_the_compound() {
    let e = two_exports();
    let c = V4c::new("10.0.0.1:900", 100, 0);
    let r = c.raw(&e, 0, vec![rootfh(), lookup("nothing"), getfh()]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOENT);
    assert_eq!(r.resarray.len(), 2);
    let r = c.raw(&e, 0, vec![getfh()]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOFILEHANDLE);
}

#[test]
fn minor_version_one_position_rules() {
    let e = two_exports();
    let c = V4c::new("10.0.0.1:900", 100, 1);
    let r = c.raw(&e, 1, vec![rootfh()]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_OP_NOT_IN_SESSION);
    let r = c.raw(
        &e,
        1,
        vec![srv4_common::exchange_id("a", [1; 8], 0), rootfh()],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOT_ONLY_OP);
    c.session(&e, "pos", 0);
    let r = c.raw(&e, 1, vec![c.sequence(false), c.sequence(false)]);
    assert_eq!(
        statuses(&r),
        vec![nfsstat4::NFS4_OK, nfsstat4::NFS4ERR_SEQUENCE_POS]
    );
    // The refused SEQUENCE took no slot.
    c.seq.set(2);
    // 4.0's operations in a 4.1 compound.
    let r = c.run(
        &e,
        vec![nfs_argop4::OP_RENEW(t::RENEW4args { clientid: 1 })],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOTSUPP);
    let r = c.run(
        &e,
        vec![nfs_argop4::OP_RELEASE_LOCKOWNER(
            t::RELEASE_LOCKOWNER4args {
                lock_owner: c.owner("x"),
            },
        )],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOTSUPP);
}

#[test]
fn refusals_of_what_is_not_done() {
    let e = two_exports();
    let c = V4c::new("10.0.0.1:900", 100, 1);
    c.session(&e, "refuse", 0);
    let layout = nfs_argop4::OP_LAYOUTGET(t::LAYOUTGET4args {
        loga_signal_layout_avail: false,
        loga_layout_type: t::layouttype4::LAYOUT4_NFSV4_1_FILES,
        loga_iomode: t::layoutiomode4::LAYOUTIOMODE4_READ,
        loga_offset: 0,
        loga_length: 1,
        loga_minlength: 1,
        loga_stateid: srv4_common::anon(),
        loga_maxcount: 100,
    });
    for op in [
        layout,
        nfs_argop4::OP_GETDEVICELIST(t::GETDEVICELIST4args {
            gdla_layout_type: t::layouttype4::LAYOUT4_NFSV4_1_FILES,
            gdla_maxdevices: 1,
            gdla_cookie: 0,
            gdla_cookieverf: [0; 8],
        }),
        nfs_argop4::OP_WANT_DELEGATION(t::WANT_DELEGATION4args {
            wda_want: 0,
            wda_claim: t::deleg_claim4::CLAIM_FH,
        }),
    ] {
        let r = c.run(&e, vec![rootfh(), op]);
        assert_eq!(r.status, nfsstat4::NFS4ERR_NOTSUPP);
    }
    let r = c.run(
        &e,
        vec![
            rootfh(),
            nfs_argop4::OP_OPENATTR(t::OPENATTR4args { createdir: false }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOTSUPP);
    let r = c.run(
        &e,
        vec![nfs_argop4::OP_SET_SSV(t::SET_SSV4args {
            ssa_ssv: vec![1],
            ssa_digest: vec![],
        })],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_INVAL);
    // State protection beyond SP4_NONE.
    let mut ex = srv4_common::exchange_id("prot", [1; 8], 0);
    if let nfs_argop4::OP_EXCHANGE_ID(a) = &mut ex {
        a.eia_state_protect = t::state_protect4_a::SP4_MACH_CRED(t::state_protect_ops4 {
            spo_must_enforce: t::bitmap4(vec![]),
            spo_must_allow: t::bitmap4(vec![]),
        });
    }
    assert_eq!(c.raw(&e, 1, vec![ex]).status, nfsstat4::NFS4ERR_INVAL);
}

#[test]
fn the_pseudo_file_system() {
    let e = two_exports();
    let c = V4c::new("10.0.0.1:900", 100, 0);
    let fsid = [
        t::FATTR4_FSID,
        t::FATTR4_TYPE,
        t::FATTR4_MOUNTED_ON_FILEID,
        t::FATTR4_FILEID,
    ];
    let r = c.raw(&e, 0, vec![rootfh(), getfh(), getattr(&fsid)]);
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let root = srv4_common::fh_of(&r);
    assert_eq!(root[0], 2, "a pseudo handle");
    // The root lists both exports' first components.
    let rd = nfs_argop4::OP_READDIR(t::READDIR4args {
        cookie: 0,
        cookieverf: [0; 8],
        dircount: 1000,
        maxcount: 8000,
        attr_request: Bits::of(&[t::FATTR4_TYPE]).to_bitmap(),
    });
    let r = c.raw(&e, 0, vec![putfh(&root), rd.clone()]);
    let nfs_resop4::OP_READDIR(t::READDIR4res::NFS4_OK(ok)) = &r.resarray[1] else {
        panic!("{r:?}");
    };
    let names: Vec<_> = ok.reply.entries.iter().map(|x| x.name.0.clone()).collect();
    assert_eq!(names, vec![b"data".to_vec(), b"home".to_vec()]);
    assert!(ok.reply.eof);
    // Down into /home/ann: a new file system, mounted on its place.
    let r = c.raw(
        &e,
        0,
        vec![
            putfh(&root),
            lookup("home"),
            lookup("ann"),
            getfh(),
            getattr(&fsid),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let ann = srv4_common::fh_of(&r);
    assert_eq!(ann[0], 1, "an export's handle");
    let a = srv4_common::attrs_of(&r);
    let mut d = Decoder::new(&a.attr_vals.0);
    assert_eq!(d.get_u32().unwrap(), 2, "a directory");
    let major = d.get_u64().unwrap();
    assert_eq!(major, 1, "fsid of export 1");
    // Back up to the pseudo directory.
    let r = c.raw(&e, 0, vec![putfh(&ann), nfs_argop4::OP_LOOKUPP, getfh()]);
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    assert_eq!(srv4_common::fh_of(&r)[0], 2);
    // The root's parent does not exist; pseudo directories are read-only.
    let r = c.raw(&e, 0, vec![rootfh(), nfs_argop4::OP_LOOKUPP]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOENT);
    let r = c.raw(
        &e,
        0,
        vec![
            rootfh(),
            nfs_argop4::OP_CREATE(t::CREATE4args {
                objtype: t::createtype4::NF4DIR,
                objname: t::utf8string(b"new".to_vec()),
                createattrs: srv4_common::no_attrs(),
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4ERR_ROFS);
    // A host the second export does not allow sees only the first.
    let other = V4c::new("10.0.0.2:900", 100, 0);
    let r = other.raw(&e, 0, vec![rootfh(), rd]);
    let nfs_resop4::OP_READDIR(t::READDIR4res::NFS4_OK(ok)) = &r.resarray[1] else {
        panic!("{r:?}");
    };
    assert_eq!(ok.reply.entries.len(), 1);
    let r = other.raw(&e, 0, vec![rootfh(), lookup("data")]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_NOENT);
    // The public handle is the root's when no export holds it.
    let r = c.raw(&e, 0, vec![nfs_argop4::OP_PUTPUBFH, getfh()]);
    assert_eq!(srv4_common::fh_of(&r), root);
    // SECINFO answers AUTH_SYS.
    let r = c.raw(
        &e,
        0,
        vec![
            rootfh(),
            nfs_argop4::OP_SECINFO(t::SECINFO4args {
                name: t::utf8string(b"home".to_vec()),
            }),
        ],
    );
    let nfs_resop4::OP_SECINFO(t::SECINFO4res::NFS4_OK(s)) = &r.resarray[1] else {
        panic!("{r:?}");
    };
    assert_eq!(s.0, vec![t::secinfo4::default(1)]);
}

#[test]
fn attributes_and_verify() {
    let e = two_exports();
    e.w.write_file("/home/ann/f", b"hello");
    let c = V4c::new("10.0.0.1:900", 100, 0);
    let want = [
        t::FATTR4_SUPPORTED_ATTRS,
        t::FATTR4_TYPE,
        t::FATTR4_SIZE,
        t::FATTR4_ACL,
        t::FATTR4_OWNER,
        t::FATTR4_LEASE_TIME,
        t::FATTR4_SUPPATTR_EXCLCREAT,
    ];
    let r = c.raw(
        &e,
        0,
        vec![
            rootfh(),
            lookup("home"),
            lookup("ann"),
            lookup("f"),
            getattr(&want),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let a = srv4_common::attrs_of(&r);
    let got = Bits::from(&a.attrmask);
    // ACL and 4.1's exclusive-create set are not 4.0 answers.
    assert!(got.has(t::FATTR4_SIZE) && got.has(t::FATTR4_OWNER));
    assert!(!got.has(t::FATTR4_ACL) && !got.has(t::FATTR4_SUPPATTR_EXCLCREAT));
    let mut d = Decoder::new(&a.attr_vals.0);
    let sup = t::bitmap4::decode(&mut d).unwrap();
    assert!(!Bits::from(&sup).has(t::FATTR4_ACL));
    assert_eq!(
        t::nfs_ftype4::decode(&mut d).unwrap(),
        t::nfs_ftype4::NF4REG
    );
    assert_eq!(d.get_u64().unwrap(), 5);
    assert_eq!(d.get_u32().unwrap(), 90);
    let owner = d.get_opaque(u32::MAX).unwrap();
    assert_eq!(owner, b"QSECOFR@example.com");
    // Write-only attributes cannot be asked for.
    let r = c.raw(&e, 0, vec![rootfh(), getattr(&[t::FATTR4_TIME_ACCESS_SET])]);
    assert_eq!(r.status, nfsstat4::NFS4ERR_INVAL);
    // VERIFY and NVERIFY compare through the same table.
    let mut v = Encoder::new();
    v.put_u64(5);
    let size5 = t::fattr4 {
        attrmask: Bits::of(&[t::FATTR4_SIZE]).to_bitmap(),
        attr_vals: t::attrlist4(v.into_bytes()),
    };
    let path = || vec![rootfh(), lookup("home"), lookup("ann"), lookup("f")];
    let mut ops = path();
    ops.push(nfs_argop4::OP_VERIFY(t::VERIFY4args {
        obj_attributes: size5.clone(),
    }));
    assert_eq!(c.raw(&e, 0, ops).status, nfsstat4::NFS4_OK);
    let mut ops = path();
    ops.push(nfs_argop4::OP_NVERIFY(t::NVERIFY4args {
        obj_attributes: size5,
    }));
    assert_eq!(c.raw(&e, 0, ops).status, nfsstat4::NFS4ERR_SAME);
    let mut ops = path();
    ops.push(nfs_argop4::OP_VERIFY(t::VERIFY4args {
        obj_attributes: t::fattr4 {
            attrmask: Bits::of(&[t::FATTR4_ACL]).to_bitmap(),
            attr_vals: t::attrlist4(vec![0, 0, 0, 0]),
        },
    }));
    assert_eq!(c.raw(&e, 0, ops).status, nfsstat4::NFS4ERR_ATTRNOTSUPP);
    // SETATTR of an attribute not supported, and of one read-only.
    for (bits, val, want) in [
        (
            t::FATTR4_ACL,
            vec![0, 0, 0, 0],
            nfsstat4::NFS4ERR_ATTRNOTSUPP,
        ),
        (t::FATTR4_TYPE, vec![0, 0, 0, 1], nfsstat4::NFS4ERR_INVAL),
    ] {
        let mut ops = path();
        ops.push(nfs_argop4::OP_SETATTR(t::SETATTR4args {
            stateid: srv4_common::anon(),
            obj_attributes: t::fattr4 {
                attrmask: Bits::of(&[bits]).to_bitmap(),
                attr_vals: t::attrlist4(val),
            },
        }));
        let r = c.raw(&e, 0, ops);
        assert_eq!(r.status, want);
        let nfs_resop4::OP_SETATTR(s) = r.resarray.last().unwrap() else {
            panic!()
        };
        assert!(s.attrsset.0.is_empty());
    }
    // An owner by name, by number, and one of another domain.
    for (who, want) in [
        (&b"BOB@example.com"[..], nfsstat4::NFS4_OK),
        (b"102", nfsstat4::NFS4_OK),
        (b"BOB@other.org", nfsstat4::NFS4ERR_BADOWNER),
    ] {
        let mut v = Encoder::new();
        v.put_opaque(who, u32::MAX).unwrap();
        let mut ops = path();
        ops.push(nfs_argop4::OP_SETATTR(t::SETATTR4args {
            stateid: srv4_common::anon(),
            obj_attributes: t::fattr4 {
                attrmask: Bits::of(&[t::FATTR4_OWNER]).to_bitmap(),
                attr_vals: t::attrlist4(v.into_bytes()),
            },
        }));
        let c0 = V4c::new("10.0.0.1:901", 0, 0);
        let mut e2 = e.w.exports.by_id_mem_root(1);
        e2.root_hosts = vec![HostPat::Addr("10.0.0.1".parse().unwrap())];
        e.w.put(e2);
        assert_eq!(c0.raw(&e, 0, ops).status, want, "{who:?}");
    }
}

/// The export as a `MemExport` again.
trait ByIdRoot {
    fn by_id_mem_root(&self, id: u32) -> nfs::server::MemExport;
}

impl ByIdRoot for nfs::server::MemExports {
    fn by_id_mem_root(&self, id: u32) -> nfs::server::MemExport {
        use nfs::server::ExportTable;
        let e = self.by_id(id).unwrap();
        nfs::server::MemExport::new(e.id(), e.path(), e.root_id())
    }
}

#[test]
fn reply_size_limit_of_four_zero() {
    let e = two_exports();
    let c = V4c::new("10.0.0.1:900", 100, 0);
    let ops: Vec<_> = (0..129).map(|_| rootfh()).collect();
    let r = c.raw(&e, 0, ops);
    assert_eq!(r.status, nfsstat4::NFS4ERR_RESOURCE);
    assert_eq!(r.resarray.len(), 1);
}
