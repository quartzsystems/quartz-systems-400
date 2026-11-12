// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The installation media: the locator is `instload`'s, which the media
//! UKI's loader shares; the response file is read here.
//! Whatever carries the media (an ISO 9660 CD, the hybrid ISO written
//! raw to a disk or stick, or its tree copied onto FAT32), the mark is
//! `QS400/MEDIA.MANIFEST`.

use std::fs;

pub use instload::locate::{Found, locate, read_payload, with_media};
use optical::manifest;

use crate::ipl::system::System;

/// The response file of an automatic install: the one `qs400.rsp=`
/// names (a path on the media, a URL, or `dhcp`: beside the network
/// boot's boot file; `super::netrsp`), else `QS400/INSTALL.RSP` when the
/// media has one, else after a network boot the one beside the boot
/// file. `None` when there is none; an error for one that does not
/// parse or cannot be fetched.
pub fn response(system: &mut System) -> Result<Option<(String, instmedia::Response)>, String> {
    use instmedia::rsp::Source;
    let parse = |t: String| {
        instmedia::Response::parse(&t)
            .map(|r| Some((t, r)))
            .map_err(|e| e.to_string())
    };
    let path = match system.boot.rsp.as_deref().map(Source::of) {
        Some(src @ (Source::Url(_) | Source::Dhcp)) => {
            let rsp = system.boot.rsp.clone().unwrap_or_default();
            return super::netrsp::text(system, &src)
                .map_err(|e| format!("qs400.rsp={rsp}: {e}"))
                .and_then(parse);
        }
        Some(Source::Media(p)) => Some(p),
        None => None,
    };
    let on_media = match locate() {
        Ok(f) => {
            let want = path
                .clone()
                .unwrap_or_else(|| manifest::RESPONSE.to_string());
            with_media(&f, |at| fs::read_to_string(at.join(&want)).ok())?
        }
        Err(_) => None,
    };
    match (on_media, path) {
        (Some(t), _) => parse(t),
        (None, Some(p)) => Err(format!("qs400.rsp={p}: not found on the media")),
        // A network boot's answers sit beside its boot file, when the
        // server has them.
        (None, None) if system.boot.ipl_d && super::netrsp::network_boot() => {
            match super::netrsp::text(system, &Source::Dhcp) {
                Ok(t) => parse(t),
                Err(e) => {
                    system.out.serial(&format!(
                        "slic: install: no response file beside the boot file: {e}\n"
                    ));
                    Ok(None)
                }
            }
        }
        (None, None) => Ok(None),
    }
}

/// On a D IPL, the disk the media is on (a raw ISO, a stick), so it is
/// never counted or offered as a unit.
pub fn disk_on_d_ipl(ipl_d: bool) -> Option<String> {
    if ipl_d {
        locate().ok().map(|f| f.disk)
    } else {
        None
    }
}
