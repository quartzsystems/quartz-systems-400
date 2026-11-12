// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Independent verification of the final `.config`. The merge is not trusted.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use tracing::info;

use crate::ctx::Ctx;

/// Symbols every product's kernel must have `=y`: an EFI-stub image
/// with a baked command line and an embedded initramfs, compressed
/// once, and the file systems `/init` mounts first.
pub const BASELINE_Y: &[&str] = &[
    "EFI",
    "EFI_STUB",
    "CMDLINE_BOOL",
    "CMDLINE_OVERRIDE",
    "BLK_DEV_INITRD",
    "KERNEL_XZ",
    "DEVTMPFS",
    "DEVTMPFS_MOUNT",
    "PROC_FS",
    "SYSFS",
    "TMPFS",
];

/// Symbols no product's kernel may have `=y`: the image is monolithic
/// and carries no debug information.
pub const BASELINE_N: &[&str] = &["MODULES", "MODULE_SIG", "DEBUG_INFO"];

/// The baseline as a configuration fragment, merged before the
/// product's own: the symbols, and `cmdline` as the baked command line
/// (each UKI's build replaces it with its own).
pub fn baseline_fragment(cmdline: &str) -> String {
    let mut s = String::from("# Generated: the pipeline's baseline.\n");
    let _ = writeln!(s, "CONFIG_CMDLINE=\"{cmdline}\"");
    for y in BASELINE_Y {
        let _ = writeln!(s, "CONFIG_{y}=y");
    }
    for n in BASELINE_N {
        let _ = writeln!(s, "# CONFIG_{n} is not set");
    }
    // DEBUG_INFO follows a choice; say which member.
    s.push_str(
        "CONFIG_DEBUG_INFO_NONE=y
",
    );
    s
}

/// Parse a `.config` into symbol -> value (`y`, `m`, `n`, numbers, quoted strings).
pub fn parse(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("CONFIG_") {
            if let Some((k, v)) = rest.split_once('=') {
                map.insert(k.to_string(), v.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("# CONFIG_")
            && let Some(k) = rest.strip_suffix(" is not set")
        {
            map.insert(k.to_string(), "n".to_string());
        }
    }
    map
}

/// Fail unless every required symbol (the baseline's and the product's)
/// has the required value and the command line is exactly `cmdline`.
/// With `cpio`, also require the embedded initramfs settings.
pub fn verify(ctx: &Ctx, config: &Path, cmdline: &str, cpio: Option<&Path>) -> Result<()> {
    let text =
        fs::read_to_string(config).with_context(|| format!("cannot read {}", config.display()))?;
    let map = parse(&text);
    let get = |k: &str| map.get(k).map_or("n", String::as_str);
    let mut problems = Vec::new();

    let required_y = BASELINE_Y.iter().chain(ctx.product.required_y);
    let required_n = BASELINE_N.iter().chain(ctx.product.required_n);
    for sym in required_y.clone() {
        if get(sym) != "y" {
            problems.push(format!("CONFIG_{sym} must be y, is {}", get(sym)));
        }
    }
    for sym in required_n.clone() {
        if get(sym) == "y" {
            problems.push(format!("CONFIG_{sym} must not be y"));
        }
    }
    let want_cmdline = format!("\"{cmdline}\"");
    if get("CMDLINE") != want_cmdline {
        problems.push(format!(
            "CONFIG_CMDLINE must be {want_cmdline}, is {}",
            get("CMDLINE")
        ));
    }
    if let Some(cpio) = cpio {
        let want = format!("\"{}\"", cpio.display());
        if get("INITRAMFS_SOURCE") != want {
            problems.push(format!(
                "CONFIG_INITRAMFS_SOURCE must be {want}, is {}",
                get("INITRAMFS_SOURCE")
            ));
        }
        if get("INITRAMFS_COMPRESSION_NONE") != "y" {
            problems.push("CONFIG_INITRAMFS_COMPRESSION_NONE must be y (the image is compressed once, with xz)".to_string());
        }
    }

    ensure!(
        problems.is_empty(),
        "kernel: {} config problem(s) in {}:\n  {}",
        problems.len(),
        config.display(),
        problems.join("\n  ")
    );
    info!(
        "kernel: .config verified ({} symbols)",
        required_y.count() + required_n.count() + 1
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn parses_set_unset_and_strings() {
        let m = parse("CONFIG_A=y\n# CONFIG_B is not set\nCONFIG_C=\"x y\"\n# comment\n");
        assert_eq!(m["A"], "y");
        assert_eq!(m["B"], "n");
        assert_eq!(m["C"], "\"x y\"");
        assert!(!m.contains_key("comment"));
    }
}
