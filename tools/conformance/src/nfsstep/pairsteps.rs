// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 179 to 184, run by `pair` on its two guests. B mounts A's
//! exports by A's name (`guesta.nfs.test`): the name B's status monitor
//! watches is then the one A's names itself by in its restart notice.

use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use super::pair::{A_NAME, B_NAME, LIB, MEMBER_TEXT, Pair, ROOTED};
use super::prog::{self, EAGAIN, ETIMEDOUT, Said};
use super::{
    CMD, EXPORT, NET, clean, end_servers, opr_count, qsh, sql, start_servers, stat, wait_opr,
    wait_path, want,
};
use crate::qemu::session::Session;
use crate::steps::Suite;

/// The locked range: bytes 0 to 99.
const RANGE: (u32, u32) = (0, 100);
/// `MOUNT` may try the server a few times.
const MOUNT_TIME: Duration = Duration::from_secs(180);

/// `MOUNT TYPE(*NFS)`: `CPCA1B0`.
pub(crate) fn mount(
    s: &mut Session,
    mfs: &str,
    over: &str,
    options: &str,
) -> Result<String, String> {
    s.command(&format!("svc.ifs mkdir {over}"), CMD)?;
    want(
        s,
        &format!("MOUNT TYPE(*NFS) MFS('{mfs}') MNTOVRDIR('{over}') OPTIONS('{options}')"),
        &["CPCA1B0"],
        MOUNT_TIME,
    )
}

/// `UNMOUNT TYPE(*NFS)`: `CPCA1B1`.
pub(crate) fn unmount(s: &mut Session, over: &str) -> Result<String, String> {
    want(
        s,
        &format!("UNMOUNT TYPE(*NFS) MNTOVRDIR('{over}')"),
        &["CPCA1B1"],
        Duration::from_secs(120),
    )
}

/// The free space of the IFS volume, grown first so `mib` more fit.
pub(crate) fn room(s: &mut Session, mib: u64) -> Result<(), String> {
    let out = s.command("svc.ifs", CMD)?;
    let (_, free) = crate::ifs::capacity(&out)?;
    if free >= mib {
        return Ok(());
    }
    let grow = (mib - free).div_ceil(256) * 256;
    let r = s.command(&format!("QSGROWIFS SIZE({grow})"), Duration::from_mins(5))?;
    if !r.contains("CPC3E20") {
        return Err(format!("QSGROWIFS SIZE({grow}):\n{r}"));
    }
    Ok(())
}

/// A line of `nfs-perf.txt`, the figures for `docs/NFS.md` section 11.
pub(crate) fn perf_line(dir: &Path, line: &str) {
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("nfs-perf.txt"))
    {
        let _ = writeln!(f, "{line}");
    }
}

/// `svc.ifs rm` of each path, quietly.
fn rm(s: &mut Session, paths: &[String]) -> Result<(), String> {
    for p in paths {
        s.command(&format!("svc.ifs rm {p}"), CMD)?;
    }
    Ok(())
}

/// `SBMJOB` of `cmd` to `QBATCH`; the job's `number/user/name`.
fn submit(s: &mut Session, cmd: &str, job: &str) -> Result<String, String> {
    let out = s.command(
        &format!("SBMJOB CMD({cmd}) JOB({job}) JOBQ(QGPL/QBATCH)"),
        CMD,
    )?;
    crate::qsh::submitted(&out)
}

/// Whether `job` is in `WRKACTJOB`.
fn active(s: &mut Session, job: &str) -> Result<bool, String> {
    let list = s.command("wrkactjob", CMD)?;
    Ok(list
        .lines()
        .any(|l| l.split_whitespace().next() == Some(job)))
}

/// Wait until `job` is in `WRKACTJOB` (it leaves the job queue).
fn running(s: &mut Session, job: &str, limit: Duration) -> Result<(), String> {
    let end = Instant::now() + limit;
    while !active(s, job)? {
        if Instant::now() > end {
            return Err(format!("{job} did not become active within {limit:?}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Ok(())
}

/// Wait until `job` has left `WRKACTJOB`; how long that took.
fn gone(s: &mut Session, job: &str, limit: Duration) -> Result<Duration, String> {
    let t = Instant::now();
    loop {
        if !active(s, job)? {
            return Ok(t.elapsed());
        }
        if t.elapsed() > limit {
            return Err(format!("{job} still active after {limit:?}"));
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// A batch job holding the lock on `path` (`prog::hold`).
struct Holder {
    /// The program and job name.
    name: String,
    /// Its `number/user/name`.
    job: String,
    /// The marker files' stem.
    marker: String,
}

/// Submit a holder of `path` on `s` and wait until it holds the lock.
fn hold(s: &mut Session, name: &str, path: &str) -> Result<Holder, String> {
    let marker = format!("/tmp/{name}");
    let release = format!("{marker}.rel");
    rm(
        s,
        &["ok", "bad", "wrote", "rel"].map(|x| format!("{marker}.{x}")),
    )?;
    prog::build(
        s,
        name,
        &prog::hold(name, path, RANGE.0, RANGE.1, &release, &marker),
    )?;
    let job = submit(s, &format!("CALL PGM(QGPL/{name})"), name)?;
    let end = Instant::now() + NET;
    loop {
        if stat(s, &format!("{marker}.ok"))?.is_some() {
            break;
        }
        if stat(s, &format!("{marker}.bad"))?.is_some() || Instant::now() > end {
            let log = s
                .command(&format!("svc.dspjoblog JOB({job})"), CMD)
                .unwrap_or_default();
            return Err(format!("{name} did not take the lock on {path}:\n{log}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Ok(Holder {
        name: name.to_string(),
        job,
        marker,
    })
}

/// Let a holder go: it writes 8 bytes at 4096, closes and ends; whether
/// the write went through.
fn release(s: &mut Session, h: &Holder) -> Result<bool, String> {
    s.command(&format!("svc.ifs put {}.rel go", h.marker), CMD)?;
    gone(s, &h.name, Duration::from_secs(120)).map_err(|e| format!("{e} ({})", h.job))?;
    Ok(stat(s, &format!("{}.wrote", h.marker))?.is_some())
}

/// Build a lock try of `path` once, call it: what it said.
fn try_at(
    s: &mut Session,
    name: &str,
    path: &str,
    built: &mut Vec<String>,
) -> Result<Said, String> {
    if !built.iter().any(|b| b == name) {
        prog::build(s, name, &prog::try_lock(name, path, RANGE.0, RANGE.1))?;
        built.push(name.to_string());
    }
    Ok(prog::call(s, name, NET)?.0)
}

/// A try that must be refused with `EAGAIN`.
fn refused(
    s: &mut Session,
    name: &str,
    path: &str,
    built: &mut Vec<String>,
    who: &str,
) -> Result<(), String> {
    match try_at(s, name, path, built)? {
        Said::Failed(t, EAGAIN) if t == "NFSLK-REFUSED" => Ok(()),
        other => Err(format!(
            "{who}: a lock on {path} held elsewhere was not refused with EAGAIN: {other:?}"
        )),
    }
}

/// A try that must get the lock, within `limit` (an order is carried out
/// at the daemon's next pause).
fn locked(
    s: &mut Session,
    name: &str,
    path: &str,
    built: &mut Vec<String>,
    limit: Duration,
) -> Result<(), String> {
    let end = Instant::now() + limit;
    loop {
        let got = try_at(s, name, path, built)?;
        if got == Said::Ok("NFSLK-LOCKED".into()) {
            return Ok(());
        }
        if Instant::now() > end {
            return Err(format!("the lock on {path} was not free: {got:?}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// B's view of a file on a mount through QSH, and A's of the same file:
/// write, read back, rename, list; the owner A records; remove.
fn files(p: &mut Pair, over: &str, adir: &str, tag: &str, owner: &str) -> Result<String, String> {
    let d = format!("{over}/{tag}");
    let text = format!("from-guest-b-{tag}");
    let out = qsh(
        &mut p.b,
        &format!(
            "mkdir {d} && echo {text} > {d}/f1.txt && cat {d}/f1.txt && mv {d}/f1.txt {d}/f2.txt && ls -l {d}"
        ),
        NET,
    )?;
    if !out.contains(&text) || !out.contains("f2.txt") {
        return Err(format!("B's write, read and rename on {over}:\n{out}"));
    }
    let on_a = format!("{adir}/{tag}/f2.txt");
    let st = stat(&mut p.a, &on_a)?.ok_or_else(|| format!("A has no {on_a}"))?;
    if st.owner != owner || st.size < text.len() as u64 {
        return Err(format!("A's {on_a}: {st:?}, not owned by {owner}"));
    }
    qsh(
        &mut p.b,
        &format!("rm {d}/f2.txt && rmdir {d} && ls {over}"),
        NET,
    )?;
    if stat(&mut p.a, &format!("{adir}/{tag}"))?.is_some() {
        return Err(format!("{adir}/{tag} is still on A after B removed it"));
    }
    Ok(format!("{owner} owns it on A ({} bytes)", st.size))
}

/// What the mount table and `DSPMFSINF` say of `over` on B.
fn mount_info(b: &mut Session, over: &str, kind: &str) -> Result<String, String> {
    let rows = sql(b, "SELECT * FROM QSYS2/MOUNTED_FS_INFO")?;
    if !rows.contains(kind) {
        return Err(format!("MOUNTED_FS_INFO has no {kind} row:\n{rows}"));
    }
    let mfs = clean(b, &format!("DSPMFSINF OBJ('{over}')"), NET)?;
    if !mfs.contains("File system type") {
        return Err(format!("DSPMFSINF OBJ('{over}'):\n{mfs}"));
    }
    let shown = mfs
        .lines()
        .find(|l| l.contains("File system type"))
        .map_or("", str::trim);
    Ok(format!(
        "{kind} in MOUNTED_FS_INFO{}; DSPMFSINF: {shown}",
        if rows.contains(over) {
            " with its path"
        } else {
            " (path not shown)"
        }
    ))
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 179: version 3 from B: files through QSH, the mount in the
    /// table and `DSPMFSINF`; root squashed on `/nfstest`, root on
    /// `/nfsroot`.
    pub(crate) fn nfs_pair_v3(&self, p: &mut Pair) -> Result<String, String> {
        let over = "/mnt/a";
        mount(
            &mut p.b,
            &format!("{A_NAME}:{EXPORT}"),
            over,
            "rw,hard,vers=3,retry=2",
        )?;
        let squashed = files(p, over, EXPORT, "b3", "QNFSANON")?;
        let info = mount_info(&mut p.b, over, "nfs3")?;
        unmount(&mut p.b, over)?;
        mount(
            &mut p.b,
            &format!("{A_NAME}:{ROOTED}"),
            "/mnt/r",
            "rw,vers=3,retry=2",
        )?;
        let rooted = files(p, "/mnt/r", ROOTED, "r3", "QSECOFR")?;
        unmount(&mut p.b, "/mnt/r")?;
        Ok(format!(
            "{EXPORT}: written, read, renamed, listed, removed from B, {squashed}; {ROOTED}: {rooted}; {info}"
        ))
    }

    /// Step 180: version 4 the same way; then `/QSYS.LIB/NFSLIB.LIB`
    /// with data CCSID 819 for B: a source member read as ASCII, one
    /// written as ASCII and kept as EBCDIC records on A.
    pub(crate) fn nfs_pair_v4(&self, p: &mut Pair) -> Result<String, String> {
        p.after_grace();
        let over = "/mnt/a4";
        mount(
            &mut p.b,
            &format!("{A_NAME}:{EXPORT}"),
            over,
            "rw,vers=4,retry=2",
        )?;
        let squashed = files(p, over, EXPORT, "b4", "QNFSANON")?;
        let info = mount_info(&mut p.b, over, "nfs4")?;
        unmount(&mut p.b, over)?;
        let q = "/mnt/q";
        mount(
            &mut p.b,
            &format!("{A_NAME}:/QSYS.LIB/{LIB}.LIB"),
            q,
            "rw,vers=4,retry=2",
        )?;
        let hello = format!("{q}/QCLSRC.FILE/HELLO.MBR");
        let ascii = MEMBER_TEXT.split(' ').next().unwrap_or(MEMBER_TEXT);
        prog::build(
            &mut p.b,
            "NFSRQ",
            &prog::read("NFSRQ", &hello, Some(ascii.as_bytes()), "/tmp/NFSRQ"),
        )?;
        let (said, out) = prog::call(&mut p.b, "NFSRQ", NET)?;
        if said != Said::Ok("NFSRD-SAME".into()) {
            return Err(format!("B reading {hello}: not ASCII {ascii:?}:\n{out}"));
        }
        let line = "WRITTEN BY GUEST B";
        let data = format!("{line:<80}");
        let fromb = format!("{q}/QCLSRC.FILE/FROMB.MBR");
        prog::build(
            &mut p.b,
            "NFSWQ",
            &prog::write("NFSWQ", &fromb, data.as_bytes()),
        )?;
        let (said, out) = prog::call(&mut p.b, "NFSWQ", NET)?;
        if said != Said::Ok("NFSWR-OK".into()) {
            return Err(format!("B writing {fromb}:\n{out}"));
        }
        unmount(&mut p.b, q)?;
        let src =
            p.a.command(&format!("svc.srcget {LIB}/QCLSRC FROMB"), CMD)?;
        if !src.contains(line) {
            return Err(format!("A's member FROMB does not read {line:?}:\n{src}"));
        }
        Ok(format!(
            "{EXPORT}: {squashed}; {info}; /QSYS.LIB: HELLO read by B as ASCII, FROMB written in ASCII and read on A as {line:?}"
        ))
    }

    /// Step 181: a byte-range lock a job on one guest holds refuses the
    /// other guest's, both ways, version 3 (NLM) and 4; `RLSIFSLCK
    /// RMTLOCNAME` on A frees B's.
    pub(crate) fn nfs_pair_locks(&self, p: &mut Pair) -> Result<String, String> {
        p.after_grace();
        let mut built_a = Vec::new();
        let mut built_b = Vec::new();
        let mut detail = Vec::new();
        for (v, opts) in [("3", "rw,vers=3,retry=2"), ("4", "rw,vers=4,retry=2")] {
            let over = format!("/mnt/l{v}");
            let file = format!("lock{v}.dat");
            let on_a = format!("{ROOTED}/{file}");
            let on_b = format!("{over}/{file}");
            p.a.command(&format!("svc.ifs put {on_a} lock-file-version-{v}"), CMD)?;
            mount(&mut p.b, &format!("{A_NAME}:{ROOTED}"), &over, opts)?;
            let (ta, tb) = (format!("NFT{v}A"), format!("NFT{v}B"));
            // B holds; A is refused.
            let h = hold(&mut p.b, &format!("NFH{v}B"), &on_b)?;
            refused(&mut p.a, &ta, &on_a, &mut built_a, "A")?;
            release(&mut p.b, &h)?;
            locked(&mut p.a, &ta, &on_a, &mut built_a, Duration::from_secs(10))?;
            // A holds; B is refused.
            let h = hold(&mut p.a, &format!("NFH{v}A"), &on_a)?;
            refused(&mut p.b, &tb, &on_b, &mut built_b, "B")?;
            release(&mut p.a, &h)?;
            locked(&mut p.b, &tb, &on_b, &mut built_b, Duration::from_secs(10))?;
            // B holds again; A releases B's locks.
            let h = hold(&mut p.b, &format!("NFH{v}B"), &on_b)?;
            refused(&mut p.a, &ta, &on_a, &mut built_a, "A")?;
            want(
                &mut p.a,
                &format!("RLSIFSLCK RMTLOCNAME('{B_NAME}')"),
                &["CPCA1C7"],
                CMD,
            )?;
            locked(&mut p.a, &ta, &on_a, &mut built_a, Duration::from_secs(15))?;
            release(&mut p.b, &h)?;
            unmount(&mut p.b, &over)?;
            detail.push(format!(
                "version {v}: refused both ways (EAGAIN), freed by RLSIFSLCK"
            ));
        }
        Ok(detail.join("; "))
    }

    /// Step 182: with B's lock held, A's daemons ended and started: B's
    /// write during the outage lands after it, the mount reports not
    /// responding and responding once each, B's lock is reclaimed in
    /// the grace period (A is still refused after it) and B's open file
    /// goes on; then `QNFSNFSD` killed: `CPFA1B5`, started again, B reads.
    pub(crate) fn nfs_pair_recovery(&self, p: &mut Pair) -> Result<String, String> {
        p.after_grace();
        let over = "/mnt/rc";
        let on_a = format!("{ROOTED}/recover.dat");
        let on_b = format!("{over}/recover.dat");
        p.a.command(&format!("svc.ifs put {on_a} lock-file-recovery"), CMD)?;
        p.a.command(&format!("svc.ifs rm {ROOTED}/outage.txt"), CMD)?;
        // A short major timeout (0.5 + 1 + 2 s): the outage is seen soon.
        mount(
            &mut p.b,
            &format!("{A_NAME}:{ROOTED}"),
            over,
            "rw,hard,vers=3,timeo=5,retrans=2,retry=2",
        )?;
        let h = hold(&mut p.b, "NFHRB", &on_b)?;
        let before = opr_count(&mut p.b, "CPDA1B9")?;
        end_servers(&mut p.a, "*ALL")?;
        super::daemons_gone(&mut p.a)?;
        let down = Instant::now();
        let writer = submit(
            &mut p.b,
            &format!("QSH CMD('echo written-during-the-outage > {over}/outage.txt')"),
            "NFSOUTW",
        )?;
        running(&mut p.b, "NFSOUTW", Duration::from_secs(30))?;
        // Longer than the mount's major timeout (3.5 s): not responding.
        std::thread::sleep(Duration::from_secs(15));
        if !active(&mut p.b, "NFSOUTW")? {
            return Err(format!(
                "B's writer {writer} ended while A's server was down"
            ));
        }
        start_servers(&mut p.a, "*ALL")?;
        p.a_started = Instant::now();
        let outage = down.elapsed();
        gone(&mut p.b, "NFSOUTW", Duration::from_secs(180))?;
        let landed = wait_path(
            &mut p.a,
            &format!("{ROOTED}/outage.txt"),
            Duration::from_secs(30),
        )?;
        if landed.size == 0 {
            return Err(format!(
                "the write made during the outage is empty on A: {landed:?}"
            ));
        }
        let reports = wait_opr(&mut p.b, "CPDA1B9", before + 2, Duration::from_secs(60))? - before;
        if reports != 2 {
            return Err(format!(
                "B's QSYSOPR has {reports} CPDA1B9 for the outage, not 2 (not responding, responding again)"
            ));
        }
        // After the grace period B's reclaimed lock still refuses A.
        p.after_grace();
        let mut built = Vec::new();
        refused(
            &mut p.a,
            "NFTRA",
            &on_a,
            &mut built,
            "A after the grace period",
        )
        .map_err(|e| format!("{e} (B's lock was not reclaimed)"))?;
        let wrote = release(&mut p.b, &h)?;
        let size = stat(&mut p.a, &on_a)?.map_or(0, |s| s.size);
        if !wrote || size != 4104 {
            return Err(format!(
                "B's open file after the restart: written {wrote}, A's size {size}, not 4104"
            ));
        }
        // An unexpected end of the server job.
        let had = opr_count(&mut p.a, "CPFA1B5")?;
        let killed = p.a.command("dbgkill JOB(QNFSNFSD)", CMD)?;
        if !killed.contains("killed") {
            return Err(format!("dbgkill JOB(QNFSNFSD): {killed}"));
        }
        if wait_opr(&mut p.a, "CPFA1B5", had + 1, Duration::from_secs(60))? <= had {
            return Err("no CPFA1B5 on A's QSYSOPR after QNFSNFSD was killed".into());
        }
        let (n, _) = start_servers(&mut p.a, "*SVR")?;
        let read = qsh(&mut p.b, &format!("cat {over}/outage.txt"), NET)?;
        if !read.contains("written-during-the-outage") {
            return Err(format!("B reading after the restart of QNFSNFSD:\n{read}"));
        }
        unmount(&mut p.b, over)?;
        Ok(format!(
            "outage of {:.0}s: B's write landed ({} bytes), CPDA1B9 twice on B; B's lock reclaimed (A refused after the grace period); B's descriptor wrote after the restart (size 4104); QNFSNFSD killed: CPFA1B5, {n} started again, B read",
            outage.as_secs_f64(),
            landed.size
        ))
    }

    /// Step 183: a `soft` mount's read fails `ETIMEDOUT` within its major
    /// timeout while A's server is ended; `ENDJOB` of a job waiting on a
    /// `hard,intr` mount ends it.
    pub(crate) fn nfs_pair_soft(&self, p: &mut Pair) -> Result<String, String> {
        let on_a = format!("{ROOTED}/soft.dat");
        p.a.command(&format!("svc.ifs put {on_a} soft-and-intr"), CMD)?;
        // 0.5 s, then 1 s: a major timeout of 1.5 s.
        mount(
            &mut p.b,
            &format!("{A_NAME}:{ROOTED}"),
            "/mnt/s",
            "rw,soft,vers=3,timeo=5,retrans=1,noac,retry=2",
        )?;
        mount(
            &mut p.b,
            &format!("{A_NAME}:{ROOTED}"),
            "/mnt/i",
            "rw,hard,intr,vers=3,timeo=5,retrans=1,noac,retry=2",
        )?;
        prog::build(
            &mut p.b,
            "NFSRS",
            &prog::read("NFSRS", "/mnt/s/soft.dat", None, "/tmp/NFSRS"),
        )?;
        prog::build(
            &mut p.b,
            "NFSRI",
            &prog::read("NFSRI", "/mnt/i/soft.dat", None, "/tmp/NFSRI"),
        )?;
        let (warm, out) = prog::call(&mut p.b, "NFSRS", NET)?;
        if warm != Said::Ok("NFSRD-READ".into()) {
            return Err(format!("B's first read on the soft mount:\n{out}"));
        }
        end_servers(&mut p.a, "*SVR")?;
        let t = Instant::now();
        let (said, out) = prog::call(&mut p.b, "NFSRS", Duration::from_secs(120))?;
        let took = t.elapsed();
        let bound = Duration::from_millis(1500) + Duration::from_secs(20);
        let soft = match said {
            Said::Failed(_, ETIMEDOUT) if took <= bound => format!(
                "soft read: ETIMEDOUT after {:.1}s (major timeout 1.5 s)",
                took.as_secs_f64()
            ),
            other => {
                return Err(format!(
                    "the soft mount's read: {other:?} after {took:?}, not ETIMEDOUT within {bound:?}:\n{out}"
                ));
            }
        };
        rm(
            &mut p.b,
            &["ok", "eintr", "etimedout", "err"].map(|x| format!("/tmp/NFSRI.{x}")),
        )?;
        let job = submit(&mut p.b, "CALL PGM(QGPL/NFSRI)", "NFSRI")?;
        running(&mut p.b, "NFSRI", Duration::from_secs(30))?;
        std::thread::sleep(Duration::from_secs(8));
        if !active(&mut p.b, "NFSRI")? {
            return Err(format!("{job} did not wait on the hard mount"));
        }
        s_endjob(&mut p.b, &job)?;
        let ended = gone(&mut p.b, "NFSRI", Duration::from_secs(60))?;
        let eintr = stat(&mut p.b, "/tmp/NFSRI.eintr")?.is_some();
        let (n, _) = start_servers(&mut p.a, "*SVR")?;
        unmount(&mut p.b, "/mnt/s")?;
        unmount(&mut p.b, "/mnt/i")?;
        Ok(format!(
            "{soft}; ENDJOB ended the job waiting on the hard,intr mount in {:.1}s ({}); {n} server job(s) started again",
            ended.as_secs_f64(),
            if eintr {
                "its read answered EINTR"
            } else {
                "it ended before its read answered"
            }
        ))
    }

    /// Step 184: a file of `QS400_NFS_COPY_MIB` (1024) MiB made on A,
    /// copied to B through the mount and back with `CPY`, version 3
    /// then 4; MiB/s each way.
    pub(crate) fn nfs_pair_copy(&self, p: &mut Pair) -> Result<String, String> {
        let mib: u64 = std::env::var("QS400_NFS_COPY_MIB")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|n: &u64| n.is_power_of_two())
            .unwrap_or(1024);
        let bytes = mib << 20;
        room(&mut p.a, 2 * mib + 256)?;
        room(&mut p.b, mib + 256)?;
        let dir = format!("{ROOTED}/cp");
        p.a.command(&format!("svc.ifs mkdir {dir}"), CMD)?;
        let t = Instant::now();
        let made = make_file(&mut p.a, &dir, mib)?;
        if made != bytes {
            return Err(format!("the file made on A is {made} bytes, not {bytes}"));
        }
        let make_secs = t.elapsed().as_secs_f64();
        p.after_grace();
        let mut detail = Vec::new();
        for (v, opts) in [("3", "rw,vers=3,retry=2"), ("4", "rw,vers=4,retry=2")] {
            let over = format!("/mnt/cp{v}");
            mount(&mut p.b, &format!("{A_NAME}:{ROOTED}"), &over, opts)?;
            rm(&mut p.b, &["/tmp/nfscp".to_string()])?;
            p.a.command(&format!("svc.ifs rm {dir}/b"), CMD)?;
            let limit = Duration::from_mins(60);
            let t = Instant::now();
            want(
                &mut p.b,
                &format!("CPY OBJ('{over}/cp/a') TOOBJ('/tmp/nfscp')"),
                &["CPCA087"],
                limit,
            )?;
            let read = t.elapsed();
            let got = stat(&mut p.b, "/tmp/nfscp")?.map_or(0, |s| s.size);
            if got != bytes {
                return Err(format!("B's copy is {got} bytes, not {bytes}"));
            }
            let t = Instant::now();
            want(
                &mut p.b,
                &format!("CPY OBJ('/tmp/nfscp') TOOBJ('{over}/cp/b')"),
                &["CPCA087"],
                limit,
            )?;
            let wrote = t.elapsed();
            let back = stat(&mut p.a, &format!("{dir}/b"))?.map_or(0, |s| s.size);
            if back != bytes {
                return Err(format!("A's copy from B is {back} bytes, not {bytes}"));
            }
            unmount(&mut p.b, &over)?;
            #[allow(clippy::cast_precision_loss)] // MiB far below 2^52
            let rate = |d: Duration| mib as f64 / d.as_secs_f64().max(1e-3);
            let line = format!(
                "guest B <- guest A, version {v}, {mib} MiB: {:.1} MiB/s; guest B -> guest A: {:.1} MiB/s",
                rate(read),
                rate(wrote)
            );
            perf_line(&self.out_dir, &line);
            detail.push(line);
        }
        rm(&mut p.b, &["/tmp/nfscp".to_string()])?;
        rm(&mut p.a, &[format!("{dir}/a"), format!("{dir}/b")])?;
        let mut d = detail.join("; ");
        let _ = write!(d, " (the file made on A in {make_secs:.0}s)");
        Ok(d)
    }
}

/// `ENDJOB OPTION(*IMMED)` of a job.
fn s_endjob(s: &mut Session, job: &str) -> Result<(), String> {
    s.command(&format!("ENDJOB JOB({job}) OPTION(*IMMED)"), CMD)
        .map(|_| ())
}

/// A file of `mib` MiB at `dir/a` on a guest, made by doubling a 64-byte
/// line with QSH; its size.
fn make_file(s: &mut Session, dir: &str, mib: u64) -> Result<u64, String> {
    // 64 bytes, 2^(log2(mib) + 14) times.
    let rounds = mib.trailing_zeros() + 14;
    let line = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ-";
    let mut script = format!("cd {dir} && echo {line} > a");
    for _ in 0..rounds {
        script.push_str(" && cat a a > t && mv t a");
    }
    qsh(s, &script, Duration::from_mins(60))?;
    Ok(stat(s, &format!("{dir}/a"))?.map_or(0, |st| st.size))
}

#[cfg(test)]
mod tests {
    #[test]
    fn doubling_reaches_the_size() {
        for mib in [1u64, 64, 1024] {
            let rounds = mib.trailing_zeros() + 14;
            assert_eq!(64u64 << rounds, mib << 20);
        }
    }
}
