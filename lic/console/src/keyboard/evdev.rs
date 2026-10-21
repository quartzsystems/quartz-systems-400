// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading `struct input_event` from every `/dev/input/event*` that
//! reports keys. One thread per device.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

use crate::keyboard::KeyEvent;
use crate::service::Command;

/// `struct input_event` on `x86_64`: timeval (16) + type (2) + code (2) + value (4).
const EVENT_LEN: usize = 24;
const EV_KEY: u16 = 1;

/// Open every key-capable event device and forward its key events.
/// Returns how many devices are being read.
pub fn spawn_readers(tx: &Sender<Command>) -> usize {
    let mut count = 0;
    let Ok(dir) = fs::read_dir("/sys/class/input") else {
        return 0;
    };
    let mut names: Vec<String> = dir
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("event"))
        .collect();
    names.sort();
    for name in names {
        if !reports_keys(&Path::new("/sys/class/input").join(&name)) {
            continue;
        }
        let Ok(file) = File::open(Path::new("/dev/input").join(&name)) else {
            continue;
        };
        let tx = tx.clone();
        thread::spawn(move || pump(file, &tx));
        count += 1;
    }
    count
}

/// `capabilities/ev` has bit 1 (`EV_KEY`) set.
fn reports_keys(sys: &Path) -> bool {
    let ev = fs::read_to_string(sys.join("device/capabilities/ev")).unwrap_or_default();
    u64::from_str_radix(ev.trim(), 16).is_ok_and(|bits| bits & 0b10 != 0)
}

fn pump(mut file: File, tx: &Sender<Command>) {
    let mut buf = [0u8; EVENT_LEN];
    while file.read_exact(&mut buf).is_ok() {
        let kind = u16::from_ne_bytes([buf[16], buf[17]]);
        if kind != EV_KEY {
            continue;
        }
        let code = u16::from_ne_bytes([buf[18], buf[19]]);
        let value = i32::from_ne_bytes([buf[20], buf[21], buf[22], buf[23]]);
        let ev = KeyEvent {
            code,
            pressed: value != 0,
        };
        if tx.send(Command::Key(ev)).is_err() {
            break;
        }
    }
}
