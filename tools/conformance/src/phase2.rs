// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 2 steps of the disk suite: the system libraries, system values
//! in the store, the object bench with a clean close, and the seeded
//! power-pull loop.

use std::thread;
use std::time::Duration;

use std::fs;

use crate::error::HarnessError;
use crate::qemu::Launch;
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

/// Whether the counts are a state only whole committed batches leave.
/// Creation spreads every batch of 1000 over the ten libraries, so the
/// counts rise together; the leftover delete that precedes it empties
/// the libraries in order, 1000 at a time. So counts are multiples of
/// 100, never decrease along the list, and at most one library is
/// partway between empty and the last one.
fn whole_batches(counts: &[u64]) -> bool {
    let Some(&last) = counts.last() else {
        return false;
    };
    counts.iter().all(|c| c.is_multiple_of(100))
        && counts.windows(2).all(|w| w[0] <= w[1])
        && counts.iter().filter(|&&c| c != 0 && c != last).count() <= 1
}

/// The console screen must equal `conformance/golden/dst/<name>.txt`,
/// whitespace-exact after trailing blanks are dropped from every line.
fn golden_matches(s: &mut Session, name: &str) -> Result<&'static str, String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("golden")
        .join("dst")
        .join(format!("{name}.txt"));
    let want = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let got = s.command("screen", CMD)?;
    let norm = |t: &str| -> Vec<String> {
        let mut lines: Vec<String> = t.lines().map(|l| l.trim_end().to_string()).collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines
    };
    // Whatever the screen echo left in the buffer comes first; the dump
    // is the last 24 lines before the prompt.
    let raw: Vec<&str> = got.lines().collect();
    let tail = raw[raw.len().saturating_sub(24)..].join("\n");
    let (g, w) = (norm(&tail), norm(&want));
    if g != w {
        let first = g
            .iter()
            .zip(w.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(g.len().min(w.len()));
        return Err(format!(
            "screen {name} differs from its golden file at line {}:\n  got:  {:?}\n  want: {:?}",
            first + 1,
            g.get(first),
            w.get(first)
        ));
    }
    Ok("ok")
}

/// Poll `mirror status` until the background resume ends. Returns the
/// last progress line seen.
fn wait_resume(s: &mut Session) -> Result<String, String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(600);
    let mut last = String::new();
    loop {
        let out = s.command("svc.mirror status", CMD)?;
        if out.contains("no resume running") {
            return Ok(if last.is_empty() {
                "at once".into()
            } else {
                last
            });
        }
        last = out.trim().to_string();
        if std::time::Instant::now() > deadline {
            return Err(format!("resume did not finish: {last}"));
        }
        thread::sleep(Duration::from_millis(500));
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
        let listing = s.command("svc.lib ls", CMD)?;
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
        let security = s.command("svc.sysval QSECURITY", CMD)?;
        if !security.contains("QSECURITY: 40") {
            return Err(format!("QSECURITY is not 40: {security}"));
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
        let out = s.command(&format!("svc.obj bench {n}"), timeout)?;
        let line = out
            .lines()
            .find(|l| l.starts_with("created "))
            .ok_or_else(|| format!("no bench result:\n{out}"))?
            .to_string();
        let check = s.command("svc.rclstg *CHECK", Duration::from_secs(300))?;
        if !check.contains("0 problem(s)") {
            return Err(format!("svc.rclstg *CHECK after the bench:\n{check}"));
        }
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001010", CMD)?;
        s.expect("SRC D1001090", CMD)?;
        let recovery = s.expect("SRC C1001080", self.boot_timeout)?;
        if !recovery.contains("0 transaction(s) replayed") {
            return Err(format!("the clean close left work to replay:\n{recovery}"));
        }
        s.expect("SRC C6001005", self.boot_timeout)?;
        s.expect(crate::steps::SRC_IPL_COMPLETE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        Ok(format!(
            "{line}; reclaim check clean; clean restart replayed 0"
        ))
    }

    /// Step 11: pair the unit with a second blank disk over the service
    /// console, synchronize, restart; boot without the second disk
    /// (A6001255, the pair runs on one half, writes still work); boot
    /// with it again (stale, suspended), resume, and verify both halves.
    pub(crate) fn mirrored_pair(&self) -> Result<StepReport, HarnessError> {
        let name = "11 mirrored pair";
        let start = std::time::Instant::now();
        let second = self.out_dir.join("mirror.img");
        let f = fs::File::create(&second)?;
        f.set_len(4 * 1024 * 1024 * 1024)?;
        drop(f);
        let with = Launch {
            extra_disks: vec![second.clone()],
            ..self.launch.clone()
        };
        let fail = |s: &Session, reason: String| HarnessError::Failed {
            step: name,
            reason,
            log: s.log.clone(),
        };
        // Pair and synchronize.
        let mut s = Session::spawn(&with)?;
        let synced = self.pair_and_sync(&mut s).map_err(|e| fail(&s, e))?;
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        drop(s);
        // Without the second disk.
        let mut s = Session::spawn(&self.launch)?;
        self.run_degraded(&mut s).map_err(|e| fail(&s, e))?;
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        drop(s);
        // Both again: the returned half is stale until resumed.
        let mut s = Session::spawn(&with)?;
        let resumed = self.resume_stale(&mut s).map_err(|e| fail(&s, e))?;
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        Ok(StepReport {
            name,
            elapsed: start.elapsed(),
            detail: format!(
                "synchronized {synced}; ran on one half with A6001255; resumed {resumed}; halves agree"
            ),
        })
    }

    fn pair_and_sync(&self, s: &mut Session) -> Result<String, String> {
        s.expect(crate::PROMPT, self.boot_timeout)?;
        let out = s.command("svc.mirror start 0 /dev/vdb", CMD)?;
        if !out.contains("paired with new unit 1") {
            return Err(format!("svc.mirror start: {out}"));
        }
        let out = s.command("svc.mirror resume 1", CMD)?;
        if !out.contains("resuming in the background") {
            return Err(format!("svc.mirror resume: {out}"));
        }
        let sync = wait_resume(s)?;
        let asp = s.command("svc.asp", CMD)?;
        if !asp.contains("protection: mirrored") || asp.matches("Active").count() != 2 {
            return Err(format!(
                "svc.asp after the sync:
{asp}"
            ));
        }
        let chk = s.command("svc.chkstg", Duration::from_secs(300))?;
        if !chk.contains(": 0 bad") {
            return Err(format!(
                "svc.chkstg after the sync:
{chk}"
            ));
        }
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001090", CMD)?;
        s.expect_in_order(SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let chk = s.command("svc.chkstg", Duration::from_secs(300))?;
        if !chk.contains(": 0 bad") {
            return Err(format!(
                "svc.chkstg after the restart:
{chk}"
            ));
        }
        Ok(sync)
    }

    fn run_degraded(&self, s: &mut Session) -> Result<(), String> {
        s.expect("SRC A6001255", self.boot_timeout)?;
        s.expect(crate::PROMPT, self.boot_timeout)?;
        let asp = s.command("svc.asp", CMD)?;
        if !asp.contains("Missing") {
            return Err(format!(
                "svc.asp without the second disk:
{asp}"
            ));
        }
        let out = s.command("svc.obj crt *DTAARA QGPL/MIRRTEST written on one half", CMD)?;
        if !out.contains("created") {
            return Err(format!("create while degraded: {out}"));
        }
        Ok(())
    }

    fn resume_stale(&self, s: &mut Session) -> Result<String, String> {
        s.expect(crate::PROMPT, self.boot_timeout)?;
        let asp = s.command("svc.asp", CMD)?;
        if !asp.contains("Suspended") {
            return Err(format!(
                "svc.asp with the stale half:
{asp}"
            ));
        }
        s.command("svc.mirror resume 1", CMD)?;
        let sync = wait_resume(s)?;
        let chk = s.command("svc.chkstg", Duration::from_secs(300))?;
        if !chk.contains(": 0 bad") {
            return Err(format!(
                "svc.chkstg after the resume:
{chk}"
            ));
        }
        let rcl = s.command("svc.rclstg *CHECK", Duration::from_secs(300))?;
        if !rcl.contains("0 problem(s)") {
            return Err(format!(
                "svc.rclstg after the resume:
{rcl}"
            ));
        }
        let dsp = s.command("svc.obj dsp QGPL/MIRRTEST", CMD)?;
        if !dsp.contains("written on one half") {
            return Err(format!(
                "object written while degraded is gone:
{dsp}"
            ));
        }
        Ok(sync)
    }

    /// Step 12: a manual IPL, option 3, DST sign-on with the shipped
    /// QSECOFR password (expired: it must be changed), the main menu,
    /// every screen compared with its golden file, then option 1 IPLs.
    pub(crate) fn dst_signon(&self) -> Result<StepReport, HarnessError> {
        let name = "12 DST sign-on";
        let start = std::time::Instant::now();
        let mut s = Session::spawn(&self.launch)?;
        let fail = |s: &Session, reason: String| HarnessError::Failed {
            step: name,
            reason,
            log: s.log.clone(),
        };
        let detail = self.dst_dialog(&mut s).map_err(|e| fail(&s, e))?;
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(15));
        Ok(StepReport {
            name,
            elapsed: start.elapsed(),
            detail,
        })
    }

    fn dst_dialog(&self, s: &mut Session) -> Result<String, String> {
        s.expect(crate::PROMPT, self.boot_timeout)?;
        s.command("mode manual", CMD)?;
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001090", CMD)?;
        s.expect("IPL or Install the System", self.boot_timeout)?;
        s.expect("Selection", CMD)?;
        s.send("3")?;
        s.expect("Dedicated Service Tools (DST) Sign On", CMD)?;
        self.snap("dst-signon");
        let mut compared = Vec::new();
        compared.push(golden_matches(s, "signon")?);
        // Wrong password once, then the shipped one, which is expired.
        s.send("type QSECOFR")?;
        s.send("key tab")?;
        s.send("type WRONG")?;
        s.send("key enter")?;
        s.expect("User ID or password is not correct", CMD)?;
        s.send("key home")?;
        s.send("key fieldexit")?;
        s.send("type QSECOFR")?;
        s.send("key tab")?;
        s.send("type QSECOFR")?;
        s.send("key enter")?;
        s.expect("Change Service Tools User Password", CMD)?;
        compared.push(golden_matches(s, "change-password")?);
        s.send("key tab")?;
        s.send("type QSECOFR")?;
        s.send("key tab")?;
        s.send("type Quartz1")?;
        s.send("key tab")?;
        s.send("type Quartz1")?;
        s.send("key enter")?;
        s.expect("Use Dedicated Service Tools (DST)", CMD)?;
        self.snap("dst-main");
        compared.push(golden_matches(s, "main")?);
        // Option 1: perform an IPL.
        s.send("1")?;
        s.expect("slic: DST ended: PerformIpl", CMD)?;
        crate::dst::answer_ipl_options(s, self.boot_timeout)?;
        s.expect(SRC_SEQUENCE[SRC_SEQUENCE.len() - 1], self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        // The new password persists and the activity log recorded nothing.
        let lic = s.command("lic", CMD)?;
        if !lic.contains("tlv 0x0102") {
            return Err(format!("the LIC record has no service tools users:\n{lic}"));
        }
        Ok(format!(
            "{} screens match their golden files; QSECOFR password changed and saved",
            compared.len()
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
            if let Err(e) = s.send(&format!("svc.obj bench {PULL_BENCH}")) {
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
                .command("svc.chkstg", Duration::from_secs(300))
                .map_err(|e| fail(&s, e))?;
            if !chk.contains(": 0 bad") {
                return Err(fail(&s, format!("svc.chkstg after the pull:\n{chk}")));
            }
            let rcl = s
                .command("svc.rclstg *CHECK", Duration::from_secs(300))
                .map_err(|e| fail(&s, e))?;
            if !rcl.contains("0 problem(s)") {
                return Err(fail(&s, format!("svc.rclstg *CHECK after the pull:\n{rcl}")));
            }
            let listing = s.command("svc.lib ls", CMD).map_err(|e| fail(&s, e))?;
            let counts = bench_counts(&listing);
            let total: u64 = counts.iter().sum();
            if !whole_batches(&counts) || total > PULL_BENCH {
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

#[cfg(test)]
mod tests {
    use super::whole_batches;

    #[test]
    fn committed_batches_are_recognized() {
        assert!(whole_batches(&[1100; 10]));
        assert!(whole_batches(&[0; 10]));
        assert!(whole_batches(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 1400]));
        assert!(whole_batches(&[
            0, 0, 400, 1400, 1400, 1400, 1400, 1400, 1400, 1400
        ]));
        assert!(!whole_batches(&[
            1100, 1100, 1100, 1100, 1100, 1100, 1100, 1100, 1100, 1050
        ]));
        assert!(!whole_batches(&[
            0, 400, 300, 1400, 1400, 1400, 1400, 1400, 1400, 1400
        ]));
        assert!(!whole_batches(&[1400, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
    }
}
