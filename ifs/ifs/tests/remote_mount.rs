// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `MOUNT TYPE(*NFS)` and `UNMOUNT` over a remote file system in
//! memory: the factory, the mount-over rules, the mount table's view,
//! `statvfs` and `pathconf`, the busy rules and the write-back first.

mod remote_common;

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_CREAT, O_RDONLY, O_WRONLY};
use ifs::remote::{CcsidSel, MountCcsid, RemoteConfig};
use remote_common::{info, mounted, rig_with};

#[test]
fn mount_needs_a_factory_iosyscfg_and_a_directory() {
    let r = rig_with(info(), RemoteConfig::default());
    let sec = r.client("QSECOFR", 1);
    let bob = r.client("BOB", 2);
    let m = |c: &dyn IfsClient, over: &str| {
        c.mount_remote("*NFS", "srv:/exp", over, "", MountCcsid::default())
    };
    assert_eq!(m(&bob, "/mnt"), Err(Errno::EPERM));
    assert_eq!(m(&sec, "/nowhere"), Err(Errno::ENOENT));
    sec.mkdir("/f", 0o755).unwrap();
    let fd = sec
        .open("/file", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o644))
        .unwrap();
    sec.close(fd).unwrap();
    assert_eq!(m(&sec, "/file"), Err(Errno::ENOTDIR));
    assert_eq!(
        sec.mount_remote("*NFS", "nosuch:/x", "/mnt", "", MountCcsid::default()),
        Err(Errno::ENOENT)
    );
    // A kind the factory does not make is refused as before.
    assert_eq!(
        sec.mount_remote("*NETWORK", "srv:/exp", "/mnt", "", MountCcsid::default()),
        Err(Errno::ENOTSUP)
    );
    let id = m(&sec, "/mnt").unwrap();
    assert!(id >= 100);
    assert_eq!(m(&sec, "/mnt"), Err(Errno::EBUSY));
    // Nothing is mounted inside a remote mount.
    sec.mkdir("/mnt/sub", 0o777).unwrap();
    assert_eq!(m(&sec, "/mnt/sub"), Err(Errno::ENOTSUP));
    assert_eq!(
        sec.mount("/dev/QASP01/x.udfs", "/mnt/sub", false),
        Err(Errno::ENOENT)
    );
    // With no factory there is no remote mount.
    r.ifs.set_remote_factory(None, RemoteConfig::default());
    assert_eq!(m(&sec, "/f"), Err(Errno::ENOTSUP));
}

#[test]
fn the_mount_table_shows_it() {
    let r = mounted();
    let sec = r.client("QSECOFR", 1);
    let all = sec.mounts().unwrap();
    let nfs = all.iter().find(|m| m.kind == "*NFS").unwrap();
    assert_eq!(nfs.at, "/mnt");
    assert_eq!(nfs.source, "srv:/exp");
    assert_eq!(nfs.responding, Some(true));
    assert_eq!(nfs.path_ccsid, 819);
    assert_eq!(nfs.data_ccsid, 65535);
    assert!(all.iter().any(|m| m.kind == "root" && m.at == "/"));
    assert_eq!(
        r.factory.last_options.lock().unwrap().as_deref(),
        Some("rw")
    );
    let v = sec.statvfs("/mnt").unwrap();
    assert_eq!(v.name, "NFS");
    assert!(v.case_sensitive);
    assert_eq!(v.flag, 0);
    assert_eq!(v.blocks, (1u64 << 30) / 4096);
    assert_eq!(
        sec.pathconf("/mnt", ifs::ops::flags::PC_NAME_MAX).unwrap(),
        Some(255)
    );
}

#[test]
fn ccsids_resolve_against_the_job() {
    let r = rig_with(info(), RemoteConfig::default());
    let sec = r.client("QSECOFR", 1);
    sec.mount_remote(
        "*NFS",
        "srv:/exp",
        "/mnt",
        "",
        MountCcsid {
            data: CcsidSel::Ccsid(819),
            path: CcsidSel::Job,
        },
    )
    .unwrap();
    let got = r.factory.last_ccsid.lock().unwrap().unwrap();
    assert_eq!(got.data, CcsidSel::Ccsid(819));
    assert_eq!(got.path, CcsidSel::Ccsid(37));
    // Names go in EBCDIC to such a server.
    sec.mkdir("/mnt/Abc", 0o777).unwrap();
    let ebcdic = ccsid::conv::from_str(37, "Abc").unwrap();
    let shown = String::from_utf8_lossy(&ebcdic).into_owned();
    assert_eq!(r.server.names(""), vec![shown]);
    assert_eq!(sec.stat("/mnt/Abc", true).unwrap().ccsid, 819);
    assert_eq!(sec.read_dir("/mnt").unwrap()[0].name, "Abc");
}

#[test]
fn unmount_waits_for_nothing_in_use() {
    let r = mounted();
    let sec = r.client("QSECOFR", 1);
    let bob = r.client("BOB", 2);
    let fd = bob
        .open("/mnt/a", OpenArgs::new(O_WRONLY | O_CREAT).mode(0o666))
        .unwrap();
    bob.write(fd, b"kept").unwrap();
    assert_eq!(bob.unmount_kind("*NFS", "/mnt"), Err(Errno::EPERM));
    assert_eq!(sec.unmount_kind("*NFS", "/mnt"), Err(Errno::EBUSY));
    bob.close(fd).unwrap();
    bob.chdir("/mnt").unwrap();
    assert_eq!(sec.unmount_kind("*NFS", "/mnt"), Err(Errno::EBUSY));
    bob.chdir("/").unwrap();
    let h = bob.opendir("/mnt").unwrap();
    assert_eq!(sec.unmount_kind("*NFS", "/mnt"), Err(Errno::EBUSY));
    bob.closedir(h).unwrap();
    assert_eq!(sec.unmount_kind("*UDFS", "/mnt"), Err(Errno::EINVAL));
    assert_eq!(sec.unmount_kind("*NFS", "/MNT"), Ok(1));
    assert!(r.server.unmounted());
    assert_eq!(r.server.file("a").as_deref(), Some(&b"kept"[..]));
    // The directory under it is back.
    assert_eq!(sec.read_dir("/mnt").unwrap(), Vec::new());
    assert_eq!(sec.unmount_kind("*NFS", "/mnt"), Err(Errno::EINVAL));
}

#[test]
fn unmount_all_of_every_type() {
    let r = mounted();
    let sec = r.client("QSECOFR", 1);
    sec.crtudfs(
        "/dev/QASP01/u.udfs",
        false,
        ifs::meta::CrtObjAud::SysVal,
        "",
    )
    .unwrap();
    sec.mkdir("/u", 0o755).unwrap();
    sec.mount("/dev/QASP01/u.udfs", "/u", false).unwrap();
    assert_eq!(sec.unmount_kind("*ALL", "*ALL"), Ok(2));
    assert!(sec.mounts().unwrap().iter().all(|m| m.id < 100));
    let fd = sec.open("/mnt", OpenArgs::new(O_RDONLY)).unwrap();
    sec.close(fd).unwrap();
}
