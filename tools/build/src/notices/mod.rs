// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NOTICES.txt`: every third-party crate of `Cargo.lock` with its
//! version and license, the text of each license taken, and the
//! notices the crates ship (their own license files, which carry the
//! copyright lines), plus the toolchain parts linked into the image and
//! the kernel. `deny.toml`'s `[licenses] allow` is the authority: a
//! crate that needs anything else fails the build.
//!
//! The generated file is kept in the tree
//! (`lic/osobjects/shipped/NOTICES.txt`): the storage service writes it
//! to `/QQS/ProdData/Notices/` and the media carries it at its root.
//! `build notices` rewrites it; `build release` fails when it is stale.

pub mod spdx;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use tracing::info;

use crate::ctx::Ctx;
use crate::host::cmd::Cmd;

/// Where the generated file lives in the tree.
pub const TREE_FILE: &str = "lic/osobjects/shipped/NOTICES.txt";

/// The license texts, by SPDX identifier.
const TEXTS: &[(&str, &str)] = &[
    ("MIT", include_str!("../../licenses/MIT.txt")),
    ("Apache-2.0", include_str!("../../licenses/Apache-2.0.txt")),
    (
        "LLVM-exception",
        include_str!("../../licenses/LLVM-exception.txt"),
    ),
    (
        "BSD-2-Clause",
        include_str!("../../licenses/BSD-2-Clause.txt"),
    ),
    (
        "BSD-3-Clause",
        include_str!("../../licenses/BSD-3-Clause.txt"),
    ),
    ("ISC", include_str!("../../licenses/ISC.txt")),
    ("Zlib", include_str!("../../licenses/Zlib.txt")),
    ("0BSD", include_str!("../../licenses/0BSD.txt")),
    ("CC0-1.0", include_str!("../../licenses/CC0-1.0.txt")),
    ("Unlicense", include_str!("../../licenses/Unlicense.txt")),
    (
        "Unicode-3.0",
        include_str!("../../licenses/Unicode-3.0.txt"),
    ),
    (
        "CDLA-Permissive-2.0",
        include_str!("../../licenses/CDLA-Permissive-2.0.txt"),
    ),
    ("GPL-2.0", include_str!("../../licenses/GPL-2.0.txt")),
    (
        "Linux-syscall-note",
        include_str!("../../licenses/Linux-syscall-note.txt"),
    ),
];

/// The text of one license or exception.
pub fn text(id: &str) -> Option<&'static str> {
    TEXTS.iter().find(|(k, _)| *k == id).map(|(_, t)| *t)
}

/// One crate of the lock file.
#[derive(Debug, Clone)]
pub struct Crate {
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// The license expression as the crate states it.
    pub expr: String,
    /// The licenses QS/400 takes it under.
    pub chosen: Vec<String>,
    /// Its license and notice files: name and normalised text.
    pub files: Vec<(String, String)>,
}

/// `[licenses] allow` from `deny.toml`.
pub fn allowed(repo: &Path) -> Result<Vec<String>> {
    let text = fs::read_to_string(repo.join("deny.toml")).context("deny.toml")?;
    let v: toml::Value = toml::from_str(&text).context("deny.toml")?;
    let list = v
        .get("licenses")
        .and_then(|l| l.get("allow"))
        .and_then(toml::Value::as_array)
        .context("deny.toml has no [licenses] allow")?;
    Ok(list
        .iter()
        .filter_map(|s| s.as_str().map(str::to_string))
        .collect())
}

fn normalise(t: &str) -> String {
    let t = t.replace("\r\n", "\n");
    let mut out: String = t.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
    out = out.trim_matches('\n').to_string();
    out.push('\n');
    out
}

/// The license and notice files at the top of a package.
fn license_files(dir: &Path, chosen: &[String]) -> Vec<(String, String)> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            let u = n.to_ascii_uppercase();
            [
                "LICENSE",
                "LICENCE",
                "COPYING",
                "COPYRIGHT",
                "NOTICE",
                "UNLICENSE",
            ]
            .iter()
            .any(|p| u.starts_with(p))
        })
        .collect();
    names.sort();
    // With one file per license (LICENSE-MIT, LICENSE-APACHE), keep the
    // ones of the licenses taken, and every NOTICE.
    let keyed = |n: &str| {
        let u = n.to_ascii_uppercase();
        let kinds = [
            ("MIT", "MIT"),
            ("APACHE", "Apache-2.0"),
            ("BSD", "BSD"),
            ("ZLIB", "Zlib"),
            ("UNICODE", "Unicode-3.0"),
            ("ISC", "ISC"),
        ];
        kinds.iter().find(|(k, _)| u.contains(k)).map(|(_, id)| *id)
    };
    let specific: Vec<&String> = names.iter().filter(|n| keyed(n).is_some()).collect();
    let keep: Vec<String> = if specific.len() > 1 {
        names
            .iter()
            .filter(|n| {
                let u = n.to_ascii_uppercase();
                u.starts_with("NOTICE")
                    || u.starts_with("COPYRIGHT")
                    || keyed(n).is_none_or(|id| chosen.iter().any(|c| c.starts_with(id)))
            })
            .cloned()
            .collect()
    } else {
        names
    };
    keep.into_iter()
        .filter_map(|n| {
            fs::read(dir.join(&n))
                .ok()
                .map(|b| (n, normalise(&String::from_utf8_lossy(&b))))
        })
        .collect()
}

/// Every third-party crate of the workspace's lock file.
pub fn crates(repo: &Path) -> Result<Vec<Crate>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let out = Cmd::new(cargo, "cargo-metadata")
        .cwd(repo)
        .args(["metadata", "--format-version", "1", "--locked"])
        .capture()?;
    let m: serde_json::Value = serde_json::from_str(&out).context("cargo metadata")?;
    let members: Vec<&str> = m["workspace_members"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let allow = allowed(repo)?;
    let mut list = Vec::new();
    let mut errors = Vec::new();
    for p in m["packages"]
        .as_array()
        .context("cargo metadata: no packages")?
    {
        let id = p["id"].as_str().unwrap_or("");
        if members.contains(&id) {
            continue;
        }
        let name = p["name"].as_str().unwrap_or("").to_string();
        let version = p["version"].as_str().unwrap_or("").to_string();
        let Some(expr) = p["license"].as_str().map(str::to_string) else {
            errors.push(format!("{name} {version}: no license expression"));
            continue;
        };
        let chosen = match spdx::chosen(&expr, &allow) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("{name} {version}: {e}"));
                continue;
            }
        };
        let dir = PathBuf::from(p["manifest_path"].as_str().unwrap_or(""))
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        let files = license_files(&dir, &chosen);
        list.push(Crate {
            name,
            version,
            expr,
            chosen,
            files,
        });
    }
    if !errors.is_empty() {
        bail!(
            "licenses outside deny.toml's allow list:\n  {}",
            errors.join("\n  ")
        );
    }
    list.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(list)
}

fn rule(out: &mut String, title: &str) {
    let _ = writeln!(out, "\n{}\n{title}\n{}\n", "=".repeat(72), "=".repeat(72));
}

/// The text of `NOTICES.txt` for `crates`, the kernel `kernel_version`.
pub fn render(crates: &[Crate], kernel_version: &str) -> Result<String> {
    let mut o = String::new();
    let _ = writeln!(o, "QS/400 {} third-party notices", release::RELEASE);
    let _ = write!(
        o,
        "\nQS/400 is licensed under the MIT license (the LICENSE file of its source).\n\
         It contains the work of others, listed here with their licenses:\n\n\
         1. The Linux kernel {kernel_version}, under GPL-2.0 with the Linux syscall note.\n\
         \x20  The kernel's source offer is QS400/SRC/ on the install media and\n\
         \x20  SOURCE-OFFER.txt beside each release. No firmware is built into the\n\
         \x20  kernel (CONFIG_EXTRA_FIRMWARE is empty) and the media carry none.\n\
         2. musl libc, under MIT: Copyright (c) 2005-2020 Rich Felker, et al.\n\
         \x20  musl's COPYRIGHT file names its authors and the few files it holds\n\
         \x20  under other permissive terms.\n\
         3. The Rust standard library (core, alloc, std and their dependencies),\n\
         \x20  under MIT OR Apache-2.0, taken under MIT; LLVM's libunwind and\n\
         \x20  compiler-rt builtins, under Apache-2.0 WITH LLVM-exception.\n\
         4. The {} Rust crates below, each under the license named.\n",
        crates.len()
    );
    rule(
        &mut o,
        "Crates: name, version, license as stated, license taken",
    );
    for c in crates {
        let _ = writeln!(
            o,
            "{} {}\n    {}\n    taken under: {}",
            c.name,
            c.version,
            c.expr,
            c.chosen.join(" AND ")
        );
    }
    // Every license taken, the kernel's and the toolchain's included.
    let mut ids: Vec<String> = crates
        .iter()
        .flat_map(|c| c.chosen.iter().cloned())
        .collect();
    ids.extend(
        [
            "GPL-2.0 WITH Linux-syscall-note",
            "MIT",
            "Apache-2.0 WITH LLVM-exception",
        ]
        .map(str::to_string),
    );
    ids.sort();
    ids.dedup();
    rule(&mut o, "License texts");
    for id in &ids {
        let _ = writeln!(o, "---- {id} ----\n");
        for part in id.split(" WITH ") {
            let t = text(part).with_context(|| {
                format!("no text for license {part} under tools/build/licenses/")
            })?;
            o.push_str(&normalise(t));
            o.push('\n');
        }
    }
    // The crates' own files, each distinct text once.
    let mut by_text: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for c in crates {
        for (name, t) in &c.files {
            by_text
                .entry(t.as_str())
                .or_default()
                .push(format!("{} {} ({name})", c.name, c.version));
        }
    }
    let mut blocks: Vec<(Vec<String>, &str)> =
        by_text.into_iter().map(|(t, who)| (who, t)).collect();
    blocks.sort();
    rule(&mut o, "Notices and license files shipped by the crates");
    for (who, t) in blocks {
        let _ = writeln!(o, "---- {} ----\n", who.join(", "));
        o.push_str(t);
        o.push('\n');
    }
    let bare: Vec<String> = crates
        .iter()
        .filter(|c| c.files.is_empty())
        .map(|c| format!("{} {}", c.name, c.version))
        .collect();
    if !bare.is_empty() {
        let _ = writeln!(
            o,
            "---- No license file in the package (the license text above applies) ----\n\n{}\n",
            bare.join(", ")
        );
    }
    Ok(o)
}

/// The file as the tree's lock file and registry sources make it now,
/// and the crates in it.
pub fn generate(repo: &Path) -> Result<(String, Vec<Crate>)> {
    let version = fs::read_to_string(repo.join("kernel/VERSION")).context("kernel/VERSION")?;
    let crates = crates(repo)?;
    Ok((render(&crates, version.trim())?, crates))
}

/// `build notices`: rewrite the tree's file; with `check`, fail when it
/// differs instead.
pub fn notices(ctx: &Ctx, check: bool) -> Result<PathBuf> {
    let (text, crates) = generate(&ctx.paths.repo)?;
    for (license, n) in summary(&crates) {
        info!("notices: {n:>4} crates under {license}");
    }
    let path = ctx.paths.repo.join(TREE_FILE);
    let old = fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    if old == text {
        info!("notices: {} is current", path.display());
    } else {
        if check {
            bail!("{TREE_FILE} is stale: run `cargo run -p build -- notices` and commit it");
        }
        fs::write(&path, &text).with_context(|| path.display().to_string())?;
        info!("notices: {} written ({} bytes)", path.display(), text.len());
    }
    Ok(path)
}

/// `(license taken, crates)`, for the report.
pub fn summary(crates: &[Crate]) -> Vec<(String, usize)> {
    let mut m: BTreeMap<String, usize> = BTreeMap::new();
    for c in crates {
        *m.entry(c.chosen.join(" AND ")).or_default() += 1;
    }
    m.into_iter().collect()
}
