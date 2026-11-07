// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 130: STRSQL at a TN5250 display.

use super::{LIB, Tn, local_rdb, loop_rdb, setup, sql, sql_has};
use crate::qemu::session::Session;
use crate::steps::Suite;

/// One statement typed on Enter SQL Statements and run.
pub(super) fn stmt(tn: &mut Tn, text: &str) -> Result<String, String> {
    tn.run(&format!(
        "key eraseinput\npick \"===>\" {text}\nkey enter\nsettle 1500\nunlock\n"
    ))
    .map_err(|e| format!("{text}: {e}"))?;
    Ok(tn.screen())
}

/// A query: Display Data, the screen.
fn query(tn: &mut Tn, text: &str) -> Result<String, String> {
    tn.run(&format!(
        "key eraseinput\npick \"===>\" {text}\nkey enter\nwait \"Display Data\"\nsettle 800\n"
    ))
    .map_err(|e| format!("{text}: {e}"))?;
    Ok(tn.screen())
}

/// The data row of the screen whose text holds `word`.
fn row_of<'a>(screen: &'a str, word: &str) -> Option<&'a str> {
    screen.lines().find(|l| l.contains(word))
}

/// Row 4, the ruler of Display Data.
fn ruler(screen: &str) -> String {
    screen.lines().nth(3).unwrap_or("").to_string()
}

impl Suite {
    /// Step 130: STRSQL. A table made and filled; Display Data paged
    /// and shifted (F20, F19); F13 option 1 changes the date format and
    /// the next query shows it; F4 on INSERT builds a statement that
    /// runs; CONNECT to an *RDB entry for this system and back; option 1
    /// saves the session and STRSQL resumes it (SQL6102).
    #[allow(clippy::too_many_lines)] // one script, in order
    pub(crate) fn sql_strsql(&self, s: &mut Session) -> Result<String, String> {
        setup(s)?;
        loop_rdb(s)?;
        let local = local_rdb(s)?;
        let _ = sql(s, &format!("DROP TABLE {LIB}/STRT"))?;
        let mut tn = Tn::open(self)?;
        tn.run("key eraseinput\ntype strsql commit(*none) naming(*sys)\nkey enter\nwait \"Enter SQL Statements\"\nsettle 1000\n")?;
        let lib = LIB.to_ascii_lowercase();
        stmt(
            &mut tn,
            &format!(
                "create table {lib}/strt (id int not null, name char(10), d date, note char(60) not null default 'note')"
            ),
        )?;
        stmt(
            &mut tn,
            &format!(
                "insert into {lib}/strt (id, name, d) values (1, 'ANN', '2026-01-15'), (2, 'BOB', '2026-02-03'), (3, 'CID', '2026-03-04')"
            ),
        )?;
        for _ in 0..4 {
            stmt(
                &mut tn,
                &format!(
                    "insert into {lib}/strt (id, name, d) select id + (select count(*) from {lib}/strt), name, d from {lib}/strt"
                ),
            )?;
        }
        sql_has(s, &format!("SELECT COUNT(*) FROM {LIB}/STRT"), &["48"])?;

        // Display Data: three pages, shifted right and back.
        let select = format!("select id, name, d, note from {lib}/strt order by id");
        let p1 = query(&mut tn, &select)?;
        let ann = row_of(&p1, "ANN")
            .ok_or_else(|| format!("no ANN row:\n{p1}"))?
            .to_string();
        if ann.contains("2026-01-15") {
            return Err(format!("the job's date format is already *ISO:\n{p1}"));
        }
        tn.run("key pagedown\nsettle 1000\n")?;
        let p2 = tn.screen();
        if p2 == p1 {
            return Err(format!("Page Down did not move:\n{p2}"));
        }
        let mut end = false;
        for _ in 0..4 {
            if tn.screen().contains("End of data") {
                end = true;
                break;
            }
            tn.run("key pagedown\nsettle 1000\n")?;
        }
        if !end {
            return Err(format!("no End of data after paging:\n{}", tn.screen()));
        }
        let before = ruler(&tn.screen());
        tn.run("key f20\nsettle 1000\n")?;
        let right = ruler(&tn.screen());
        if right == before {
            return Err(format!("F20 did not shift the data:\n{}", tn.screen()));
        }
        tn.run("key f19\nsettle 1000\n")?;
        if ruler(&tn.screen()) != before {
            return Err(format!("F19 did not shift back:\n{}", tn.screen()));
        }
        tn.run("key f3\nwait \"Enter SQL Statements\"\n")?;

        // F13 option 1: the date format.
        tn.run(
            "key eraseinput\nkey f13\nwait \"Interactive SQL Session Services\"\ntype 1\nkey enter\nwait \"Change Session Attributes\"\npick \"Date format\" *ISO\nkey enter\nsettle 1500\nseek \"Interactive SQL Session Services\" 3 enter\nkey f12\nwait \"Enter SQL Statements\"\n",
        )?;
        let iso = query(&mut tn, &select)?;
        let ann_iso = row_of(&iso, "ANN").unwrap_or("").to_string();
        if !ann_iso.contains("2026-01-15") {
            return Err(format!("*ISO is not applied (before: {ann:?}):\n{iso}"));
        }
        tn.run("key f3\nwait \"Enter SQL Statements\"\n")?;

        // F4 on INSERT.
        tn.run(&format!(
            "key eraseinput\npick \"===>\" insert\nkey f4\nwait \"Specify INSERT Statement\"\ncursor 5 32\ntype strt\ncursor 6 34\ntype {lib}\nkey enter\nwait \" NOTE\"\npick \"re:^ ID\\b\" 100\npick \"re:^ NAME\\b\" 'ZED'\npick \"re:^ D\\b\" '2026-12-31'\npick \"re:^ NOTE\\b\" 'prompted'\nkey enter\nwait \"Enter SQL Statements\"\nsettle 800\n"
        ))?;
        let built = tn.screen();
        if !built.to_ascii_uppercase().contains("INSERT INTO") {
            return Err(format!(
                "the prompt left no INSERT on the input lines:\n{built}"
            ));
        }
        tn.run("key enter\nsettle 1500\nunlock\n")?;
        sql_has(
            s,
            &format!("SELECT NAME, NOTE FROM {LIB}/STRT WHERE ID = 100"),
            &["ZED", "prompted"],
        )?;

        // CONNECT to this system again, a change there, and back.
        let at = stmt(&mut tn, "connect to loop")?;
        if !at.contains("relational database LOOP") {
            return Err(format!("CONNECT TO LOOP:\n{at}"));
        }
        stmt(
            &mut tn,
            &format!("insert into {lib}/strt (id, name, d) values (200, 'REMOTE', '2026-05-05')"),
        )?;
        let back = stmt(&mut tn, &format!("connect to {local}"))?;
        if !back.contains(&format!("relational database {local}")) {
            return Err(format!("CONNECT TO {local}:\n{back}"));
        }
        sql_has(
            s,
            &format!("SELECT NAME FROM {LIB}/STRT WHERE ID = 200"),
            &["REMOTE"],
        )?;

        // Exit saving the session; STRSQL resumes it. Reported, not
        // required, until it has passed on a booted system.
        tn.run(
            "key eraseinput\nkey f3\nwait \"Exit Interactive SQL\"\ncursor 5 31\nkey eraseeof\ntype 1\nkey enter\nsettle 2000\nunlock\n",
        )?;
        let saved = if tn.screen().contains("Command Entry") {
            resume(&mut tn)
        } else {
            let why = tn.screen().lines().last().unwrap_or("").trim().to_string();
            tn.run("cursor 5 31\nkey eraseeof\ntype 2\nkey enter\nwait \"Command Entry\"\n")?;
            format!("open: option 1 did not save the session ({why})")
        };
        tn.close()?;
        Ok(format!(
            "48 rows made in STRSQL; Display Data paged to its end, F20 and F19; *ISO from F13 option 1 shown ({}); F4 on INSERT ran; CONNECT TO LOOP, a change there, back to {local}; {saved}",
            ann_iso
                .split_whitespace()
                .find(|w| w.contains("2026"))
                .unwrap_or("")
        ))
    }
}

/// STRSQL again after a save: the statements back and SQL6102; left
/// with option 2. What happened, for the report.
pub(super) fn resume(tn: &mut Tn) -> String {
    let run = |tn: &mut Tn| -> Result<String, String> {
        tn.run(
            "key eraseinput\ntype strsql\nkey enter\nwait \"Enter SQL Statements\"\nsettle 1000\n",
        )?;
        let resumed = tn.screen();
        // SQL6102 is a line of the history: F1 on it.
        let rows = tn.d.term.text_rows();
        let at = rows
            .iter()
            .rposition(|r| r.contains("saved when STRSQL"))
            .map(|i| (i + 1, rows[i].len() - rows[i].trim_start().len() + 1));
        let id = match at {
            Some((r, c)) => tn.msgid_at(r, c)?,
            None => "no message".into(),
        };
        Ok(match (resumed.contains("connect to"), id == "SQL6102") {
            (true, true) => "saved and resumed with SQL6102".into(),
            (has, _) => format!("open: the resumed session showed {id}, statements back: {has}"),
        })
    };
    let r = run(tn)
        .unwrap_or_else(|e| format!("open: resuming failed: {}", e.lines().next().unwrap_or("")));
    // Back to Enter SQL Statements whatever happened, then out unsaved.
    for _ in 0..3 {
        if tn.screen().contains("Enter SQL Statements") && !tn.screen().contains("Help") {
            break;
        }
        let _ = tn.run("key f12\nsettle 800\n");
    }
    let _ = tn.run(
        "key eraseinput\nkey f3\nwait \"Exit Interactive SQL\"\ncursor 5 31\nkey eraseeof\ntype 2\nkey enter\nwait \"Command Entry\"\n",
    );
    r
}
