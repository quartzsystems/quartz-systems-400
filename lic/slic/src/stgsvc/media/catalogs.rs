// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Image catalogs over the IFS: each `*IMGCLG` object in `QUSRSYS`
//! names its directory (the catalog's own state is the metadata file
//! there, `imgclg`'s). The catalogs reach the IFS root's xfs directly,
//! as the service that owns it; virtual tape volumes are files there
//! that `tape::VirtualTape` opens.

use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::sync::Arc;

use imgclg::fs::join;
use imgclg::model::META_FILE;
use imgclg::{CatalogFs, Registry, StdFs, TapeImages};
use store::obj::{JobHandle, ObjectId, ops};
use store::txn::Txn;
use tape::volume::{Check, InzOptions, inztap};
use tape::{VirtualTape, VolumeOptions};

use crate::ipl::system::{System, now_unix};
use crate::stgsvc::ifs::ROOT;

/// The first line of an `*IMGCLG` object's payload; the directory follows.
const MAGIC: &str = "QS400-IMGCLG-OBJ 1";

/// The `*IMGCLG` type code.
pub(crate) const IMGCLG: u16 = 0x0020;

/// Where an IFS path is on the host.
pub(crate) fn host_path(path: &str) -> PathBuf {
    let mut p = PathBuf::from(ROOT);
    for c in path.split('/').filter(|c| !c.is_empty() && *c != "." && *c != "..") {
        p.push(c);
    }
    p
}

/// An image file opened for reading and writing.
pub(crate) fn open_image(path: &str) -> std::io::Result<File> {
    OpenOptions::new().read(true).write(true).open(host_path(path))
}

/// Virtual tape volumes as files under the IFS root.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct VirtTapes;

impl TapeImages for VirtTapes {
    fn create(&self, _: &dyn CatalogFs, path: &str, volume: &str, size_mb: u64) -> Result<(), String> {
        let f = File::create_new(host_path(path)).map_err(|e| format!("{path}: {e}"))?;
        let mut t = VirtualTape::create(f, &VolumeOptions::imgsiz(size_mb, volume)).map_err(|e| e.to_string())?;
        // A new volume is labeled with its name, as INZTAP would.
        inztap(
            &mut t,
            &InzOptions {
                new_vol: Some(volume.chars().take(6).collect()),
                check: Check::No,
                ..InzOptions::default()
            },
        )
        .map_err(|e| e.to_string())
    }

    fn volume_id(&self, _: &dyn CatalogFs, path: &str) -> Result<String, String> {
        let f = open_image(path).map_err(|e| format!("{path}: {e}"))?;
        let t = VirtualTape::open(f).map_err(|e| e.to_string())?;
        Ok(t.tag().to_string())
    }
}

fn fs() -> Arc<dyn CatalogFs> {
    Arc::new(StdFs::new(ROOT))
}

/// No catalogs, over the IFS.
pub(crate) fn empty() -> Registry {
    Registry::new(fs(), Arc::new(VirtTapes))
}

/// An `*IMGCLG` object's payload.
fn payload(dir: &str) -> Vec<u8> {
    format!("{MAGIC}\n{dir}\n").into_bytes()
}

/// The directory an `*IMGCLG` payload names.
fn dir_of(b: &[u8]) -> Option<String> {
    let t = String::from_utf8_lossy(b);
    let mut l = t.lines();
    (l.next() == Some(MAGIC)).then(|| l.next().unwrap_or_default().to_string())
}

fn with_qusrsys<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| {
        let lib = wm::obj::library(txn, qsys, "QUSRSYS").map_err(|e| e.to_string())?;
        f(txn, lib)
    })
}

fn dirs(system: &mut System) -> Result<Vec<String>, String> {
    with_qusrsys(system, |txn, lib| {
        let mut out = Vec::new();
        for (_, id) in ops::list(txn, lib, Some(IMGCLG)).map_err(|e| e.to_string())? {
            if let Some(d) = wm::obj::read(txn, id).ok().as_deref().and_then(dir_of) {
                out.push(d);
            }
        }
        Ok(out)
    })
}

/// The catalogs whose objects exist and whose directories hold their
/// metadata file.
pub(crate) fn load(system: &mut System) -> Registry {
    let fs = fs();
    let dirs: Vec<String> = match dirs(system) {
        Ok(d) => d.into_iter().filter(|d| fs.exists(&join(d, META_FILE))).collect(),
        Err(e) => {
            eprintln!("qsstg: image catalogs: {e}");
            Vec::new()
        }
    };
    let refs: Vec<&str> = dirs.iter().map(String::as_str).collect();
    Registry::load(Arc::clone(&fs), Arc::new(VirtTapes), &refs).unwrap_or_else(|e| {
        eprintln!("qsstg: image catalogs: {e}");
        empty()
    })
}

/// The `*IMGCLG` object of a new catalog.
pub(crate) fn create_object(system: &mut System, name: &str, dir: &str, text: &str, owner: &str) -> Result<(), String> {
    let st = wm::obj::Stamp {
        owner: owner.into(),
        system: "QS400".into(),
        now: now_unix(),
    };
    let n = qsname::Name::parse(name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
    with_qusrsys(system, |txn, lib| {
        wm::obj::create(txn, lib, n, IMGCLG, text, &st, &payload(dir))
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Delete a catalog's object.
pub(crate) fn delete_object(system: &mut System, name: &str) -> Result<(), String> {
    let n = qsname::Name::parse(name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
    with_qusrsys(system, |txn, lib| {
        let l = store::obj::library::Library::load(txn, lib).map_err(|e| e.to_string())?;
        if let Some(id) = l.lookup(txn, IMGCLG, n).map_err(|e| e.to_string())? {
            ops::delete(txn, JobHandle(0), id).map_err(|e| e.to_string())?;
        }
        Ok(())
    })
}
