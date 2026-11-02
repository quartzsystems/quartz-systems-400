// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS volume in the storage service (`docs/IFS.md` "The block
//! device"): served to the kernel through ublk from threads of its own,
//! which reach the units through the store's shared I/O handle and
//! never wait on the request thread; mounted at
//! [`MOUNT_POINT`](crate::ipl::ifs::MOUNT_POINT); grown when it fills;
//! unmounted and stopped before the store closes.

use std::fmt::Write as _;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use store::asp::ifs::IfsIo;
use store::asp::ifs::ops::DEFAULT_THRESHOLD;
use store::{Refusal, StoreError};

use crate::ipl::ifs::{MOUNT_POINT, QOPENSYS_EXEC};
use crate::ipl::system::System;
use crate::platform::ifs as plat;
use crate::src;

const MIB: u64 = 1024 * 1024;
/// How often the free space is looked at.
const CHECK_EVERY: Duration = Duration::from_secs(60);
/// The volume grows when less than this share of it is free (1/10).
const FREE_DIVISOR: u64 = 10;
/// The IFS needs to grow and ASP 1 has no room (to `QSYSOPR`).
pub const NO_ROOM_MSG: &str = "CPI3E21";

/// The volume as the kernel's device sees it.
struct Target(IfsIo);

impl ublk::Target for Target {
    fn blocks(&self) -> u64 {
        self.0.pages()
    }
    fn read(&self, block: u64, buf: &mut [u8]) -> Result<(), i32> {
        self.0.read(block, buf).map_err(|_| ublk::target::EIO)
    }
    fn write(&self, block: u64, buf: &[u8]) -> Result<(), i32> {
        self.0.write(block, buf).map_err(|_| ublk::target::EIO)
    }
    fn flush(&self) -> Result<(), i32> {
        self.0.flush().map_err(|_| ublk::target::EIO)
    }
}

/// The served device.
struct Served {
    device: ublk::Device,
    io: IfsIo,
    checked: Instant,
    /// `CPI3E21` was sent and no growth has succeeded since.
    warned: bool,
}

static SERVED: Mutex<Option<Served>> = Mutex::new(None);

fn served() -> std::sync::MutexGuard<'static, Option<Served>> {
    SERVED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Why the IFS did not grow.
#[derive(Debug)]
pub enum GrowError {
    /// ASP 1 would pass its threshold: the pages that fit.
    NoRoom(u64),
    /// The IFS is in its own partition.
    InPartition,
    /// Anything else.
    Failed(String),
}

/// Serve the volume and mount it (C6001012; A6001276, A6001275). A line
/// for the start message; `None` when the IFS is not in the ASP.
pub fn start(system: &mut System) -> Option<String> {
    let s = system.storage.as_mut()?;
    if s.store.ifs_volume().is_empty() {
        return None;
    }
    if !s.store.ifs_formatted() {
        return Some("IFS volume not formatted".into());
    }
    let io = s.store.ifs_io();
    let pages = io.pages();
    // This thread holds the locks the device's thread needs (the units,
    // the parity sets): its memory reclaim must not wait on the device.
    if let Err(e) = rustix::process::configure_io_flusher_behavior(true) {
        eprintln!("qsstg: not an I/O flusher: {e}");
    }
    let device = match ublk::Device::start(Arc::new(Target(io.clone()))) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("qsstg: IFS device: {e}");
            system.emit(&src::IFS_NOT_SERVED);
            return Some(format!("IFS device not served: {e}"));
        }
    };
    let path = device.block_path();
    // devtmpfs makes the node as the disk is added.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let line = match plat::mount_xfs(&path, MOUNT_POINT) {
        Ok(()) => {
            system.emit(&src::IFS_MOUNTED);
            format!("IFS {} ({} MiB in ASP 1) at {MOUNT_POINT}", path.display(), pages * 4096 / MIB)
        }
        Err(e) => {
            eprintln!("qsstg: IFS {}: mount: {e}", path.display());
            system.emit(&src::IFS_MOUNT_FAILED);
            format!("IFS {} not mounted: {e}", path.display())
        }
    };
    *served() = Some(Served {
        device,
        io,
        checked: Instant::now(),
        warned: false,
    });
    Some(line)
}

/// Between requests: once a minute, grow the volume when less than a
/// tenth of the filesystem is free.
pub fn tick(system: &mut System) {
    {
        let mut g = served();
        let Some(sv) = g.as_mut() else {
            return;
        };
        if sv.checked.elapsed() < CHECK_EVERY {
            return;
        }
        sv.checked = Instant::now();
    }
    let Some((size, free, _)) = plat::capacity(MOUNT_POINT) else {
        return;
    };
    if free >= size / FREE_DIVISOR {
        return;
    }
    match grow(system, None) {
        Ok((added, total)) => eprintln!(
            "qsstg: IFS grew by {} MiB to {} MiB",
            added * 4096 / MIB,
            total * 4096 / MIB
        ),
        Err(GrowError::NoRoom(_)) => {
            let first = served().as_mut().is_some_and(|sv| !std::mem::replace(&mut sv.warned, true));
            if first {
                no_room(system);
            }
        }
        Err(e) => eprintln!("qsstg: IFS growth: {e:?}"),
    }
}

/// Tell the operator the IFS cannot grow.
fn no_room(system: &mut System) {
    let m = wm::obj::msgq::QMsg {
        msgid: NO_ROOM_MSG.into(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        severity: 80,
        text: "The integrated file system is nearly full and cannot grow: ASP 1 has no room left below its threshold.".into(),
        job: ("SCPF".into(), "QSYS".into(), 0),
        from_pgm: "QSYSARB".into(),
        ..wm::obj::msgq::QMsg::default()
    };
    let _ = super::msgs::send(system, "QSYS", "QSYSOPR", m);
}

/// Grow the volume by `pages` (a step when `None`): extents held and
/// checkpointed, the kernel told the device's size, xfs grown over it.
/// The pages added and the new total.
pub fn grow(system: &mut System, pages: Option<u64>) -> Result<(u64, u64), GrowError> {
    let Some(s) = system.storage.as_mut() else {
        return Err(GrowError::Failed("the object store is not open".into()));
    };
    let store = &mut s.store;
    if store.ifs_volume().is_empty() {
        return Err(GrowError::InPartition);
    }
    let mut g = served();
    let Some(sv) = g.as_mut() else {
        return Err(GrowError::Failed("the IFS device is not served".into()));
    };
    let want = pages.unwrap_or_else(|| store.ifs_step());
    let added = store.ifs_grow(want, DEFAULT_THRESHOLD).map_err(|e| match e {
        StoreError::Refused {
            reason: Refusal::IfsNoRoom(room),
            ..
        } => GrowError::NoRoom(room),
        e => GrowError::Failed(e.to_string()),
    })?;
    let total = store.ifs_volume().pages();
    sv.io.set_volume(store.ifs_volume().clone());
    sv.warned = false;
    sv.device
        .resize(total)
        .map_err(|e| GrowError::Failed(e.to_string()))?;
    let dir = std::fs::File::open(MOUNT_POINT).map_err(|e| GrowError::Failed(e.to_string()))?;
    ublk::xfs_growfs(&dir, total).map_err(|e| GrowError::Failed(e.to_string()))?;
    Ok((added, total))
}

/// What the close reply says when the IFS was unmounted cleanly; `slic`
/// shows D1001012 for it.
pub const UNMOUNTED: &str = "IFS unmounted";

/// Before the store closes: everything written, `/QOpenSys`'s second
/// mount and the IFS unmounted, the device stopped. A line for the
/// close reply.
pub fn close() -> Option<String> {
    let sv = served().take()?;
    rustix::fs::sync();
    if plat::mounted(QOPENSYS_EXEC).is_some() {
        let _ = plat::unmount_xfs(QOPENSYS_EXEC);
    }
    super::ifs::shut();
    let line = match plat::unmount_xfs(MOUNT_POINT) {
        Ok(()) => UNMOUNTED.to_string(),
        Err(e) => {
            let _ = rustix::mount::unmount(MOUNT_POINT, rustix::mount::UnmountFlags::DETACH);
            format!("IFS detached: {e}")
        }
    };
    if let Err(e) = sv.device.stop() {
        eprintln!("qsstg: IFS device: {e}");
    }
    let _ = sv.io.flush();
    Some(line)
}

/// `svc.ifs`'s lines about the volume.
pub fn report(system: &System) -> String {
    let mut out = String::new();
    let Some(s) = system.storage.as_ref() else {
        return out;
    };
    let v = s.store.ifs_volume();
    if v.is_empty() {
        return out;
    }
    let _ = writeln!(
        out,
        "volume in ASP 1: {} MiB in {} extent(s); grows by {} MiB, {} MiB more fit below {}%",
        v.pages() * 4096 / MIB,
        v.extents().len(),
        s.store.ifs_step() * 4096 / MIB,
        s.store.ifs_room(DEFAULT_THRESHOLD) * 4096 / MIB,
        DEFAULT_THRESHOLD
    );
    if let Some(sv) = served().as_ref() {
        let _ = writeln!(out, "served as {}", sv.device.block_path().display());
    }
    out
}
