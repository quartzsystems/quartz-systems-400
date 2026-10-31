// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Catalog operations and status transitions over the real file system
//! and memory.

#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::items_after_statements
)]

use std::sync::Arc;

use imgclg::fs::join;
use imgclg::model::META_FILE;
use imgclg::ops::{
    AddImgClgE, AddVrtVol, ChgImgClgE, CrtImgClg, EntryOption, LoadOption, Problem, ToFile,
    VOLUME_LIST, VfyImgClg, VfyType, imgsiz,
};
use imgclg::{CatalogFs, EntryStatus, Error, ImgType, MemFs, NoTape, Registry, StdFs, TapeImages};
use optical::{IsoBuilder, VolumeSet};

fn crt(name: &str, dir: &str, kind: ImgType) -> CrtImgClg {
    CrtImgClg {
        name: name.into(),
        dir: dir.into(),
        kind,
        create_dir: true,
        add: AddVrtVol::None,
        text: format!("{name} images"),
        epoch: 1000,
    }
}

fn status(r: &Registry, name: &str) -> Vec<(u16, EntryStatus)> {
    r.catalog(name)
        .unwrap()
        .entries
        .iter()
        .map(|e| (e.index, e.status))
        .collect()
}

fn plain_iso(volid: &str, set: Option<(u16, u16)>, install: bool) -> Vec<u8> {
    let mut b = IsoBuilder::new(volid, 0);
    b.add_file("README", b"x".to_vec()).unwrap();
    if install {
        b.add_file(optical::manifest::PATH, b"QS400-MEDIA 1\n".to_vec())
            .unwrap();
    }
    if let Some((size, seq)) = set {
        b.volume_set("QS400_SET", size, seq);
    }
    b.build().unwrap()
}

fn exercise(fs: Arc<dyn CatalogFs>) {
    let mut r = Registry::new(fs.clone(), Arc::new(NoTape));
    assert!(matches!(
        r.crtimgclg(&CrtImgClg {
            create_dir: false,
            ..crt("A", "/clg/a", ImgType::Opt)
        }),
        Err(Error::NoDir(_))
    ));
    r.crtimgclg(&crt("myclg", "/clg/a", ImgType::Opt)).unwrap();
    assert!(matches!(
        r.crtimgclg(&crt("MYCLG", "/clg/b", ImgType::Opt)),
        Err(Error::Exists(_))
    ));
    assert!(matches!(
        r.crtimgclg(&crt("OTHER", "/clg/a", ImgType::Opt)),
        Err(Error::DirInUse { .. })
    ));
    assert!(fs.exists(&join("/clg/a", META_FILE)));

    // A new writable image, an ISO copied in, and one already there.
    let mut a = AddImgClgE::new_image("save.iso", 8);
    a.volume = Some("SAVE01".into());
    assert_eq!(r.addimgclge("MYCLG", &a).unwrap(), 1);
    fs.create_dir_all("/downloads").unwrap();
    fs.write("/downloads/cd.iso", &plain_iso("CDVOL", None, false))
        .unwrap();
    assert_eq!(
        r.addimgclge("MYCLG", &AddImgClgE::file("/downloads/cd.iso"))
            .unwrap(),
        2
    );
    assert!(fs.exists("/clg/a/cd.iso"));
    fs.write("/clg/a/local.iso", &plain_iso("LOCAL", None, false))
        .unwrap();
    let mut l = AddImgClgE::file("local.iso");
    l.index = Some(5);
    assert_eq!(r.addimgclge("MYCLG", &l).unwrap(), 5);
    assert!(matches!(
        r.addimgclge("MYCLG", &l),
        Err(Error::IndexInUse(5))
    ));
    fs.write("/clg/a/dup.iso", &plain_iso("LOCAL", None, false))
        .unwrap();
    assert!(matches!(
        r.addimgclge("MYCLG", &AddImgClgE::file("dup.iso")),
        Err(Error::DuplicateVolume(_))
    ));
    fs.write("/clg/a/junk.iso", b"not an image").unwrap();
    assert!(matches!(
        r.addimgclge("MYCLG", &AddImgClgE::file("junk.iso")),
        Err(Error::BadImage { .. })
    ));
    assert!(matches!(
        r.addimgclge("MYCLG", &AddImgClgE::file("none.iso")),
        Err(Error::NoImage(_))
    ));
    assert!(
        r.addimgclge(
            "MYCLG",
            &AddImgClgE {
                size_mb: None,
                ..AddImgClgE::new_image("x.iso", 1)
            }
        )
        .is_err()
    );

    let c = r.catalog("myclg").unwrap();
    assert_eq!(c.entries[0].volume, "SAVE01");
    assert!(!c.entries[0].write_protect, "a new image is writable");
    assert!(c.entries[1].write_protect, "a plain ISO is protected");
    assert!(matches!(
        r.chgimgclge(
            "MYCLG",
            2,
            &ChgImgClgE {
                write_protect: Some(false),
                text: None
            }
        ),
        Err(Error::ReadOnlyImage(2))
    ));
    r.chgimgclge(
        "MYCLG",
        1,
        &ChgImgClgE {
            write_protect: None,
            text: Some("saves".into()),
        },
    )
    .unwrap();

    // Status: all loaded; loading the catalog mounts the first.
    use EntryStatus::{Available, Loaded, Mounted};
    assert_eq!(status(&r, "MYCLG"), [(1, Loaded), (2, Loaded), (5, Loaded)]);
    r.lodimgclg("MYCLG", LoadOption::Load, "opt01").unwrap();
    assert_eq!(
        status(&r, "MYCLG"),
        [(1, Mounted), (2, Loaded), (5, Loaded)]
    );
    r.lodimgclge("MYCLG", 2, EntryOption::Mount).unwrap();
    assert_eq!(
        status(&r, "MYCLG"),
        [(1, Loaded), (2, Mounted), (5, Loaded)]
    );
    r.lodimgclge("MYCLG", 2, EntryOption::Unload).unwrap();
    assert_eq!(
        status(&r, "MYCLG"),
        [(1, Mounted), (2, Available), (5, Loaded)]
    );
    r.lodimgclge("MYCLG", 2, EntryOption::Load).unwrap();
    assert_eq!(
        status(&r, "MYCLG"),
        [(1, Mounted), (2, Loaded), (5, Loaded)]
    );

    // /QOPT shows the mounted volume; it can be written through the set.
    let set = VolumeSet::new(Arc::new(|| 2000));
    r.sync_volumes(&set).unwrap();
    assert_eq!(set.names(), ["SAVE01"]);
    set.with("SAVE01", |v| v.put_file("/QGPL", b"lib".to_vec(), false))
        .unwrap();
    r.lodimgclge("MYCLG", 5, EntryOption::Mount).unwrap();
    r.sync_volumes(&set).unwrap();
    assert_eq!(set.names(), ["LOCAL"]);
    let img = optical::Volume::open(fs.image("/clg/a/save.iso").unwrap(), false).unwrap();
    assert_eq!(img.read_all("/QGPL").unwrap(), b"lib", "unloading flushed");

    // Devices.
    r.crtimgclg(&crt("SECOND", "/clg/b", ImgType::Opt)).unwrap();
    assert!(matches!(
        r.lodimgclg("SECOND", LoadOption::Load, "OPT01"),
        Err(Error::DeviceInUse(_))
    ));
    assert!(matches!(
        r.lodimgclg("MYCLG", LoadOption::Load, "OPT02"),
        Err(Error::DeviceInUse(_))
    ));
    assert!(matches!(r.dltimgclg("MYCLG", true), Err(Error::Loaded(_))));

    let rows = r.wrkimgclg();
    assert_eq!(rows[0].name, "MYCLG");
    assert_eq!(rows[0].status, "*READY");
    let d = r.wrkimgclge("MYCLG").unwrap();
    assert_eq!(d.device, "OPT01");
    assert_eq!(d.entries[2].status, "*MOUNTED");
    assert_eq!(d.entries[0].text, "saves");

    // The metadata file carries everything.
    let again = Registry::load(fs.clone(), Arc::new(NoTape), &["/clg/a", "/clg/b"]).unwrap();
    assert_eq!(again.catalog("MYCLG").unwrap(), r.catalog("MYCLG").unwrap());

    // Removing the mounted entry mounts the next loaded one.
    r.rmvimgclge("MYCLG", 5, true).unwrap();
    assert_eq!(status(&r, "MYCLG"), [(1, Mounted), (2, Loaded)]);
    assert!(fs.exists("/clg/a/local.iso"));
    r.rmvimgclge("MYCLG", 2, false).unwrap();
    assert!(!fs.exists("/clg/a/cd.iso"));
    assert!(matches!(
        r.rmvimgclge("MYCLG", 9, true),
        Err(Error::NoEntry { .. })
    ));

    r.lodimgclg("MYCLG", LoadOption::Unload, "OPT01").unwrap();
    r.sync_volumes(&set).unwrap();
    assert!(set.names().is_empty());
    r.dltimgclg("MYCLG", false).unwrap();
    assert!(!fs.exists("/clg/a/save.iso"));
    assert!(matches!(r.catalog("MYCLG"), Err(Error::NotFound(_))));
}

#[test]
fn catalogs_in_memory() {
    exercise(Arc::new(MemFs::new()));
}

#[test]
fn catalogs_on_the_real_file_system() {
    let root = std::env::temp_dir().join(format!("imgclg-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    exercise(Arc::new(StdFs::new(&root)));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn verify_an_install_media_set() {
    let fs: Arc<dyn CatalogFs> = Arc::new(MemFs::new());
    let mut r = Registry::new(fs.clone(), Arc::new(NoTape));
    r.crtimgclg(&crt("INSTALL", "/inst", ImgType::Opt)).unwrap();
    fs.write(
        "/inst/disk2.iso",
        &plain_iso("QS400_2", Some((2, 2)), false),
    )
    .unwrap();
    fs.write("/inst/disk1.iso", &plain_iso("QS400_1", Some((2, 1)), true))
        .unwrap();
    r.addimgclge("INSTALL", &AddImgClgE::file("disk2.iso"))
        .unwrap();
    let lic = VfyImgClg {
        kind: VfyType::Lic,
        sort: false,
    };
    let rep = r.vfyimgclg("INSTALL", &lic).unwrap();
    assert!(
        rep.problems
            .contains(&Problem::Incomplete { have: 1, need: 2 })
    );
    r.addimgclge("INSTALL", &AddImgClgE::file("disk1.iso"))
        .unwrap();
    let rep = r.vfyimgclg("INSTALL", &lic).unwrap();
    assert_eq!(rep.problems, [Problem::OutOfOrder]);
    let rep = r
        .vfyimgclg("INSTALL", &VfyImgClg { sort: true, ..lic })
        .unwrap();
    assert!(rep.ok() && rep.sorted);
    let c = r.catalog("INSTALL").unwrap();
    assert_eq!(c.entries[0].volume, "QS400_1");
    assert_eq!(c.entries[0].index, 1);
    assert!(r.vfyimgclg("INSTALL", &lic).unwrap().ok());
    fs.remove_file("/inst/disk2.iso").unwrap();
    let rep = r
        .vfyimgclg(
            "INSTALL",
            &VfyImgClg {
                kind: VfyType::Other,
                sort: false,
            },
        )
        .unwrap();
    assert_eq!(rep.problems, [Problem::Missing(2)]);

    // A single volume that is not install media.
    r.crtimgclg(&crt("PLAIN", "/plain", ImgType::Opt)).unwrap();
    fs.write("/plain/a.iso", &plain_iso("PLAINVOL", None, false))
        .unwrap();
    r.addimgclge("PLAIN", &AddImgClgE::file("a.iso")).unwrap();
    let rep = r.vfyimgclg("PLAIN", &lic).unwrap();
    assert_eq!(rep.problems, [Problem::NotInstallMedia]);
    assert!(
        r.vfyimgclg(
            "PLAIN",
            &VfyImgClg {
                kind: VfyType::Ptf,
                sort: false
            }
        )
        .unwrap()
        .ok()
    );
}

#[test]
fn addvrtvol_dir_follows_volume_list() {
    let fs: Arc<dyn CatalogFs> = Arc::new(MemFs::new());
    fs.create_dir_all("/d").unwrap();
    for (f, v) in [("a.iso", "AVOL"), ("b.iso", "BVOL"), ("c.iso", "CVOL")] {
        fs.write(&join("/d", f), &plain_iso(v, None, false))
            .unwrap();
    }
    fs.write("/d/notes.txt", b"not an image").unwrap();
    let mut r = Registry::new(fs.clone(), Arc::new(NoTape));
    r.crtimgclg(&CrtImgClg {
        add: AddVrtVol::Dir,
        ..crt("BYNAME", "/d", ImgType::Opt)
    })
    .unwrap();
    let vols: Vec<String> = r
        .catalog("BYNAME")
        .unwrap()
        .entries
        .iter()
        .map(|e| e.volume.clone())
        .collect();
    assert_eq!(vols, ["AVOL", "BVOL", "CVOL"]);
    r.dltimgclg("BYNAME", true).unwrap();
    fs.write(&join("/d", VOLUME_LIST), b"c.iso\n# comment\na.iso\n")
        .unwrap();
    r.crtimgclg(&CrtImgClg {
        add: AddVrtVol::Dir,
        ..crt("LISTED", "/d", ImgType::Opt)
    })
    .unwrap();
    let vols: Vec<String> = r
        .catalog("LISTED")
        .unwrap()
        .entries
        .iter()
        .map(|e| e.volume.clone())
        .collect();
    assert_eq!(vols, ["CVOL", "AVOL"]);
}

/// A tape stand-in: the volume name is the file's text.
struct FakeTape;
impl TapeImages for FakeTape {
    fn create(
        &self,
        fs: &dyn CatalogFs,
        path: &str,
        volume: &str,
        _size: u64,
    ) -> Result<(), String> {
        fs.write(path, volume.as_bytes()).map_err(|e| e.to_string())
    }
    fn volume_id(&self, fs: &dyn CatalogFs, path: &str) -> Result<String, String> {
        fs.read(path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| e.to_string())
    }
}

#[test]
fn tape_catalogs_call_the_constructor() {
    let fs: Arc<dyn CatalogFs> = Arc::new(MemFs::new());
    let mut r = Registry::new(fs.clone(), Arc::new(FakeTape));
    r.crtimgclg(&CrtImgClg {
        add: AddVrtVol::New {
            count: 3,
            size_mb: 100,
        },
        ..crt("TAPES", "/tapes", ImgType::Tap)
    })
    .unwrap();
    let c = r.catalog("TAPES").unwrap();
    assert_eq!(c.entries.len(), 3);
    assert_eq!(c.entries[2].file, "TAPES003");
    assert_eq!(fs.read("/tapes/TAPES001").unwrap(), b"TAPES001");
    let mut a = AddImgClgE::new_image("", 10);
    a.to_file = ToFile::Gen;
    a.volume = Some("VOLX".into());
    assert_eq!(r.addimgclge("TAPES", &a).unwrap(), 4);
    assert!(
        r.vfyimgclg(
            "TAPES",
            &VfyImgClg {
                kind: VfyType::Other,
                sort: false
            }
        )
        .unwrap()
        .ok()
    );
    r.lodimgclg("TAPES", LoadOption::Load, "TAPVRT01").unwrap();
    assert!(r.presented().is_empty(), "tape volumes are not optical");

    let mut none = Registry::new(fs, Arc::new(NoTape));
    none.crtimgclg(&crt("NOTAPE", "/nt", ImgType::Tap)).unwrap();
    assert!(matches!(
        none.addimgclge("NOTAPE", &a),
        Err(Error::BadImage { .. })
    ));
}

#[test]
fn sizes_and_names() {
    assert_eq!(imgsiz("*DVD4700").unwrap(), 4700);
    assert_eq!(imgsiz("100").unwrap(), 100);
    assert!(imgsiz("0").is_err() && imgsiz("big").is_err());
    let mut r = Registry::new(Arc::new(MemFs::new()), Arc::new(NoTape));
    let e = r.crtimgclg(&crt("1BAD", "/x", ImgType::Opt)).unwrap_err();
    assert_eq!(e.msgid(), "CPFBC0F");
    assert_eq!(Error::NotFound("X".into()).msgid(), "CPFBC01");
}
