// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Communications traces in the storage service. The traces of the
//! system are one `nettools::cmntrc::registry::Registry`; each active
//! trace has a thread reading its link's `AF_PACKET` socket into the
//! trace until `ENDCMNTRC` (or a full buffer with `TRCFULL(*STOPTRC)`)
//! stops it. Traces live in memory until `DLTCMNTRC`, as the system's
//! trace buffers do, and are gone after an IPL.

use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nettools::cmntrc::registry::{Entry, Registry};
use nettools::cmntrc::report::{self, Select};
use nettools::cmntrc::{CfgType, Header, Status};
use nettools::msg::{self, Kind, Msg};
use nettools::sys::Capture;

use crate::ipl::system::System;

fn registry() -> MutexGuard<'static, Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(Registry::default()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Microseconds since the epoch.
fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
}

/// The kernel link a configuration object is: the link a line is varied
/// on over, else a link of the object's name (`ETH0` is `eth0`), which
/// lets a link with no line description be traced.
pub fn link_of(obj: &str, ty: CfgType) -> Result<String, Msg> {
    let cannot = || {
        Msg::new(
            msg::TRC_CANNOT,
            Kind::Escape,
            format!("{obj} type {} has no link to trace: vary it on first.", ty.name()),
        )
    };
    if ty != CfgType::Line {
        return Err(cannot());
    }
    let varied = crate::stgsvc::cmn::lock()
        .state
        .line(obj)
        .map(|l| l.ifname.clone())
        .filter(|n| !n.is_empty());
    if let Some(n) = varied {
        return Ok(n);
    }
    let name = obj.to_ascii_lowercase();
    if nettools::sys::ifindex(&name).is_ok() {
        return Ok(name);
    }
    Err(cannot())
}

/// `STRCMNTRC`: the trace made, its capture started.
pub fn start(mut h: Header) -> Result<Msg, Msg> {
    h.started = now_us();
    let (entry, done) = registry().start(h)?;
    let name = format!("cmntrc-{}", entry.lock().header.cfgobj.to_ascii_lowercase());
    let spawned = std::thread::Builder::new().name(name).spawn(move || capture(&entry));
    if let Err(e) = spawned {
        return Err(Msg::new(msg::TRC_FAILED, Kind::Escape, format!("The trace could not start: {e}")));
    }
    Ok(done)
}

/// The capture thread: frames into the trace until it stops.
fn capture(e: &Entry) {
    let ifname = e.lock().header.ifname.clone();
    let mut cap = match Capture::open(&ifname) {
        Ok(c) => c,
        Err(err) => {
            let mut t = e.lock();
            t.header.status = Status::Failed;
            t.header.text = format!("{} (link {ifname}: {err})", t.header.text).trim().to_string();
            return;
        }
    };
    let mut buf = vec![0u8; 65_536];
    while !e.stop.load(Ordering::SeqCst) {
        match cap.next(&mut buf, Duration::from_millis(200)) {
            Ok(Some((n, sent))) => {
                let n = n.min(buf.len());
                if !e.lock().push(now_us(), sent, &buf[..n]) {
                    return;
                }
            }
            Ok(None) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
}

/// `ENDCMNTRC`.
pub fn end(obj: &str, ty: CfgType) -> Result<Msg, Msg> {
    registry().end(obj, ty, now_us())
}

/// `DLTCMNTRC`.
pub fn delete(obj: &str, ty: CfgType) -> Result<Msg, Msg> {
    registry().delete(obj, ty)
}

/// `CHKCMNTRC`.
pub fn check(obj: Option<&str>, ty: Option<CfgType>) -> Result<Vec<Msg>, Msg> {
    registry().check(obj, ty)
}

/// `PRTCMNTRC`: the listing's lines, times in `QTIMZON`'s zone.
pub fn listing(system: &System, obj: &str, ty: CfgType, sel: &Select) -> Result<Vec<String>, Msg> {
    let r = registry();
    let e = r.get(obj, ty).ok_or_else(|| {
        Msg::new(
            msg::TRC_NOT_FOUND,
            Kind::Escape,
            format!("There is no communications trace of {obj} type {}.", ty.name()),
        )
    })?;
    let t = e.lock().clone();
    drop(r);
    if t.records.is_empty() {
        return Err(Msg::new(
            msg::TRC_NO_DATA,
            Kind::Escape,
            format!("The communications trace of {obj} type {} holds no data.", ty.name()),
        ));
    }
    let off = system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QTIMZON").ok())
        .and_then(|v| sysval::tz::find(v.text().trim()))
        .map_or(0, |z| i64::from(z.offset_at(t.header.started / 1_000_000)));
    Ok(report::listing(&t, sel, off))
}
