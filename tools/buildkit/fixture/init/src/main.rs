// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSDEMO's PID 1: mount `/proc`, `/sys` and `/dev`, say who we are on
//! the console, count the boot in the A/B record, then use what an
//! appliance takes from QS/400 beside the pipeline: a panel of the
//! screen kit on the framebuffer and the serial terminal ([`panel`]), a
//! mirrored set served through ublk with an xfs on it ([`raid`]), a
//! route under its own protocol number ([`net`]). Each prints
//! `QSDEMO <what>: ...` when it held and `QSDEMO <what> FAILED: ...`
//! when it did not, and the system powers off. The kernel has the
//! console open as our standard output.

mod net;
mod panel;
mod raid;

use std::ffi::CStr;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::time::Duration;

use abrec::{AbError, AbPart, Slot, Spec};
use rustix::mount::{MountFlags, mount};
use rustix::system::{RebootCommand, reboot};
use uuid::uuid;

/// The release `build.rs` captured from the fixture's own `VERSION`.
const RELEASE: vrm::Release = vrm::release_from_env!("QSDEMO_");

/// QSDEMO's A/B record: its own magic, on the partition of its own GPT
/// type (the orchestrator's `AB_TYPE`). The payload is the boot count.
const RECORD: Spec = Spec {
    magic: *b"QSDEMOAB",
    version: 1,
    type_guid: uuid!("5d0f6f0e-3a57-4c1b-8a55-515344454d01"),
    slot_bytes: 4096,
    min_payload: 4,
};

/// Count this boot in the record: the copy written, its sequence, the count.
fn count_boot() -> Result<(Slot, u64, u32), String> {
    // The disk may still be on its way.
    let mut node = abrec::locate(&RECORD);
    for _ in 0..100 {
        if node.is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
        node = abrec::locate(&RECORD);
    }
    let node = node.map_err(|e| e.to_string())?;
    let dev = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&node)
        .map_err(|e| format!("{}: {e}", node.display()))?;
    let mut part = AbPart::open(RECORD, dev);
    let before = match part.read() {
        Ok(copy) => u32::from_le_bytes(copy.payload[..4].try_into().expect("four bytes")),
        Err(AbError::NoValidCopy) => 0,
        Err(e) => return Err(e.to_string()),
    };
    let boots = before + 1;
    let (slot, sequence) = part
        .write(&boots.to_le_bytes())
        .map_err(|e| e.to_string())?;
    Ok((slot, sequence, boots))
}

/// Print what a proof found.
fn report(what: &str, found: Result<Vec<String>, String>) {
    match found {
        Ok(lines) => {
            for l in lines {
                println!("QSDEMO {what}: {l}");
            }
        }
        Err(e) => println!("QSDEMO {what} FAILED: {e}"),
    }
    let _ = std::io::stdout().flush();
}

fn main() {
    // The set's server is this program again.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("serve") {
        raid::serve(&args[2..]);
    }
    for (fs, at) in [("proc", "/proc"), ("sysfs", "/sys"), ("devtmpfs", "/dev")] {
        let _ = std::fs::create_dir_all(at);
        let _ = mount(fs, at, fs, MountFlags::empty(), None::<&CStr>);
    }
    let banner = format!("QSDEMO {RELEASE} {}", env!("QSDEMO_BUILD_ID"));
    println!("{banner}");
    let magic = String::from_utf8_lossy(&RECORD.magic);
    match count_boot() {
        Ok((slot, sequence, boots)) => {
            println!("QSDEMO record {magic} copy {slot:?} sequence {sequence} boots {boots}");
        }
        Err(e) => println!("QSDEMO record {magic} failed: {e}"),
    }
    report("panel", panel::prove(&banner).map(|l| vec![l]));
    report("set", raid::prove());
    report("route", net::prove().map(|l| vec![l]));
    let _ = std::io::stdout().flush();
    rustix::fs::sync();
    let _ = reboot(RebootCommand::PowerOff);
    // PID 1 must not return.
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
