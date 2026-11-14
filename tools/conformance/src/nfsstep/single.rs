// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 173 to 178 on the main guest. Each step sets up what it needs
//! when an earlier one did not (`--only` runs any of them alone): the
//! export of `/nfstest`, the daemons, and forwards of 111 and 2049; the
//! other daemons' ports are learnt from the binder (`nfstest probe`) and
//! forwarded as they are found, since nothing pins them (`docs/NFS.md`
//! section 11.1).

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use super::host::{self, Outcome};
use super::{
    AFTER_GRACE, CMD, EXPORT, NET, clean, daemons_gone, daemons_in_qsyswrk, end_servers, export,
    full_log, public_dir, qsh, sql, start_servers, stat, want,
};
use crate::qemu::session::Session;
use crate::steps::Suite;

/// `QNFSANON`'s UID (`docs/NFS.md` section 9), the export's `ANON=`.
pub(crate) const ANON_UID: u64 = 4_294_770_710;
/// The scenarios' directory under the export.
const SCRATCH: &str = "nfstest";

/// Step 176's second export, whose host's writes are `*ASYNC`.
const ASYNC_EXPORT: &str = "/nfsasync";
/// Scenarios that fail against the server on main, each a finding in
/// `tools/nfstest/README.md` (its `tests/loopback.rs` keeps the same
/// list): reported, not failing the step.
const KNOWN: &[&str] = &[];
/// Scenarios left out: the server would have to call the host back,
/// which one-way forwards cannot carry (README, `--callbacks`).
const CALLBACKS: &[&str] = &["nlm-blocked-granted"];
/// The scenario run inside the NLM grace period (`--expect-grace`).
const GRACE_SCENARIO: &str = "nlm-grace";
/// How long a run of the scenarios may take.
const RUN_LIMIT: Duration = Duration::from_mins(20);

/// The scenario names `nfstest --list` prints.
fn names() -> Result<Vec<String>, String> {
    let r = host::run(&["--list".to_string()], Duration::from_secs(60))?;
    let v: Vec<String> = r
        .out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if v.is_empty() {
        return Err(format!("{}: no scenarios\n{}{}", r.line, r.out, r.err));
    }
    Ok(v)
}

/// The version 3 run's names in three groups: those any time, the lock
/// manager's after its grace period, and `nlm-grace` within it.
fn v3_groups(all: &[String]) -> (Vec<String>, Vec<String>) {
    let v3 = all.iter().filter(|n| !n.starts_with("v4"));
    let mut any = Vec::new();
    let mut nlm = Vec::new();
    for n in v3 {
        if CALLBACKS.contains(&n.as_str()) || n == GRACE_SCENARIO {
            continue;
        }
        if n.starts_with("nlm-") {
            nlm.push(n.clone());
        } else {
            any.push(n.clone());
        }
    }
    (any, nlm)
}

/// Failures not in [`KNOWN`].
fn new_failures(o: &Outcome) -> Vec<&str> {
    o.failed
        .iter()
        .filter(|l| !KNOWN.contains(&Outcome::name(l)))
        .map(String::as_str)
        .collect()
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// The export, the daemons and the two fixed forwards, unless an
    /// earlier step made them.
    fn nfs_ensure(&self, s: &mut Session) -> Result<(), String> {
        if self.nfs.borrow().started.is_some() {
            return Ok(());
        }
        self.nfs_setup(s).map(|_| ())
    }

    /// Step 173's setup: `/nfstest` exported, `STRNFSSVR SERVER(*ALL)`,
    /// 111 and 2049 forwarded. The `CPIA1BB` and `CPDA1BA` counts.
    fn nfs_setup(&self, s: &mut Session) -> Result<(usize, usize), String> {
        // Built before the start, so step 174 runs within the grace period.
        host::binary()?;
        full_log(s)?;
        public_dir(s, EXPORT)?;
        export(s, EXPORT, &format!("RW,ANON={ANON_UID}"), "")?;
        let counts = start_servers(s, "*ALL")?;
        let mut st = self.nfs.borrow_mut();
        st.started = Some(Instant::now());
        for g in [111, 2049] {
            if host::host_port(&st.map, g).is_none() {
                let h = host::forward_both(&self.launch.qmp, g)?;
                st.map.push((g, h));
            }
        }
        Ok(counts)
    }

    /// The binder's table through the forwarded 111, every port in it
    /// forwarded; the probe's text (with the export list).
    fn nfs_probe(&self) -> Result<String, String> {
        for _ in 0..3 {
            let map = self.nfs.borrow().map.clone();
            let pmap = host::host_port(&map, 111).ok_or("111 is not forwarded")?;
            let mut args = vec![
                "probe".to_string(),
                "127.0.0.1".into(),
                "--pmap-port".into(),
                pmap.to_string(),
            ];
            args.extend(host::map_args(&map));
            args.push(EXPORT.into());
            let r = host::run(&args, Duration::from_secs(120))?;
            let regs = host::registrations(&r.out);
            if regs.is_empty() {
                return Err(format!("{}:\n{}{}", r.line, r.out, r.err));
            }
            let new = host::unmapped(&regs, &map);
            if new.is_empty() {
                return Ok(r.out);
            }
            for g in new {
                let h = host::forward_both(&self.launch.qmp, g)?;
                self.nfs.borrow_mut().map.push((g, h));
            }
        }
        Err("the binder's ports kept changing".into())
    }

    /// The arguments that point `nfstest` at the export through the forwards.
    fn nfs_target(&self, scratch: &str) -> Vec<String> {
        self.nfs_target_at(EXPORT, scratch)
    }

    /// [`Self::nfs_target`] for the export `export`.
    fn nfs_target_at(&self, export: &str, scratch: &str) -> Vec<String> {
        let map = self.nfs.borrow().map.clone();
        let pmap = host::host_port(&map, 111).unwrap_or(111);
        let mut a = vec![
            "127.0.0.1".to_string(),
            export.into(),
            scratch.into(),
            "--pmap-port".into(),
            pmap.to_string(),
        ];
        a.extend(host::map_args(&map));
        a
    }

    /// Wait until the grace periods of the last start are over.
    fn nfs_after_grace(&self) {
        if let Some(t) = self.nfs.borrow().started {
            let left = AFTER_GRACE.saturating_sub(t.elapsed());
            std::thread::sleep(left);
        }
    }

    /// A step's result, with the guest's state written out when it failed.
    fn nfs_checked(
        &self,
        s: &mut Session,
        step: &str,
        r: Result<String, String>,
    ) -> Result<String, String> {
        if r.is_ok() && std::env::var_os("QS400_NFS_KEEP_SERIAL").is_some() {
            // Kept on success too: the daemons' and the service's notes.
            let _ = super::diag::dump(&self.out_dir, step, "main", s, "");
        }
        r.map_err(|e| {
            let probe = self.nfs_probe().unwrap_or_else(|p| p);
            let path = super::diag::dump(&self.out_dir, step, "main", s, &probe);
            format!("{e}\n(guest state: {path})")
        })
    }

    /// Step 173: the daemons start, run in `QSYSWRK` and register with
    /// the binder (`/etc/RPCBTAB`); the export is in the table.
    pub(crate) fn nfs_start(&self, s: &mut Session) -> Result<String, String> {
        let r = self.nfs_start_body(s);
        self.nfs_checked(s, "173", r)
    }

    fn nfs_start_body(&self, s: &mut Session) -> Result<String, String> {
        let (started, running) = self.nfs_setup(s)?;
        if started + running < super::DAEMONS.len() {
            return Err(format!(
                "STRNFSSVR logged {started} CPIA1BB and {running} running already, not {}",
                super::DAEMONS.len()
            ));
        }
        let rows = daemons_in_qsyswrk(s)?;
        // The binder's writer thread follows the registrations.
        let end = Instant::now() + Duration::from_secs(30);
        let records = loop {
            let st = stat(s, "/etc/RPCBTAB")?;
            if let Some(st) = &st
                && st.size.is_multiple_of(357)
                && st.size / 357 >= 10
            {
                break st.size / 357;
            }
            if Instant::now() > end {
                return Err(format!("/etc/RPCBTAB: {st:?}"));
            }
            std::thread::sleep(Duration::from_secs(1));
        };
        let table = sql(s, "SELECT * FROM QSYS2/NFS_EXPORT_INFO")?;
        if !table.contains(EXPORT) || !table.contains("row(s)") {
            return Err(format!("NFS_EXPORT_INFO has no {EXPORT}:\n{table}"));
        }
        let mfs = clean(s, "DSPMFSINF OBJ('/')", CMD)?;
        if !mfs.contains("File system type") {
            return Err(format!("DSPMFSINF OBJ('/'):\n{mfs}"));
        }
        // The binder and the server in the kernel's socket table.
        let net = sql(
            s,
            "SELECT LOCAL_PORT, PROTOCOL, TCP_STATE FROM QSYS2/NETSTAT_INFO WHERE LOCAL_PORT IN (111, 2049)",
        )?;
        let listening = |port: &str| {
            net.lines().any(|l| {
                let w: Vec<&str> = l.split_whitespace().collect();
                w.first() == Some(&port) && w.contains(&"TCP") && w.contains(&"LISTEN")
            })
        };
        if !listening("111") || !listening("2049") {
            return Err(format!("NETSTAT_INFO lacks 111 or 2049 listening:\n{net}"));
        }
        let probe = self.nfs_probe()?;
        for want in ["portmapper", "nfs", "mountd", "nlockmgr", "status"] {
            if !probe.contains(want) {
                return Err(format!("the binder lacks {want}:\n{probe}"));
            }
        }
        if !host::exports(&probe).iter().any(|e| e == EXPORT) {
            return Err(format!("EXPORT does not list {EXPORT}:\n{probe}"));
        }
        let jobs: Vec<&str> = rows
            .iter()
            .filter_map(|r| r.split_whitespace().next())
            .collect();
        let map = self.nfs.borrow().map.clone();
        Ok(format!(
            "{started} daemons started ({running} running already): {} in QSYSWRK; /etc/RPCBTAB {records} records; {EXPORT} in NFS_EXPORT_INFO and the EXPORT list; 111 and 2049 listening in NETSTAT_INFO; {} registrations, forwards {map:?}",
            jobs.join(" "),
            host::registrations(&probe).len()
        ))
    }

    /// Step 174: every version 3, MOUNT, NLM and NSM scenario from the
    /// host; `nlm-grace` while the lock manager's grace period runs, the
    /// other lock scenarios after it.
    pub(crate) fn nfs_test_v3(&self, s: &mut Session) -> Result<String, String> {
        let r = self.nfs_test_v3_body(s);
        self.nfs_checked(s, "174", r)
    }

    fn nfs_test_v3_body(&self, s: &mut Session) -> Result<String, String> {
        self.nfs_ensure(s)?;
        self.nfs_probe()?;
        let all = names()?;
        let (any, nlm) = v3_groups(&all);
        let mut log = String::new();
        let mut detail = Vec::new();
        let since = self.nfs.borrow().started.map(|t| t.elapsed());
        // Within the grace period, with a margin: else not run, and said.
        let grace = match since {
            Some(t) if t < Duration::from_secs(60) => {
                let mut a = self.nfs_target(SCRATCH);
                a.extend([
                    "--only".into(),
                    GRACE_SCENARIO.into(),
                    "--expect-grace".into(),
                ]);
                let r = host::run(&a, RUN_LIMIT)?;
                let _ = write!(log, "$ {}\n{}{}\n", r.line, r.out, r.err);
                Some(Outcome::of(&r.out))
            }
            _ => None,
        };
        let mut a = self.nfs_target(SCRATCH);
        a.extend(["--only".into(), any.join(",")]);
        let r = host::run(&a, RUN_LIMIT)?;
        let _ = write!(log, "$ {}\n{}{}\n", r.line, r.out, r.err);
        let mut outcome = Outcome::of(&r.out);
        self.nfs_after_grace();
        // The forwards may need the lock manager's ports again.
        self.nfs_probe()?;
        let mut a = self.nfs_target(SCRATCH);
        a.extend(["--only".into(), nlm.join(",")]);
        let r = host::run(&a, RUN_LIMIT)?;
        let _ = write!(log, "$ {}\n{}{}\n", r.line, r.out, r.err);
        let locks = Outcome::of(&r.out);
        outcome.ok.extend(locks.ok);
        outcome.failed.extend(locks.failed);
        outcome.skipped.extend(locks.skipped);
        let file = self.out_dir.join("nfs-174-nfstest.txt");
        let _ = std::fs::write(&file, &log);
        let grace_said = match &grace {
            Some(g) if g.ok.iter().any(|n| n == GRACE_SCENARIO) => "ok".to_string(),
            Some(g) => {
                outcome.failed.extend(g.failed.iter().cloned());
                format!("not ok ({:?} {:?})", g.failed, g.skipped)
            }
            None => format!("not run ({since:?} after the start, past the 60 s margin)"),
        };
        let new = new_failures(&outcome);
        if !new.is_empty() {
            return Err(format!(
                "nfstest version 3: {}\n(every run: {})",
                new.join("\n"),
                file.display()
            ));
        }
        let expected = any.len() + nlm.len();
        let seen = outcome.ok.len() + outcome.failed.len() + outcome.skipped.len();
        if seen < expected {
            return Err(format!(
                "nfstest version 3 reported {seen} of {expected} scenarios (every run: {})",
                file.display()
            ));
        }
        detail.push(format!("{} ok", outcome.ok.len()));
        let known: Vec<&str> = outcome.failed.iter().map(|l| Outcome::name(l)).collect();
        if !known.is_empty() {
            detail.push(format!("known failing {}", known.join(", ")));
        }
        let now_passing: Vec<&&str> = KNOWN
            .iter()
            .filter(|k| outcome.ok.iter().any(|o| o == *k))
            .collect();
        if !now_passing.is_empty() {
            detail.push(format!("now passing (take out of KNOWN): {now_passing:?}"));
        }
        if !outcome.skipped.is_empty() {
            detail.push(format!(
                "skipped {}",
                outcome
                    .skipped
                    .iter()
                    .map(|l| Outcome::name(l))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        detail.push(format!("nlm-grace {grace_said}"));
        detail.push(format!(
            "left out (needs callbacks): {}",
            CALLBACKS.join(", ")
        ));
        Ok(detail.join("; "))
    }

    /// Step 175: the 4.0 and 4.1 scenarios through the forwarded 2049.
    pub(crate) fn nfs_test_v4(&self, s: &mut Session) -> Result<String, String> {
        let r = self.nfs_test_v4_body(s);
        self.nfs_checked(s, "175", r)
    }

    fn nfs_test_v4_body(&self, s: &mut Session) -> Result<String, String> {
        self.nfs_ensure(s)?;
        // A grace period answers OPEN with GRACE: wait it out.
        self.nfs_after_grace();
        let port = host::host_port(&self.nfs.borrow().map, 2049).ok_or("2049 is not forwarded")?;
        let mut log = String::new();
        let mut detail = Vec::new();
        let mut bad = Vec::new();
        for vers in ["4.0", "4.1"] {
            let args: Vec<String> = [
                "127.0.0.1",
                EXPORT,
                SCRATCH,
                "--vers",
                vers,
                "--port",
                &port.to_string(),
            ]
            .iter()
            .map(|a| (*a).to_string())
            .collect();
            let r = host::run(&args, RUN_LIMIT)?;
            let _ = write!(log, "$ {}\n{}{}\n", r.line, r.out, r.err);
            let o = Outcome::of(&r.out);
            if o.ok.is_empty() || r.code.is_none() {
                bad.push(format!("{vers}: no scenario passed (exit {:?})", r.code));
            }
            bad.extend(o.failed.iter().map(|l| format!("{vers}: {l}")));
            detail.push(format!(
                "{vers} {} ok{}",
                o.ok.len(),
                if o.skipped.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (skipped {})",
                        o.skipped
                            .iter()
                            .map(|l| Outcome::name(l))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            ));
        }
        let file = self.out_dir.join("nfs-175-nfstest.txt");
        let _ = std::fs::write(&file, &log);
        if !bad.is_empty() {
            return Err(format!(
                "nfstest version 4: {}\n(every run: {})",
                bad.join("\n"),
                file.display()
            ));
        }
        Ok(format!("{} through host port {port}", detail.join("; ")))
    }

    /// Step 176: `nfstest bench` writes and reads `QS400_NFS_BENCH_MIB`
    /// (1024) MiB from the host, version 3 then 4.1.
    pub(crate) fn nfs_bench(&self, s: &mut Session) -> Result<String, String> {
        let r = self.nfs_bench_body(s);
        self.nfs_checked(s, "176", r)
    }

    fn nfs_bench_body(&self, s: &mut Session) -> Result<String, String> {
        self.nfs_ensure(s)?;
        let mib: u64 = std::env::var("QS400_NFS_BENCH_MIB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1024);
        super::pairsteps::room(s, mib + 256)?;
        self.nfs_probe()?;
        // Version 3 in the server's preferred size (`FSINFO`) and in
        // 64 KiB; 4.1, which has no preference, in 1 MiB and 64 KiB.
        // A call waits up to two minutes: the host's write-back can stop
        // the guest's disk for tens of seconds.
        let wait = ["--timeout".to_string(), "120".to_string()];
        let mut runs = Vec::new();
        for bs in [None, Some(65_536)] {
            let mut a = vec!["bench".to_string()];
            a.extend(self.nfs_target("nfsbench"));
            a.extend(["--mib".into(), mib.to_string()]);
            a.extend(wait.clone());
            if let Some(bs) = bs {
                a.extend(["--bs".into(), bs.to_string()]);
            }
            runs.push(("3", host::run(&a, Duration::from_mins(60))?));
        }
        // Version 4.1 needs only the forwarded 2049 (after the grace period).
        self.nfs_after_grace();
        let port = host::host_port(&self.nfs.borrow().map, 2049).ok_or("2049 is not forwarded")?;
        for bs in [1_048_576, 65_536] {
            let a4: Vec<String> = [
                "bench",
                "127.0.0.1",
                EXPORT,
                "nfsbench",
                "--vers",
                "4.1",
                "--port",
                &port.to_string(),
                "--mib",
                &mib.to_string(),
                "--bs",
                &bs.to_string(),
                "--timeout",
                "120",
            ]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
            runs.push(("4.1", host::run(&a4, Duration::from_mins(60))?));
        }
        // Last, the same through an `*ASYNC` host, whose unstable writes
        // are not synced each: what the server costs without the sync.
        // A figure only, not a check: unsynced, the guest's dirty data
        // can stall its disk past any time-out.
        public_dir(s, ASYNC_EXPORT)?;
        export(
            s,
            ASYNC_EXPORT,
            &format!("RW,ANON={ANON_UID}"),
            "('10.0.2.2' *BINARY *ASCII *ASYNC)",
        )?;
        // The daemons read the table once a second.
        std::thread::sleep(Duration::from_secs(3));
        let mut a = vec!["bench".to_string()];
        a.extend(self.nfs_target_at(ASYNC_EXPORT, "nfsbench"));
        a.extend(["--mib".into(), mib.to_string()]);
        a.extend(wait);
        let async_run = host::run(&a, Duration::from_mins(60))?;
        let mut text = String::new();
        for r in runs.iter().map(|(_, r)| r).chain([&async_run]) {
            let _ = writeln!(text, "$ {}\n{}{}", r.line, r.out, r.err);
        }
        let _ = std::fs::write(self.out_dir.join("nfs-176-bench.txt"), &text);
        let rates = |r: &host::Run| {
            Some((
                host::value(&r.out, "write MiB/s: ")?.to_string(),
                host::value(&r.out, "read MiB/s: ")?.to_string(),
            ))
        };
        let async_line = match rates(&async_run) {
            Some((w, rd)) => format!(
                "host -> guest, version 3, *ASYNC host, {mib} MiB: write {w} MiB/s, read {rd} MiB/s (block {})",
                host::value(&async_run.out, "block: ").unwrap_or("?")
            ),
            None => format!(
                "host -> guest, version 3, *ASYNC host: not measured ({})",
                async_run.out.trim()
            ),
        };
        let mut lines = Vec::new();
        for (vers, r) in &runs {
            let Some((w, rd)) = rates(r) else {
                return Err(format!("no rates for version {vers}:\n{text}"));
            };
            let line = format!(
                "host -> guest, version {vers}, {mib} MiB: write {w} MiB/s, read {rd} MiB/s (block {})",
                host::value(&r.out, "block: ").unwrap_or("?")
            );
            super::pairsteps::perf_line(&self.out_dir, &line);
            lines.push(line);
        }
        super::pairsteps::perf_line(&self.out_dir, &async_line);
        lines.push(async_line);
        Ok(lines.join("; "))
    }

    /// Step 177: the guest mounts a Linux export, versions 3 and 4, and
    /// writes and reads a file in it; skipped unless `QS400_NFS_LINUX`
    /// names one (`host:/path`, as the guest reaches it: `10.0.2.2` is
    /// the host). The export must take clients from unreserved ports
    /// (`insecure`: QEMU's user network sends from the host's own) and
    /// let the squashed root write (`QS400_NFS_LINUX_V4` names the
    /// version 4 path when it differs, a `fsid=0` root).
    pub(crate) fn nfs_linux(&self, s: &mut Session) -> Result<String, String> {
        let Ok(mfs) = std::env::var("QS400_NFS_LINUX") else {
            return Ok(
                "skipped: QS400_NFS_LINUX (host:/path, as the guest reaches it) is not set".into(),
            );
        };
        let r = self.nfs_linux_body(s, &mfs);
        self.nfs_checked(s, "177", r)
    }

    fn nfs_linux_body(&self, s: &mut Session, mfs: &str) -> Result<String, String> {
        self.nfs_ensure(s)?;
        let v4 = std::env::var("QS400_NFS_LINUX_V4").unwrap_or_else(|_| mfs.to_string());
        s.command("svc.ifs mkdir /mnt", CMD)?;
        let mut done = Vec::new();
        for (vers, source, dir) in [("3", mfs, "/mnt/linux3"), ("4", v4.as_str(), "/mnt/linux4")] {
            s.command(&format!("svc.ifs mkdir {dir}"), CMD)?;
            super::pairsteps::mount(s, source, dir, &format!("rw,vers={vers},retry=2"))?;
            let file = format!("{dir}/qs400-v{vers}.txt");
            let text = format!("written-by-qs400-v{vers}");
            let r = qsh(
                s,
                &format!("echo {text} > {file} && cat {file} && ls -l {dir} && rm {file}"),
                NET,
            );
            let un = super::pairsteps::unmount(s, dir);
            let out = r?;
            un?;
            if !out.contains(&text) {
                return Err(format!(
                    "{source} version {vers}: the file read back:\n{out}"
                ));
            }
            done.push(format!("version {vers} ({source})"));
        }
        Ok(format!(
            "wrote, read and removed a file over {}",
            done.join(" and ")
        ))
    }

    /// Step 178: a loopback mount, `UNMOUNT TYPE(*ALL) MNTOVRDIR(*ALL)`,
    /// `ENDNFSSVR SERVER(*ALL)`: no NFS mount, no daemon job, nothing
    /// listening on 111 or 2049, the binder silent from the host.
    pub(crate) fn nfs_end(&self, s: &mut Session) -> Result<String, String> {
        let r = self.nfs_end_body(s);
        self.nfs_checked(s, "178", r)
    }

    fn nfs_end_body(&self, s: &mut Session) -> Result<String, String> {
        self.nfs_ensure(s)?;
        let before = stat(s, "/etc/RPCBTAB")?.map_or(0, |st| st.size / 357);
        s.command("svc.ifs mkdir /mnt", CMD)?;
        s.command("svc.ifs mkdir /mnt/self", CMD)?;
        super::pairsteps::mount(
            s,
            &format!("127.0.0.1:{EXPORT}"),
            "/mnt/self",
            "rw,vers=3,retry=2",
        )?;
        let mounted = sql(s, "SELECT * FROM QSYS2/MOUNTED_FS_INFO")?;
        if !mounted.contains("nfs3") {
            return Err(format!("MOUNTED_FS_INFO has no nfs3 row:\n{mounted}"));
        }
        want(
            s,
            "UNMOUNT TYPE(*ALL) MNTOVRDIR(*ALL)",
            &["CPCA1B1"],
            Duration::from_secs(120),
        )?;
        let after = sql(s, "SELECT * FROM QSYS2/MOUNTED_FS_INFO")?;
        if after.contains("nfs3") || after.contains("nfs4") {
            return Err(format!("an NFS mount is left:\n{after}"));
        }
        end_servers(s, "*ALL")?;
        daemons_gone(s)?;
        let net = sql(
            s,
            "SELECT * FROM QSYS2/NETSTAT_INFO WHERE LOCAL_PORT IN (111, 2049)",
        )?;
        if net.contains("LISTEN") {
            return Err(format!("still listening:\n{net}"));
        }
        let left = stat(s, "/etc/RPCBTAB")?.map_or(0, |st| st.size / 357);
        let map = self.nfs.borrow().map.clone();
        let pmap = host::host_port(&map, 111).unwrap_or(111);
        let r = host::run(
            &[
                "probe".into(),
                "127.0.0.1".into(),
                "--pmap-port".into(),
                pmap.to_string(),
            ],
            Duration::from_secs(120),
        )?;
        if !host::registrations(&r.out).is_empty() {
            return Err(format!("the binder still answers:\n{}", r.out));
        }
        self.nfs.borrow_mut().started = None;
        Ok(format!(
            "the loopback mount unmounted by UNMOUNT *ALL; daemons ended (CPCA1BC), none active; 111 and 2049 not listening; /etc/RPCBTAB {before} then {left} records; the binder silent from the host"
        ))
    }

    /// Step 185: a version 3 handle the host saved names its file again
    /// after `ENDNFSSVR`/`STRNFSSVR` and after an IPL, used on 2049 with
    /// no `MOUNT` and no `LOOKUP` (file IDs resolve by the xfs handle).
    pub(crate) fn nfs_handles(&self, s: &mut Session) -> Result<String, String> {
        let r = self.nfs_handles_body(s);
        self.nfs_checked(s, "185", r)
    }

    fn nfs_handles_body(&self, s: &mut Session) -> Result<String, String> {
        self.nfs_ensure(s)?;
        self.nfs_probe()?;
        let file = self.out_dir.join("nfs-185-handle.txt");
        let path = file.to_string_lossy().to_string();
        let handle = |cmd: &str, args: Vec<String>| -> Result<String, String> {
            let mut a = vec!["handle".to_string(), cmd.to_string()];
            a.extend(args);
            a.extend(["--file".into(), path.clone()]);
            let r = host::run(&a, Duration::from_secs(120))?;
            let line = format!("ok handle-{cmd}: ");
            match r.out.lines().find_map(|l| l.strip_prefix(&line)) {
                Some(said) if r.code == Some(0) => Ok(said.to_string()),
                _ => Err(format!("{}:\n{}{}", r.line, r.out, r.err)),
            }
        };
        let saved = handle("save", self.nfs_target(SCRATCH))?;
        // Only NFS's port from here on: the handle alone.
        let port = host::host_port(&self.nfs.borrow().map, 2049).ok_or("2049 is not forwarded")?;
        let direct = || {
            vec![
                "127.0.0.1".to_string(),
                EXPORT.into(),
                SCRATCH.into(),
                "--port".into(),
                port.to_string(),
            ]
        };
        end_servers(s, "*ALL")?;
        daemons_gone(s)?;
        start_servers(s, "*ALL")?;
        self.nfs.borrow_mut().started = Some(Instant::now());
        let restarted = handle("use", direct())?;
        // An IPL: the forwards stay (the same QEMU), the daemons do not.
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001090", Duration::from_secs(120))?;
        s.expect_in_order(crate::steps::SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        crate::tcp::guest_address(s)?;
        // `EXPORTFS -I` lasts until the IPL: exported again, as before it.
        self.nfs.borrow_mut().started = None;
        self.nfs_setup(s)?;
        let opr = super::opr_count(s, "CPFA1B5")? + super::opr_count(s, "CPFA1CB")?;
        if opr != 0 {
            return Err(format!(
                "the power down left {opr} CPFA1B5/CPFA1CB on QSYSOPR: the daemons' ends were asked for"
            ));
        }
        let after_ipl = handle("use", direct())?;
        end_servers(s, "*ALL")?;
        daemons_gone(s)?;
        self.nfs.borrow_mut().started = None;
        Ok(format!(
            "saved ({saved}); after ENDNFSSVR/STRNFSSVR: {restarted}; after an IPL: {after_ipl}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_the_version_3_scenarios() {
        let all: Vec<String> = [
            "null-pmap",
            "nlm-test-lock-unlock",
            "nlm-blocked-granted",
            "nlm-grace",
            "nsm-stat",
            "v4-null",
            "v40-locks",
            "v41-exchange-session",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let (any, nlm) = v3_groups(&all);
        assert_eq!(any, ["null-pmap", "nsm-stat"]);
        assert_eq!(nlm, ["nlm-test-lock-unlock"]);
    }

    #[test]
    fn every_failure_counts_with_none_known() {
        let o = Outcome::of("fail nlm-share: denied\nfail mount-cycle: x\nok null-nfs\n");
        assert_eq!(
            new_failures(&o),
            ["fail nlm-share: denied", "fail mount-cycle: x"]
        );
    }
}
