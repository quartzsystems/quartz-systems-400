// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading `struct input_event` from every `/dev/input/event*` that
//! reports keys (a mouse's and a tablet's buttons are keys too). One
//! thread per device. A caller with a reader of its own decodes its
//! events with [`decode`] and the axes of [`abs_ranges`].

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::thread;

use crate::keyboard::raw;
use crate::pointer::{AbsRanges, AxisRange, DeviceEvent, EVENT_LEN, decode_raw};
use crate::service::Command;

/// Open every key-capable event device and forward its key and pointer
/// events. Returns how many devices are being read.
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

/// The ranges of an event device's `ABS_X` and `ABS_Y` (`EVIOCGABS`); an
/// axis it lacks, or one with no range, is `None`.
#[must_use]
pub fn abs_ranges(file: &File) -> AbsRanges {
    let axis = |y| {
        raw::abs_info(file, y)
            .ok()
            .filter(|a| a.maximum > a.minimum)
            .map(|a| AxisRange {
                min: a.minimum,
                max: a.maximum,
            })
    };
    AbsRanges {
        x: axis(false),
        y: axis(true),
    }
}

/// The console's command for one raw `struct input_event` of a device
/// with these axes: a key, the pointer, or `None`.
#[must_use]
pub fn decode(raw: &[u8], ranges: &AbsRanges) -> Option<Command> {
    Some(match decode_raw(raw, ranges)? {
        DeviceEvent::Key(k) => Command::Key(k),
        DeviceEvent::Pointer(p) => Command::Pointer(p),
    })
}

fn pump(mut file: File, tx: &Sender<Command>) {
    let ranges = abs_ranges(&file);
    let mut buf = [0u8; EVENT_LEN];
    while file.read_exact(&mut buf).is_ok() {
        let Some(cmd) = decode(&buf, &ranges) else {
            continue;
        };
        if tx.send(cmd).is_err() {
            break;
        }
    }
}
