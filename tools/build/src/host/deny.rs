// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `cargo deny check` plus a direct scan of the dependency graph that is
//! actually compiled, for `links` keys and `cc`-style build helpers.
//!
//! `cargo tree` is the feature- and target-aware view of the graph, so the
//! active crate set comes from it (normal and build edges) for the host and
//! for the image target. `Cargo.lock` and `cargo metadata` also list optional
//! dependencies nobody activates (for example `rustls-webpki`'s `ring`);
//! those are never downloaded or built.
//!
//! Two graphs are checked: the workspace, and `tools/smbtest`, which sits
//! outside it with its own lockfile (see its `Cargo.toml`).

use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;

use anyhow::{Context, Result, bail};
use tracing::info;

use crate::ctx::Ctx;
use crate::host::cmd::{Cmd, on_path};
use crate::userland::RUST_TARGET;

const C_BUILDERS: &[&str] = &["cc", "cmake", "bindgen", "pkg-config", "vcpkg"];

/// A crate outside the workspace with its own lockfile, checked the same
/// way under `deny.toml` plus advisories ignored for its graph only.
struct Standalone {
    manifest: &'static str,
    /// `(advisory, reason)` ignored in this graph alone.
    ignore: &'static [(&'static str, &'static str)],
}

const STANDALONE: &[Standalone] = &[Standalone {
    manifest: "tools/smbtest/Cargo.toml",
    // `rsa` comes only through the `smb` client's sspi/picky, which pin a
    // release candidate; the host test client signs on with NTLM only.
    ignore: &[(
        "RUSTSEC-2023-0071",
        "rsa only via sspi/picky in the host test client tools/smbtest (NTLM, no RSA use)",
    )],
}];

/// `build deny`.
pub fn deny(ctx: &Ctx) -> Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    if !on_path("cargo-deny") {
        bail!("cargo-deny not installed: cargo install cargo-deny --locked");
    }
    check_graph(ctx, &cargo, None)?;
    for s in STANDALONE {
        check_graph(ctx, &cargo, Some(s))?;
    }
    Ok(())
}

/// `deny.toml` with `ignores` in place of its empty advisory ignore list.
fn standalone_config(root: &str, ignores: &[(&str, &str)]) -> Result<String> {
    let list: Vec<String> = ignores
        .iter()
        .map(|(id, why)| format!("    {{ id = \"{id}\", reason = \"{why}\" }},"))
        .collect();
    let line = format!("ignore = [\n{}\n]", list.join("\n"));
    let mut found = false;
    let out: Vec<String> = root
        .lines()
        .map(|l| {
            if l.trim() == "ignore = []" && !found {
                found = true;
                line.clone()
            } else {
                l.to_owned()
            }
        })
        .collect();
    if !found {
        bail!("deny.toml has no `ignore = []` under [advisories]");
    }
    Ok(out.join("\n") + "\n")
}

/// One graph: the workspace (`None`) or a standalone manifest.
fn check_graph(ctx: &Ctx, cargo: &OsString, standalone: Option<&Standalone>) -> Result<()> {
    let manifest = standalone.map(|s| s.manifest);
    let what = manifest.unwrap_or("workspace");
    let scope: Vec<&str> = match manifest {
        Some(m) => vec!["--manifest-path", m],
        None => vec![],
    };
    // The workspace uses ./deny.toml; a standalone graph a copy of it with
    // its own advisory ignores.
    let config = match standalone {
        Some(s) if !s.ignore.is_empty() => {
            let root = std::fs::read_to_string(ctx.paths.repo.join("deny.toml"))
                .context("read deny.toml")?;
            let path = ctx.paths.target.join("deny-standalone.toml");
            std::fs::create_dir_all(&ctx.paths.target).context("create target dir")?;
            std::fs::write(&path, standalone_config(&root, s.ignore)?)
                .context("write the standalone deny config")?;
            Some(path)
        }
        _ => None,
    };
    info!("{what}: cargo deny check");
    let mut deny = Cmd::new(cargo, "cargo-deny");
    deny.cwd(&ctx.paths.repo).arg("deny").args(&scope);
    if let Some(c) = &config {
        deny.arg("--config").arg(c);
    }
    deny.arg("check").run(ctx)?;

    let json = Cmd::new(cargo, "cargo-metadata")
        .cwd(&ctx.paths.repo)
        .args(["metadata", "--format-version", "1", "--locked"])
        .args(&scope)
        .capture()?;
    let meta: serde_json::Value = serde_json::from_str(&json).context("cargo metadata output")?;
    let links: HashMap<String, String> = meta["packages"]
        .as_array()
        .context("no packages in metadata")?
        .iter()
        .filter_map(|p| {
            Some((
                p["name"].as_str()?.to_string(),
                p["links"].as_str()?.to_string(),
            ))
        })
        .collect();

    let host = host_triple()?;
    let mut platforms = vec![host.as_str(), RUST_TARGET];
    platforms.dedup();
    for platform in platforms {
        let tree = Cmd::new(cargo, "cargo-tree")
            .cwd(&ctx.paths.repo)
            .arg("tree")
            .args(if manifest.is_some() {
                &scope[..]
            } else {
                &["--workspace"][..]
            })
            .args(["--locked", "-e", "normal,build", "--prefix", "none"])
            .args(["--format", "{p}", "--target", platform])
            .capture()?;
        let active: HashSet<&str> = tree
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .collect();

        let mut offenders: Vec<String> = Vec::new();
        for name in &active {
            if C_BUILDERS.contains(name) {
                offenders.push(format!("{name}: C build helper is in the graph"));
            }
            if let Some(l) = links.get(*name) {
                offenders.push(format!("{name}: links = \"{l}\""));
            }
        }
        if !offenders.is_empty() {
            offenders.sort();
            bail!(
                "{what}, {platform}: C-linking crates in the compiled graph:\n  {}",
                offenders.join("\n  ")
            );
        }
        info!(
            "{what}, {platform}: {} crates compiled, no links keys, no C build helpers",
            active.len()
        );
    }
    Ok(())
}

/// The toolchain's host triple, from `rustc -vV`.
fn host_triple() -> Result<String> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let out = Cmd::new(rustc, "rustc").arg("-vV").capture()?;
    out.lines()
        .find_map(|l| l.strip_prefix("host: "))
        .map(str::to_string)
        .context("rustc -vV printed no host line")
}
