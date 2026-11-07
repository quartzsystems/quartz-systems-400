// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 139: the DRDA client between two QS/400 guests. After the main
//! session, guest A boots the suite's disk and guest B a copy of it,
//! each on a user-mode network of its own; B reaches A's DDM server
//! through the host (`10.0.2.2`, the port QEMU forwards to A's 446).
//! Two 1 GiB guests at once.

use std::time::Duration;

use super::{LIB, Tn, local_rdb, loop_rdb, put, setup, sql, sql_has};
use crate::phase5::SECOFR_PASSWORD;
use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, StepReport, Suite, free_port};

/// The step's name.
const NAME: &str = "139 SQL DRDA between two guests";

/// The entry guest B names guest A by.
const RDB: &str = "GUESTA";

/// B's program: CONNECT to A, a row inserted there and counted, the
/// outcome kept in a table of B's own after CONNECT RESET.
const TWOCLI: &str = "**FREE
ctl-opt dftactgrp(*no);
dcl-s n int(10);
dcl-s w char(10);
dcl-s c1 int(10);
dcl-s c2 int(10);
dcl-s c3 int(10);
dcl-s c4 int(10);
exec sql set option commit = *none;
exec sql connect to GUESTA;
c1 = sqlcode;
exec sql insert into sqlq/twot values (139, 'FROM B');
c2 = sqlcode;
exec sql select count(*), max(w) into :n, :w from sqlq/twot;
c3 = sqlcode;
exec sql connect reset;
c4 = sqlcode;
exec sql insert into sqlq/twores values (:c1, :c2, :c3, :n, :w, :c4);
*inlr = *on;
";

/// Wait for a guest's IPL and service console.
fn booted(s: &mut Session, limit: Duration) -> Result<(), String> {
    s.expect(crate::steps::SRC_IPL_COMPLETE, limit)?;
    s.expect("Service console ready", limit)?;
    s.expect(crate::PROMPT, CMD)?;
    Ok(())
}

impl Suite {
    /// Step 139 in a pair of guests of its own (with `--only` when
    /// named).
    pub(crate) fn two_guest_steps(&self) -> Result<Vec<StepReport>, crate::HarnessError> {
        if let Some(only) = &self.only
            && !only.iter().any(|p| NAME.starts_with(p.as_str()))
        {
            return Ok(Vec::new());
        }
        // B's disk: a copy of A's, taken while neither runs.
        if let Some(image) = &self.launch.image {
            std::fs::copy(image, self.out_dir.join("guest-b.img"))?;
        }
        let mut a = Session::spawn(&self.launch)?;
        let r = self.step(&mut a, NAME, Self::sql_two_guests)?;
        a.command("pwrdwn", CMD).ok();
        a.wait_exit(Duration::from_secs(20));
        Ok(vec![r])
    }

    /// Step 139: guest A (this session) serves; guest B, booted here
    /// from a copy of A's disk, runs `TWOCLI` against it.
    fn sql_two_guests(&self, a: &mut Session) -> Result<String, String> {
        let limit = 4 * self.boot_timeout;
        let copy = self.out_dir.join("guest-b.img");
        if !copy.is_file() {
            return Err("no copy of the disk for guest B".into());
        }
        booted(a, limit)?;
        let telnet = free_port();
        let launch = Launch {
            image: Some(copy),
            qmp: self.out_dir.join("qmp-b.sock"),
            telnet_port: Some(telnet),
            host_ports: crate::hostsvr::forwards(Some(telnet)),
            ..self.launch.clone()
        };
        let mut b = Session::spawn(&launch).map_err(|e| format!("guest B: {e}"))?;
        let r = self.two_guests(a, &mut b, telnet, limit);
        b.command("pwrdwn", CMD).ok();
        b.wait_exit(Duration::from_secs(20));
        r
    }

    fn two_guests(
        &self,
        a: &mut Session,
        b: &mut Session,
        b_telnet: u16,
        limit: Duration,
    ) -> Result<String, String> {
        // A: its address, the DDM server, the table.
        crate::tcp::guest_address(a)?;
        setup(a)?;
        loop_rdb(a)?;
        let a_rdb = local_rdb(a)?;
        let ddm = self
            .launch
            .host_ports
            .iter()
            .find(|(_, g)| *g == 446)
            .map(|(h, _)| *h)
            .ok_or("guest A's port 446 is not forwarded")?;
        let _ = sql(a, &format!("DROP TABLE {LIB}/TWOT"))?;
        sql_has(
            a,
            &format!("CREATE TABLE {LIB}/TWOT (N INT, W CHAR(10))"),
            &["OK"],
        )?;
        sql_has(
            a,
            &format!("INSERT INTO {LIB}/TWOT VALUES (1, 'ON A'), (2, 'ON A')"),
            &["2 row(s)"],
        )?;

        // B: booted, its address, the directory entry for A.
        booted(b, limit).map_err(|e| format!("guest B: {e}"))?;
        crate::tcp::guest_address(b).map_err(|e| format!("guest B: {e}"))?;
        setup(b)?;
        let _ = b.command(&format!("RMVRDBDIRE RDB({RDB})"), CMD);
        let add = b.command(
            &format!("ADDRDBDIRE RDB({a_rdb} {RDB}) RMTLOCNAME('10.0.2.2' *IP) PORT({ddm}) TEXT('Guest A')"),
            CMD,
        )?;
        if add.trim_start().starts_with("CPF") {
            return Err(format!("guest B: ADDRDBDIRE: {add}"));
        }
        let _ = b.command("CHGSYSVAL SYSVAL(QRETSVRSEC) VALUE('1')", CMD);
        let _ = b.command(&format!("RMVSVRAUTE USRPRF(QSECOFR) SERVER({RDB})"), CMD);
        let aute = b.command(
            &format!("ADDSVRAUTE USRPRF(QSECOFR) SERVER({RDB}) USRID(QSECOFR) PASSWORD('{SECOFR_PASSWORD}')"),
            CMD,
        )?;
        if aute.trim_start().starts_with("CPF") {
            return Err(format!("guest B: ADDSVRAUTE: {aute}"));
        }
        let _ = sql(b, &format!("DROP TABLE {LIB}/TWORES"))?;
        sql_has(
            b,
            &format!(
                "CREATE TABLE {LIB}/TWORES (C1 INT, C2 INT, C3 INT, N INT, W CHAR(10), C4 INT)"
            ),
            &["OK"],
        )?;
        put(b, "QRPGLESRC", "TWOCLI", "SQLRPGLE", TWOCLI)?;
        let mut tn = Tn::open_at(b_telnet).map_err(|e| format!("guest B: {e}"))?;
        tn.cmd(
            &format!("CRTSQLRPGI OBJ({LIB}/TWOCLI) SRCFILE({LIB}/QRPGLESRC) COMMIT(*NONE)"),
            "Program TWOCLI created",
        )?;
        let shown = tn.cmd(&format!("CALL PGM({LIB}/TWOCLI)"), "")?;
        tn.close()?;

        // B kept what A answered; A holds B's row.
        let good = sql(
            b,
            &format!(
                "SELECT 'ALLGOOD' FROM {LIB}/TWORES WHERE C1 = 0 AND C2 = 0 AND C3 = 0 AND N = 3 AND W = 'ON A' AND C4 = 0"
            ),
        )?;
        if !good.contains("ALLGOOD") {
            let res = sql(b, &format!("SELECT C1, C2, C3, N, W, C4 FROM {LIB}/TWORES"))?;
            return Err(format!(
                "guest B's results (connect, insert, query SQLCODEs, the count 3 and max 'ON A', reset) are not as wanted:\n{res}\nthe call showed:\n{shown}"
            ));
        }
        sql_has(
            a,
            &format!("SELECT N, W FROM {LIB}/TWOT WHERE N = 139"),
            &["FROM B", "1 row(s)"],
        )?;
        let jobs = a.command("WRKACTJOB", CMD)?;
        let served = jobs.lines().filter(|l| l.contains("QRWTSRVR")).count();
        Ok(format!(
            "guest B's TWOCLI connected to guest A ({a_rdb} as {RDB}, 10.0.2.2:{ddm}), inserted a row there and counted 3, each SQLCODE 0; A holds B's row; {served} QRWTSRVR jobs on A"
        ))
    }
}
