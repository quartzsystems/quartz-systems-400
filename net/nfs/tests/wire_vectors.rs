// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The generated types against byte vectors computed by hand from the
//! RFCs' layouts: each value encodes to its bytes and they decode back.

#![allow(clippy::many_single_char_names, clippy::items_after_statements)]

mod wire_common;

use nfs::rpc::Credential;
use nfs::rpc::msg::{encode_call, parse_call};
use nfs::types::rpc::{call_body, opaque_auth};
use nfs::types::{mount, nfs3, nfs4, nlm, rpcb};
use nfs::xdr::{self, Decoder, Encoder, ProcBody, Xdr};
use wire_common::{exact, hex, hexs, seed};

#[test]
fn an_auth_sys_call_header() {
    let want = hex("12345678 00000000 00000002 000186a3 00000003 00000001
         00000001 00000024 01020304 00000005 71733430 30000000 000003e8 00000064
         00000002 00000064 00000001
         00000000 00000000
         00000004 66683031");
    let Credential::Sys(mut p) = Credential::sys(b"qs400", 1000, 100, &[100, 1]) else {
        unreachable!()
    };
    p.stamp = 0x0102_0304;
    let body = call_body {
        rpcvers: 2,
        prog: 100_003,
        vers: 3,
        proc: 1,
        cred: Credential::Sys(p.clone()).to_opaque().unwrap(),
        verf: opaque_auth {
            flavor: 0,
            body: vec![],
        },
    };
    let args = nfs3::GETATTR3args {
        object: nfs3::nfs_fh3 {
            data: b"fh01".to_vec(),
        },
    };
    let m = encode_call(0x1234_5678, &body, &xdr::to_bytes(&args).unwrap()).unwrap();
    assert_eq!(hexs(&m), hexs(&want));
    let mut c = parse_call(&want).unwrap();
    assert_eq!(c.xid, 0x1234_5678);
    assert_eq!(c.body, body);
    assert_eq!(
        Credential::from_opaque(&c.body.cred),
        Ok(Credential::Sys(p))
    );
    assert_eq!(nfs3::GETATTR3args::decode(&mut c.args).unwrap(), args);
    assert!(c.args.is_empty());
    seed("nfsrpc", &nfs::rpc::record::frame(&want));
}

#[test]
fn a_getport_call() {
    use rpcb::PMAP_PROG::PMAP_VERS::Args;
    let a = Args::PMAPPROC_GETPORT(rpcb::mapping {
        prog: 100_003,
        vers: 3,
        prot: 17,
        port: 0,
    });
    let mut e = Encoder::new();
    a.encode_body(&mut e).unwrap();
    assert_eq!(e.as_bytes(), hex("000186a3 00000003 00000011 00000000"));
    assert_eq!(a.procedure(), 3);
    assert_eq!(<Args as ProcBody>::PROGRAM, 100_000);
    assert_eq!(<Args as ProcBody>::VERSION, 2);
    let back = Args::decode_body(3, &mut Decoder::new(e.as_bytes())).unwrap();
    assert_eq!(back, Some(a));
    assert_eq!(
        Args::decode_body(9, &mut Decoder::new(e.as_bytes())).unwrap(),
        None
    );
}

#[test]
fn a_mount_mnt_reply() {
    let v = mount::mountres3::MNT3_OK(mount::mountres3_ok {
        fhandle: mount::fhandle3((1..=8).collect()),
        auth_flavors: vec![1],
    });
    exact(
        &v,
        &hex("00000000 00000008 01020304 05060708 00000001 00000001"),
    );
    exact(
        &mount::mountres3::default(mount::mountstat3::MNT3ERR_ACCES),
        &hex("0000000d"),
    );
    // A handle over FHSIZE3 is refused either way.
    let big = mount::mountres3::MNT3_OK(mount::mountres3_ok {
        fhandle: mount::fhandle3(vec![0; 65]),
        auth_flavors: vec![],
    });
    assert!(xdr::to_bytes(&big).is_err());
    assert!(xdr::from_bytes::<mount::mountres3>(&hex("00000000 00000041")).is_err());
    seed(
        "nfsmount",
        &[1, 0, 0, 0, 5, b'/', b'h', b'o', b'm', b'e', 0, 0, 0],
    );
}

#[test]
fn a_mount_export_list_is_a_vec() {
    let v = mount::exports(vec![
        mount::exportnode {
            ex_dir: mount::dirpath(b"/a".to_vec()),
            ex_groups: mount::groups(vec![mount::groupnode {
                gr_name: mount::name(b"h".to_vec()),
            }]),
        },
        mount::exportnode {
            ex_dir: mount::dirpath(b"/b".to_vec()),
            ex_groups: mount::groups(vec![]),
        },
    ]);
    exact(
        &v,
        &hex(
            "00000001 00000002 2f610000 00000001 00000001 68000000 00000000
              00000001 00000002 2f620000 00000000
              00000000",
        ),
    );
}

fn fattr() -> nfs3::fattr3 {
    nfs3::fattr3 {
        r#type: nfs3::ftype3::NF3REG,
        mode: 0o644,
        nlink: 1,
        uid: 1000,
        gid: 100,
        size: 5,
        used: 4096,
        rdev: nfs3::specdata3 {
            specdata1: 0,
            specdata2: 0,
        },
        fsid: 7,
        fileid: 42,
        atime: nfs3::nfstime3 {
            seconds: 1,
            nseconds: 2,
        },
        mtime: nfs3::nfstime3 {
            seconds: 3,
            nseconds: 4,
        },
        ctime: nfs3::nfstime3 {
            seconds: 5,
            nseconds: 6,
        },
    }
}

#[test]
fn nfs3_getattr_reply() {
    let v = nfs3::GETATTR3res::NFS3_OK(nfs3::GETATTR3resok {
        obj_attributes: fattr(),
    });
    exact(
        &v,
        &hex("00000000 00000001 000001a4 00000001 000003e8 00000064
              00000000 00000005 00000000 00001000 00000000 00000000
              00000000 00000007 00000000 0000002a
              00000001 00000002 00000003 00000004 00000005 00000006"),
    );
    seed(
        "nfs3",
        &[
            1, 0, 0, 0, 8, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h',
        ],
    );
}

#[test]
fn nfs3_lookup_replies() {
    let ok = nfs3::LOOKUP3res::NFS3_OK(nfs3::LOOKUP3resok {
        object: nfs3::nfs_fh3 {
            data: vec![0xab, 0xcd],
        },
        obj_attributes: None,
        dir_attributes: None,
    });
    exact(&ok, &hex("00000000 00000002 abcd0000 00000000 00000000"));
    let fail = nfs3::LOOKUP3res::default(
        nfs3::nfsstat3::NFS3ERR_NOENT,
        nfs3::LOOKUP3resfail {
            dir_attributes: None,
        },
    );
    exact(&fail, &hex("00000002 00000000"));
    // The default arm cannot carry a value that has its own arm.
    let wrong = nfs3::LOOKUP3res::default(
        nfs3::nfsstat3::NFS3_OK,
        nfs3::LOOKUP3resfail {
            dir_attributes: None,
        },
    );
    assert_eq!(
        xdr::to_bytes(&wrong),
        Err(xdr::Error::DefaultArm { ty: "LOOKUP3res" })
    );
    // An unknown status is an error.
    assert!(matches!(
        xdr::from_bytes::<nfs3::LOOKUP3res>(&hex("00000003 00000000")),
        Err(xdr::Error::Discriminant {
            ty: "nfsstat3",
            value: 3
        })
    ));
}

#[test]
fn nfs3_readdirplus_reply() {
    let v = nfs3::READDIRPLUS3res::NFS3_OK(nfs3::READDIRPLUS3resok {
        dir_attributes: None,
        cookieverf: [0; 8],
        reply: nfs3::dirlistplus3 {
            entries: vec![
                nfs3::entryplus3 {
                    fileid: 2,
                    name: nfs3::filename3(b"a".to_vec()),
                    cookie: 1,
                    name_attributes: None,
                    name_handle: Some(nfs3::nfs_fh3 { data: vec![9, 9] }),
                },
                nfs3::entryplus3 {
                    fileid: 3,
                    name: nfs3::filename3(b"bc".to_vec()),
                    cookie: 2,
                    name_attributes: None,
                    name_handle: None,
                },
            ],
            eof: true,
        },
    });
    exact(
        &v,
        &hex("00000000 00000000 00000000 00000000
              00000001 00000000 00000002 00000001 61000000 00000000 00000001 00000000
              00000001 00000002 09090000
              00000001 00000000 00000003 00000002 62630000 00000000 00000002 00000000
              00000000
              00000000 00000001"),
    );
}

#[test]
fn an_nlm4_lock_call() {
    use nlm::NLM_PROG::NLM4_VERS::Args;
    let a = nlm::nlm4_lockargs {
        cookie: nlm::netobj(vec![1, 2, 3, 4]),
        block: true,
        exclusive: true,
        alock: nlm::nlm4_lock {
            caller_name: b"host".to_vec(),
            fh: nlm::netobj(vec![0xaa; 4]),
            oh: nlm::netobj(b"o".to_vec()),
            svid: 77,
            l_offset: 0,
            l_len: 100,
        },
        reclaim: false,
        state: 3,
    };
    let want = hex("00000004 01020304 00000001 00000001
         00000004 686f7374 00000004 aaaaaaaa 00000001 6f000000 0000004d
         00000000 00000000 00000000 00000064
         00000000 00000003");
    exact(&a, &want);
    let call = Args::NLMPROC4_LOCK(a);
    assert_eq!(call.procedure(), nlm::NLMPROC4_LOCK);
    let mut s = vec![2u8];
    s.extend_from_slice(&want);
    seed("nfsnlm", &s);
}

fn compound_args() -> nfs4::COMPOUND4args {
    nfs4::COMPOUND4args {
        tag: nfs4::utf8string(vec![]),
        minorversion: 0,
        argarray: vec![
            nfs4::nfs_argop4::OP_PUTROOTFH,
            nfs4::nfs_argop4::OP_GETFH,
            nfs4::nfs_argop4::OP_GETATTR(nfs4::GETATTR4args {
                attr_request: nfs4::bitmap4(vec![0x1a, 2]),
            }),
        ],
    }
}

#[test]
fn an_nfs4_compound() {
    let want =
        hex("00000000 00000000 00000003 00000018 0000000a 00000009 00000002 0000001a 00000002");
    exact(&compound_args(), &want);
    seed("nfs4", &want);
    let res = nfs4::COMPOUND4res {
        status: nfs4::nfsstat4::NFS4_OK,
        tag: nfs4::utf8string(b"test".to_vec()),
        resarray: vec![
            nfs4::nfs_resop4::OP_PUTROOTFH(nfs4::PUTROOTFH4res {
                status: nfs4::nfsstat4::NFS4_OK,
            }),
            nfs4::nfs_resop4::OP_GETFH(nfs4::GETFH4res::NFS4_OK(nfs4::GETFH4resok {
                object: nfs4::nfs_fh4(vec![1, 2, 3, 4]),
            })),
            nfs4::nfs_resop4::OP_GETATTR(nfs4::GETATTR4res::NFS4_OK(nfs4::GETATTR4resok {
                obj_attributes: nfs4::fattr4 {
                    attrmask: nfs4::bitmap4(vec![2]),
                    attr_vals: nfs4::attrlist4(vec![0, 0, 0, 1]),
                },
            })),
        ],
    };
    let want = hex("00000000 00000004 74657374 00000003
         00000018 00000000
         0000000a 00000000 00000004 01020304
         00000009 00000000 00000001 00000002 00000004 00000001");
    exact(&res, &want);
    seed("nfs4", &want);
    // The program table carries it as procedure 1.
    use nfs4::NFS4_PROGRAM::NFS_V4::Args;
    assert_eq!(Args::NFSPROC4_COMPOUND(compound_args()).procedure(), 1);
}

#[test]
fn an_unknown_operation_names_its_type() {
    // OP 70 (no such operation): the v4 server answers OP_ILLEGAL from this.
    let b = hex("00000000 00000001 00000001 00000046");
    assert!(matches!(
        xdr::from_bytes::<nfs4::COMPOUND4args>(&b),
        Err(xdr::Error::Discriminant {
            ty: "nfs_opnum4",
            value: 70
        })
    ));
}

#[test]
fn exchange_id_create_session_sequence() {
    let ex = nfs4::EXCHANGE_ID4args {
        eia_clientowner: nfs4::client_owner4 {
            co_verifier: [1; 8],
            co_ownerid: b"linux".to_vec(),
        },
        eia_flags: nfs4::EXCHGID4_FLAG_SUPP_MOVED_REFER | nfs4::EXCHGID4_FLAG_BIND_PRINC_STATEID,
        eia_state_protect: nfs4::state_protect4_a::SP4_NONE,
        eia_client_impl_id: vec![],
    };
    exact(
        &ex,
        &hex("01010101 01010101 00000005 6c696e75 78000000 00000101 00000000 00000000"),
    );
    let cs = nfs4::CREATE_SESSION4args {
        csa_clientid: 0x1122_3344_5566_7788,
        csa_sequence: 1,
        csa_flags: 3,
        csa_fore_chan_attrs: nfs4::channel_attrs4 {
            ca_headerpadsize: 0,
            ca_maxrequestsize: 1_049_620,
            ca_maxresponsesize: 1_049_480,
            ca_maxresponsesize_cached: 3428,
            ca_maxoperations: 8,
            ca_maxrequests: 64,
            ca_rdma_ird: vec![],
        },
        csa_back_chan_attrs: nfs4::channel_attrs4 {
            ca_headerpadsize: 0,
            ca_maxrequestsize: 4096,
            ca_maxresponsesize: 4096,
            ca_maxresponsesize_cached: 0,
            ca_maxoperations: 2,
            ca_maxrequests: 1,
            ca_rdma_ird: vec![],
        },
        csa_cb_program: 0x4000_0000,
        csa_sec_parms: vec![nfs4::callback_sec_parms4::AUTH_NONE],
    };
    exact(
        &cs,
        &hex("11223344 55667788 00000001 00000003
              00000000 00100414 00100388 00000d64 00000008 00000040 00000000
              00000000 00001000 00001000 00000000 00000002 00000001 00000000
              40000000 00000001 00000000"),
    );
    // ca_rdma_ird holds at most one.
    let mut two = cs.clone();
    two.csa_fore_chan_attrs.ca_rdma_ird = vec![1, 2];
    assert!(xdr::to_bytes(&two).is_err());
    let sq = nfs4::SEQUENCE4args {
        sa_sessionid: [0xab; 16],
        sa_sequenceid: 1,
        sa_slotid: 0,
        sa_highest_slotid: 0,
        sa_cachethis: false,
    };
    exact(
        &sq,
        &hex("abababab abababab abababab abababab 00000001 00000000 00000000 00000000"),
    );
    let c = nfs4::COMPOUND4args {
        tag: nfs4::utf8string(vec![]),
        minorversion: 1,
        argarray: vec![
            nfs4::nfs_argop4::OP_EXCHANGE_ID(ex),
            nfs4::nfs_argop4::OP_CREATE_SESSION(cs),
            nfs4::nfs_argop4::OP_SEQUENCE(sq),
        ],
    };
    let b = xdr::to_bytes(&c).unwrap();
    assert_eq!(xdr::from_bytes::<nfs4::COMPOUND4args>(&b).unwrap(), c);
    seed("nfs4", &b);
}

#[test]
fn operation_numbers_are_the_rfcs() {
    use nfs4::nfs_opnum4 as Op;
    // 4.0 is 3 to 39, 4.1 adds 40 to 58, and OP_ILLEGAL is 10044.
    let ops = [
        Op::OP_ACCESS,
        Op::OP_RELEASE_LOCKOWNER,
        Op::OP_BACKCHANNEL_CTL,
        Op::OP_RECLAIM_COMPLETE,
        Op::OP_ILLEGAL,
    ];
    let v: Vec<i32> = ops.iter().map(|o| o.value()).collect();
    assert_eq!(v, vec![3, 39, 40, 58, 10044]);
    for n in 3..=58 {
        assert!(Op::from_value(n).is_some(), "operation {n}");
    }
    for n in 3..=14 {
        assert!(nfs4::nfs_cb_opnum4::from_value(n).is_some(), "callback {n}");
    }
    assert_eq!(nfs4::nfs_cb_opnum4::OP_CB_ILLEGAL.value(), 10044);
    assert_eq!(nfs4::nfsstat4::NFS4ERR_DELEG_REVOKED.value(), 10087);
    assert_eq!(nfs4::nfsstat4::from_value(10073), None);
    assert_eq!(nfs4::FATTR4_FS_CHARSET_CAP, 76);
    assert_eq!(nfs4::NFS4_CALLBACK, 0x4000_0000);
    assert_eq!(nfs3::NFSPROC3_COMMIT, 21);
    assert_eq!(nlm::NLMPROC4_FREE_ALL, 23);
}

#[test]
fn every_nfs3_procedure_round_trips_void_and_minimal() {
    use nfs3::NFS_PROGRAM::NFS_V3::{Args, PROCEDURES, Res};
    assert_eq!(PROCEDURES.len(), 22);
    // A failure reply of each procedure: status NFS3ERR_IO and empty attributes.
    for (n, _) in PROCEDURES.iter().skip(1) {
        let mut b = hex("00000005");
        // Every resfail is made of post_op_attr and wcc_data: all absent.
        let filler = [0u8; 16];
        for k in 0..=filler.len() / 4 {
            let mut try_b = b.clone();
            try_b.extend_from_slice(&filler[..k * 4]);
            let mut d = Decoder::new(&try_b);
            if let Ok(Some(r)) = Res::decode_body(*n, &mut d)
                && d.is_empty()
            {
                let mut e = Encoder::new();
                r.encode_body(&mut e).unwrap();
                assert_eq!(e.as_bytes(), &try_b[..]);
                b.clear();
                break;
            }
        }
        assert!(
            b.is_empty(),
            "procedure {n} has no failure reply of absent attributes"
        );
    }
    assert_eq!(
        Args::decode_body(0, &mut Decoder::new(&[])).unwrap(),
        Some(Args::NFSPROC3_NULL)
    );
}
