// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/QOPT` over loaded optical volumes: an ISO 9660 image read-only, a
//! writable virtual volume read-write.

mod common;

use std::sync::Arc;

use ifs::backend::mem::MemBackend;
use ifs::errno::Errno;
use ifs::ops::audit::MemAudit;
use ifs::ops::flags as f;
use ifs::ops::{Config, Ifs, OpenArgs};
use ifs::vfs::optical::Volumes;
use ifs::vfs::qsys::MemQsys;
use optical::{ImageStore, IsoBuilder, MemImage, Volume, VolumeSet};
use sec::auth::bits::Auth;

const MIB: usize = 1024 * 1024;

fn setup() -> (common::Sys, VolumeSet, Arc<dyn ImageStore>) {
    let mut b = IsoBuilder::new("CDVOL", 0);
    b.add_file("README", b"read me".to_vec()).unwrap();
    b.add_file("docs/Guide.txt", b"a guide".to_vec()).unwrap();
    let cd: Arc<dyn ImageStore> = Arc::new(MemImage::new(b.build().unwrap()));
    let save: Arc<dyn ImageStore> = Arc::new(MemImage::default());
    let set = VolumeSet::new(Arc::new(|| 1234));
    set.insert(Volume::open(cd, false).unwrap()).unwrap();
    set.insert(Volume::create(save.clone(), "SAVE01", "", MIB as u64, 1).unwrap())
        .unwrap();
    let env = common::env();
    let ifs = Ifs::with_optical(
        MemBackend::new(),
        Box::new(MemQsys::new(Auth::USE)),
        Box::new(Volumes::new(set.clone())),
        Config::default(),
    );
    ifs.ensure_layout(&env).unwrap();
    (
        common::Sys {
            ifs,
            env,
            audit: MemAudit::default(),
        },
        set,
        save,
    )
}

#[test]
fn iso_volumes_read_only() {
    let (s, _, _) = setup();
    let bob = common::user("BOB");
    let cx = s.cx(&bob, 1);
    let names: Vec<String> = s
        .ifs
        .read_dir(&cx, "/QOPT")
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert_eq!(names, ["CDVOL", "SAVE01"]);
    assert_eq!(s.get(&cx, "/QOPT/CDVOL/README"), b"read me");
    assert_eq!(s.get(&cx, "/qopt/cdvol/docs/guide.txt"), b"a guide");
    assert_eq!(s.ifs.stat(&cx, "/QOPT/CDVOL/README").unwrap().size, 7);
    assert_eq!(s.ifs.mkdir(&cx, "/QOPT/CDVOL/x", 0o755), Err(Errno::EROFS));
    assert_eq!(
        s.ifs.creat(&cx, "/QOPT/CDVOL/new", 0o644),
        Err(Errno::EROFS)
    );
    assert_eq!(
        s.ifs
            .open(&cx, "/QOPT/CDVOL/README", OpenArgs::new(f::O_RDWR))
            .err(),
        Some(Errno::EROFS)
    );
    assert_eq!(s.ifs.unlink(&cx, "/QOPT/CDVOL/README"), Err(Errno::EROFS));
    assert_eq!(s.ifs.mkdir(&cx, "/QOPT/NEWVOL", 0o755), Err(Errno::EROFS));
}

#[test]
fn writable_volumes_take_changes() {
    let (s, set, save) = setup();
    let bob = common::user("BOB");
    let cx = s.cx(&bob, 1);
    s.ifs.mkdir(&cx, "/QOPT/SAVE01/QGPL", 0o755).unwrap();
    s.put(&cx, "/QOPT/SAVE01/QGPL/SAVF", b"saved objects");
    assert_eq!(s.get(&cx, "/QOPT/SAVE01/QGPL/SAVF"), b"saved objects");
    // Append through a second open.
    let fd = s
        .ifs
        .open(
            &cx,
            "/QOPT/SAVE01/QGPL/SAVF",
            OpenArgs::new(f::O_WRONLY | f::O_APPEND),
        )
        .unwrap();
    s.ifs.write(&cx, fd, b" and more").unwrap();
    s.ifs.close(&cx, fd).unwrap();
    // Closing wrote the image: a fresh look at the store sees it.
    let v = Volume::open(save.clone(), false).unwrap();
    assert_eq!(v.read_all("/QGPL/SAVF").unwrap(), b"saved objects and more");
    assert_eq!(v.label().unwrap().changed, 1234);

    assert_eq!(s.ifs.rmdir(&cx, "/QOPT/SAVE01/QGPL"), Err(Errno::ENOTEMPTY));
    s.ifs.unlink(&cx, "/QOPT/SAVE01/QGPL/SAVF").unwrap();
    s.ifs.rmdir(&cx, "/QOPT/SAVE01/QGPL").unwrap();
    assert!(s.ifs.read_dir(&cx, "/QOPT/SAVE01").unwrap().is_empty());

    // Over capacity: the close reports it and nothing lands.
    let fd = s.ifs.creat(&cx, "/QOPT/SAVE01/BIG", 0o644).unwrap();
    s.ifs.write(&cx, fd, &vec![0u8; 2 * MIB]).unwrap();
    assert_eq!(s.ifs.close(&cx, fd), Err(Errno::ENOSPC));
    assert_eq!(s.ifs.stat(&cx, "/QOPT/SAVE01/BIG").unwrap().size, 0);

    // Write protected: no more changes.
    set.with("SAVE01", |v| {
        v.set_write_protect(true);
        Ok(())
    })
    .unwrap();
    assert_eq!(s.ifs.creat(&cx, "/QOPT/SAVE01/x", 0o644), Err(Errno::EROFS));
    assert_eq!(s.ifs.mkdir(&cx, "/QOPT/SAVE01/d", 0o755), Err(Errno::EROFS));
}
