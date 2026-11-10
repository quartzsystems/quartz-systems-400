// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The driver store the read-only `print$` share serves
//! ([`DRIVER_STORE`]): Windows' layout, one directory per environment and
//! driver version (`x64/3`, `W32X86/3`, `ARM64/3`, and `4` for version 4
//! drivers), holding the driver's files and a manifest, `NAME.qsdrv`, for
//! the driver `NAME` the print shares name. A manifest is `key=value`
//! lines: `driver`, `data` and `config` once each, `help` at most once,
//! `file` for each dependent file, every value a file name in the same
//! directory; empty lines and lines starting `#` are skipped. An
//! administrator fills the directory with an IFS copy.

use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::flags::{O_RDONLY, O_SHARE_RDWR};
use smb_server::print::DriverFiles;
use tracing::warn;

/// Where the driver store is, in the IFS. IBM i has none (the
/// primary's `/QIBM/UserData/OS400/NetServer` holds only `QAZLSCFG` and
/// `QAZLSSHR`); QS/400 keeps it beside them.
pub const DRIVER_STORE: &str = "/QIBM/UserData/OS400/NetServer/print$";

/// The share name Windows looks for driver files under.
pub const PRINT_DOLLAR: &str = "print$";

/// Environment directories and the environments they hold.
const ENVIRONMENTS: [(&str, &str); 3] = [
    ("x64", "Windows x64"),
    ("W32X86", "Windows NT x86"),
    ("ARM64", "Windows ARM64"),
];

/// Driver versions looked for, newest first.
const VERSIONS: [u32; 2] = [4, 3];

/// The longest manifest read.
const MAX_MANIFEST: usize = 16 << 10;

/// Why a manifest was not taken.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// A line without `=`, or a key not listed above.
    #[error("line {0}: not a driver manifest line")]
    Line(usize),
    /// `driver`, `data` or `config` missing, or a key given twice.
    #[error("driver, data and config are each needed once")]
    Incomplete,
    /// A value naming another directory.
    #[error("line {0}: a file name, not a path")]
    Path(usize),
}

/// Reads one manifest's text for the files of `env_dir/version`.
///
/// # Errors
///
/// What [`ManifestError`] lists.
pub fn parse(
    text: &str,
    env_dir: &str,
    environment: &str,
    version: u32,
) -> Result<DriverFiles, ManifestError> {
    let (mut driver, mut data, mut config, mut help) = (None, None, None, None);
    let mut dependent = Vec::new();
    let path = |name: &str| format!("{env_dir}\\{version}\\{name}");
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let n = i + 1;
        let (key, value) = line.split_once('=').ok_or(ManifestError::Line(n))?;
        let value = value.trim();
        if value.is_empty() || value.contains(['/', '\\', ':']) || value == ".." {
            return Err(ManifestError::Path(n));
        }
        let slot = match key.trim() {
            "driver" => &mut driver,
            "data" => &mut data,
            "config" => &mut config,
            "help" => &mut help,
            "file" => {
                dependent.push(path(value));
                continue;
            }
            _ => return Err(ManifestError::Line(n)),
        };
        if slot.replace(path(value)).is_some() {
            return Err(ManifestError::Incomplete);
        }
    }
    let (Some(driver_path), Some(data_file), Some(config_file)) = (driver, data, config) else {
        return Err(ManifestError::Incomplete);
    };
    Ok(DriverFiles {
        environment: environment.into(),
        version,
        driver_path,
        data_file,
        config_file,
        help_file: help.unwrap_or_default(),
        dependent_files: dependent,
    })
}

/// The files of `driver` in the store at `root`, one entry per
/// environment that has a manifest for it (the newest version). A
/// manifest that does not parse is left out, with a warning.
pub fn find(c: &dyn IfsClient, root: &str, driver: &str) -> Vec<DriverFiles> {
    let mut found = Vec::new();
    if driver.is_empty() || driver.contains(['/', '\\']) {
        return found;
    }
    for (dir, environment) in ENVIRONMENTS {
        for version in VERSIONS {
            let file = format!("{root}/{dir}/{version}/{driver}.qsdrv");
            let Some(text) = read_text(c, &file) else {
                continue;
            };
            match parse(&text, dir, environment, version) {
                Ok(files) => {
                    found.push(files);
                    break;
                }
                Err(e) => warn!(file, error = %e, "driver manifest left out"),
            }
        }
    }
    found
}

/// The text of the file at `path`, when it can be read.
fn read_text(c: &dyn IfsClient, path: &str) -> Option<String> {
    let fd = c.open(path, OpenArgs::new(O_RDONLY | O_SHARE_RDWR)).ok()?;
    let mut bytes = Vec::new();
    while bytes.len() <= MAX_MANIFEST {
        match c.read(fd, 4096) {
            Ok(chunk) if chunk.is_empty() => break,
            Ok(chunk) => bytes.extend(chunk),
            Err(_) => {
                let _ = c.close(fd);
                return None;
            }
        }
    }
    let _ = c.close(fd);
    if bytes.len() > MAX_MANIFEST {
        return None;
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_names_the_files_in_its_directory() {
        let text = "# HP\ndriver = unidrv.dll\ndata=hpcu.gpd\nconfig=unidrvui.dll\nfile=stdnames.gpd\nfile=unires.dll\n";
        let files = parse(text, "x64", "Windows x64", 3).unwrap();
        assert_eq!(files.driver_path, "x64\\3\\unidrv.dll");
        assert_eq!(files.data_file, "x64\\3\\hpcu.gpd");
        assert_eq!(files.help_file, "");
        assert_eq!(
            files.dependent_files,
            ["x64\\3\\stdnames.gpd", "x64\\3\\unires.dll"]
        );
    }

    #[test]
    fn manifests_outside_the_rules_are_refused() {
        let base = "driver=a.dll\ndata=b.gpd\nconfig=c.dll\n";
        assert_eq!(
            parse("driver=a.dll\ndata=b\n", "x64", "e", 3),
            Err(ManifestError::Incomplete)
        );
        assert_eq!(
            parse(&format!("{base}driver=d.dll\n"), "x64", "e", 3),
            Err(ManifestError::Incomplete)
        );
        assert_eq!(
            parse(&format!("{base}colour=1\n"), "x64", "e", 3),
            Err(ManifestError::Line(4))
        );
        assert_eq!(
            parse(&format!("{base}just text\n"), "x64", "e", 3),
            Err(ManifestError::Line(4))
        );
        assert_eq!(
            parse(&format!("{base}file=..\\x.dll\n"), "x64", "e", 3),
            Err(ManifestError::Path(4))
        );
        assert_eq!(
            parse(&format!("{base}file=/etc/x\n"), "x64", "e", 3),
            Err(ManifestError::Path(4))
        );
    }
}
