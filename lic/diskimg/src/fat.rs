// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! FAT volume images built in memory with `fatfs`, used for the disk ESP
//! and the ISO's El Torito boot image. Deterministic: fixed volume id and
//! label, 1980 timestamps (no `chrono`), files in the order given.

use std::io::{Cursor, Write};

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

/// The ISO boot image: 24 MiB FAT16 at 4 KiB clusters (6144 clusters).
/// El Torito's sector count is sixteen bits of 512-byte sectors, 32 MiB
/// at most, and FAT32 cannot be made that small.
pub const MEDIA_ESP: Spec = Spec {
    bytes: 24 * 1024 * 1024,
    fat: FatType::Fat16,
    bytes_per_cluster: 4096,
};

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
        assert_eq!(img.len(), 24 * 1024 * 1024);
        assert_eq!(
            list(&img).unwrap(),
            vec!["EFI/", "EFI/BOOT/", "EFI/BOOT/BOOTX64.EFI"]
        );
        assert_eq!(&img[54..59], b"FAT16");
        assert_eq!(
            build(&MEDIA_ESP, &[]).unwrap(),
            build(&MEDIA_ESP, &[]).unwrap()
        );
    }
}
