// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Writing the report to a USB stick: the first FAT file system on a
//! USB block device, written with the pure-Rust `fatfs` (no mount, so
//! nothing else on the stick is touched and nothing stays mounted).

use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::Path;

/// A USB block device the report can go to: the partition (or whole
/// disk) with a FAT file system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stick {
    /// The kernel name (`sdb1`).
    pub name: String,
    /// The whole disk's model as sysfs reports it.
    pub model: String,
}

/// Whether block device `dev` (`class/block/<name>`) sits on a USB bus.
fn on_usb(sys: &Path, name: &str) -> bool {
    fs::canonicalize(sys.join("class/block").join(name)).is_ok_and(|p| {
        p.components()
            .any(|c| c.as_os_str().to_string_lossy().starts_with("usb"))
    })
}

/// Whether the first sector holds a FAT boot sector.
fn fat_at(dev: &Path) -> bool {
    let mut b = [0u8; 512];
    let Ok(mut f) = fs::File::open(dev) else {
        return false;
    };
    if f.read_exact(&mut b).is_err() || b[510..512] != [0x55, 0xAA] {
        return false;
    }
    &b[82..87] == b"FAT32"
        || &b[54..59] == b"FAT16"
        || &b[54..59] == b"FAT12"
        || &b[54..57] == b"FAT"
}

/// The USB sticks with a FAT file system, by kernel name: each disk's
/// partitions first, the whole disk when it has none.
#[must_use]
pub fn sticks(sys: &Path, dev: &Path) -> Vec<Stick> {
    let mut names: Vec<String> = fs::read_dir(sys.join("class/block"))
        .map(|rd| {
            rd.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let mut v = Vec::new();
    for disk in names
        .iter()
        .filter(|n| !sys.join("class/block").join(n).join("partition").exists())
    {
        if !on_usb(sys, disk) {
            continue;
        }
        let model = fs::read_to_string(sys.join("class/block").join(disk).join("device/model"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let parts: Vec<&String> = names
            .iter()
            .filter(|p| p.starts_with(disk.as_str()) && *p != disk)
            .collect();
        let tries: Vec<&String> = if parts.is_empty() { vec![disk] } else { parts };
        for p in tries {
            if fat_at(&dev.join(p)) {
                v.push(Stick {
                    name: p.clone(),
                    model: model.clone(),
                });
            }
        }
    }
    v
}

/// Write `text` as `file` into the FAT file system on `disk`, replacing
/// a file of that name.
///
/// # Errors
/// The volume is not FAT, or the write failed.
pub fn write_to<D: Read + Write + Seek>(disk: D, file: &str, text: &str) -> Result<(), String> {
    let fs = fatfs::FileSystem::new(disk, fatfs::FsOptions::new())
        .map_err(|e| format!("not a FAT volume: {e}"))?;
    {
        let root = fs.root_dir();
        let mut f = root.create_file(file).map_err(|e| e.to_string())?;
        f.truncate().map_err(|e| e.to_string())?;
        // DOS line ends, so any machine's editor shows the lines.
        f.write_all(text.replace('\n', "\r\n").as_bytes())
            .map_err(|e| e.to_string())?;
        f.flush().map_err(|e| e.to_string())?;
    }
    fs.unmount().map_err(|e| e.to_string())
}

/// The text of `file` on the FAT file system on `disk`, for checking a
/// stick the report was written to.
///
/// # Errors
/// The volume is not FAT, or has no such file.
pub fn read_from<D: Read + Write + Seek>(disk: D, file: &str) -> Result<String, String> {
    let fs = fatfs::FileSystem::new(disk, fatfs::FsOptions::new())
        .map_err(|e| format!("not a FAT volume: {e}"))?;
    let mut s = String::new();
    fs.root_dir()
        .open_file(file)
        .map_err(|e| format!("{file}: {e}"))?
        .read_to_string(&mut s)
        .map_err(|e| e.to_string())?;
    Ok(s)
}

/// Write the report to the first USB stick found; what was done, as a
/// line for the operator.
///
/// # Errors
/// No stick, or the write failed.
pub fn write_report(sys: &Path, dev: &Path, text: &str) -> Result<String, String> {
    let Some(s) = sticks(sys, dev).into_iter().next() else {
        return Err(
            "No USB device with a FAT file system was found. Insert one and try again.".into(),
        );
    };
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .open(dev.join(&s.name))
        .map_err(|e| format!("{}: {e}", s.name))?;
    write_to(f, crate::USB_FILE, text)?;
    let on = if s.model.is_empty() {
        s.name.clone()
    } else {
        format!("{} ({})", s.name, s.model)
    };
    Ok(format!(
        "The report was written to {} on {on}.",
        crate::USB_FILE
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn written_to_a_fat_volume() {
        let mut img = Cursor::new(vec![0u8; 4 * 1024 * 1024]);
        fatfs::format_volume(&mut img, fatfs::FormatVolumeOptions::new()).unwrap();
        img.set_position(0);
        write_to(&mut img, "QSHWQUAL.TXT", "line 1\nline 2\n").unwrap();
        // Again: the file is replaced, not appended to.
        img.set_position(0);
        write_to(&mut img, "QSHWQUAL.TXT", "short\n").unwrap();
        img.set_position(0);
        let fs = fatfs::FileSystem::new(&mut img, fatfs::FsOptions::new()).unwrap();
        let mut s = String::new();
        fs.root_dir()
            .open_file("QSHWQUAL.TXT")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert_eq!(s, "short\r\n");
        drop(fs);
        let p = std::env::temp_dir().join(format!("hwqual-fat-{}", std::process::id()));
        fs::write(&p, img.get_ref()).unwrap();
        assert!(fat_at(&p));
        fs::write(&p, [0u8; 1024]).unwrap();
        assert!(!fat_at(&p));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn not_fat() {
        assert!(write_to(Cursor::new(vec![0u8; 65536]), "X", "y").is_err());
        let dir = std::env::temp_dir().join(format!("hwqual-usb-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sys/class/block")).unwrap();
        assert!(sticks(&dir.join("sys"), &dir.join("dev")).is_empty());
        assert!(write_report(&dir.join("sys"), &dir.join("dev"), "x").is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
