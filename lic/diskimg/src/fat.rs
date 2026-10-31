// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! FAT volume images built in memory with `fatfs`, used for the disk ESP
//! and the ISO's El Torito boot image. Deterministic: fixed volume id and
//! label, 1980 timestamps (no `chrono`), files in the order given.

use std::io::{Cursor, Read as _, Write};

use fatfs::{Dir, FatType, FileSystem, FormatVolumeOptions, FsOptions, ReadWriteSeek};

use crate::Error;

/// Volume label shared by every QS/400 FAT image.
pub const LABEL: [u8; 11] = *b"QS400 ESP  ";
/// Volume id shared by every QS/400 FAT image.
pub const VOLUME_ID: u32 = 0x5153_3430;

/// Geometry of a volume.
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    /// Volume size.
    pub bytes: u64,
    /// FAT type the geometry must produce. `fatfs` picks the type from the
    /// cluster count, so this is checked after formatting.
    pub fat: FatType,
    /// Cluster size.
    pub bytes_per_cluster: u32,
}

/// The disk ESP: 256 MiB FAT32 at 2 KiB clusters (131072 clusters).
pub const DISK_ESP: Spec = Spec {
    bytes: crate::plan::ESP_BYTES,
    fat: FatType::Fat32,
    bytes_per_cluster: 2048,
};

/// The ISO boot image: 31 MiB FAT16 at 4 KiB clusters (7936 clusters),
/// room for the media UKI. El Torito's sector count is sixteen bits of
/// 512-byte sectors, 32 MiB at most, and FAT32 cannot be made that small.
pub const MEDIA_ESP: Spec = Spec {
    bytes: 31 * 1024 * 1024,
    fat: FatType::Fat16,
    bytes_per_cluster: 4096,
};

/// The ISO boot image for a UKI of `payload` bytes: [`MEDIA_ESP`] while
/// the UKI fits, else grown in whole MiB with 2 MiB to spare (FAT16 at
/// 4 KiB clusters holds up to 255 MiB). Past 32 MiB the boot catalog
/// cannot give the size, and firmware reads the volume's own.
#[must_use]
pub fn media_esp(payload: u64) -> Spec {
    const MIB: u64 = 1024 * 1024;
    let need = (payload + 2 * MIB).div_ceil(MIB) * MIB;
    Spec {
        bytes: need.max(MEDIA_ESP.bytes),
        ..MEDIA_ESP
    }
}

fn fat_err(what: &'static str) -> impl FnOnce(std::io::Error) -> Error {
    move |e| Error::Fat {
        what,
        detail: e.to_string(),
    }
}

/// Format a volume and write `files` (`"EFI/BOOT/BOOTX64.EFI"` style
/// paths; directories are created as needed).
pub fn build(spec: &Spec, files: &[(&str, &[u8])]) -> Result<Vec<u8>, Error> {
    let size = usize::try_from(spec.bytes).expect("volume fits in memory");
    let mut cur = Cursor::new(vec![0u8; size]);
    fatfs::format_volume(
        &mut cur,
        FormatVolumeOptions::new()
            .fat_type(spec.fat)
            .bytes_per_cluster(spec.bytes_per_cluster)
            .volume_id(VOLUME_ID)
            .volume_label(LABEL),
    )
    .map_err(fat_err("format"))?;
    let fs = FileSystem::new(&mut cur, FsOptions::new()).map_err(fat_err("mount"))?;
    if fs.fat_type() != spec.fat {
        return Err(Error::Fat {
            what: "format",
            detail: format!(
                "volume came out as {:?}, wanted {:?}",
                fs.fat_type(),
                spec.fat
            ),
        });
    }
    for (path, data) in files {
        let (dirs, name) = path.rsplit_once('/').unwrap_or(("", path));
        let mut dir = fs.root_dir();
        for part in dirs.split('/').filter(|p| !p.is_empty()) {
            dir = dir.create_dir(part).map_err(fat_err("mkdir"))?;
        }
        let mut f = dir.create_file(name).map_err(fat_err("create"))?;
        f.write_all(data).map_err(Error::io("write file"))?;
        f.flush().map_err(Error::io("flush file"))?;
    }
    fs.unmount().map_err(fat_err("unmount"))?;
    Ok(cur.into_inner())
}

/// A FAT32 volume of at least `payload` bytes of files, with room to
/// spare: 2 KiB clusters, 256 MiB at the least, whole MiB.
#[must_use]
pub fn fat32_for(payload: u64) -> Spec {
    const MIB: u64 = 1024 * 1024;
    let need = (payload + payload / 4 + 64 * MIB).div_ceil(MIB) * MIB;
    Spec {
        bytes: need.max(256 * MIB),
        fat: FatType::Fat32,
        bytes_per_cluster: 2048,
    }
}

/// One file's bytes (`"EFI/BOOT/BOOTX64.EFI"` style path).
pub fn read_file(image: &[u8], path: &str) -> Result<Vec<u8>, Error> {
    use std::io::Read;
    let mut cur = Cursor::new(image.to_vec());
    let fs = FileSystem::new(&mut cur, FsOptions::new()).map_err(fat_err("re-mount"))?;
    let mut f = fs.root_dir().open_file(path).map_err(fat_err("open"))?;
    let mut out = Vec::new();
    f.read_to_end(&mut out).map_err(Error::io("read file"))?;
    Ok(out)
}

/// A file's bytes on a mounted volume (`dev` is the partition).
pub fn read_file_at<T: ReadWriteSeek>(mut dev: T, path: &str) -> Result<Vec<u8>, Error> {
    dev.seek(std::io::SeekFrom::Start(0)).map_err(Error::io("seek"))?;
    let fs = FileSystem::new(dev, FsOptions::new()).map_err(fat_err("mount"))?;
    let mut f = fs.root_dir().open_file(path).map_err(fat_err("open"))?;
    let mut out = Vec::new();
    f.read_to_end(&mut out).map_err(Error::io("read file"))?;
    drop(f);
    fs.unmount().map_err(fat_err("unmount"))?;
    Ok(out)
}

/// Replace a file on a volume: the bytes go to a new file beside it
/// first, which is then renamed over the old one, so a torn write
/// leaves the old file whole. Directories are created as needed.
pub fn write_file<T: ReadWriteSeek>(mut dev: T, path: &str, data: &[u8]) -> Result<(), Error> {
    dev.seek(std::io::SeekFrom::Start(0)).map_err(Error::io("seek"))?;
    let fs = FileSystem::new(dev, FsOptions::new()).map_err(fat_err("mount"))?;
    {
        let (dirs, name) = path.rsplit_once('/').unwrap_or(("", path));
        let mut dir = fs.root_dir();
        for part in dirs.split('/').filter(|p| !p.is_empty()) {
            dir = match dir.open_dir(part) {
                Ok(d) => d,
                Err(_) => dir.create_dir(part).map_err(fat_err("mkdir"))?,
            };
        }
        let tmp = "QS400NEW.TMP";
        let _ = dir.remove(tmp);
        {
            let mut f = dir.create_file(tmp).map_err(fat_err("create"))?;
            f.truncate().map_err(fat_err("truncate"))?;
            f.write_all(data).map_err(Error::io("write file"))?;
            f.flush().map_err(Error::io("flush file"))?;
        }
        let _ = dir.remove(name);
        dir.rename(tmp, &dir, name).map_err(fat_err("rename"))?;
    }
    fs.unmount().map_err(fat_err("unmount"))?;
    Ok(())
}

/// Every path on a volume, directories with a trailing slash, sorted.
pub fn list(image: &[u8]) -> Result<Vec<String>, Error> {
    let mut cur = Cursor::new(image.to_vec());
    let fs = FileSystem::new(&mut cur, FsOptions::new()).map_err(fat_err("re-mount"))?;
    let mut out = Vec::new();
    walk(&fs.root_dir(), "", &mut out)?;
    out.sort();
    Ok(out)
}

fn walk<T: ReadWriteSeek>(
    dir: &Dir<'_, T>,
    prefix: &str,
    out: &mut Vec<String>,
) -> Result<(), Error> {
    for entry in dir.iter() {
        let entry = entry.map_err(fat_err("read directory"))?;
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let path = format!("{prefix}{name}");
        if entry.is_dir() {
            out.push(format!("{path}/"));
            walk(&entry.to_dir(), &format!("{path}/"), out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_image_is_fat16_with_the_boot_file() {
        let img = build(&MEDIA_ESP, &[("EFI/BOOT/BOOTX64.EFI", b"uki")]).unwrap();
        assert_eq!(read_file(&img, "EFI/BOOT/BOOTX64.EFI").unwrap(), b"uki");
        assert!(read_file(&img, "EFI/NONE").is_err());
        assert_eq!(fat32_for(1).bytes, 256 * 1024 * 1024);
        assert_eq!(img.len(), 31 * 1024 * 1024);
        assert_eq!(
            list(&img).unwrap(),
            vec!["EFI/", "EFI/BOOT/", "EFI/BOOT/BOOTX64.EFI"]
        );
        assert_eq!(&img[54..59], b"FAT16");
        let big = media_esp(40 * 1024 * 1024);
        assert_eq!(big.bytes, 42 * 1024 * 1024);
        let img = build(&big, &[("EFI/BOOT/BOOTX64.EFI", &vec![7u8; 40 * 1024 * 1024])]).unwrap();
        assert_eq!(&img[54..59], b"FAT16");
        assert_eq!(media_esp(1).bytes, MEDIA_ESP.bytes);
        assert_eq!(
            build(&MEDIA_ESP, &[]).unwrap(),
            build(&MEDIA_ESP, &[]).unwrap()
        );
    }

    #[test]
    fn slot_files_are_replaced_in_place() {
        let img = build(&MEDIA_ESP, &[("EFI/BOOT/BOOTX64.EFI", b"slot a"), ("EFI/QS400/B.EFI", b"slot b")]).unwrap();
        let mut cur = Cursor::new(img);
        write_file(&mut cur, "EFI/QS400/B.EFI", b"new slot b, longer than before").unwrap();
        assert_eq!(read_file_at(&mut cur, "EFI/QS400/B.EFI").unwrap(), b"new slot b, longer than before");
        assert_eq!(read_file_at(&mut cur, "EFI/BOOT/BOOTX64.EFI").unwrap(), b"slot a");
        write_file(&mut cur, "EFI/QS400/B.EFI", b"short").unwrap();
        assert_eq!(read_file_at(&mut cur, "EFI/QS400/B.EFI").unwrap(), b"short");
        assert_eq!(
            list(cur.get_ref()).unwrap(),
            vec!["EFI/", "EFI/BOOT/", "EFI/BOOT/BOOTX64.EFI", "EFI/QS400/", "EFI/QS400/B.EFI"]
        );
    }
}
