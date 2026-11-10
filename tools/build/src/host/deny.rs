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

/// Crates outside the workspace with their own lockfile, checked the same way.
const STANDALONE: &[&str] = &["tools/smbtest/Cargo.toml"];

/// `build deny`.
pub fn deny(ctx: &Ctx) -> Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    if !on_path("cargo-deny") {
        bail!("cargo-deny not installed: cargo install cargo-deny --locked");
    }
    check_graph(ctx, &cargo, None)?;
    for manifest in STANDALONE {
        check_graph(ctx, &cargo, Some(manifest))?;
    }
    Ok(())
}

/// One graph: the workspace (`None`) or a standalone manifest.
fn check_graph(ctx: &Ctx, cargo: &OsString, manifest: Option<&str>) -> Result<()> {
    let what = manifest.unwrap_or("workspace");
    let scope: Vec<&str> = match manifest {
        Some(m) => vec!["--manifest-path", m],
        None => vec![],
    };
    info!("{what}: cargo deny check");
    // cargo-deny reads ./deny.toml (the repository's) for every graph.
    Cmd::new(cargo, "cargo-deny")
        .cwd(&ctx.paths.repo)
        .arg("deny")
        .args(&scope)
        .arg("check")
        .run(ctx)?;

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
            .args(if manifest.is_some() { &scope[..] } else { &["--workspace"][..] })
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
