// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One FTP session with the oracle: `RCMD` for commands, binary
//! transfers for members and tables, and the bookkeeping that deletes
//! what a run made. The server job's spooled files land in the
//! account's `QPRTJOB` job, so they are found with a query of
//! `QSYS2.OUTPUT_QUEUE_ENTRIES_BASIC`; the call's messages come from
//! `QSYS2.JOBLOG_INFO('*')` between two marker messages. Queries run
//! as `RUNSQLSTM` over an uploaded member (an `RCMD` line holds about
//! 250 characters) into fixed-width CP037 tables read back in binary.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use tn5250_driver::Target;
use tn5250_driver::env::Env;
use tn5250_driver::ftp::Ftp;

use rpg_oracle::Case;
use rpg_oracle::case::DbStep;
use rpg_oracle::listing::{self, Mask};
use rpg_oracle::outcome::{Logged, Outcome, Run, Spooled, fcfc_rows};

/// The spool copy file's record length: the widest printer line and
/// its carriage control.
const SPL_RCDLEN: usize = 400;

/// The least time between two compiles.
const COMPILE_GAP: Duration = Duration::from_secs(10);

/// An open session.
pub struct Session {
    ftp: Ftp,
    lib: String,
    user: String,
    rcdlen: Option<usize>,
    ddssrc: bool,
    in_sql_listing: bool,
    release: String,
    _lock: Lock,
}

/// One row of the spool query.
struct Entry {
    name: String,
    number: String,
    job: String,
    created: String,
}

impl Session {
    /// Connect to pub400 and prepare the job, once no other session on
    /// this machine is open.
    pub fn open() -> Result<Self> {
        let lock = Lock::take()?;
        let env = Env::load()?;
        let creds = env.credentials(Target::Pub400, None)?;
        let ftp = Ftp::connect(&creds.host, &creds.user, &creds.password)
            .context("connecting to the oracle")?;
        let user = creds.user.to_ascii_uppercase();
        let mut s = Self {
            ftp,
            lib: format!("{user}B"),
            user,
            rcdlen: None,
            ddssrc: false,
            in_sql_listing: false,
            release: String::new(),
            _lock: lock,
        };
        s.ftp.command("SITE NAMEFMT 1")?;
        s.rcmd("CHGJOB INQMSGRPY(*DFT)")?;
        // The work library first in this job's list: /COPY members and
        // programs called by name are found there.
        let lib = s.lib.clone();
        s.try_rcmd(&format!("ADDLIBLE LIB({lib}) POSITION(*FIRST)"));
        Ok(s)
    }

    /// `RCMD`: whether it worked and the cause code it gave.
    fn rcmd(&mut self, cl: &str) -> Result<(bool, String)> {
        let (ok, cause, _) = self.rcmd_text(cl)?;
        Ok((ok, cause))
    }

    /// `RCMD`: whether it worked, the cause code and the reply.
    fn rcmd_text(&mut self, cl: &str) -> Result<(bool, String, String)> {
        let (code, text) = self.ftp.command(&format!("RCMD {cl}"))?;
        let cause = text
            .split("Cause code is ")
            .nth(1)
            .map(|r| r.chars().take(7).collect::<String>())
            .unwrap_or_default();
        Ok((code == 250, cause, text))
    }

    /// Quietly try a command.
    fn try_rcmd(&mut self, cl: &str) {
        let _ = self.rcmd(cl);
    }

    fn member_path(&self, file: &str, member: &str) -> String {
        format!("/QSYS.LIB/{}.LIB/{file}.FILE/{member}.MBR", self.lib)
    }

    /// Make sure `QRPGLESRC` has the record length a member needs.
    fn source_file(&mut self, rcdlen: usize) -> Result<()> {
        if self.rcdlen == Some(rcdlen) {
            return Ok(());
        }
        let lib = self.lib.clone();
        self.try_rcmd(&format!("DLTF FILE({lib}/QRPGLESRC)"));
        let (ok, cause) = self.rcmd(&format!(
            "CRTSRCPF FILE({lib}/QRPGLESRC) RCDLEN({rcdlen}) CCSID(37)"
        ))?;
        if !ok {
            bail!("CRTSRCPF failed {cause}");
        }
        self.try_rcmd(&format!("DLTF FILE({lib}/RPGSPL)"));
        let (ok, cause) = self.rcmd(&format!("CRTPF FILE({lib}/RPGSPL) RCDLEN({SPL_RCDLEN})"))?;
        if !ok {
            bail!("CRTPF failed {cause}");
        }
        self.rcdlen = Some(rcdlen);
        Ok(())
    }

    /// Upload lines as a member: binary data records, which the server
    /// numbers 1.00, 2.00, ... with zero dates.
    fn put_member(&mut self, member: &str, lines: &[String], width: usize) -> Result<()> {
        self.put_source("QRPGLESRC", member, lines, width)
    }

    /// Upload lines as a member of a source file.
    fn put_source(&mut self, file: &str, member: &str, lines: &[String], width: usize) -> Result<()> {
        let mut data = Vec::with_capacity(lines.len() * width);
        for (i, l) in lines.iter().enumerate() {
            let mut rec = vec![0x40u8; width];
            ccsid::encode_fixed(l, &mut rec)
                .map_err(|e| anyhow::anyhow!("line {}: {e}", i + 1))?;
            data.extend(rec);
        }
        let path = self.member_path(file, member);
        self.ftp.put(&path, &data, true)
    }

    /// A member's database steps: DDS files created from `QDDSSRC`,
    /// SQL statements run; the names of the files made, in order.
    fn db_setup(&mut self, c: &Case) -> Result<Vec<String>> {
        let lib = self.lib.clone();
        let mut made = Vec::new();
        let mut batch: Vec<String> = Vec::new();
        for (i, step) in c.db.iter().enumerate() {
            match step {
                DbStep::Dds { name, logical, source } => {
                    if !self.ddssrc {
                        self.try_rcmd(&format!("DLTF FILE({lib}/QDDSSRC)"));
                        let (ok, cause) = self.rcmd(&format!(
                            "CRTSRCPF FILE({lib}/QDDSSRC) RCDLEN(92) CCSID(37)"
                        ))?;
                        if !ok {
                            bail!("CRTSRCPF QDDSSRC failed {cause}");
                        }
                        self.ddssrc = true;
                    }
                    self.try_rcmd(&format!("DLTF FILE({lib}/{name})"));
                    self.try_rcmd(&format!("RMVM FILE({lib}/QDDSSRC) MBR({name})"));
                    self.put_source("QDDSSRC", name, source, 80)?;
                    let cmd = if *logical { "CRTLF" } else { "CRTPF" };
                    let (ok, cause) = self.rcmd(&format!(
                        "{cmd} FILE({lib}/{name}) SRCFILE({lib}/QDDSSRC) SRCMBR({name})"
                    ))?;
                    self.try_rcmd(&format!("RMVM FILE({lib}/QDDSSRC) MBR({name})"));
                    if !ok {
                        bail!("{cmd} {name} failed {cause}");
                    }
                    made.push(name.clone());
                }
                DbStep::Sql(s) => {
                    if let Some(n) = step.creates() {
                        self.try_rcmd(&format!("DLTF FILE({lib}/{n})"));
                        made.push(n);
                    }
                    batch.push(c.fill(s, &lib));
                }
                DbStep::SqlFile(_) => {}
            }
            // A run of statements goes through one RUNSQLSTM.
            let next_sql = matches!(c.db.get(i + 1), Some(DbStep::Sql(_)));
            if !batch.is_empty() && !next_sql {
                let script = batch.join(" ; ");
                batch.clear();
                self.sql(&script)?;
            }
        }
        Ok(made)
    }

    /// Delete what [`Self::db_setup`] made, the last first.
    fn db_cleanup(&mut self, made: &[String]) {
        let lib = self.lib.clone();
        for n in made.iter().rev() {
            self.try_rcmd(&format!("DLTF FILE({lib}/{n})"));
        }
    }

    /// Run an SQL statement through `RUNSQLSTM` over a member.
    fn sql(&mut self, stmt: &str) -> Result<()> {
        let width = self.rcdlen.unwrap_or(112) - 12;
        let mut lines = Vec::new();
        let mut cur = String::new();
        for w in stmt.split_whitespace() {
            // A line never ends inside a literal: the margin's blanks
            // would join it.
            let open = cur.matches('\'').count() % 2 == 1;
            if !open && cur.len() + w.len() + 1 > 70.min(width) {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(w);
        }
        lines.push(cur);
        self.put_member("QSQ", &lines, width)?;
        let lib = self.lib.clone();
        let (ok, cause) = self.rcmd(&format!(
            "RUNSQLSTM SRCFILE({lib}/QRPGLESRC) SRCMBR(QSQ) COMMIT(*NONE) NAMING(*SQL) MARGINS(80) OUTPUT(*PRINT)"
        ))?;
        self.try_rcmd(&format!("RMVM FILE({lib}/QRPGLESRC) MBR(QSQ)"));
        if !ok {
            // The statement listing says why, unless this is that query.
            let mut why = Vec::new();
            if !self.in_sql_listing {
                self.in_sql_listing = true;
                if let Ok(list) = self.spools() {
                    for e in list {
                        if let Ok(lines) = self.take_spool(&e)
                            && e.name == "QSQ"
                        {
                            why.extend(lines.into_iter().filter(|l| l.contains("SQL")));
                        }
                    }
                }
                self.in_sql_listing = false;
            }
            bail!("RUNSQLSTM failed {cause}\n{}", why.join("\n"));
        }
        Ok(())
    }

    /// Fetch a table's records.
    fn table(&mut self, table: &str, rcdlen: usize) -> Result<Vec<Vec<u8>>> {
        let path = self.member_path(table, table);
        let data = self.ftp.get(&path, true)?;
        let lib = self.lib.clone();
        self.try_rcmd(&format!("DLTF FILE({lib}/{table})"));
        Ok(data.chunks(rcdlen).map(<[u8]>::to_vec).collect())
    }

    /// The account's spooled files made in the last half hour, oldest
    /// first (job logs excluded).
    fn spools(&mut self) -> Result<Vec<Entry>> {
        let lib = self.lib.clone();
        self.sql(&format!(
            "CREATE TABLE {lib}.RPGSPLS AS ( SELECT \
             CAST(SPOOLED_FILE_NAME AS CHAR(10) CCSID 37) N, \
             CAST(DIGITS(CAST(FILE_NUMBER AS DEC(7, 0))) AS CHAR(7) CCSID 37) K, \
             CAST(JOB_NAME AS CHAR(28) CCSID 37) J, \
             CAST(CHAR(CREATE_TIMESTAMP) AS CHAR(26) CCSID 37) T \
             FROM QSYS2.OUTPUT_QUEUE_ENTRIES_BASIC \
             WHERE USER_NAME = USER AND SPOOLED_FILE_NAME <> 'QPJOBLOG' \
             AND STATUS <> 'OPEN' \
             AND CREATE_TIMESTAMP > CURRENT TIMESTAMP - 30 MINUTES ) WITH DATA"
        ))?;
        let rows = self.table("RPGSPLS", 10 + 7 + 28 + 26)?;
        let mut out: Vec<Entry> = rows
            .iter()
            .filter(|r| r.len() == 71)
            .map(|r| Entry {
                name: ccsid::from_ebcdic(&r[0..10]).trim().to_string(),
                number: ccsid::from_ebcdic(&r[10..17]).trim_start_matches('0').to_string(),
                job: ccsid::from_ebcdic(&r[17..45]).trim().to_string(),
                created: ccsid::from_ebcdic(&r[45..71]),
            })
            .collect();
        out.sort_by(|a, b| a.created.cmp(&b.created));
        Ok(out)
    }

    /// Copy a spooled file's lines and delete it.
    fn take_spool(&mut self, e: &Entry) -> Result<Vec<String>> {
        let lib = self.lib.clone();
        let (ok, _, reply) = self.rcmd_text(&format!(
            "CPYSPLF FILE({}) TOFILE({lib}/RPGSPL) JOB({}) SPLNBR({}) MBROPT(*REPLACE) CTLCHAR(*FCFC)",
            e.name, e.job, e.number
        ))?;
        if !ok {
            bail!("CPYSPLF {} {} failed: {}", e.name, e.number, reply.trim());
        }
        let path = self.member_path("RPGSPL", "RPGSPL");
        let data = self.ftp.get(&path, true)?;
        self.try_rcmd(&format!(
            "DLTSPLF FILE({}) JOB({}) SPLNBR({})",
            e.name, e.job, e.number
        ));
        Ok(data
            .chunks(SPL_RCDLEN)
            .map(|r| ccsid::from_ebcdic(r).trim_end().to_string())
            .collect())
    }

    /// The job log between the markers, without request messages.
    fn log_between(&mut self, begin: &str, end: &str) -> Result<Vec<Logged>> {
        let lib = self.lib.clone();
        self.sql(&format!(
            "CREATE TABLE {lib}.RPGJLOG AS ( SELECT \
             CAST(DIGITS(CAST(ORDINAL_POSITION AS DEC(9, 0))) AS CHAR(9) CCSID 37) O, \
             CAST(COALESCE(MESSAGE_ID, ' ') AS CHAR(7) CCSID 37) I, \
             CAST(COALESCE(MESSAGE_TYPE, ' ') AS CHAR(13) CCSID 37) Y, \
             CAST(COALESCE(MESSAGE_TEXT, ' ') AS CHAR(40) CCSID 37) X, \
             CAST(COALESCE(HEX(MESSAGE_TOKENS), ' ') AS CHAR(512) CCSID 37) H \
             FROM TABLE(QSYS2.JOBLOG_INFO('*')) A ) WITH DATA"
        ))?;
        let n = 9 + 7 + 13 + 40 + 512;
        let mut rows: Vec<(u64, Logged, String)> = self
            .table("RPGJLOG", n)?
            .iter()
            .filter(|r| r.len() == n)
            .map(|r| {
                let ord = ccsid::from_ebcdic(&r[0..9]).trim().parse().unwrap_or(0);
                let kind = match ccsid::from_ebcdic(&r[16..29]).trim() {
                    "INFORMATIONAL" => "*INFO",
                    "ESCAPE" => "*ESCAPE",
                    "INQUIRY" => "*INQ",
                    "REPLY" => "*RPY",
                    "DIAGNOSTIC" => "*DIAG",
                    "COMPLETION" => "*COMP",
                    "NOTIFY" => "*NOTIFY",
                    "REQUEST" => "*RQS",
                    "SENDER" => "*SENDER",
                    other => return (ord, Logged::default(), other.to_string()),
                }
                .to_string();
                let text = ccsid::from_ebcdic(&r[29..69]).trim().to_string();
                (
                    ord,
                    Logged {
                        msgid: ccsid::from_ebcdic(&r[9..16]).trim().to_string(),
                        kind,
                        data: ccsid::from_ebcdic(&r[69..]).trim().to_string(),
                    },
                    text,
                )
            })
            .collect();
        rows.sort_by_key(|r| r.0);
        let mut out = Vec::new();
        let mut inside = false;
        for (_, m, text) in rows {
            if text == begin {
                inside = true;
                continue;
            }
            if text == end {
                break;
            }
            if inside && m.kind != "*RQS" && !m.kind.is_empty() {
                out.push(m);
            }
        }
        Ok(out)
    }

    /// Wait out the compile gap, then note this compile.
    fn throttle() -> Result<()> {
        let stamp = stamp_file();
        if let Ok(t) = std::fs::read_to_string(&stamp)
            && let Ok(secs) = t.trim().parse::<u64>()
        {
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let wait = (secs + COMPILE_GAP.as_secs()).saturating_sub(now);
            if wait > 0 {
                std::thread::sleep(Duration::from_secs(wait));
            }
        }
        if let Some(d) = stamp.parent() {
            std::fs::create_dir_all(d)?;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        std::fs::write(&stamp, now.to_string())?;
        Ok(())
    }

    /// Compile and run one member: its outcome and masked listing.
    pub fn case(&mut self, c: &Case) -> Result<(Outcome, Vec<String>)> {
        self.source_file(c.rcdlen)?;
        let lib = self.lib.clone();
        self.try_rcmd(&format!("RMVM FILE({lib}/QRPGLESRC) MBR({})", c.name));
        self.put_member(&c.name, &c.lines, c.width())?;
        for (w, lines) in &c.with {
            self.try_rcmd(&format!("RMVM FILE({lib}/QRPGLESRC) MBR({w})"));
            self.put_member(w, lines, c.width())?;
        }
        for s in &c.setup {
            let cl = c.fill(s, &lib);
            if cl.trim_start().to_ascii_uppercase().starts_with("CRT") {
                Self::throttle()?;
            }
            let (ok, _, reply) = self.rcmd_text(&cl)?;
            if !ok {
                eprintln!("{}: setup failed: {cl}: {}", c.name, reply.trim());
            }
        }
        let made = match self.db_setup(c) {
            Ok(m) => m,
            Err(e) => {
                let all: Vec<String> = c.db.iter().filter_map(DbStep::creates).collect();
                self.db_cleanup(&all);
                return Err(e);
            }
        };
        // Spooled files left from before are not this member's.
        for e in self.spools()? {
            let _ = self.take_spool(&e);
        }
        Self::throttle()?;
        let (created, cause) = self.rcmd(&c.fill(&c.cmd, &lib))?;
        let mut o = Outcome {
            created,
            cause: if created { String::new() } else { cause },
            ..Outcome::default()
        };
        let mut listing = Vec::new();
        for e in self.spools()? {
            let lines = self.take_spool(&e)?;
            if e.name == "QSQ" {
                continue;
            }
            if listing.is_empty() || e.name == c.name {
                listing = lines;
            }
        }
        if self.release.is_empty() {
            self.release = listing
                .iter()
                .find_map(|l| l.split_whitespace().nth(2).filter(|w| w.starts_with('V')))
                .unwrap_or("")
                .to_string();
        }
        o.diags = listing::diags(&listing);
        if created && !c.call.is_empty() {
            let begin = format!("RPGORA BEGIN {}", c.name);
            let end = format!("RPGORA END {}", c.name);
            self.try_rcmd(&format!("SNDPGMMSG MSG('{begin}') TOPGMQ(*SAME)"));
            let (ok, cause) = self.rcmd(&c.fill(&c.call, &lib))?;
            self.try_rcmd(&format!("SNDPGMMSG MSG('{end}') TOPGMQ(*SAME)"));
            o.run = if ok { Run::Ok } else { Run::Escape(cause) };
            // Files a program left open (it returned without LR) close,
            // so their spooled files can be read; this job only.
            self.try_rcmd("RCLRSC LVL(*CALLER)");
            self.try_rcmd("RCLACTGRP ACTGRP(*ELIGIBLE)");
            for e in self.spools()? {
                let records = self.take_spool(&e)?;
                if e.name == "QSQ" {
                    continue;
                }
                o.spool.push(Spooled {
                    file: e.name.clone(),
                    lines: fcfc_rows(&records, &["https://pub400.com"]),
                });
            }
            o.log = self.log_between(&begin, &end)?;
        }
        for cmd in ["DLTPGM PGM", "DLTMOD MODULE", "DLTSRVPGM SRVPGM"] {
            self.try_rcmd(&format!("{cmd}({lib}/{})", c.name));
        }
        for s in &c.cleanup {
            let cl = c.fill(s, &lib);
            self.try_rcmd(&cl);
        }
        for (w, _) in &c.with {
            self.try_rcmd(&format!("RMVM FILE({lib}/QRPGLESRC) MBR({w})"));
        }
        self.try_rcmd(&format!("RMVM FILE({lib}/QRPGLESRC) MBR({})", c.name));
        self.db_cleanup(&made);
        o.provenance = vec![
            format!("{} {} pub400.com", c.name, self.release),
            format!("captured {} by rpg-oracle", today()),
        ];
        let m = Mask {
            libs: vec![lib],
            users: vec![self.user.clone()],
            systems: vec!["PUB400".into()],
        };
        Ok((o, listing::mask(&listing, &m)))
    }

    /// Delete the run's files and any spooled file left behind.
    pub fn clean(&mut self) {
        if let Ok(list) = self.spools() {
            for e in list {
                let _ = self.take_spool(&e);
            }
        }
        let lib = self.lib.clone();
        for f in ["QRPGLESRC", "QDDSSRC", "RPGSPL", "RPGSPLS", "RPGJLOG"] {
            self.try_rcmd(&format!("DLTF FILE({lib}/{f})"));
        }
    }

    /// Delete the work files and say goodbye.
    pub fn close(mut self) {
        let lib = self.lib.clone();
        for f in ["QRPGLESRC", "QDDSSRC", "RPGSPL"] {
            self.try_rcmd(&format!("DLTF FILE({lib}/{f})"));
        }
        self.ftp.quit();
    }
}

/// The folder every `rpg-oracle` process on this machine shares, so
/// the compile gap and the one-session rule hold across worktrees.
fn shared_dir() -> PathBuf {
    std::env::temp_dir().join("qs400-rpg-oracle")
}

/// Where the last compile's time is kept.
fn stamp_file() -> PathBuf {
    shared_dir().join("last-compile")
}

/// The machine-wide session lock: held while a session is open.
pub struct Lock(PathBuf);

impl Lock {
    /// Wait for the lock (another process's session), taking it over
    /// when its holder has been gone for an hour.
    pub fn take() -> Result<Self> {
        let dir = shared_dir();
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("session.lock");
        loop {
            if std::fs::OpenOptions::new().write(true).create_new(true).open(&path).is_ok() {
                return Ok(Self(path));
            }
            let stale = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|e| e > Duration::from_secs(3600));
            if stale {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Today as `yyyy-mm-dd` (UTC).
fn today() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    // Civil from days (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}
