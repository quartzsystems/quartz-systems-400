// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where things live. Everything generated goes under one target directory.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::kernel::UkiKind;
use crate::userland::RUST_TARGET;

/// Repository root and the target directory.
pub struct Paths {
    /// Repository root (two levels above `tools/build`).
    pub repo: PathBuf,
    /// Output root. `QS400_TARGET_DIR`, else `CARGO_TARGET_DIR`, else `<repo>/target`.
    pub target: PathBuf,
}

impl Paths {
    /// Resolve from the compile-time crate location and the environment.
    pub fn discover() -> Result<Self> {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .context("build crate is not under tools/")?
            .to_path_buf();
        let target = env::var_os("QS400_TARGET_DIR")
            .or_else(|| env::var_os("CARGO_TARGET_DIR"))
            .map_or_else(|| repo.join("target"), PathBuf::from);
        let target = if target.is_absolute() {
            target
        } else {
            env::current_dir()?.join(target)
        };
        Ok(Self { repo, target })
    }

    /// `kernel/` in the repository: VERSION and SHA256.
    pub fn kernel_pin_dir(&self) -> PathBuf {
        self.repo.join("kernel")
    }

    /// `kernel/config/` in the repository.
    pub fn fragments(&self) -> PathBuf {
        self.repo.join("kernel").join("config")
    }

    /// Untrusted downloads and extracted sources.
    pub fn src(&self) -> PathBuf {
        self.target.join("src")
    }

    /// Kernel build directory.
    pub fn kernel_build(&self) -> PathBuf {
        self.target.join("kernel")
    }

    /// Image output directory, where the UKI, disk image and manifest live.
    pub fn images(&self) -> PathBuf {
        self.target.join("images")
    }

    /// Tool logs.
    pub fn logs(&self) -> PathBuf {
        self.target.join("logs")
    }

    /// Generated dev signing keys.
    pub fn keys(&self) -> PathBuf {
        self.target.join("keys")
    }

    /// The built `slic` binary.
    pub fn slic(&self) -> PathBuf {
        self.target.join(RUST_TARGET).join("release").join("slic")
    }

    /// The built `qsjob` binary.
    pub fn qsjob(&self) -> PathBuf {
        self.target.join(RUST_TARGET).join("release").join("qsjob")
    }

    /// The initramfs cpio.
    pub fn initramfs(&self) -> PathBuf {
        self.images().join("initramfs.cpio")
    }

    /// Pointer file naming the current UKI of one kind.
    pub fn uki_pointer(&self, kind: UkiKind) -> PathBuf {
        self.images().join(kind.pointer())
    }

    /// Pointer file naming the current ISO.
    pub fn iso_pointer(&self) -> PathBuf {
        self.images().join("iso.path")
    }

    /// Pointer file naming the current disk image.
    pub fn disk_pointer(&self) -> PathBuf {
        self.images().join("disk.path")
    }

    /// A path relative to the target directory, with forward slashes.
    pub fn relative(&self, path: &Path) -> String {
        let rel = path.strip_prefix(&self.target).unwrap_or(path);
        rel.to_string_lossy().replace('\\', "/")
    }
}

/// Read a pointer file and check the target exists.
pub fn read_pointer(pointer: &Path) -> Result<PathBuf> {
    let text =
        fs::read_to_string(pointer).with_context(|| format!("missing {}", pointer.display()))?;
    let path = PathBuf::from(text.trim());
    anyhow::ensure!(
        path.is_file(),
        "{} names a missing file {}",
        pointer.display(),
        path.display()
    );
    Ok(path)
}

/// Write a pointer file.
pub fn write_pointer(pointer: &Path, target: &Path) -> Result<()> {
    fs::write(pointer, format!("{}\n", target.display()))
        .with_context(|| format!("cannot write {}", pointer.display()))
}
