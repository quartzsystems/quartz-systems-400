// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFSv4 with a host's text options (`HOSTOPT`): a file tagged with an
//! EBCDIC CCSID read and written through an open converted; path names
//! in the host's CCSID; a `/QSYS.LIB` export through the pseudo file
//! system.

#![allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    clippy::doc_markdown
)]

mod srv3_common;
mod srv4_common;

use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_WRONLY};
use nfs::server::{HostOpts, HostPat, MemExport};
use nfs::types::nfs4::{self as t, nfs_argop4, nfs_resop4, nfsstat4};
use srv3_common::World;
use srv4_common::{Env, V4c, getfh, lookup, read, rootfh, write};

fn opts(data: Option<u16>, path_ccsid: u16) -> Vec<(HostPat, HostOpts)> {
    vec![(
        HostPat::Addr("10.0.0.5".parse().unwrap()),
        HostOpts {
            data_ccsid: data,
            path_ccsid,
            sync: false,
        },
    )]
}

#[test]
fn tagged_text_through_an_open() {
    let w = World::new();
    let mut e = w.export(1, "/home/exp");
    e.host_opts = opts(Some(819), 819);
    w.put(e);
    let j = w.job(9);
    let fd = j
        .open(
            "/home/exp/e.txt",
            OpenArgs::new(O_WRONLY | O_CREAT | O_CCSID)
                .mode(0o666)
                .conv(37),
        )
        .unwrap();
    ifs::client::write_all(&j, fd, &ccsid::to_ebcdic("Hello, world").unwrap()).unwrap();
    j.close(fd).unwrap();
    j.chmod("/home/exp/e.txt", 0o666).unwrap();
    let env = Env::new(w);
    let c = V4c::new("10.0.0.5:900", 100, 1);
    c.session(&env, "text", 0);
    let cur = t::stateid4 {
        seqid: 1,
        other: [0; 12],
    };
    let r = c.run(
        &env,
        vec![
            rootfh(),
            lookup("home"),
            lookup("exp"),
            c.open("o", "e.txt", false, 3, 0),
            read(&cur, 7, 5),
            write(&cur, 0, b"HELLO"),
            getfh(),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let nfs_resop4::OP_READ(t::READ4res::NFS4_OK(rd)) = &r.resarray[4] else {
        panic!("{r:?}")
    };
    assert_eq!(rd.data, b"world");
    assert_eq!(
        env.w.read_file("/home/exp/e.txt"),
        ccsid::to_ebcdic("HELLO, world").unwrap()
    );
    // A name in ISO 8859-1 on the wire is UTF-8 in the IFS; a name that
    // is not UTF-8 is fine for this host.
    let r = c.run(
        &env,
        vec![
            rootfh(),
            lookup("home"),
            lookup("exp"),
            nfs_argop4::OP_CREATE(t::CREATE4args {
                objtype: t::createtype4::NF4DIR,
                objname: t::utf8string(b"caf\xe9".to_vec()),
                createattrs: srv4_common::no_attrs(),
            }),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    assert!(env.w.job(9).stat("/home/exp/caf\u{e9}", false).is_ok());
}

#[test]
fn a_qsys_lib_export_through_the_pseudo_file_system() {
    let w = World::new();
    let mut ex = MemExport::new(1, "/QSYS.LIB/QGPL.LIB", w.fid("/QSYS.LIB/QGPL.LIB"));
    ex.root_hosts = vec![HostPat::Addr("10.0.0.5".parse().unwrap())];
    ex.host_opts = opts(Some(819), 1208);
    w.put(ex);
    let env = Env::new(w);
    let c = V4c::new("10.0.0.5:900", 0, 0);
    let r = c.raw(
        &env,
        0,
        vec![
            rootfh(),
            lookup("QSYS.LIB"),
            lookup("QGPL.LIB"),
            lookup("QCLSRC.FILE"),
            lookup("HELLO.MBR"),
            read(&srv4_common::anon(), 0, 4096),
        ],
    );
    assert_eq!(r.status, nfsstat4::NFS4_OK, "{r:?}");
    let nfs_resop4::OP_READ(t::READ4res::NFS4_OK(rd)) = &r.resarray[5] else {
        panic!("{r:?}")
    };
    let text = String::from_utf8_lossy(&rd.data);
    assert!(text.contains("PGM") && text.contains("ENDPGM"), "{text:?}");
    assert!(rd.eof);
}
