// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `qsfuzz list | run TARGET [SECS] | all [SECS] | repro TARGET FILE`.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use fuzz::engine::{self, Options};
use fuzz::targets::{self, TARGETS, Target};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run(t: &Target, secs: u64) -> Result<bool> {
    let o = Options {
        time: Duration::from_secs(secs),
        corpus: dir().join("corpus").join(t.name),
        crashes: dir().join("crashes").join(t.name),
        hang: Duration::from_secs(10),
        seed: std::env::var("QS400_FUZZ_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0),
    };
    let r = engine::fuzz(t, &o)?;
    println!(
        "{:<8} {:>10} execs  {:>6} in corpus  {:>7} edges  {} crash(es)",
        t.name,
        r.execs,
        r.corpus,
        r.covered,
        r.crashes.len()
    );
    for (p, f) in &r.crashes {
        println!("         {p}\n         -> {}", f.display());
    }
    Ok(r.crashes.is_empty())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let secs = |i: usize| args.get(i).and_then(|s| s.parse().ok()).unwrap_or(60);
    match args.first().map(String::as_str) {
        Some("list") | None => {
            println!(
                "{} targets; coverage {}",
                TARGETS.len(),
                if fuzz::cov::instrumented() { "on" } else { "off (build with `build fuzz`)" }
            );
            for t in TARGETS {
                println!("{:<8} {}", t.name, t.what);
            }
        }
        Some("run") => {
            let name = args.get(1).context("run TARGET [SECS]")?;
            let t = targets::find(name).with_context(|| format!("no target {name}"))?;
            if !run(t, secs(2))? {
                bail!("{name} crashed");
            }
        }
        Some("all") => {
            let mut bad = Vec::new();
            for t in TARGETS {
                if !run(t, secs(1))? {
                    bad.push(t.name);
                }
            }
            if !bad.is_empty() {
                bail!("crashes in: {}", bad.join(", "));
            }
        }
        Some("repro") => {
            let name = args.get(1).context("repro TARGET FILE")?;
            let t = targets::find(name).with_context(|| format!("no target {name}"))?;
            let data = std::fs::read(args.get(2).context("repro TARGET FILE")?)?;
            (t.run)(&data);
            println!("{name}: no panic");
        }
        Some(o) => bail!("unknown command {o}"),
    }
    Ok(())
}
