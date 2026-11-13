// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Who a call runs as and what an export lets a host do: root squash,
//! `ROOT=`, `ANON=`, `ANON=-1`, `AUTH_NONE`, a UID no profile has, `RO`,
//! `RW=`, `ACCESS=`, `NOSUID`, `VERS=`.

#![allow(clippy::many_single_char_names)]
mod srv3_common;

use nfs::rpc::Credential;
use nfs::server::{Anon, HostPat};
use nfs::types::nfs3::{self as t3, nfsstat3 as S};
use srv3_common::{ANON_UID, Client, V3, World, mode};

const PEER: &str = "10.0.0.5:900";

fn host() -> HostPat {
    HostPat::Addr("10.0.0.5".parse().unwrap())
}

fn owner_uid(w: &World, path: &str) -> u32 {
    use ifs::client::IfsClient;
    w.job(9).stat(path, false).unwrap().uid
}

#[test]
fn root_is_squashed_unless_the_host_is_in_root() {
    let w = World::new();
    let mut e = w.export(1, "/home/exp");
    e.anon = Anon::Uid(ANON_UID);
    w.put(e.clone());
    let root = w.handle(1, "/home/exp");
    let p = w.nfs3();
    let c = Client::new(PEER, 0);
    let v = V3 { p: &p, c: &c };
    v.create(&root, "squashed").unwrap();
    assert_eq!(owner_uid(&w, "/home/exp/squashed"), ANON_UID);
    e.root_hosts = vec![host()];
    w.put(e);
    v.create(&root, "rooted").unwrap();
    assert_eq!(owner_uid(&w, "/home/exp/rooted"), 0);
    // Another host is still squashed.
    let other = Client::new("10.0.0.6:900", 0);
    V3 { p: &p, c: &other }.create(&root, "other").unwrap();
    assert_eq!(owner_uid(&w, "/home/exp/other"), ANON_UID);
}

#[test]
fn unknown_uids_and_auth_none_are_the_anonymous_user_or_refused() {
    let w = World::new();
    let mut e = w.export(1, "/home/exp");
    e.anon = Anon::Uid(ANON_UID);
    w.put(e.clone());
    let root = w.handle(1, "/home/exp");
    let p = w.nfs3();
    // A UID no profile has: retried as the anonymous user.
    let c = Client::new(PEER, 4242);
    let v = V3 { p: &p, c: &c };
    v.create(&root, "nobody").unwrap();
    assert_eq!(owner_uid(&w, "/home/exp/nobody"), ANON_UID);
    // AUTH_NONE: the anonymous user.
    let mut none = Client::new(PEER, 0);
    none.cred = Credential::None;
    V3 { p: &p, c: &none }.create(&root, "none").unwrap();
    assert_eq!(owner_uid(&w, "/home/exp/none"), ANON_UID);
    // ANON=-1: refused, the known user still served.
    e.anon = Anon::Refuse;
    w.put(e.clone());
    assert_eq!(v.getattr(&root), Err(S::NFS3ERR_ACCES));
    assert_eq!(V3 { p: &p, c: &none }.getattr(&root), Err(S::NFS3ERR_ACCES));
    let bob = Client::new(PEER, 100);
    assert!(V3 { p: &p, c: &bob }.getattr(&root).is_ok());
    // The default anonymous user is the service's: an IFS without one
    // (LocalIfs names none) refuses it as a profile it cannot find.
    e.anon = Anon::Default;
    w.put(e);
    assert_eq!(v.getattr(&root), Err(S::NFS3ERR_ACCES));
}

#[test]
fn read_only_exports_and_rw_lists() {
    let w = World::new();
    let mut e = w.export(1, "/home/exp");
    w.write_file("/home/exp/f", b"data");
    e.ro = true;
    w.put(e.clone());
    let root = w.handle(1, "/home/exp");
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    let f = v.lookup(&root, "f").unwrap();
    assert_eq!(v.read(&f, 0, 10).unwrap().0, b"data");
    assert_eq!(v.create(&root, "x"), Err(S::NFS3ERR_ROFS));
    assert_eq!(v.mkdir(&root, "x"), Err(S::NFS3ERR_ROFS));
    assert_eq!(v.remove(&root, "f"), Err(S::NFS3ERR_ROFS));
    assert_eq!(
        v.write(&f, 0, b"x", t3::stable_how::FILE_SYNC).err(),
        Some(S::NFS3ERR_ROFS)
    );
    assert_eq!(v.setattr(&f, mode(0o600)), Err(S::NFS3ERR_ROFS));
    // RW= another host: this one may only read.
    e.ro = false;
    e.rw = Some(vec![HostPat::Addr("10.9.9.9".parse().unwrap())]);
    w.put(e.clone());
    assert_eq!(v.create(&root, "x"), Err(S::NFS3ERR_ROFS));
    e.rw = Some(vec![HostPat::Net("10.0.0.0".parse().unwrap(), 24)]);
    w.put(e);
    v.create(&root, "x").unwrap();
}

#[test]
fn hosts_not_in_access_are_refused() {
    let w = World::new();
    let mut e = w.export(1, "/home/exp");
    e.access = Some(vec![HostPat::Addr("10.1.1.1".parse().unwrap())]);
    w.put(e.clone());
    let root = w.handle(1, "/home/exp");
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    assert_eq!(v.getattr(&root), Err(S::NFS3ERR_ACCES));
    assert_eq!(v.lookup(&root, "x"), Err(S::NFS3ERR_ACCES));
    e.access = Some(vec![host()]);
    // VERS=4 only: version 3 calls are refused.
    e.versions = vec![4];
    w.put(e.clone());
    assert_eq!(v.getattr(&root), Err(S::NFS3ERR_ACCES));
    e.versions = vec![3];
    w.put(e);
    assert!(v.getattr(&root).is_ok());
}

#[test]
fn nosuid_clears_set_id_bits() {
    let w = World::new();
    let mut e = w.export(1, "/home/exp");
    e.nosuid = true;
    w.put(e.clone());
    let root = w.handle(1, "/home/exp");
    let p = w.nfs3();
    let c = Client::new(PEER, 100);
    let v = V3 { p: &p, c: &c };
    let f = v
        .create_how(&root, "s", t3::createhow3::GUARDED(mode(0o4755)))
        .unwrap();
    assert_eq!(v.getattr(&f).unwrap().mode & 0o6000, 0);
    v.setattr(&f, mode(0o6755)).unwrap();
    assert_eq!(v.getattr(&f).unwrap().mode & 0o6000, 0);
    e.nosuid = false;
    w.put(e);
    v.setattr(&f, mode(0o4755)).unwrap();
    assert_eq!(v.getattr(&f).unwrap().mode & 0o4000, 0o4000);
}

#[test]
fn authority_is_the_profiles_not_the_wires_gids() {
    let w = World::new();
    let e = w.export(1, "/home/exp");
    w.put(e);
    let root = w.handle(1, "/home/exp");
    let p = w.nfs3();
    let bob = Client::new(PEER, 100);
    let vb = V3 { p: &p, c: &bob };
    let d = vb.mkdir(&root, "bob").unwrap();
    vb.setattr(&d, mode(0o700)).unwrap();
    // ZED claims BOB's GID and every group on the wire: still refused.
    let mut zed = Client::new(PEER, 102);
    zed.cred = Credential::sys(b"client1", 102, 100, &[0, 100, 200, 500]);
    let vz = V3 { p: &p, c: &zed };
    assert_eq!(vz.create(&d, "x"), Err(S::NFS3ERR_ACCES));
    assert_eq!(vz.lookup(&d, "x"), Err(S::NFS3ERR_ACCES));
    vb.create(&d, "x").unwrap();
}
