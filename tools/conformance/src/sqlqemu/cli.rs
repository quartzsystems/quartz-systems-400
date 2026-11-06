// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 133: the SQL CLI in server mode, compiled and run in the guest.

use std::fmt::Write as _;
use std::path::Path;
use std::time::{Duration, Instant};

use super::{LIB, Tn, put, setup, spooled, sql};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// A server-mode connection held while the job waits, so its
/// `QSQSRVR` job shows serving.
const CLIWAIT: &str = "**FREE
ctl-opt dftactgrp(*no);
/copy CLIH
dcl-pr qcmdexc extpgm('QCMDEXC');
  cmd char(100) const;
  len packed(15:5) const;
end-pr;
dcl-s henv int(10);
dcl-s hdbc int(10);
dcl-s rc int(10);
dcl-s on int(10) inz(1);
dcl-s dsn char(10) inz('*LOCAL');
rc = SQLAllocEnv(%addr(henv));
rc = SQLSetEnvAttr(henv : SQL_ATTR_SERVER_MODE : %addr(on) : 0);
rc = SQLAllocConnect(henv : %addr(hdbc));
rc = SQLConnect(hdbc : %addr(dsn) : 6 : *null : 0 : *null : 0);
qcmdexc('DLYJOB DLY(45)' : 14);
rc = SQLDisconnect(hdbc);
rc = SQLFreeConnect(hdbc);
rc = SQLFreeEnv(henv);
*inlr = *on;
";

fn kit(rel: &str) -> Result<String, String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))
}

/// The text of each printed line of a `dspsplf` display.
fn printed(display: &str) -> Vec<String> {
    display
        .lines()
        .filter_map(|l| {
            let t = l.trim_start();
            let (pl, rest) = t.split_once(char::is_whitespace)?;
            let (p, n) = pl.split_once('/')?;
            (p.chars().all(|c| c.is_ascii_digit()) && n.chars().all(|c| c.is_ascii_digit()) && !p.is_empty())
                .then(|| rest.trim().to_string())
        })
        .collect()
}

/// The spooled lines of an `rpg-oracle` outcome file.
fn expected_lines(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.strip_prefix("  |"))
        .map(|l| l.split_once(char::is_whitespace).map_or("", |(_, t)| t).trim().to_string())
        .collect()
}

/// The `QSQSRVR` rows of `WRKACTJOB`.
fn qsqsrvr(s: &mut Session) -> Result<Vec<String>, String> {
    let out = s.command("WRKACTJOB", CMD)?;
    Ok(out.lines().filter(|l| l.contains("QSQSRVR")).map(str::to_string).collect())
}

impl Suite {
    /// Step 133: `CLI001` of the RPG kit compiled with `CRTBNDRPG` and
    /// called; its printed lines are pub400's. `QSQSRVR` prestart jobs
    /// wait in `QSYSWRK` (PJ, PSRW) and one serves a connection that a
    /// batch job holds open (CNDW).
    pub(crate) fn sql_cli(&self, s: &mut Session) -> Result<String, String> {
        setup(s)?;
        let _ = sql(s, &format!("DROP TABLE {LIB}/CLIT"))?;
        let clit = kit("rpg/dds/CLIT.sql")?.replace("{lib}.", &format!("{LIB}/"));
        let body: String = clit.lines().filter(|l| !l.starts_with("--")).collect::<Vec<_>>().join(" ");
        for stmt in body.split(';').map(str::trim).filter(|t| !t.is_empty()) {
            let out = sql(s, stmt)?;
            if out.contains("SQLCODE -") {
                return Err(format!("{stmt}: {out}"));
            }
        }
        put(s, "QRPGLESRC", "CLIH", "RPGLEINC", &kit("rpg/b23cli/CLIH.rpginc")?)?;
        put(s, "QRPGLESRC", "CLI001", "RPGLE", &kit("rpg/b23cli/CLI001.rpgle")?)?;
        put(s, "QRPGLESRC", "CLIWAIT", "RPGLE", CLIWAIT)?;
        let mut tn = Tn::open(self)?;
        for p in ["CLI001", "CLIWAIT"] {
            tn.cmd(
                &format!("CRTBNDRPG PGM({LIB}/{p}) SRCFILE({LIB}/QRPGLESRC)"),
                &format!("Program {p} created"),
            )?;
        }
        // CALL pads the literal to 32 with blanks, as IBM's does.
        tn.cmd(&format!("CALL PGM({LIB}/CLI001) PARM('{LIB}')"), "")?;
        tn.close()?;
        let got = printed(&spooled(s, "QSYSPRT")?);
        let want = expected_lines(&kit("rpg/expected/CLI001.out")?);
        if got != want {
            let mut d = String::new();
            for i in 0..want.len().max(got.len()) {
                if want.get(i) != got.get(i) {
                    let _ = writeln!(d, "  line {}: want {:?} got {:?}", i + 1, want.get(i), got.get(i));
                }
            }
            return Err(format!("CLI001 printed differently from pub400:\n{d}"));
        }

        // The prestart jobs: waiting, then one serving a held connection.
        let idle = qsqsrvr(s)?;
        if !idle.iter().any(|l| l.contains("QSYSWRK") || l.contains(" PJ ")) || !idle.iter().any(|l| l.contains("PSRW")) {
            return Err(format!("no QSQSRVR prestart job waiting (PJ, PSRW):\n{}", idle.join("\n")));
        }
        let _ = s.command("STRSBS SBSD(QBATCH)", CMD);
        let sub = s.command(&format!("SBMJOB CMD(CALL PGM({LIB}/CLIWAIT)) JOB(CLIWAIT) JOBQ(QGPL/QBATCH)"), CMD)?;
        let until = Instant::now() + Duration::from_secs(40);
        let serving = loop {
            let rows = qsqsrvr(s)?;
            if let Some(r) = rows.iter().find(|l| l.contains("CNDW")) {
                break r.clone();
            }
            if Instant::now() >= until {
                return Err(format!("no QSQSRVR job serving CLIWAIT (CNDW) ({sub}):\n{}", rows.join("\n")));
            }
            std::thread::sleep(Duration::from_secs(1));
        };
        Ok(format!(
            "CLI001 compiled and called: its {} printed lines are pub400's; QSQSRVR waits as PJ/PSRW and serves CLIWAIT's connection: {}",
            got.len(),
            serving.split_whitespace().collect::<Vec<_>>().join(" ")
        ))
    }
}
