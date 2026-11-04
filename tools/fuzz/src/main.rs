// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `qsfuzz list | run TARGET [SECS] | all [SECS [JOBS]] | repro TARGET FILE`.

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
            // `all SECS [JOBS]`: each target in its own process, JOBS at once.
            let jobs: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1).max(1);
            let exe = std::env::current_exe()?;
            let mut bad = Vec::new();
            for chunk in TARGETS.chunks(jobs) {
                let kids: Vec<_> = chunk
                    .iter()
                    .map(|t| {
                        std::process::Command::new(&exe)
                            .args(["run", t.name, &secs(1).to_string()])
                            .spawn()
                            .map(|c| (t.name, c))
                    })
                    .collect::<Result<_, _>>()?;
                for (name, mut c) in kids {
                    if !c.wait()?.success() {
                        bad.push(name);
                    }
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
