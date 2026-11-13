// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Text conversion per host (`HOSTOPT`): data of files tagged with a
//! single-byte CCSID, path names in the host's CCSID; and an export of
//! `/QSYS.LIB` (libraries, files and members as directories and files).

#![allow(clippy::many_single_char_names)]
mod srv3_common;

use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_WRONLY};
use nfs::server::{HostOpts, HostPat};
use nfs::types::nfs3::NFS_PROGRAM::NFS_V3::{Args, Res};
use nfs::types::nfs3::{self as t3, nfsstat3 as S};
use srv3_common::{Client, V3, World, dirop, fh};

const PEER: &str = "10.0.0.5:900";

fn tagged(w: &World, path: &str, ccsid: u32, data: &[u8]) {
    let j = w.job(9);
    let fd = j
        .open(
            path,
            OpenArgs::new(O_WRONLY | O_CREAT | O_CCSID)
                .mode(0o666)
                .conv(ccsid),
        )
        .unwrap();
    ifs::client::write_all(&j, fd, data).unwrap();
    j.close(fd).unwrap();
    j.chmod(path, 0o666).unwrap();
}

fn text_world(data: Option<u16>, path_ccsid: u16) -> (World, Vec<u8>) {
    let w = World::new();
    let mut e = w.export(1, "/home/exp");
    e.host_opts = vec![(
        HostPat::Addr("10.0.0.5".parse().unwrap()),
        HostOpts {
            data_ccsid: data,
            path_ccsid,
            sync: false,
        },
    )];
    w.put(e);
    let root = w.handle(1, "/home/exp");
    (w, root)
}

#[test]
fn tagged_text_is_converted_for_the_host() {
    let (w, root) = text_world(Some(819), 1208);
    let ebcdic = ccsid::to_ebcdic("Hello, world").unwrap();
    tagged(&w, "/home/exp/e.txt", 37, &ebcdic);
    tagged(&w, "/home/exp/u.txt", 1208, b"Hello");
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    let e = v.lookup(&root, "e.txt").unwrap();
    assert_eq!(v.read(&e, 0, 100).unwrap().0, b"Hello, world");
    assert_eq!(v.read(&e, 7, 5).unwrap().0, b"world");
    // Sizes are the converted size (one byte a character).
    assert_eq!(v.getattr(&e).unwrap().size, 12);
    v.write(&e, 0, b"HELLO", t3::stable_how::FILE_SYNC).unwrap();
    assert_eq!(
        w.read_file("/home/exp/e.txt"),
        ccsid::to_ebcdic("HELLO, world").unwrap()
    );
    // UTF-8 is served as stored.
    let u = v.lookup(&root, "u.txt").unwrap();
    assert_eq!(v.read(&u, 0, 100).unwrap().0, b"Hello");
    // A file made through the export takes the host's CCSID.
    v.create(&root, "new.txt").unwrap();
    assert_eq!(
        w.job(9).stat("/home/exp/new.txt", false).unwrap().ccsid,
        819
    );
    // Another host, *BINARY: as stored.
    let other = Client::new("10.0.0.6:900", 100);
    let vo = V3 { p: &p, c: &other };
    assert_eq!(
        vo.read(&e, 0, 5).unwrap().0,
        &ccsid::to_ebcdic("HELLO").unwrap()[..]
    );
}

#[test]
fn path_names_in_the_hosts_ccsid() {
    let (w, root) = text_world(None, 819);
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    // `café` in ISO 8859-1 on the wire, UTF-8 in the IFS.
    let a = t3::CREATE3args {
        r#where: t3::diropargs3 {
            dir: fh(&root),
            name: t3::filename3(b"caf\xe9".to_vec()),
        },
        how: t3::createhow3::UNCHECKED(srv3_common::mode(0o644)),
    };
    assert!(matches!(
        c.v3(&p, &Args::NFSPROC3_CREATE(a)),
        Res::NFSPROC3_CREATE(t3::CREATE3res::NFS3_OK(_))
    ));
    assert!(w.job(9).stat("/home/exp/caf\u{e9}", false).is_ok());
    let r = t3::READDIR3args {
        dir: fh(&root),
        cookie: 0,
        cookieverf: [0; 8],
        count: 4096,
    };
    match c.v3(&p, &Args::NFSPROC3_READDIR(r)) {
        Res::NFSPROC3_READDIR(t3::READDIR3res::NFS3_OK(ok)) => {
            assert!(ok.reply.entries.iter().any(|e| e.name.0 == b"caf\xe9"));
        }
        r => panic!("{r:?}"),
    }
    // In UTF-8 from a host whose names are UTF-8.
    let u = Client::new("10.0.0.6:900", 100);
    let vu = V3 { p: &p, c: &u };
    assert!(vu.lookup(&root, "caf\u{e9}").is_ok());
    assert_eq!(v.lookup(&root, "nope\u{e9}").err(), Some(S::NFS3ERR_NOENT));
    let _ = dirop(&root, "x");
}

#[test]
fn qsys_lib_shows_libraries_files_and_members() {
    let w = World::new();
    // QSECOFR (a ROOT= host) reads the members.
    let root = Client::new(PEER, 0);
    let mut ex = nfs::server::MemExport::new(1, "/QSYS.LIB/QGPL.LIB", w.fid("/QSYS.LIB/QGPL.LIB"));
    ex.root_hosts = vec![HostPat::Addr("10.0.0.5".parse().unwrap())];
    ex.host_opts = vec![(
        HostPat::Addr("10.0.0.5".parse().unwrap()),
        HostOpts {
            data_ccsid: Some(819),
            path_ccsid: 1208,
            sync: false,
        },
    )];
    w.put(ex);
    let lib = w.handle(1, "/QSYS.LIB/QGPL.LIB");
    let p = w.nfs3();
    let v = V3 { p: &p, c: &root };
    assert_eq!(v.getattr(&lib).unwrap().r#type, t3::ftype3::NF3DIR);
    let f = v.lookup(&lib, "QCLSRC.FILE").unwrap();
    assert_eq!(v.getattr(&f).unwrap().r#type, t3::ftype3::NF3DIR);
    let m = v.lookup(&f, "HELLO.MBR").unwrap();
    assert_eq!(v.getattr(&m).unwrap().r#type, t3::ftype3::NF3REG);
    let (data, eof) = v.read(&m, 0, 4096).unwrap();
    let size = v.getattr(&m).unwrap().size;
    assert!(eof, "{} of {size}", data.len());
    let text = String::from_utf8_lossy(&data);
    assert!(text.contains("PGM") && text.contains("ENDPGM"), "{text:?}");
    let s = v.lookup(&lib, "SPACE.USRSPC").unwrap();
    assert_eq!(v.getattr(&s).unwrap().r#type, t3::ftype3::NF3REG);
    // `..` of the export's root stays in it.
    assert_eq!(v.lookup(&lib, ".."), Ok(lib.clone()));
}
