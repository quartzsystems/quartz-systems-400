// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 2 steps of the disk suite: the system libraries, system values
//! in the store, the object bench with a clean close, and the seeded
//! power-pull loop.

use std::thread;
use std::time::Duration;

use crate::error::HarnessError;
use crate::qemu::session::Session;
use crate::steps::{SRC_SEQUENCE, StepReport, Suite};

const CMD: Duration = Duration::from_secs(10);

/// The libraries a fresh system has, in the order `lib ls` must show
/// them (EBCDIC: letters before digits, blank before everything).
pub const SYSTEM_LIBRARIES: &[&str] = &[
    "QGPL",
    "QHLPSYS",
    "QRCL",
    "QRECOVERY",
    "QSPL",
    "QSYS",
    "QSYS2",
    "QUSRSYS",
];

/// Objects the power-pull bench asks for.
const PULL_BENCH: u64 = 200_000;

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: u64) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D) % n
    }
}

/// `QBENCHn` object counts from a `lib ls` listing, in library order.
fn bench_counts(listing: &str) -> Vec<u64> {
    let mut counts = Vec::new();
    for i in 0..10 {
        let name = format!("QBENCH{i}");
        let count = listing
            .lines()
            .find(|l| l.split_whitespace().next() == Some(name.as_str()))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        counts.push(count);
    }
    counts
}

impl Suite {
    /// Step 7: the first boot initialized ASP 1, and `lib ls` lists the
    /// system libraries in EBCDIC order.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn libraries(&self, s: &mut Session) -> Result<String, String> {
        if !s.log.contains("SRC C6001007") {
            return Err("C6001007 (ASP 1 initialized) did not appear on the first boot".into());
        }
        let listing = s.command("lib ls", CMD)?;
        let names: Vec<&str> = listing
            .lines()
            .skip(1)
            .filter_map(|l| l.split_whitespace().next())
            .take(SYSTEM_LIBRARIES.len())
            .collect();
        if names != SYSTEM_LIBRARIES {
            return Err(format!("libraries out of order: {names:?}\n{listing}"));
        }
        Ok(format!("{} system libraries in EBCDIC order", names.len()))
    }

    /// Step 8: the value set earlier lives in `*SYSVAL`; the LIC record
    /// carries only the IPL subset.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn sysval_in_store(&self, s: &mut Session) -> Result<String, String> {
        let security = s.command("sysval QSECURITY", CMD)?;
        if !security.contains("QSECURITY: 50") {
            return Err(format!("QSECURITY is not 50: {security}"));
        }
        let lic = s.command("lic", CMD)?;
        if lic.contains("tlv 0x0100") {
            return Err(format!(
                "the LIC record still carries every system value:\n{lic}"
            ));
        }
        if !lic.contains("tlv 0x0101") {
            return Err(format!("the LIC record has no IPL subset:\n{lic}"));
        }
        Ok("QSECURITY from *SYSVAL; LIC record holds only the IPL subset".into())
    }

    /// Step 9: `obj bench N`, a clean reclaim check, then a clean power
    /// down and restart that replays nothing.
    pub(crate) fn object_bench(&self, s: &mut Session) -> Result<String, String> {
        let n = self.bench;
        let timeout = Duration::from_secs(120 + n / 100);
        let out = s.command(&format!("obj bench {n}"), timeout)?;
        let line = out
            .lines()
            .find(|l| l.starts_with("created "))
            .ok_or_else(|| format!("no bench result:\n{out}"))?
            .to_string();
        let check = s.command("rclstg *CHECK", Duration::from_secs(300))?;
        if !check.contains("0 problem(s)") {
            return Err(format!("rclstg *CHECK after the bench:\n{check}"));
        }
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001010", CMD)?;
        s.expect("SRC D1001090", CMD)?;
        let recovery = s.expect("SRC C1001080", self.boot_timeout)?;
        if !recovery.contains("0 transaction(s) replayed") {
            return Err(format!("the clean close left work to replay:\n{recovery}"));
        }
        s.expect("SRC C6001005", self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        Ok(format!(
            "{line}; reclaim check clean; clean restart replayed 0"
        ))
    }

    /// Step 10: start a bench, pull the power at a seeded moment, boot
    /// again, and check recovery, checksums, reclaim and object counts.
    pub(crate) fn power_pulls(&self) -> Result<StepReport, HarnessError> {
        let name = "10 power pulls";
        let start = std::time::Instant::now();
        let mut details = Vec::new();
        for seed in 0..u64::from(self.pulls) {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (seed + 1));
            let delay = Duration::from_millis(1500 + rng.below(6000));
            let fail = |s: &Session, reason: String| HarnessError::Failed {
                step: name,
                reason: format!("seed {seed}: {reason}"),
                log: s.log.clone(),
            };
            // Boot, start the bench, pull the power.
            let mut s = Session::spawn(&self.launch)?;
            if let Err(e) = s.expect(crate::PROMPT, self.boot_timeout) {
                return Err(fail(&s, e));
            }
            if let Err(e) = s.send(&format!("obj bench {PULL_BENCH}")) {
                return Err(fail(&s, e));
            }
            thread::sleep(delay);
            s.kill();
            drop(s);
            // Recovery.
            let mut s = Session::spawn(&self.launch)?;
            let recovery = match s.expect("SRC C1001080", self.boot_timeout) {
                Ok(text) => text,
                Err(e) => return Err(fail(&s, e)),
            };
            let replayed = recovery
                .lines()
                .find(|l| l.contains("transaction(s) replayed"))
                .map(|l| l.trim().to_string())
                .unwrap_or_default();
            if let Err(e) =
                s.expect_in_order(&SRC_SEQUENCE[SRC_SEQUENCE.len() - 4..], self.boot_timeout)
            {
                return Err(fail(&s, e));
            }
            if let Err(e) = s.expect(crate::PROMPT, CMD) {
                return Err(fail(&s, e));
            }
            let chk = s
                .command("chkstg", Duration::from_secs(300))
                .map_err(|e| fail(&s, e))?;
            if !chk.contains(": 0 bad") {
                return Err(fail(&s, format!("chkstg after the pull:\n{chk}")));
            }
            let rcl = s
                .command("rclstg *CHECK", Duration::from_secs(300))
                .map_err(|e| fail(&s, e))?;
            if !rcl.contains("0 problem(s)") {
                return Err(fail(&s, format!("rclstg *CHECK after the pull:\n{rcl}")));
            }
            let listing = s.command("lib ls", CMD).map_err(|e| fail(&s, e))?;
            let counts = bench_counts(&listing);
            let total: u64 = counts.iter().sum();
            let per_lib = counts[0];
            if counts.iter().any(|&c| c != per_lib)
                || !per_lib.is_multiple_of(100)
                || total > PULL_BENCH
            {
                return Err(fail(
                    &s,
                    format!("bench object counts are not whole committed batches: {counts:?}"),
                ));
            }
            s.command("pwrdwn", CMD).ok();
            s.wait_exit(Duration::from_secs(15));
            details.push(format!(
                "seed {seed}: pulled after {:.1}s, {replayed}, {total} objects kept",
                delay.as_secs_f64()
            ));
        }
        Ok(StepReport {
            name,
            elapsed: start.elapsed(),
            detail: if details.is_empty() {
                "skipped (pulls 0)".into()
            } else {
                details.join("; ")
            },
        })
    }
}
