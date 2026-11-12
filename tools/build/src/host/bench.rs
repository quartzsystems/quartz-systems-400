// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `build bench`: the host-side benchmarks, no QEMU. The SQL benchmarks
//! (`tools/conformance/tests/sql_bench.rs`, the numbers of `docs/SQL.md`
//! §11) and the database loop of Phase 3 (`db_bench`), each a release
//! build of its test; their `bench` lines go to the log and the console.

use std::env;

use anyhow::Result;
use tracing::info;

use buildkit::Ctx;
use buildkit::host::cmd::Cmd;

/// The benchmark tests: (test, what it measures).
const BENCHES: &[(&str, &str)] = &[("sql_bench", "SQL"), ("db_bench", "record-level access")];

/// Run every benchmark; `rows` scales the SQL one's large table.
pub fn bench(ctx: &Ctx, rows: Option<u64>) -> Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    for (test, what) in BENCHES {
        info!("bench: {what} ({test})");
        let mut c = Cmd::new(&cargo, *test);
        c.cwd(&ctx.paths.repo).args([
            "test",
            "--release",
            "-p",
            "conformance",
            "--test",
            test,
            "--",
            "--ignored",
            "--nocapture",
        ]);
        if let Some(n) = rows {
            c.env("QS400_SQL_BENCH_ROWS", n.to_string());
        }
        let (ok, out) = c.capture_all()?;
        for line in out
            .lines()
            .filter(|l| l.starts_with("bench ") || l.contains(" ms") || l.contains("/s"))
        {
            println!("{line}");
        }
        anyhow::ensure!(ok, "bench: {test} failed:\n{out}");
    }
    Ok(())
}
