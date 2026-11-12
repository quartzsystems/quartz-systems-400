// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media UKI's `/init`. It finds the installation media, checks its
//! manifest against the keys the UKI trusts and `slic` and the job
//! program against the manifest, puts them at `/init` and `/qsjob`, and
//! runs `slic` in its place as PID 1. Nothing from the media runs
//! unchecked: under Secure Boot the firmware vouches for the UKI, and so
//! for this loader and the keys beside it.
//!
//! Until the media is there it waits and says so: a USB stick may take
//! seconds to appear, and a network boot's media may be put in later.

use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::Command;
use std::time::{Duration, Instant};

use instmedia::licfile;
use rustix::mount::{MountFlags, mount};

use crate::locate::{self, Found};

/// The signers this UKI trusts, 32 bytes each.
const TRUSTED_KEYS: &str = "/qs400/keys/system.pub";
/// Where `slic` goes: it runs itself again from here in its other roles.
const INIT: &str = "/init";
/// Where the job program goes.
const QSJOB: &str = "/qsjob";
/// Both consoles of the baked command line.
const CONSOLES: [&str; 2] = ["/dev/ttyS0", "/dev/tty0"];
/// Between looks for the media.
const RETRY: Duration = Duration::from_millis(250);
/// Before media that did not check out is read again.
const REFUSED: Duration = Duration::from_secs(5);
/// Waiting this long is worth a line on the console.
const PATIENCE: Duration = Duration::from_secs(5);

/// A line on the serial port and the display.
fn say(text: &str) {
    for c in CONSOLES {
        if let Ok(mut f) = fs::OpenOptions::new().write(true).open(c) {
            let _ = f.write_all(format!("{text}\r\n").as_bytes());
        }
    }
}

/// The pseudo file systems the locator reads; `slic` finds them mounted.
fn mounts() {
    let none = None::<&std::ffi::CStr>;
    for (source, target, fstype) in [
        ("proc", "/proc", "proc"),
        ("sysfs", "/sys", "sysfs"),
        ("devtmpfs", "/dev", "devtmpfs"),
    ] {
        let _ = fs::create_dir_all(target);
        let _ = mount(source, target, fstype, MountFlags::NOSUID, none);
    }
}

/// `data` as the executable `path`, never seen half written.
fn place(path: &str, data: &[u8]) -> Result<(), String> {
    let tmp = format!("{path}.new");
    fs::write(&tmp, data).map_err(|e| format!("{tmp}: {e}"))?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("{tmp}: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("{path}: {e}"))
}

/// Check the media and put its programs in place.
fn load(found: &Found, keys: &[u8]) -> Result<(), String> {
    let release = release::RELEASE.to_string();
    licfile::check_manifest(&found.manifest, keys, &release).map_err(|e| e.to_string())?;
    let read = |path: &str| -> Result<Vec<u8>, String> {
        let data = locate::with_media(found, |at| fs::read(at.join(path)))?
            .map_err(|e| format!("{path} on the media: {e}"))?;
        licfile::check_file(&found.manifest, path, &data).map_err(|e| e.to_string())?;
        Ok(data)
    };
    // Both read and checked before either is in place.
    let slic = read(licfile::SLIC)?;
    let job = read(licfile::QSJOB)?;
    place(QSJOB, &job)?;
    place(INIT, &slic)
}

/// Run as PID 1 of the media UKI. Never returns: `slic` takes its place.
pub fn run() -> ! {
    if !rustix::process::getpid().is_init() {
        eprintln!("instload: not PID 1, refusing to run");
        std::process::exit(1);
    }
    mounts();
    let keys = fs::read(TRUSTED_KEYS).unwrap_or_default();
    let started = Instant::now();
    let mut told = String::new();
    let mut tell = |text: String| {
        if text != told {
            say(&text);
            told = text;
        }
    };
    loop {
        match locate::locate() {
            Ok(found) => match load(&found, &keys) {
                Ok(()) => {
                    say(&format!(
                        "QS/400: Licensed Internal Code loaded from the installation media ({})",
                        found.device
                    ));
                    break;
                }
                Err(e) => {
                    tell(format!(
                        "QS/400: the installation media in {} cannot be used: {e}",
                        found.device
                    ));
                    locate::forget();
                    std::thread::sleep(REFUSED);
                }
            },
            Err(e) if started.elapsed() >= PATIENCE => {
                tell(format!("QS/400: waiting for the installation media: {e}"));
            }
            Err(_) => {}
        }
        std::thread::sleep(RETRY);
    }
    // The kernel's arguments and environment go on to slic unchanged.
    let e = Command::new(INIT).args(std::env::args_os().skip(1)).exec();
    say(&format!("QS/400: {INIT} from the media did not start: {e}"));
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
