// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The saved resource names (`hwrsrc::names`) in the LIC partition, TLV
//! `0x010A`: read before the first walk, written when a name is given,
//! a resource goes or one is removed. Each write rereads the record, as
//! the storage service's do, so what `slic` wrote is kept.

use std::fs::OpenOptions;
use std::sync::Mutex;

use hwrsrc::names::TAG_HW_NAMES;
use hwrsrc::nic::PermMacs;
use hwrsrc::{NameMap, Table};

/// What was last written, so an unchanged map is not written again.
static LAST: Mutex<Option<Vec<u8>>> = Mutex::new(None);

fn open() -> Option<licpart::LicPart<Box<dyn licpart::Device>>> {
    let path = licpart::locate().ok()?;
    let f = OpenOptions::new().read(true).write(true).open(path).ok()?;
    let dev: Box<dyn licpart::Device> = Box::new(f);
    Some(licpart::LicPart::open(dev))
}

/// The saved map; empty when there is none (no LIC partition: a media
/// IPL, or the first IPL) or it does not read.
pub fn load() -> NameMap {
    let Some(mut part) = open() else {
        return NameMap::default();
    };
    let Ok(record) = part.read() else {
        return NameMap::default();
    };
    let Some(bytes) = record.tlv(TAG_HW_NAMES) else {
        return NameMap::default();
    };
    match NameMap::decode(bytes) {
        Ok(map) => {
            *LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(bytes.to_vec());
            map
        }
        Err(e) => {
            println!("hwrsrc: saved resource names unreadable ({e}); named afresh");
            NameMap::default()
        }
    }
}

/// Save the table's names when they changed.
pub fn save(t: &Table) {
    let bytes = NameMap::of(t).encode();
    let mut last = LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if last.as_deref() == Some(bytes.as_slice()) {
        return;
    }
    let Some(mut part) = open() else {
        return;
    };
    let Ok(mut record) = part.read() else {
        return;
    };
    if record.set_tlv(TAG_HW_NAMES, bytes.clone()).is_err() {
        return;
    }
    match part.write(&mut record) {
        Ok(_) => *last = Some(bytes),
        Err(e) => println!("hwrsrc: resource names not saved: {e}"),
    }
}

/// Each interface's permanent MAC address, from netlink.
pub fn perm_macs() -> PermMacs {
    let Ok(links) = netlink::route::Rtnl::open().and_then(|mut r| r.links()) else {
        return PermMacs::new();
    };
    links
        .into_iter()
        .filter(|l| l.perm_mac.len() == 6)
        .map(|l| {
            let mac = l
                .perm_mac
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(":");
            (l.name, mac)
        })
        .collect()
}
