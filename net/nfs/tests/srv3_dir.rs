// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `READDIR` and `READDIRPLUS`: `.` and `..`, paging by `count`,
//! `dircount` and `maxcount`, the cookie verifier, `NFS3ERR_TOOSMALL`,
//! and paging through 2,000 entries while the directory changes.

#![allow(clippy::many_single_char_names)]
mod srv3_common;

use std::collections::HashSet;

use ifs::client::IfsClient;
use nfs::types::nfs3::NFS_PROGRAM::NFS_V3::{Args, Res};
use nfs::types::nfs3::{self as t3, nfsstat3 as S};
use srv3_common::{Client, World, fh};

const PEER: &str = "10.0.0.5:900";

fn world() -> (World, Vec<u8>) {
    let w = World::new();
    let e = w.export(1, "/home/exp");
    w.put(e);
    let root = w.handle(1, "/home/exp");
    (w, root)
}

type Page = (Vec<(String, u64)>, [u8; 8], bool);

fn readdir(
    c: &Client,
    p: &dyn nfs::rpc::LocalProgram,
    dir: &[u8],
    cookie: u64,
    verf: [u8; 8],
    count: u32,
) -> Result<Page, S> {
    let a = t3::READDIR3args {
        dir: fh(dir),
        cookie,
        cookieverf: verf,
        count,
    };
    match c.v3(p, &Args::NFSPROC3_READDIR(a)) {
        Res::NFSPROC3_READDIR(t3::READDIR3res::NFS3_OK(ok)) => Ok((
            ok.reply
                .entries
                .into_iter()
                .map(|e| (String::from_utf8(e.name.0).unwrap(), e.cookie))
                .collect(),
            ok.cookieverf,
            ok.reply.eof,
        )),
        Res::NFSPROC3_READDIR(t3::READDIR3res::default(s, _)) => Err(s),
        r => panic!("{r:?}"),
    }
}

fn plus(
    c: &Client,
    p: &dyn nfs::rpc::LocalProgram,
    dir: &[u8],
    cookie: u64,
    verf: [u8; 8],
    dircount: u32,
    maxcount: u32,
) -> Result<(Vec<t3::entryplus3>, [u8; 8], bool), S> {
    let a = t3::READDIRPLUS3args {
        dir: fh(dir),
        cookie,
        cookieverf: verf,
        dircount,
        maxcount,
    };
    match c.v3(p, &Args::NFSPROC3_READDIRPLUS(a)) {
        Res::NFSPROC3_READDIRPLUS(t3::READDIRPLUS3res::NFS3_OK(ok)) => {
            Ok((ok.reply.entries, ok.cookieverf, ok.reply.eof))
        }
        Res::NFSPROC3_READDIRPLUS(t3::READDIRPLUS3res::default(s, _)) => Err(s),
        r => panic!("{r:?}"),
    }
}

#[test]
fn small_directories_dots_and_errors() {
    let (w, root) = world();
    for n in ["a", "b", "c"] {
        w.write_file(&format!("/home/exp/{n}"), b"x");
    }
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let (all, verf, eof) = readdir(&c, &p, &root, 0, [0; 8], 4096).unwrap();
    assert!(eof);
    let names: Vec<&str> = all.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(&names[..2], [".", ".."]);
    let mut rest: Vec<&str> = names[2..].to_vec();
    rest.sort_unstable();
    assert_eq!(rest, ["a", "b", "c"]);
    // From the cookie of `..`: the files only.
    let (more, _, _) = readdir(&c, &p, &root, 2, verf, 4096).unwrap();
    assert_eq!(more.len(), 3);
    // Room for no entry.
    assert_eq!(
        readdir(&c, &p, &root, 0, [0; 8], 120),
        Err(S::NFS3ERR_TOOSMALL)
    );
    // Another directory's verifier with a cookie.
    let sub = {
        w.job(9).mkdir("/home/exp/sub", 0o777).unwrap();
        w.handle(1, "/home/exp/sub")
    };
    let (_, sverf, _) = readdir(&c, &p, &sub, 0, [0; 8], 4096).unwrap();
    assert_ne!(sverf, verf);
    assert_eq!(
        readdir(&c, &p, &root, 5, sverf, 4096),
        Err(S::NFS3ERR_BAD_COOKIE)
    );
    // A file is not a directory.
    let f = w.handle(1, "/home/exp/a");
    assert_eq!(readdir(&c, &p, &f, 0, [0; 8], 4096), Err(S::NFS3ERR_NOTDIR));
    // READDIRPLUS: handles and attributes, `..` of the root is the root.
    let (ents, _, eof) = plus(&c, &p, &root, 0, [0; 8], 4096, 65536).unwrap();
    assert!(eof);
    assert_eq!(ents[1].name_handle.as_ref().unwrap().data, root);
    let a = ents.iter().find(|e| e.name.0 == b"a").unwrap();
    assert_eq!(a.name_handle.as_ref().unwrap().data, f);
    assert_eq!(a.name_attributes.as_ref().unwrap().size, 1);
    assert_eq!(a.fileid, a.name_attributes.as_ref().unwrap().fileid);
}

#[test]
fn readdirplus_pages_through_a_changing_directory() {
    let (w, root) = world();
    let j = w.job(9);
    for i in 0..2000 {
        w.write_file(&format!("/home/exp/f{i:04}"), b"");
    }
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let mut seen: Vec<String> = Vec::new();
    let mut cookie = 0;
    let mut verf = [0u8; 8];
    let mut pages = 0;
    loop {
        let (ents, v, eof) = plus(&c, &p, &root, cookie, verf, 2048, 16384).unwrap();
        assert!(!ents.is_empty() || eof);
        verf = v;
        pages += 1;
        // While paging: files go and come.
        if pages == 3 {
            for i in 0..50 {
                j.unlink(&format!("/home/exp/f{:04}", i * 7)).unwrap();
                w.write_file(&format!("/home/exp/new{i:02}"), b"");
            }
        }
        for e in &ents {
            assert!(e.name_handle.is_some() && e.name_attributes.is_some());
            seen.push(String::from_utf8(e.name.0.clone()).unwrap());
        }
        if eof {
            break;
        }
        cookie = ents.last().unwrap().cookie;
    }
    assert!(pages > 10, "{pages} pages");
    // No name twice.
    let set: HashSet<&String> = seen.iter().collect();
    assert_eq!(set.len(), seen.len());
    // Every file there from start to end was seen.
    let removed: HashSet<String> = (0..50).map(|i| format!("f{:04}", i * 7)).collect();
    for i in 0..2000 {
        let n = format!("f{i:04}");
        if !removed.contains(&n) {
            assert!(set.contains(&n), "{n} skipped");
        }
    }
    // READDIR's count bounds every page.
    let (one, _, eof) = readdir(&c, &p, &root, 0, [0; 8], 1024).unwrap();
    assert!(!eof);
    assert!(one.len() < 40);
}
