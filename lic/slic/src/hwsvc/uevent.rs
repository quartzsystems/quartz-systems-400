// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The hotplug listener: kernel uevents applied to the table, each
//! change written to the serial line.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

use hwrsrc::Table;
use hwrsrc::uevent::{UEVENT_GROUP, Uevent};
use rustix::net::netlink::{KOBJECT_UEVENT, SocketAddrNetlink};
use rustix::net::{AddressFamily, RecvFlags, SocketType, bind, recv, socket};

/// Bind the uevent socket and apply events for the life of the service.
/// False when the socket cannot be opened.
pub fn spawn(table: Arc<Mutex<Table>>) -> bool {
    let Ok(sock) = socket(AddressFamily::NETLINK, SocketType::RAW, Some(KOBJECT_UEVENT)) else {
        return false;
    };
    if bind(&sock, &SocketAddrNetlink::new(0, UEVENT_GROUP)).is_err() {
        return false;
    }
    thread::spawn(move || {
        let mut buf = vec![0u8; 8192];
        loop {
            match recv(&sock, &mut buf[..], RecvFlags::empty()) {
                Ok((n, _)) => {
                    let Some(ev) = Uevent::parse(&buf[..n.min(buf.len())]) else { continue };
                    let Ok(mut t) = table.lock() else { break };
                    for line in t.apply_uevent(Path::new(super::SYS), &ev) {
                        println!("hwrsrc: {line}");
                    }
                }
                Err(rustix::io::Errno::INTR) => {}
                Err(_) => break,
            }
        }
    });
    true
}
