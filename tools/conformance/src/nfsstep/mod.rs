// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS steps of the disk suite (`docs/NFS.md` section 11).
//!
//! On the main guest, after the `NetServer` steps:
//!
//! | Step | What |
//! |---|---|
//! | 173 | `STRNFSSVR SERVER(*ALL)`, the seven daemons in `QSYSWRK`, `/etc/RPCBTAB`, `EXPORTFS`, `NFS_EXPORT_INFO`, `DSPMFSINF` |
//! | 174 | `nfstest` version 3 (RPC, MOUNT, NFS, NLM, NSM) from the host through forwards |
//! | 175 | `nfstest` versions 4.0 and 4.1 through the forwarded 2049 |
//! | 176 | `nfstest bench` from the host, MiB/s |
//! | 177 | the guest mounting a Linux export (`QS400_NFS_LINUX`), skipped without it |
//! | 178 | `UNMOUNT TYPE(*ALL)`, `ENDNFSSVR SERVER(*ALL)`, the jobs and ports gone |
//! | 185 | a version 3 handle the host saved, used again after a restart of the daemons and after an IPL |
//!
//! In a pair of guests of their own (`pair`), A exporting and B
//! mounting over a private link (`192.168.77.0/24`): 179 version 3, 180
//! version 4 and `/QSYS.LIB`, 181 locks across the guests, 182 the
//! server ended and killed with a lock reclaimed, 183 `soft` and `intr`,
//! 184 copies between the guests.
//!
//! A failed step writes what the guests show (job logs, `QSYSOPR`, the
//! jobs, the export and mount tables, the binder) to
//! `nfs-<step>-<guest>.txt` in the output directory (`diag`).

mod diag;
mod host;
mod pair;
mod pairsteps;
mod prog;
mod single;

use std::time::{Duration, Instant};

use crate::qemu::session::Session;

/// What the steps on the main guest set up, for the next ones.
#[derive(Debug, Default)]
pub(crate) struct State {
    /// When `STRNFSSVR` last finished there (the grace periods run from it).
    pub started: Option<Instant>,
    /// Guest port to host port, forwarded for TCP and UDP alike.
    pub map: Vec<(u16, u16)>,
}

/// The directory the guests export.
pub(crate) const EXPORT: &str = "/nfstest";
/// How long after a start the NLM and `NFSv4` grace periods (90 s,
/// `docs/NFS.md` section 4) are surely over.
pub(crate) const AFTER_GRACE: Duration = Duration::from_secs(97);
/// A command that reaches over the network.
pub(crate) const NET: Duration = Duration::from_secs(90);

/// The first word of a line when it is a message identifier.
fn msgid(line: &str) -> Option<&str> {
    let w = line.split_whitespace().next()?;
    let ok = w.len() == 7
        && w.starts_with("CP")
        && w[2..3].chars().all(|c| matches!(c, 'F' | 'D' | 'C' | 'I'))
        && w[3..].chars().all(|c| c.is_ascii_hexdigit());
    ok.then_some(w)
}

/// Escape (`CPF`) or diagnostic (`CPD`) messages in a command's output.
pub(crate) fn errors(out: &str) -> Vec<&str> {
    out.lines()
        .filter(|l| msgid(l).is_some_and(|m| m.starts_with("CPF") || m.starts_with("CPD")))
        .collect()
}

/// A command whose output must hold one of `ids`.
pub(crate) fn want(
    s: &mut Session,
    line: &str,
    ids: &[&str],
    limit: Duration,
) -> Result<String, String> {
    let out = s.command(line, limit)?;
    if ids.iter().any(|i| out.contains(i)) {
        Ok(out)
    } else {
        Err(format!("{line}: none of {ids:?} in:\n{out}"))
    }
}

/// A command that must end without an escape or diagnostic message.
pub(crate) fn clean(s: &mut Session, line: &str, limit: Duration) -> Result<String, String> {
    let out = s.command(line, limit)?;
    let bad = errors(&out);
    if bad.is_empty() {
        Ok(out)
    } else {
        Err(format!("{line}: {}\n{out}", bad.join("; ")))
    }
}

/// `QSH CMD(script)`, which must end with exit status 0 (`QSH0005`).
/// The script holds no single quote.
pub(crate) fn qsh(s: &mut Session, script: &str, limit: Duration) -> Result<String, String> {
    debug_assert!(!script.contains('\''));
    let out = s.command(&format!("QSH CMD('{script}')"), limit)?;
    if out.contains("QSH0005") && out.contains("exit status 0.") {
        Ok(out)
    } else {
        Err(format!("qsh {script}:\n{out}"))
    }
}

/// One SQL statement through the service console.
pub(crate) fn sql(s: &mut Session, stmt: &str) -> Result<String, String> {
    s.command(&format!("svc.sql {stmt};"), Duration::from_secs(30))
}

/// What `svc.ifs stat` shows of a local path; `None` when it is not there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Stat {
    /// `*STMF`, `*DIR` ...
    pub kind: String,
    /// The owner's profile.
    pub owner: String,
    /// The size in bytes.
    pub size: u64,
}

/// `svc.ifs stat`'s text read back.
pub(crate) fn parse_stat(out: &str) -> Option<Stat> {
    let field = |key: &str| {
        out.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|r| r.split_whitespace().next())
            .map(str::to_string)
    };
    Some(Stat {
        kind: field("Type ")?,
        owner: field("Owner ")?,
        size: field("Size ")?.parse().ok()?,
    })
}

/// A local path's attributes on a guest.
pub(crate) fn stat(s: &mut Session, path: &str) -> Result<Option<Stat>, String> {
    Ok(parse_stat(
        &s.command(&format!("svc.ifs stat {path}"), CMD)?,
    ))
}

/// Wait until a local path exists on a guest.
pub(crate) fn wait_path(s: &mut Session, path: &str, limit: Duration) -> Result<Stat, String> {
    let end = Instant::now() + limit;
    loop {
        if let Some(st) = stat(s, path)? {
            return Ok(st);
        }
        if Instant::now() > end {
            return Err(format!("{path} did not appear within {limit:?}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// The console job's log after the last request naming `cmd`.
pub(crate) fn log_after(s: &mut Session, cmd: &str) -> Result<String, String> {
    let log = s.command("svc.dspjoblog", CMD)?;
    Ok(tail_after(&log, cmd))
}

/// The lines of `log` after the last request (`>>`) naming `cmd` (any
/// case), else after the last line naming it.
pub(crate) fn tail_after(log: &str, cmd: &str) -> String {
    let cmd = cmd.to_ascii_uppercase();
    let lines: Vec<&str> = log.lines().collect();
    let names = |l: &&str| l.to_ascii_uppercase().contains(&cmd);
    let from = lines
        .iter()
        .rposition(|l| l.starts_with(">>") && names(l))
        .or_else(|| lines.iter().rposition(names))
        .map_or(0, |i| i + 1);
    lines[from..].join("\n")
}

/// The subsystem `WRKACTJOB`'s text lists `job` under: jobs are indented
/// two more than their subsystem's monitor.
pub(crate) fn subsystem_of(list: &str, job: &str) -> Option<String> {
    let mut sbs = None;
    for l in list.lines() {
        let Some(rest) = l.strip_prefix("     ") else {
            continue;
        };
        if let Some(kid) = rest.strip_prefix("  ") {
            if kid.split_whitespace().next() == Some(job) {
                return sbs.clone();
            }
        } else if let Some(name) = rest.split_whitespace().next() {
            sbs = Some(name.to_string());
        }
    }
    None
}

/// Lines of `QSYSOPR` holding `id`.
pub(crate) fn opr_count(s: &mut Session, id: &str) -> Result<usize, String> {
    let out = s.command("DSPMSG MSGQ(QSYSOPR)", CMD)?;
    Ok(out.lines().filter(|l| l.contains(id)).count())
}

/// Wait until `QSYSOPR` holds `n` lines with `id`; their count.
pub(crate) fn wait_opr(
    s: &mut Session,
    id: &str,
    n: usize,
    limit: Duration,
) -> Result<usize, String> {
    let end = Instant::now() + limit;
    loop {
        let got = opr_count(s, id)?;
        if got >= n || Instant::now() > end {
            return Ok(got);
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// The daemon jobs, in `STRNFSSVR`'s order.
pub(crate) const DAEMONS: [&str; 7] = [
    "QNFSRPCD", "QNFSBIOD", "QNFSRGYD", "QNFSNFSD", "QNFSMNTD", "QNFSNSMD", "QNFSNLMD",
];

/// Wait until every daemon job runs in `QSYSWRK`; their rows.
pub(crate) fn daemons_in_qsyswrk(s: &mut Session) -> Result<Vec<String>, String> {
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        let list = s.command("wrkactjob", CMD)?;
        let missing: Vec<&str> = DAEMONS
            .iter()
            .copied()
            .filter(|d| subsystem_of(&list, d).as_deref() != Some("QSYSWRK"))
            .collect();
        if missing.is_empty() {
            return Ok(DAEMONS
                .iter()
                .filter_map(|d| {
                    list.lines()
                        .find(|l| l.split_whitespace().next() == Some(d))
                        .map(|l| l.trim().to_string())
                })
                .collect());
        }
        if Instant::now() > end {
            return Err(format!("not active in QSYSWRK: {missing:?}\n{list}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Wait until no daemon job is active.
pub(crate) fn daemons_gone(s: &mut Session) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        let list = s.command("wrkactjob", CMD)?;
        let left: Vec<&str> = DAEMONS
            .iter()
            .copied()
            .filter(|d| list.lines().any(|l| l.split_whitespace().next() == Some(d)))
            .collect();
        if left.is_empty() {
            return Ok(());
        }
        if Instant::now() > end {
            return Err(format!("still active: {left:?}\n{list}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// `STRNFSSVR SERVER(*ALL)`: no escape, `CPCA1BB` in the job log; the
/// `CPIA1BB` (started) and `CPDA1BA` (running already) it logged.
pub(crate) fn start_servers(s: &mut Session, server: &str) -> Result<(usize, usize), String> {
    let line = format!("STRNFSSVR SERVER({server}) STRJOBTIMO(120)");
    clean(s, &line, Duration::from_secs(240))?;
    let log = log_after(s, "STRNFSSVR")?;
    if !log.contains("CPCA1BB") {
        return Err(format!("{line}: no CPCA1BB in the job log:\n{log}"));
    }
    let count = |id: &str| log.lines().filter(|l| l.contains(id)).count();
    Ok((count("CPIA1BB"), count("CPDA1BA") + count("CPDA1BD")))
}

/// `ENDNFSSVR`: no escape, `CPCA1BC` in the job log.
pub(crate) fn end_servers(s: &mut Session, server: &str) -> Result<(), String> {
    let line = format!("ENDNFSSVR SERVER({server}) ENDJOBTIMO(60)");
    clean(s, &line, Duration::from_secs(180))?;
    let log = log_after(s, "ENDNFSSVR")?;
    if !log.contains("CPCA1BC") {
        return Err(format!("{line}: no CPCA1BC in the job log:\n{log}"));
    }
    Ok(())
}

/// The console job keeps every message (`DSPJOBLOG` shows the `CPI`s).
pub(crate) fn full_log(s: &mut Session) -> Result<(), String> {
    s.command("chgjob LOG(4 0 *NOLIST)", CMD).map(|_| ())
}

/// A directory, `*PUBLIC *RWX`, so the anonymous user writes in it.
pub(crate) fn public_dir(s: &mut Session, dir: &str) -> Result<(), String> {
    s.command(&format!("MKDIR DIR('{dir}')"), CMD)?;
    match stat(s, dir)? {
        Some(st) if st.kind == "*DIR" => {}
        other => return Err(format!("MKDIR {dir}: {other:?}")),
    }
    clean(
        s,
        &format!("CHGAUT OBJ('{dir}') USER(*PUBLIC) DTAAUT(*RWX)"),
        CMD,
    )?;
    Ok(())
}

/// `EXPORTFS -I` of `dir` with `options`: `CPCA1C4`.
pub(crate) fn export(
    s: &mut Session,
    dir: &str,
    options: &str,
    hostopt: &str,
) -> Result<(), String> {
    let hostopt = if hostopt.is_empty() {
        String::new()
    } else {
        format!(" HOSTOPT({hostopt})")
    };
    want(
        s,
        &format!("EXPORTFS OPTIONS('-I -O ''{options}''') DIR('{dir}'){hostopt}"),
        &["CPCA1C4"],
        CMD,
    )
    .map(|_| ())
}

/// The command time-out of the console.
pub(crate) const CMD: Duration = crate::steps::CMD;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_identifiers() {
        assert_eq!(msgid("CPFA1B8 Not authorized."), Some("CPFA1B8"));
        assert_eq!(msgid("CPDA1BA *SVR running"), Some("CPDA1BA"));
        assert_eq!(msgid("CPCA1BB done"), Some("CPCA1BB"));
        assert_eq!(msgid("TCP1A07 x"), None);
        assert_eq!(msgid("CPFX"), None);
        assert_eq!(
            errors("CPIA1BB a\nCPFA1BC b\nCPCA1BB c\nCPDA1C5 d"),
            ["CPFA1BC b", "CPDA1C5 d"]
        );
    }

    #[test]
    fn stat_text() {
        let out = "Path       /nfstest/f\nType       *STMF\nOwner      QNFSANON (uid 4294770710), group *NONE (gid 0)\nMode       100644\nSize       4104 (8192 allocated)\n";
        assert_eq!(
            parse_stat(out),
            Some(Stat {
                kind: "*STMF".into(),
                owner: "QNFSANON".into(),
                size: 4104
            })
        );
        assert_eq!(parse_stat("/x: No such path (ENOENT, CPFA0A9)\n"), None);
    }

    #[test]
    fn subsystems_of_jobs() {
        let list = "Opt  Subsystem/Job  User        Type  CPU %  Function        Status\n     QBASE          QSYS        SBS     0.0                  DEQW\n       QPADEV0001   QSECOFR     INT     0.0                  DSPW\n     QSYSWRK        QSYS        SBS     0.0                  DEQW\n       QNFSRPCD     QSECOFR     BCH     0.0                  RUN\n       QNFSNFSD     QSECOFR     BCH     0.0                  RUN\n";
        assert_eq!(subsystem_of(list, "QNFSNFSD").as_deref(), Some("QSYSWRK"));
        assert_eq!(subsystem_of(list, "QPADEV0001").as_deref(), Some("QBASE"));
        assert_eq!(subsystem_of(list, "QNFSNLMD"), None);
    }

    #[test]
    fn the_log_after_a_request() {
        let log = ">> Request STRNFSSVR SERVER(*ALL)\nCPIA1BB a\nCPCA1BB b\n>> Request strnfssvr SERVER(*SVR)\nCPDA1BA c\nCPCA1BB d";
        assert_eq!(tail_after(log, "STRNFSSVR"), "CPDA1BA c\nCPCA1BB d");
    }
}
