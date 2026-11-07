// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 89 and 90: the QAP server (docs/QAP.md) from the host, over
//! TLS 1.3 on the guest's 2320, with a certificate the step assigns to
//! `QS400_ACCESS_SERVER` and pins.
//!
//! Step 89: `Hello` (the newest version), `JobInfo` before and after `SignOn` as
//! `QAPTEST`, `SystemInfo`, `RunCommand`, `CheckCommand`,
//! `DescribeCommand`; a job log listed and read as SCS (whole, page 1,
//! from page 2 on) and as text; a second connection attached, its own
//! job and library list; `Sql`: options, `CREATE SCHEMA` and `TABLE`, a
//! multi-row `INSERT` failing on its second row, `Prepare`/`Execute`/
//! `Fetch`, the catalog lists, commit and rollback on a journaled table,
//! `SQL7008` on an unjournaled one, a long query cancelled; `SignOff` on
//! one connection ends the other with `SessionEnded`, and `ENDTCPSVR`
//! ends the last with `ServerEnding`.
//!
//! Step 90, the lanes end to end: no statement holds up another job.
//! While an aggregate over `EXISTS` runs on one connection of the
//! session and a slow select on a second, the third's command, catalog
//! lists and query answer within a bound, and the earlier statement is
//! cancelled while the later still runs (`docs/STORAGE_SERVICE.md` "Long
//! statements"). Then the aggregate again under `RepeatableRead`: the
//! third's command, catalog lists and query answer as before, and a
//! change of a row the query read waits for it on the second, until its
//! unit of work ends. Then an `UPDATE` whose condition has subqueries
//! and a `CREATE INDEX` over a large table, each on one connection while
//! the third's command, catalog lists and a read of the table being
//! indexed answer; then a `CPYF` of that table (a command that takes
//! turns) while they answer again.
//!
//! Step 92, the `Ifs` service as `QAPTEST`: see `ifs.rs`.
//!
//! Step 123, the `Manage` service as `QAPTEST` and `QSECOFR`: see
//! `manage.rs`.
//!
//! Step 93, the `Messages` service and the spool watches as `QAPTEST`
//! and `QAPTEST2`: see `messages.rs`.

mod client;
mod ifs;
mod manage;
mod messages;
mod sql;

use std::time::{Duration, Instant};

use qap_proto::ops::command::{
    CheckReply, CommandDefinition, CommandReply, CommandText, DescribeCommand,
};
use qap_proto::ops::spool::{
    ListSpooledFiles, ReadFormat, ReadSpooledFile, SpooledData, SpooledFile, SpooledFiles,
};
use qap_proto::ops::sql::{
    Columns, CommitLevel, CursorRef, Executed, Fetch, Fetched, ListColumns, ListSchemas,
    ListTables, Schemas, SqlFailure, SqlValue, StatementRef, TableKind, Tables,
};
use qap_proto::ops::system::SystemInfo;
use qap_proto::types::ObjectName;
use qap_proto::{Op, Service, Status};

use crate::httpadm::secofr_password;
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};
use crate::tcp::{GUEST, forward_port};
use client::{Conn, brief, msgid};

/// The profile the steps sign on as.
const USER: &str = "QAPTEST";
const PASSWORD: &str = "Qap4Step";
/// The certificate's label and name.
const LABEL: &str = "QAP89";
const CN: &str = "qs400.step89.test";
/// Certificate commands derive keys: slow without acceleration.
const SLOW: Duration = Duration::from_secs(120);

/// A console command whose output holds one of `want`.
fn expect(s: &mut Session, line: &str, want: &[&str]) -> Result<String, String> {
    let out = s.command(line, SLOW)?;
    if want.iter().any(|w| out.contains(w)) {
        Ok(out)
    } else {
        Err(format!("{line}: none of {want:?} in:\n{out}"))
    }
}

/// The server's host port and pinned certificate.
struct Server {
    port: u16,
    pin: Vec<u8>,
}

impl Server {
    /// A new connection, hello'd at the newest version.
    fn open(&mut self) -> Result<Conn, String> {
        let (mut c, der) = client::connect(self.port, CN, &self.pin)?;
        if self.pin.is_empty() {
            self.pin = der;
        }
        let h = c.hello()?;
        if h.version != qap_proto::VERSION {
            return Err(format!("Hello chose version {}", h.version));
        }
        Ok(c)
    }
}

/// The profile, the certificate and `*QAP` started; the forwarded port.
/// Each step can run alone, so whatever an earlier run made is fine.
fn setup(suite: &Suite, s: &mut Session) -> Result<Server, String> {
    secofr_password(s)?;
    expect(
        s,
        &format!("CRTUSRPRF USRPRF({USER}) PASSWORD('{PASSWORD}') PWDEXP(*NO) TEXT('QAP step')"),
        &["QAPTEST created", "CPF2214", "exists"],
    )?;
    expect(
        s,
        "QSCRTCRTS KEYSTORE(*SYSTEM) PASSWORD('step89pw')",
        &["TCP1C11", "TCP1C10"],
    )?;
    expect(
        s,
        &format!("QSCRTCRT LABEL({LABEL}) CN('{CN}') VALIDITY(365) TEXT('Step 89')"),
        &["TCP1C02", "TCP1C14", "TCP1C03"],
    )?;
    expect(
        s,
        &format!("QSASNCRT APPID(QS400_ACCESS_SERVER) LABEL({LABEL})"),
        &["TCP1C08"],
    )?;
    expect(s, "STRTCPSVR SERVER(*QAP)", &["TCP1A07", "TCP1A08"])?;
    let port = forward_port(suite, GUEST, qap_proto::DEFAULT_PORT)?;
    Ok(Server {
        port,
        pin: Vec::new(),
    })
}

/// A failure with the history log's last lines, which say how a
/// connection job ended.
fn with_history(s: &mut Session, e: &str) -> String {
    let log = s.command("DSPLOG", CMD).unwrap_or_default();
    let tail: Vec<&str> = log
        .lines()
        .rev()
        .take(25)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{e}\nDSPLOG (last lines):\n{}", tail.join("\n"))
}

/// After `ENDTCPSVR` (its `ServerEnding` already seen) the listener is
/// gone: a new connection gets no TLS session. The forwarded host port
/// accepts and then closes, so the guest's refusal shows as that.
/// `STRTCPSVR` then takes connections again; it ends the server after.
fn refused_then_restarted(s: &mut Session, srv: &mut Server) -> Result<String, String> {
    if client::connect_for(srv.port, CN, &srv.pin, Duration::ZERO).is_ok() {
        return Err("a connection got a TLS session after ENDTCPSVR".into());
    }
    // STRTCPSVR says TCP1A08 while the ended QAPD is still finishing.
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        let out = expect(s, "STRTCPSVR SERVER(*QAP)", &["TCP1A07", "TCP1A08"])?;
        if out.contains("TCP1A07") {
            break;
        }
        if Instant::now() > until {
            return Err(format!("STRTCPSVR after ENDTCPSVR:\n{out}"));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    let mut c = srv.open()?;
    let ping = c.call(Op::Ping, &())?;
    if ping.status != Status::Ok {
        return Err(format!("Ping after STRTCPSVR: {}", brief(&ping)));
    }
    expect(s, "ENDTCPSVR SERVER(*QAP)", &["TCP1A0A"])?;
    Ok("a connection after ENDTCPSVR was refused and STRTCPSVR took one again".into())
}

/// Run a command; its reply's messages' IDs, which must be `Ok`.
fn command(c: &mut Conn, line: &str) -> Result<Vec<String>, String> {
    let r = c.call(
        Op::RunCommand,
        &CommandText {
            command: line.into(),
        },
    )?;
    if r.status != Status::Ok {
        return Err(format!("RunCommand {line}: {}", brief(&r)));
    }
    let body: CommandReply = r.body_as().map_err(|e| e.to_string())?;
    Ok(body.messages.into_iter().map(|m| m.id).collect())
}

/// Step 123's `*PUBLIC *EXCLUDE` objects, made by the security
/// officer: a program in `QGPL` and a library with a data area in it.
fn excluded(s: &mut Session) -> Result<(), String> {
    let _ = s.command("CRTSRCPF FILE(QGPL/QCLSRC)", CMD);
    crate::qir::qemu::push(s, "QAPXSRC", "PGM\nENDPGM\n")?;
    expect(
        s,
        "svc.srcput QGPL/QCLSRC QAPXPGM SRCTYPE(CLP) FROMSTMF(QAPXSRC)",
        &["written"],
    )?;
    let _ = s.command("DLTPGM PGM(QGPL/QAPXPGM)", CMD);
    expect(
        s,
        "CRTCLPGM PGM(QGPL/QAPXPGM) SRCFILE(QGPL/QCLSRC) SRCMBR(QAPXPGM)",
        &["QAPXPGM"],
    )?;
    // A CL program whose CALL of QAPXPGM must end in CPF0001, which it
    // monitors; had the call run, it ends in an escape.
    crate::qir::qemu::push(
        s,
        "QAPXCLS",
        "PGM\nCALL PGM(QGPL/QAPXPGM)\nMONMSG MSGID(CPF0001) EXEC(RETURN)\nSNDPGMMSG MSGID(CPF9898) MSGF(QCPFMSG) MSGDTA('QAPXPGM ran') MSGTYPE(*ESCAPE)\nENDPGM\n",
    )?;
    expect(
        s,
        "svc.srcput QGPL/QCLSRC QAPXCL SRCTYPE(CLP) FROMSTMF(QAPXCLS)",
        &["written"],
    )?;
    let _ = s.command("DLTPGM PGM(QGPL/QAPXCL)", CMD);
    expect(
        s,
        "CRTCLPGM PGM(QGPL/QAPXCL) SRCFILE(QGPL/QCLSRC) SRCMBR(QAPXCL)",
        &["QAPXCL"],
    )?;
    // *USE without *EXECUTE: *OBJOPR *READ.
    let _ = s.command("DLTPGM PGM(QGPL/QAPXNOX)", CMD);
    expect(
        s,
        "CRTCLPGM PGM(QGPL/QAPXNOX) SRCFILE(QGPL/QCLSRC) SRCMBR(QAPXPGM)",
        &["QAPXNOX"],
    )?;
    let _ = s.command("CRTLIB LIB(QAPXLIB)", CMD);
    let _ = s.command("CRTDTAARA DTAARA(QAPXLIB/QAPXDTA) TYPE(*CHAR) LEN(10)", CMD);
    for line in [
        "RVKOBJAUT OBJ(QGPL/QAPXNOX) OBJTYPE(*PGM) USER(*PUBLIC) AUT(*ALL)",
        "GRTOBJAUT OBJ(QGPL/QAPXNOX) OBJTYPE(*PGM) USER(*PUBLIC) AUT(*OBJOPR *READ)",
        "GRTOBJAUT OBJ(QGPL/QAPXPGM) OBJTYPE(*PGM) USER(*PUBLIC) AUT(*EXCLUDE)",
        "GRTOBJAUT OBJ(QSYS/QAPXLIB) OBJTYPE(*LIB) USER(*PUBLIC) AUT(*EXCLUDE)",
        "CHKOBJ OBJ(QAPXLIB/QAPXDTA) OBJTYPE(*DTAARA)",
    ] {
        let out = s.command(line, CMD)?;
        if out.contains("CPF") || out.contains("CPD") {
            return Err(format!("{line}:\n{out}"));
        }
    }
    Ok(())
}

/// Run a command that must fail; the status and message ID.
fn command_fails(c: &mut Conn, line: &str) -> Result<(Status, String), String> {
    let r = c.call(
        Op::RunCommand,
        &CommandText {
            command: line.into(),
        },
    )?;
    if r.status == Status::Ok {
        return Err(format!("RunCommand {line} succeeded"));
    }
    Ok((r.status, msgid(&r).to_string()))
}

#[allow(clippy::unused_self)] // the steps share one signature
impl Suite {
    /// Step 89: the QAP server.
    pub(crate) fn qap_server(&self, s: &mut Session) -> Result<String, String> {
        self.qap_server_run(s).map_err(|e| with_history(s, &e))
    }

    fn qap_server_run(&self, s: &mut Session) -> Result<String, String> {
        let mut srv = setup(self, s)?;
        let mut a = srv.open()?;
        let before = a.job_info()?;
        if before.user != "QUSER" || before.job.name != "QAPCNN" {
            return Err(format!("JobInfo before sign-on: {before:?}"));
        }
        let session = a.sign_on(USER, PASSWORD)?;
        let job_a = a.job_info()?;
        if job_a.user != USER || job_a.job.number != before.job.number {
            return Err(format!("JobInfo after sign-on: {job_a:?}"));
        }
        let info: SystemInfo = a.ok(Op::SystemInfo, &())?;
        let os = info
            .products
            .iter()
            .filter(|p| p.id == "QS00SS1" && p.option == "*BASE")
            .count();
        if info.system_name.is_empty() || os != 1 {
            return Err(format!("SystemInfo: {info:?}"));
        }
        let cmds = commands(&mut a).map_err(|e| format!("commands: {e}"))?;
        let log = job_log(s, &mut a).map_err(|e| format!("job_log: {e}"))?;
        // A second connection of the session: its own job and library list.
        let mut b = srv.open()?;
        let attached = b.attach(&session.token)?;
        let job_b = b.job_info()?;
        if attached.profile != USER || job_b.user != USER || job_b.job.number == job_a.job.number {
            return Err(format!(
                "Attach: {attached:?}; JobInfo {job_b:?} beside {job_a:?}"
            ));
        }
        let libl = library_lists(&mut a, &mut b).map_err(|e| format!("library_lists: {e}"))?;
        let sql = sql_service(&mut a).map_err(|e| format!("sql_service: {e}"))?;
        let cancelled = cancel_long(&mut a).map_err(|e| format!("cancel_long: {e}"))?;
        let kinds = value_kinds(&mut a).map_err(|e| format!("value_kinds: {e}"))?;
        // SignOff on A: A stays, not signed on; B is ended.
        let off = a.call(Op::SignOff, &())?;
        if off.status != Status::Ok {
            return Err(format!("SignOff: {}", brief(&off)));
        }
        let ended = b.event(Duration::from_secs(10))?;
        if (ended.service, ended.op, ended.status)
            != (
                Service::SignOn,
                qap_proto::ops::event::SESSION_ENDED,
                Status::Ok,
            )
            || msgid(&ended) != "TCP1A58"
        {
            return Err(format!(
                "B's event after A's SignOff: {:?} {}",
                ended.service,
                brief(&ended)
            ));
        }
        if !b.closes(Duration::from_secs(10)) {
            return Err("B stayed open after SessionEnded".into());
        }
        let after = a.call(Op::SystemInfo, &())?;
        if after.status != Status::NotSignedOn {
            return Err(format!("SystemInfo after SignOff: {}", brief(&after)));
        }
        // ENDTCPSVR: A, still open, gets ServerEnding.
        expect(s, "ENDTCPSVR SERVER(*QAP)", &["TCP1A0A"])?;
        let ending = a.event(Duration::from_secs(20))?;
        if (ending.service, ending.status) != (Service::Connection, Status::ServerEnding) {
            return Err(format!(
                "A's event at ENDTCPSVR: {:?} {}",
                ending.service,
                brief(&ending)
            ));
        }
        let restarted = refused_then_restarted(s, &mut srv)?;
        Ok(format!(
            "TLS on 2320 (host {}), jobs {} and {} as {USER}; {cmds}; {log}; {libl}; {sql}; {cancelled}; {kinds}; SignOff ended the attached connection (TCP1A58), ENDTCPSVR sent ServerEnding; {restarted}",
            srv.port, job_a.job.number, job_b.job.number
        ))
    }

    /// Step 91: a table `*PUBLIC *EXCLUDE` is `NotAuthorized` (`SQL0551`)
    /// to `USER`'s `SELECT` and `ListColumns`. Fails today: the engine's
    /// scans do not check the file's authority (only DDL does).
    pub(crate) fn qap_sql_authority(&self, s: &mut Session) -> Result<String, String> {
        let mut srv = setup(self, s)?;
        let mut a = srv.open()?;
        a.sign_on(USER, PASSWORD)?;
        let r = not_authorized(&mut srv, &mut a).map_err(|e| format!("SQL AUTHORITY: {e}"));
        let _ = a.call(Op::SignOff, &());
        expect(s, "ENDTCPSVR SERVER(*QAP)", &["TCP1A0A"])?;
        r
    }

    /// Step 92: the `Ifs` service through the real job (`ifs.rs`).
    pub(crate) fn qap_ifs(&self, s: &mut Session) -> Result<String, String> {
        // A directory USER may not use, made by the security officer.
        s.command(&format!("MKDIR DIR('{}')", ifs::EXCLUDED), CMD)?;
        s.command(
            &format!(
                "CHGAUT OBJ('{}') USER(*PUBLIC) DTAAUT(*EXCLUDE) OBJAUT(*NONE)",
                ifs::EXCLUDED
            ),
            CMD,
        )?;
        let mut srv = setup(self, s)?;
        let mut a = srv.open()?;
        a.sign_on(USER, PASSWORD)?;
        let r = ifs::run(&mut a).map_err(|e| format!("IFS: {e}"));
        let _ = a.call(Op::SignOff, &());
        expect(s, "ENDTCPSVR SERVER(*QAP)", &["TCP1A0A"])?;
        s.command(&format!("RMDIR DIR('{}')", ifs::EXCLUDED), CMD)?;
        r.map_err(|e| with_history(s, &e))
    }

    /// Step 93: `Messages` and the spool watches through the real jobs
    /// (`messages.rs`).
    pub(crate) fn qap_messages(&self, s: &mut Session) -> Result<String, String> {
        let mut srv = setup(self, s)?;
        expect(
            s,
            &format!(
                "CRTUSRPRF USRPRF({}) PASSWORD('{}') PWDEXP(*NO) TEXT('QAP step 93')",
                messages::USER2,
                messages::PASSWORD2
            ),
            &["QAPTEST2 created", "CPF2214", "exists"],
        )?;
        let mut a = srv.open()?;
        a.sign_on(USER, PASSWORD)?;
        let mut b = srv.open()?;
        b.sign_on(messages::USER2, messages::PASSWORD2)?;
        let r = messages::run(s, &mut a, &mut b).map_err(|e| format!("MESSAGES: {e}"));
        let _ = a.call(Op::SignOff, &());
        let _ = b.call(Op::SignOff, &());
        expect(s, "ENDTCPSVR SERVER(*QAP)", &["TCP1A0A"])?;
        r.map_err(|e| with_history(s, &e))
    }

    /// Step 123: `Manage` through the real jobs (`manage.rs`).
    pub(crate) fn qap_manage(&self, s: &mut Session) -> Result<String, String> {
        let mut srv = setup(self, s)?;
        excluded(s)?;
        let mut a = srv.open()?;
        a.sign_on(USER, PASSWORD)?;
        let mut sec = srv.open()?;
        sec.sign_on("QSECOFR", crate::phase5::SECOFR_PASSWORD)?;
        let r = manage::run(&mut a, &mut sec)
            .and_then(|m| Ok(format!("{m}; {}", manage::exclusions(&mut a, &mut sec)?)))
            .map_err(|e| format!("MANAGE: {e}"));
        let _ = a.call(Op::SignOff, &());
        let _ = sec.call(Op::SignOff, &());
        expect(s, "ENDTCPSVR SERVER(*QAP)", &["TCP1A0A"])?;
        r.map_err(|e| with_history(s, &e))
    }

    /// Step 90: no statement holds up another job. While an aggregate
    /// over `EXISTS` runs on one connection and a slow select on a
    /// second, a third's command, catalog lists and query answer
    /// within a bound; the earlier statement is cancelled while the
    /// later one still runs.
    pub(crate) fn qap_lanes(&self, s: &mut Session) -> Result<String, String> {
        let mut srv = setup(self, s)?;
        let mut a = srv.open()?;
        let session = a.sign_on(USER, PASSWORD)?;
        sql::big_table(&mut a, "QAPLANE", 1000)?;
        let mut b = srv.open()?;
        b.attach(&session.token)?;
        let mut third = srv.open()?;
        third.attach(&session.token)?;
        let r = lanes(&mut a, &mut b, &mut third)
            .and_then(|l| Ok(format!("{l}; {}", repeatable(&mut a, &mut b, &mut third)?)))
            .and_then(|l| {
                Ok(format!(
                    "{l}; {}",
                    long_changes(&mut a, &mut b, &mut third)?
                ))
            });
        let _ = a.call(Op::SignOff, &());
        expect(s, "ENDTCPSVR SERVER(*QAP)", &["TCP1A0A"])?;
        r
    }
}

/// The longest any request of step 90 may wait for another job's
/// statement: generous for an emulated guest, short of the statements.
const LANE_BOUND: Duration = Duration::from_secs(10);

/// One request on `c`, answered `Ok` within [`LANE_BOUND`]; how long.
fn within<B: ipc::Wire>(c: &mut Conn, what: &str, op: Op, body: &B) -> Result<Duration, String> {
    let at = Instant::now();
    let id = c.request(op, body)?;
    match c.poll_reply(id, LANE_BOUND)? {
        Some(r) if r.status == Status::Ok => Ok(at.elapsed()),
        Some(r) => Err(format!("{what}: {}", brief(&r))),
        None => Err(format!(
            "LANES: {what} got no reply in {} s while the long statements ran",
            LANE_BOUND.as_secs()
        )),
    }
}

/// Step 90's body: the two long statements, the third connection's
/// requests, then the earlier statement cancelled under the later.
fn lanes(a: &mut Conn, b: &mut Conn, c: &mut Conn) -> Result<String, String> {
    let (agg, agg_op) = sql::start_long(a, "QAPLANE")?;
    sql::still_running(a, agg, Duration::from_secs(1))?;
    let (slow, slow_op) = sql::start_query(b, &sql::slow_select("QAPLANE"))?;
    sql::still_running(b, slow, Duration::from_secs(1))?;
    let mut took = Vec::new();
    let check = CommandText {
        command: "CHKOBJ OBJ(QSYS/QGPL) OBJTYPE(*LIB)".into(),
    };
    took.push(within(c, "CHKOBJ", Op::RunCommand, &check)?);
    let schemas = ListSchemas {
        pattern: "QAP%".into(),
        start: 0,
        max: 0,
    };
    took.push(within(c, "ListSchemas", Op::ListSchemas, &schemas)?);
    let tables = ListTables {
        schema: "QAPLANE".into(),
        pattern: "%".into(),
        kinds: Vec::new(),
        start: 0,
        max: 0,
    };
    took.push(within(c, "ListTables", Op::ListTables, &tables)?);
    let columns = ListColumns {
        schema: "QAPLANE".into(),
        table: "BIG".into(),
    };
    took.push(within(c, "ListColumns", Op::ListColumns, &columns)?);
    let at = Instant::now();
    let n = sql::count(c, "SELECT COUNT(*) FROM QAPLANE.BIG")?;
    took.push(at.elapsed());
    if n != 1000 || took.iter().any(|t| *t > LANE_BOUND) {
        return Err(format!(
            "the third connection's query: {n} rows in {:?}",
            at.elapsed()
        ));
    }
    if a.poll_reply(agg, Duration::ZERO)?.is_some() || b.poll_reply(slow, Duration::ZERO)?.is_some()
    {
        return Err("a long statement ended before its cancel".into());
    }
    // The earlier statement ends under the later one, which goes on.
    let (_, stop_agg) = sql::cancel(a, agg)?;
    if stop_agg > LANE_BOUND {
        return Err(format!(
            "the aggregate's cancel took {stop_agg:?} under the later select"
        ));
    }
    sql::still_running(b, slow, Duration::ZERO)?;
    let (_, stop_slow) = sql::cancel(b, slow)?;
    let most = took.iter().max().copied().unwrap_or_default();
    Ok(format!(
        "while the aggregate's {agg_op:?} and the slow select's {slow_op:?} ran on two connections, the third's CHKOBJ, ListSchemas, ListTables, ListColumns and a query answered in at most {} ms; the aggregate cancelled in {} ms with the select still running, the select in {} ms",
        most.as_millis(),
        stop_agg.as_millis(),
        stop_slow.as_millis()
    ))
}

/// The aggregate under `RepeatableRead` (`*ALL`): it takes turns too.
/// What it read is read-locked, so another job's change of it waits
/// (on `b`) until the unit of work ends, while a third's command, lists
/// and query answer.
fn repeatable(a: &mut Conn, b: &mut Conn, c: &mut Conn) -> Result<String, String> {
    sql::options(a, CommitLevel::RepeatableRead, None)?;
    let (agg, _) = sql::start_long(a, "QAPLANE")?;
    sql::still_running(a, agg, Duration::from_secs(1))?;
    let mut took = Vec::new();
    let check = CommandText {
        command: "CHKOBJ OBJ(QSYS/QGPL) OBJTYPE(*LIB)".into(),
    };
    took.push(within(c, "CHKOBJ", Op::RunCommand, &check)?);
    let tables = ListTables {
        schema: "QAPLANE".into(),
        pattern: "%".into(),
        kinds: Vec::new(),
        start: 0,
        max: 0,
    };
    took.push(within(c, "ListTables", Op::ListTables, &tables)?);
    let columns = ListColumns {
        schema: "QAPLANE".into(),
        table: "BIG".into(),
    };
    took.push(within(c, "ListColumns", Op::ListColumns, &columns)?);
    let at = Instant::now();
    let rows = sql::count(c, "SELECT COUNT(*) FROM QAPLANE.BIG")?;
    took.push(at.elapsed());
    if rows != 1000 || took.iter().any(|t| *t > LANE_BOUND) {
        return Err(format!(
            "under RepeatableRead, the third connection's query: {rows} rows in {:?}",
            at.elapsed()
        ));
    }
    // Row N = 0 is the first the aggregate read: a change of it waits
    // (the table's WAITRCD is 60 s) while others go on.
    let upd = sql::start_exec(b, "UPDATE QAPLANE.BIG SET N = 0 WHERE N = 0")?;
    if let Some(r) = b.poll_reply(upd, Duration::from_secs(2))? {
        return Err(format!(
            "another job's change of a row read under RepeatableRead did not wait: {}",
            sql::failure(&r)
        ));
    }
    let waiting = within(c, "CHKOBJ while a change waits", Op::RunCommand, &check)?;
    if a.poll_reply(agg, Duration::ZERO)?.is_some() {
        return Err("the RepeatableRead aggregate ended before its cancel".into());
    }
    let (_, stop) = sql::cancel(a, agg)?;
    // The unit of work ends: its read locks go, and the change runs.
    let at = Instant::now();
    a.ok::<_, ()>(Op::Rollback, &())?;
    sql::done(b, upd, "the waiting UPDATE after the ROLLBACK", LANE_BOUND)?;
    let then = at.elapsed();
    sql::options(a, CommitLevel::None, None)?;
    let most = took.iter().max().copied().unwrap_or_default();
    Ok(format!(
        "under RepeatableRead the aggregate took turns: the third's CHKOBJ, ListTables, ListColumns and a query answered in at most {} ms; another job's UPDATE of a row read waited, a CHKOBJ answering in {} ms meanwhile, and ran {} ms after the ROLLBACK; cancelled in {} ms",
        most.as_millis(),
        waiting.as_millis(),
        then.as_millis(),
        stop.as_millis()
    ))
}

/// Rows of the table step 90 indexes.
const WIDE_ROWS: i64 = 256_000;

/// An `UPDATE` whose condition has subqueries (on `b`) and a `CREATE
/// INDEX` over a large table (on `a`), each while the third's command
/// and catalog lists answer.
#[allow(clippy::too_many_lines)]
fn long_changes(a: &mut Conn, b: &mut Conn, c: &mut Conn) -> Result<String, String> {
    let check = CommandText {
        command: "CHKOBJ OBJ(QSYS/QGPL) OBJTYPE(*LIB)".into(),
    };
    let tables = ListTables {
        schema: "QAPLANE".into(),
        pattern: "%".into(),
        kinds: Vec::new(),
        start: 0,
        max: 0,
    };
    let columns = ListColumns {
        schema: "QAPLANE".into(),
        table: "BIG".into(),
    };
    // The subquery UPDATE: its rows are found first, a read as long as
    // the aggregate's.
    let upd = sql::start_exec(
        b,
        "UPDATE QAPLANE.BIG A SET N = N WHERE EXISTS (SELECT 1 FROM QAPLANE.BIG B WHERE EXISTS \
         (SELECT 1 FROM QAPLANE.BIG C WHERE C.N > A.N + B.N + 1000000))",
    )?;
    sql::still_running(b, upd, Duration::from_secs(1))?;
    let mut took = vec![
        within(c, "CHKOBJ", Op::RunCommand, &check)?,
        within(c, "ListTables", Op::ListTables, &tables)?,
        within(c, "ListColumns", Op::ListColumns, &columns)?,
    ];
    sql::still_running(b, upd, Duration::ZERO)?;
    let (_, stop_upd) = sql::cancel(b, upd)?;
    // A table large enough that its index takes a while, in a library
    // of its own (not journaled); kept between runs.
    let _ = c.call(
        Op::RunCommand,
        &CommandText {
            command: "CRTLIB LIB(QAPLANEX)".into(),
        },
    )?;
    sql::run_or(
        a,
        "CREATE TABLE QAPLANEX.WIDE (N INTEGER NOT NULL, PAD CHAR(40) NOT NULL DEFAULT 'x')",
        &["42710", "58004"],
    )?;
    let mut have = sql::count(a, "SELECT COUNT(*) FROM QAPLANEX.WIDE")?;
    if have == 0 {
        sql::run(a, "INSERT INTO QAPLANEX.WIDE (N) SELECT N FROM QAPLANE.BIG")?;
        have = sql::count(a, "SELECT COUNT(*) FROM QAPLANEX.WIDE")?;
    }
    while have < WIDE_ROWS {
        sql::run(
            a,
            &format!("INSERT INTO QAPLANEX.WIDE (N) SELECT N + {have} FROM QAPLANEX.WIDE"),
        )?;
        have = sql::count(a, "SELECT COUNT(*) FROM QAPLANEX.WIDE")?;
    }
    sql::run_or(a, "DROP INDEX QAPLANEX.WIDEIX", &["42704"])?;
    let at = Instant::now();
    let idx = sql::start_exec(
        a,
        "CREATE INDEX QAPLANEX.WIDEIX ON QAPLANEX.WIDE (PAD, N DESC)",
    )?;
    took.push(within(
        c,
        "CHKOBJ beside CREATE INDEX",
        Op::RunCommand,
        &check,
    )?);
    took.push(within(
        c,
        "ListTables beside CREATE INDEX",
        Op::ListTables,
        &tables,
    )?);
    took.push(within(
        c,
        "ListColumns beside CREATE INDEX",
        Op::ListColumns,
        &columns,
    )?);
    // The table being indexed is read meanwhile. (Its change is refused
    // to other jobs; the connections of one session are one job.)
    let at_read = Instant::now();
    let read = sql::count(c, "SELECT COUNT(*) FROM QAPLANEX.WIDE WHERE N < 10")?;
    took.push(at_read.elapsed());
    let building = a.poll_reply(idx, Duration::ZERO)?;
    if let Some(r) = building {
        return Err(format!(
            "CREATE INDEX over {have} rows ended ({}) in {} ms, before the third connection's requests were answered",
            sql::failure(&r),
            at.elapsed().as_millis()
        ));
    }
    if read != 10 {
        return Err(format!("the table being indexed read {read} rows of 10"));
    }
    sql::done(a, idx, "CREATE INDEX", SLOW)?;
    let built = at.elapsed();
    let n = sql::count(c, "SELECT COUNT(*) FROM QAPLANEX.WIDEIX")?;
    if n != have {
        return Err(format!("the index has {n} rows of {have}"));
    }
    let copied = long_command(a, c, have, &mut took)?;
    let most = took.iter().max().copied().unwrap_or_default();
    Ok(format!(
        "an UPDATE with subqueries in its condition, a CREATE INDEX over {have} rows ({} ms) and a CPYF of them ({} ms) each ran beside the third's CHKOBJ, ListTables, ListColumns and a read of the table being indexed or copied, answered in at most {} ms; the UPDATE cancelled in {} ms",
        built.as_millis(),
        copied.as_millis(),
        most.as_millis(),
        stop_upd.as_millis()
    ))
}

/// A long command on `a`: `CPYF` of the wide table, while the third's
/// command, catalog lists and a read of the table copied answer. How
/// long the copy took.
fn long_command(
    a: &mut Conn,
    c: &mut Conn,
    have: i64,
    took: &mut Vec<Duration>,
) -> Result<Duration, String> {
    let check = CommandText {
        command: "CHKOBJ OBJ(QSYS/QGPL) OBJTYPE(*LIB)".into(),
    };
    let tables = ListTables {
        schema: "QAPLANEX".into(),
        pattern: "%".into(),
        kinds: Vec::new(),
        start: 0,
        max: 0,
    };
    let columns = ListColumns {
        schema: "QAPLANEX".into(),
        table: "WIDE".into(),
    };
    let _ = c.call(
        Op::RunCommand,
        &CommandText {
            command: "DLTF FILE(QAPLANEX/WIDECPY)".into(),
        },
    )?;
    let at = Instant::now();
    let copy = CommandText {
        command: "CPYF FROMFILE(QAPLANEX/WIDE) TOFILE(QAPLANEX/WIDECPY) CRTFILE(*YES)".into(),
    };
    let id = a.request(Op::RunCommand, &copy)?;
    took.push(within(c, "CHKOBJ beside CPYF", Op::RunCommand, &check)?);
    took.push(within(
        c,
        "ListTables beside CPYF",
        Op::ListTables,
        &tables,
    )?);
    took.push(within(
        c,
        "ListColumns beside CPYF",
        Op::ListColumns,
        &columns,
    )?);
    let at_read = Instant::now();
    let read = sql::count(c, "SELECT COUNT(*) FROM QAPLANEX.WIDE WHERE N < 10")?;
    took.push(at_read.elapsed());
    if let Some(r) = a.poll_reply(id, Duration::ZERO)? {
        return Err(format!(
            "CPYF of {have} rows ended ({}) in {} ms, before the third connection's requests were answered",
            brief(&r),
            at.elapsed().as_millis()
        ));
    }
    if read != 10 {
        return Err(format!("the table being copied read {read} rows of 10"));
    }
    let r = a
        .poll_reply(id, SLOW)?
        .ok_or_else(|| format!("CPYF of {have} rows got no reply in {} s", SLOW.as_secs()))?;
    if r.status != Status::Ok {
        return Err(format!("CPYF of {have} rows: {}", brief(&r)));
    }
    let done = at.elapsed();
    let n = sql::count(c, "SELECT COUNT(*) FROM QAPLANEX.WIDECPY")?;
    if n != have {
        return Err(format!("CPYF copied {n} rows of {have}"));
    }
    Ok(done)
}

/// `RunCommand`, `CheckCommand` and `DescribeCommand`.
fn commands(c: &mut Conn) -> Result<String, String> {
    // A previous run's library goes first.
    let _ = c.call(
        Op::RunCommand,
        &CommandText {
            command: "DLTLIB LIB(QAPLIB)".into(),
        },
    )?;
    let made = command(c, "CRTLIB LIB(QAPLIB) TEXT('QAP step')")?;
    if !made.iter().any(|m| m == "CPC2102") {
        return Err(format!("CRTLIB's messages: {made:?}"));
    }
    let (status, id) = command_fails(c, "CRTLIB LIB(QAPLIB)")?;
    if status != Status::Error || id != "CPF2111" {
        return Err(format!("CRTLIB of an existing library: {status:?} {id}"));
    }
    let good: CheckReply = c.ok(
        Op::CheckCommand,
        &CommandText {
            command: "CRTLIB LIB(QAPX)".into(),
        },
    )?;
    if !good.messages.is_empty() {
        return Err(format!(
            "CheckCommand of a good command: {:?}",
            good.messages
        ));
    }
    let bad = c.call(
        Op::CheckCommand,
        &CommandText {
            command: "CRTLIB LIB(QAPX) NOSUCH(1)".into(),
        },
    )?;
    let msgs: CheckReply = bad.body_as().map_err(|e| e.to_string())?;
    if bad.status != Status::Error || msgs.messages.is_empty() {
        return Err(format!(
            "CheckCommand of a bad keyword: {} {:?}",
            brief(&bad),
            msgs.messages
        ));
    }
    let def: CommandDefinition = c.ok(
        Op::DescribeCommand,
        &DescribeCommand {
            command: ObjectName {
                library: "*LIBL".into(),
                name: "CRTLIB".into(),
            },
        },
    )?;
    if def.name.name != "CRTLIB" || !def.nodes.iter().any(|n| n.keyword == "LIB" && n.required) {
        return Err(format!(
            "DescribeCommand CRTLIB: {} nodes, name {:?}",
            def.nodes.len(),
            def.name
        ));
    }
    Ok(format!(
        "CRTLIB ran ({}), again CPF2111, CheckCommand found {}, CRTLIB described in {} nodes",
        made.join(" "),
        msgs.messages.first().map_or("", |m| m.id.as_str()),
        def.nodes.len()
    ))
}

/// A job log of `USER`'s, several pages long (the printer file's page
/// made short for it), listed and read.
fn job_log(s: &mut Session, c: &mut Conn) -> Result<String, String> {
    expect(
        s,
        "CHGPRTF FILE(QSYS/QPJOBLOG) PAGESIZE(10 132) OVRFLW(8)",
        &[""],
    )?;
    // The batch subsystem, unless an earlier step started it.
    s.command("STRSBS QSYS/QBATCH", CMD)?;
    let sent = s.command(
        &format!("SBMJOB CMD(CRTLIB LIB(QGPL)) JOB(QAPLOG) USER({USER}) LOG(4 00 *SECLVL)"),
        CMD,
    )?;
    let file = wait_job_log(c);
    s.command(
        "CHGPRTF FILE(QSYS/QPJOBLOG) PAGESIZE(66 132) OVRFLW(60)",
        CMD,
    )?;
    let file = file.map_err(|e| format!("{e}\nSBMJOB: {sent}"))?;
    let read = |c: &mut Conn, format, first_page, last_page| -> Result<SpooledData, String> {
        c.ok(
            Op::ReadSpooledFile,
            &ReadSpooledFile {
                id: file.id.clone(),
                format,
                first_page,
                last_page,
            },
        )
    };
    let whole = read(c, ReadFormat::Scs, 1, 0)?;
    if whole.pages < 2 || whole.pages != file.pages {
        let text =
            read(c, ReadFormat::Text, 1, 0).map(|t| String::from_utf8_lossy(&t.data).into_owned());
        return Err(format!(
            "the job log reads as {} pages, listed as {}; as text:\n{}",
            whole.pages,
            file.pages,
            text.unwrap_or_else(|e| e)
        ));
    }
    let first = read(c, ReadFormat::Scs, 1, 1)?;
    let rest = read(c, ReadFormat::Scs, 2, 0)?;
    let tail = whole
        .data
        .strip_prefix(first.data.as_slice())
        .ok_or("page 1 is not the start of the whole file")?;
    if !rest.data.ends_with(tail) || tail.is_empty() {
        return Err(format!(
            "pages 2 on ({} bytes) do not end with the whole file after page 1 ({} bytes)",
            rest.data.len(),
            tail.len()
        ));
    }
    let state = rest.data.len() - tail.len();
    let text = read(c, ReadFormat::Text, 1, 0)?;
    let text = String::from_utf8(text.data).map_err(|e| format!("the job log as text: {e}"))?;
    if !text.contains("CPF2111") || !text.contains('\u{c}') {
        return Err(format!("the job log as text:\n{text}"));
    }
    let bad = c.call(
        Op::ReadSpooledFile,
        &ReadSpooledFile {
            id: file.id.clone(),
            format: ReadFormat::Scs,
            first_page: whole.pages + 1,
            last_page: 0,
        },
    )?;
    if bad.status != Status::InvalidRequest {
        return Err(format!("a page past the end: {}", brief(&bad)));
    }
    Ok(format!(
        "job log {}/{}/{} of {} pages: page 1 and pages 2 on ({state} bytes of state) join the whole {} bytes, text has CPF2111",
        file.id.job.number,
        file.id.job.user,
        file.id.job.name,
        whole.pages,
        whole.data.len()
    ))
}

/// `QAPLOG`'s `QPJOBLOG`, once it is there.
fn wait_job_log(c: &mut Conn) -> Result<SpooledFile, String> {
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        let list: SpooledFiles = c.ok(
            Op::ListSpooledFiles,
            &ListSpooledFiles {
                user: "*CURRENT".into(),
                outq: None,
                status: None,
                form_type: None,
                user_data: None,
                start: 0,
                max: 0,
            },
        )?;
        let found = list
            .files
            .iter()
            .filter(|f| f.id.file == "QPJOBLOG" && f.id.job.name == "QAPLOG")
            .max_by_key(|f| f.id.job.number.clone());
        if let Some(f) = found {
            return Ok(f.clone());
        }
        if Instant::now() > end {
            let names: Vec<String> = list
                .files
                .iter()
                .map(|f| format!("{}/{}", f.id.job.name, f.id.file))
                .collect();
            return Err(format!(
                "no QPJOBLOG of QAPLOG listed in 60 s; listed {names:?}"
            ));
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

/// B adds `QAPLIB` to its library list; a data area there is found by
/// `*LIBL` on B and not on A.
fn library_lists(a: &mut Conn, b: &mut Conn) -> Result<String, String> {
    command(a, "CRTDTAARA DTAARA(QAPLIB/QAPDTA) TYPE(*CHAR) LEN(10)")?;
    command(b, "ADDLIBLE LIB(QAPLIB)")?;
    command(b, "CHKOBJ OBJ(*LIBL/QAPDTA) OBJTYPE(*DTAARA)")?;
    let (status, id) = command_fails(a, "CHKOBJ OBJ(*LIBL/QAPDTA) OBJTYPE(*DTAARA)")?;
    if id != "CPF9801" {
        return Err(format!("A found B's library list entry: {status:?} {id}"));
    }
    Ok("QAPLIB on B's library list only".into())
}

/// The `Sql` service on `c`.
#[allow(clippy::too_many_lines)]
fn sql_service(c: &mut Conn) -> Result<String, String> {
    sql::options(c, CommitLevel::None, None)?;
    sql::run_or(c, "CREATE SCHEMA QAPSQL", &["42710", "58004"])?;
    sql::run_or(c, "DROP TABLE QAPSQL.ITEMS", &["42704"])?;
    sql::run(
        c,
        "CREATE TABLE QAPSQL.ITEMS (ID INTEGER NOT NULL PRIMARY KEY, NAME VARCHAR(20) NOT NULL, PRICE DECIMAL(7, 2))",
    )?;
    let ins = sql::prepare(c, "INSERT INTO QAPSQL.ITEMS VALUES(?, ?, ?)")?;
    if ins.parameters.len() != 3 {
        return Err(format!("the INSERT's parameters: {:?}", ins.parameters));
    }
    let row = |id, name, price| vec![sql::int(id), sql::text(name), sql::dec(price)];
    let r = sql::execute(
        c,
        ins.statement,
        vec![
            row(1, "One", "1.50"),
            row(2, "Two", "2.25"),
            row(3, "Three", "3.00"),
        ],
    )?;
    if r.status != Status::Ok {
        return Err(format!("three rows: {}", sql::failure(&r)));
    }
    let ran: Executed = r.body_as().map_err(|e| e.to_string())?;
    if ran.updated != 3 {
        return Err(format!("three rows inserted, {} updated", ran.updated));
    }
    // Row 1 of these is a duplicate key: row 0 stays, row 2 never runs.
    let r = sql::execute(
        c,
        ins.statement,
        vec![
            row(4, "Four", "4.00"),
            row(2, "Again", "0"),
            row(5, "Five", "5"),
        ],
    )?;
    let dup: SqlFailure = r
        .body_as()
        .map_err(|e| format!("{}: {e}", sql::failure(&r)))?;
    if r.status != Status::SqlError || dup.row != Some(1) || dup.sqlcode != -803 {
        return Err(format!("the duplicate key: {}", sql::failure(&r)));
    }
    c.ok::<_, ()>(
        Op::Release,
        &StatementRef {
            statement: ins.statement,
        },
    )?;
    // Prepare, Execute, Fetch in two batches.
    let q = sql::prepare(c, "SELECT ID, NAME, PRICE FROM QAPSQL.ITEMS ORDER BY ID")?;
    let names: Vec<&str> = q.columns.iter().map(|c| c.name.as_str()).collect();
    if names != ["ID", "NAME", "PRICE"]
        || q.columns[2].sql_type != "DECIMAL"
        || q.columns[2].scale != 2
    {
        return Err(format!("the query's columns: {:?}", q.columns));
    }
    let r = sql::execute(c, q.statement, Vec::new())?;
    let cursor = r
        .body_as::<Executed>()
        .ok()
        .and_then(|e| e.cursor)
        .ok_or_else(|| sql::failure(&r))?;
    let one: Fetched = c.ok(
        Op::Fetch,
        &Fetch {
            cursor,
            max_rows: 3,
        },
    )?;
    let two: Fetched = c.ok(
        Op::Fetch,
        &Fetch {
            cursor,
            max_rows: 3,
        },
    )?;
    let ids: Vec<SqlValue> = one
        .rows
        .iter()
        .chain(&two.rows)
        .map(|r| r[0].clone())
        .collect();
    if one.end || !two.end || ids != [sql::int(1), sql::int(2), sql::int(3), sql::int(4)] {
        return Err(format!("the rows: {one:?} then {two:?}"));
    }
    if one.rows[1] != [sql::int(2), sql::text("Two"), sql::dec("2.25")] {
        return Err(format!("row 2: {:?}", one.rows[1]));
    }
    let _ = c.call(Op::CloseCursor, &CursorRef { cursor })?;
    c.ok::<_, ()>(
        Op::Release,
        &StatementRef {
            statement: q.statement,
        },
    )?;
    let catalog = catalog(c)?;
    let commit = commitment(c)?;
    Ok(format!(
        "SQL: QAPSQL.ITEMS made, a 3-row INSERT failed at row 1 with SQL0803 keeping row 0, 4 rows fetched in 2 batches; {catalog}; {commit}"
    ))
}

/// `ListSchemas`, `ListTables`, `ListColumns`.
fn catalog(c: &mut Conn) -> Result<String, String> {
    let schemas: Schemas = c.ok(
        Op::ListSchemas,
        &ListSchemas {
            pattern: "QAP%".into(),
            start: 0,
            max: 0,
        },
    )?;
    if !schemas.schemas.iter().any(|s| s.name == "QAPSQL") {
        return Err(format!("ListSchemas QAP%: {:?}", schemas.schemas));
    }
    let tables: Tables = c.ok(
        Op::ListTables,
        &ListTables {
            schema: "QAPSQL".into(),
            pattern: "%".into(),
            kinds: Vec::new(),
            start: 0,
            max: 0,
        },
    )?;
    if !tables
        .tables
        .iter()
        .any(|t| t.name == "ITEMS" && t.kind == TableKind::Table)
    {
        return Err(format!("ListTables QAPSQL: {:?}", tables.tables));
    }
    let cols: Columns = c.ok(
        Op::ListColumns,
        &ListColumns {
            schema: "QAPSQL".into(),
            table: "ITEMS".into(),
        },
    )?;
    let names: Vec<&str> = cols.columns.iter().map(|c| c.name.as_str()).collect();
    if names != ["ID", "NAME", "PRICE"] {
        return Err(format!("ListColumns QAPSQL.ITEMS: {:?}", cols.columns));
    }
    Ok(format!(
        "{} QAP% schema(s), {} table(s) in QAPSQL, ITEMS' 3 columns",
        schemas.total, tables.total
    ))
}

/// Commit and rollback on a table of the schema, which `CREATE SCHEMA`
/// journals; `SQL7008` on one in `QAPLIB`, a library `CRTLIB` made.
fn commitment(c: &mut Conn) -> Result<String, String> {
    sql::run_or(c, "DROP TABLE QAPSQL.LEDGER", &["42704"])?;
    sql::run(
        c,
        "CREATE TABLE QAPSQL.LEDGER (ID INTEGER NOT NULL, AMT DECIMAL(9, 2))",
    )?;
    sql::run_or(c, "DROP TABLE QAPLIB.PLAIN", &["42704"])?;
    sql::run(c, "CREATE TABLE QAPLIB.PLAIN (ID INTEGER NOT NULL)")?;
    sql::options(c, CommitLevel::ReadCommitted, Some("QAPSQL"))?;
    sql::run(c, "INSERT INTO LEDGER VALUES(1, 10.00)")?;
    c.ok::<_, ()>(Op::Rollback, &())?;
    let rolled = sql::count(c, "SELECT COUNT(*) FROM LEDGER")?;
    sql::run(c, "INSERT INTO LEDGER VALUES(2, 20.00)")?;
    c.ok::<_, ()>(Op::Commit, &())?;
    let kept = sql::count(c, "SELECT COUNT(*) FROM LEDGER")?;
    if (rolled, kept) != (0, 1) {
        return Err(format!(
            "LEDGER after a rollback has {rolled} rows, after a commit {kept}"
        ));
    }
    let p = sql::prepare(c, "INSERT INTO QAPLIB.PLAIN VALUES(9)")?;
    let r = sql::execute(c, p.statement, Vec::new())?;
    let f: SqlFailure = r
        .body_as()
        .map_err(|e| format!("{}: {e}", sql::failure(&r)))?;
    if r.status != Status::SqlError || f.sqlcode != -7008 {
        return Err(format!(
            "an unjournaled table under commitment control: {}",
            sql::failure(&r)
        ));
    }
    c.ok::<_, ()>(
        Op::Release,
        &StatementRef {
            statement: p.statement,
        },
    )?;
    c.ok::<_, ()>(Op::Rollback, &())?;
    sql::options(c, CommitLevel::None, None)?;
    Ok("LEDGER: a rollback left 0 rows, a commit 1; QAPLIB.PLAIN under *CS: SQL7008".into())
}

/// `QSECOFR`, on a session of its own, makes `QAPPAY.SALARIES` with
/// `*PUBLIC *EXCLUDE`; `USER`'s `SELECT` and `ListColumns` of it are
/// `NotAuthorized` with `SQL0551`.
fn not_authorized(srv: &mut Server, c: &mut Conn) -> Result<String, String> {
    let mut sec = srv.open()?;
    sec.sign_on("QSECOFR", crate::phase5::SECOFR_PASSWORD)?;
    sql::run_or(&mut sec, "CREATE SCHEMA QAPPAY", &["42710", "58004"])?;
    sql::run_or(
        &mut sec,
        "CREATE TABLE QAPPAY.SALARIES (EMPNO CHAR(6) NOT NULL, SALARY DECIMAL(9, 2))",
        &["42710", "58004"],
    )?;
    command(
        &mut sec,
        "GRTOBJAUT OBJ(QAPPAY/SALARIES) OBJTYPE(*FILE) USER(*PUBLIC) AUT(*EXCLUDE)",
    )?;
    let _ = sec.call(Op::SignOff, &());
    let r = c.call(
        Op::Prepare,
        &qap_proto::ops::sql::Prepare {
            sql: "SELECT * FROM QAPPAY.SALARIES".into(),
        },
    )?;
    let r = if r.status == Status::Ok {
        // Refused at Execute, if not at Prepare.
        let p: qap_proto::ops::sql::Prepared = r.body_as().map_err(|e| e.to_string())?;
        sql::execute(c, p.statement, Vec::new())?
    } else {
        r
    };
    if r.status != Status::NotAuthorized || msgid(&r) != "SQL0551" {
        return Err(format!(
            "SELECT of a table excluded from {USER}: {}",
            sql::failure(&r)
        ));
    }
    let cols = c.call(
        Op::ListColumns,
        &ListColumns {
            schema: "QAPPAY".into(),
            table: "SALARIES".into(),
        },
    )?;
    if cols.status != Status::NotAuthorized {
        return Err(format!(
            "ListColumns of a table excluded from {USER}: {}",
            brief(&cols)
        ));
    }
    Ok(
        "SELECT and ListColumns of QAPPAY.SALARIES (*PUBLIC *EXCLUDE): NotAuthorized SQL0551"
            .into(),
    )
}

/// A table of the newer types fetched: each column's `SqlValue` kind,
/// `DECFLOAT`'s text with a special value, `FOR BIT DATA` as `Binary`
/// with CCSID 65535, `TIMESTAMP(12)` picoseconds, `BOOLEAN`, the LOBs,
/// and a row of nulls.
fn value_kinds(c: &mut Conn) -> Result<String, String> {
    sql::run_or(c, "DROP TABLE QAPSQL.TYPES", &["42704"])?;
    sql::run(
        c,
        "CREATE TABLE QAPSQL.TYPES (ID INTEGER NOT NULL, DF DECFLOAT(34), BITS CHAR(4) FOR BIT DATA, \
         TS12 TIMESTAMP(12), FLAG BOOLEAN, BL BLOB(1K), CL CLOB(1K), BI BINARY(2))",
    )?;
    sql::run(
        c,
        "INSERT INTO QAPSQL.TYPES VALUES(1, 1.5E-30, X'C1C2C3C4', '2026-11-02-09.14.03.123456789012', TRUE, \
         BX'255044', 'clob text', BX'0102'), (2, 'NaN', NULL, NULL, FALSE, NULL, NULL, NULL), \
         (3, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
    )?;
    let q = sql::prepare(c, "SELECT * FROM QAPSQL.TYPES ORDER BY ID")?;
    let bits = q
        .columns
        .iter()
        .find(|k| k.name == "BITS")
        .ok_or("no BITS column")?;
    if bits.ccsid != 65535 {
        return Err(format!("BITS' column: {bits:?}"));
    }
    let rows = sql::query(c, "SELECT * FROM QAPSQL.TYPES ORDER BY ID")?;
    c.ok::<_, ()>(
        Op::Release,
        &StatementRef {
            statement: q.statement,
        },
    )?;
    let [one, two, three] = rows.as_slice() else {
        return Err(format!("{} rows", rows.len()));
    };
    let ok1 = matches!(&one[1], SqlValue::Decimal { text } if text.contains('E') || text.starts_with("0.000"))
        && one[2]
            == SqlValue::Binary {
                data: vec![0xC1, 0xC2, 0xC3, 0xC4],
            }
        && matches!(one[3], SqlValue::Timestamp { picos, .. } if picos == 456_789_012)
        && one[4] == SqlValue::Boolean { value: true }
        && one[5]
            == SqlValue::Binary {
                data: vec![0x25, 0x50, 0x44],
            }
        && one[6] == sql::text("clob text")
        && one[7] == SqlValue::Binary { data: vec![1, 2] };
    let ok2 = two[1] == SqlValue::Decimal { text: "NaN".into() }
        && two[4] == SqlValue::Boolean { value: false };
    let ok3 = three[1..].iter().all(|v| *v == SqlValue::Null);
    if !(ok1 && ok2 && ok3) {
        return Err(format!(
            "the newer types' values:\n{one:?}\n{two:?}\n{three:?}"
        ));
    }
    Ok("DECFLOAT (1.5E-30, NaN) as Decimal, FOR BIT DATA as Binary with CCSID 65535, TIMESTAMP(12) picos, BOOLEAN, BLOB, CLOB, BINARY and nulls".into())
}

/// A long query cancelled.
fn cancel_long(c: &mut Conn) -> Result<String, String> {
    sql::big_table(c, "QAPSQL", 1000)?;
    let (id, op) = sql::start_long(c, "QAPSQL")?;
    sql::still_running(c, id, Duration::from_secs(1))?;
    let (r, took) = sql::cancel(c, id)?;
    Ok(format!(
        "the long query's {op:?} cancelled in {} ms ({})",
        took.as_millis(),
        msgid(&r)
    ))
}
