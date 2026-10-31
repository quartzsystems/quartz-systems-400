// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Media in the storage service: the tape, optical and media library
//! devices, the image catalogs (`*IMGCLG` objects in `QUSRSYS` over
//! directories of the IFS), the optical volumes `/QOPT` shows, and a
//! device opened for one save, restore or tape command. docs/MEDIA.md,
//! "In the storage service", has the design.

pub(crate) mod catalogs;
pub(crate) mod devices;
pub(crate) mod drive;
pub(crate) mod library;
pub(crate) mod operator;
pub(crate) mod stream;

use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use imgclg::Registry;
use optical::VolumeSet;

use crate::ipl::system::System;

/// The image catalogs, once the service has loaded them.
fn registry() -> &'static Mutex<Option<Registry>> {
    static R: OnceLock<Mutex<Option<Registry>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(None))
}

fn lock() -> MutexGuard<'static, Option<Registry>> {
    registry().lock().unwrap_or_else(PoisonError::into_inner)
}

/// The optical volumes the loaded catalogs present, shared with the IFS.
pub(crate) fn volume_set() -> VolumeSet {
    static S: OnceLock<VolumeSet> = OnceLock::new();
    S.get_or_init(|| VolumeSet::new(std::sync::Arc::new(crate::ipl::system::now_unix)))
        .clone()
}

/// At service start: the catalogs from their objects, `/QOPT` in line
/// with them, and descriptions for media hardware nobody described. A
/// line for the start message.
pub(crate) fn start(system: &mut System) -> String {
    let reg = catalogs::load(system);
    let n = reg.names().len();
    *lock() = Some(reg);
    sync();
    let made = devices::autoconfigure(system);
    devices::vary_online(system);
    format!("{n} image catalog(s), {made} media device(s) configured")
}

/// `/QOPT` in line with the catalogs.
fn sync() {
    if let Some(r) = lock().as_mut()
        && let Err(e) = r.sync_volumes(&volume_set())
    {
        eprintln!("qsstg: optical volumes: {e}");
    }
}

/// Run `f` over the catalogs, then bring `/QOPT` in line. An error is
/// its message identifier and text.
pub(crate) fn with_catalogs<T>(f: impl FnOnce(&mut Registry) -> imgclg::Result<T>) -> Result<T, String> {
    let r = {
        let mut g = lock();
        let reg = g.get_or_insert_with(catalogs::empty);
        f(reg).map_err(|e| format!("{} {e}", e.msgid()))
    };
    sync();
    r
}

/// The catalog loaded on device `dev`, if any.
pub(crate) fn catalog_on(dev: &str) -> Option<imgclg::Catalog> {
    let g = lock();
    let reg = g.as_ref()?;
    reg.names()
        .into_iter()
        .filter_map(|n| reg.catalog(&n).ok().cloned())
        .find(|c| c.device.as_deref() == Some(dev))
}
