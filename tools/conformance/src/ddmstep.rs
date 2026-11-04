// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 12 QEMU steps 100 and 101. Step 100, DDM over the wire: the
//! DDM server (`STRTCPSVR *DDM`: `QRWTLSTN` and the `QRWTSRVR` prestart
//! job) answers this harness's DRDA requester on the forwarded 446 (SQL
//! and record-level access as `QSECOFR`); in the guest, a DDM file names
//! a second directory entry for 127.0.0.1, and a CL program reading a
//! local file overridden to it gets the rows over the guest's own
//! loopback, through the requester, the listener and a server job.
//! Step 101, `*WSCST`: the shipped `QSYS/QWP...` objects, `RTVWSCST`
//! into a member, `CRTWSCST` from it, `SNDTCPSPLF WSCST()` and
//! `DLTWSCST`.

use std::net::TcpStream;
use std::time::{Duration, Instant};

use ddm::client::{ConnectOpts, Connection};
use ddm::file::{Rec, Ufcb, ufcb};
use ddm::sec::Mechanism;

use crate::phase5::{SECOFR_PASSWORD, secofr_command_entry};
use crate::phase6::{entry_wait, has, sign_off};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The CL program: the local file's rows, which an override sends to
/// the DDM file.
const DDMREAD: &str = "\
             PGM
             DCLF       FILE(QGPL/DDMTL)
             DCL        VAR(&N) TYPE(*DEC) LEN(3 0) VALUE(0)
 LOOP:       RCVF
             MONMSG     MSGID(CPF0864) EXEC(GOTO CMDLBL(DONE))
             CHGVAR     VAR(&N) VALUE(&N + 1)
             SNDPGMMSG  MSG('DDM row' *BCAT &NAME *BCAT &CITY) TOPGMQ(*EXT)
             GOTO       CMDLBL(LOOP)
 DONE:       SNDPGMMSG  MSG('DDM rows read:' *BCAT %CHAR(&N)) TOPGMQ(*EXT)
             ENDPGM
";

/// The guest port forwarded for 446.
fn ddm_port(suite: &Suite) -> Result<u16, String> {
    suite
        .launch
        .host_ports
        .iter()
        .find(|(_, g)| *g == 446)
        .map(|(h, _)| *h)
        .ok_or_else(|| "guest port 446 is not forwarded".to_string())
}

/// The local RDB name: the directory's `*LOCAL` entry.
fn local_rdb(s: &mut Session) -> Result<String, String> {
    let out = s.command("WRKRDBDIRE", CMD)?;
    out.lines()
        .find(|l| l.contains("*LOCAL"))
        .and_then(|l| l.split_whitespace().next())
        .map(str::to_string)
        .ok_or_else(|| format!("no *LOCAL entry:\n{out}"))
}

fn ok_line(out: &str, what: &str) -> Result<(), String> {
    let first = out.trim_start();
    if first.starts_with("CPF") || first.starts_with("CPD") {
        return Err(format!("{what}: {out}"));
    }
    Ok(())
}

/// The harness as a DRDA requester against the guest.
fn requester(port: u16, rdb: &str) -> Result<String, String> {
    let stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("connect 446: {e}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(60))).map_err(|e| e.to_string())?;
    let mut opts = ConnectOpts::new(rdb, "QSECOFR", SECOFR_PASSWORD);
    opts.mechanisms = vec![Mechanism::EUsrIdPwd];
    let mut c = Connection::connect(stream, &opts).map_err(|e| format!("DRDA connect to {rdb}: {e}"))?;
    let mech = c.mechanism;
    let _ = c.execute_immediate("DROP TABLE QSTEST.DDMT");
    for stmt in [
        "CREATE TABLE QSTEST.DDMT (NAME CHAR(10) NOT NULL, CITY CHAR(10) NOT NULL)",
        "INSERT INTO QSTEST.DDMT VALUES ('Ann', 'Oslo'), ('Bob', 'Lima')",
    ] {
        let ca = c.execute_immediate(stmt).map_err(|e| format!("{stmt}: {e}"))?;
        if ca.is_error() {
            return Err(format!("{stmt}: SQLCODE {} {}", ca.sqlcode, ca.sqlerrmc));
        }
    }
    let stmt = c.prepare("SELECT NAME, CITY FROM QSTEST.DDMT ORDER BY NAME").map_err(|e| e.to_string())?;
    let rows = c.query(&stmt).map_err(|e| format!("query: {e}"))?;
    let text: Vec<String> = rows
        .rows
        .iter()
        .map(|r| r.iter().map(|v| v.render().trim().to_string()).collect::<Vec<_>>().join("|"))
        .collect();
    if text != ["Ann|Oslo", "Bob|Lima"] {
        return Err(format!("the query answered {text:?}"));
    }
    // Record-level access: a record added, then all read in arrival order.
    let block = Ufcb {
        file: "DDMT".into(),
        library: "QSTEST".into(),
        member: "*FIRST".into(),
        flags: ufcb::INPUT | ufcb::OUTPUT,
        arrival: true,
        share: Ufcb::share_for(ufcb::INPUT | ufcb::OUTPUT),
        blocking: 10,
        format: "DDMT".into(),
    };
    let file = c.open_file("F1", &block, 2).map_err(|e| format!("S38OPEN: {e}"))?;
    let mut rec = ddm::wire::ebcdic_fixed("Cy", 10);
    rec.extend(ddm::wire::ebcdic_fixed("Rome", 10));
    c.put(&file, &[Rec::new(rec, 2)]).map_err(|e| format!("S38PUTM: {e}"))?;
    let all = c
        .get(&file, ddm::cp::s38::S38GET, ddm::file::op::FIRST, 0, &[])
        .map_err(|e| format!("S38GET: {e}"))?;
    c.close_file(&file).map_err(|e| format!("S38CLOSE: {e}"))?;
    if all.records.len() != 3 {
        return Err(format!("record access read {} records, not 3", all.records.len()));
    }
    Ok(format!(
        "the harness's requester signed on ({mech:?}), ran SQL (CREATE, INSERT, SELECT of 2 rows) and record access (S38PUTM, S38GET of 3)"
    ))
}

impl Suite {
    /// Step 100.
    pub(crate) fn ddm(&self, s: &mut Session) -> Result<String, String> {
        let set = s.command(&format!("CHGUSRPRF USRPRF(QSECOFR) PASSWORD('{SECOFR_PASSWORD}') PWDEXP(*NO)"), CMD)?;
        ok_line(&set, "CHGUSRPRF QSECOFR")?;
        let _ = s.command("CRTLIB LIB(QSTEST) TEXT('DDM tests')", CMD);
        let _ = s.command("STRSBS SBSD(QUSRWRK)", CMD);
        let started = s.command("STRTCPSVR SERVER(*DDM)", CMD)?;
        let until = Instant::now() + Duration::from_secs(90);
        loop {
            let out = s.command("WRKACTJOB", CMD)?;
            if out.contains("QRWTLSTN") && out.contains("QRWTSRVR") {
                break;
            }
            if Instant::now() >= until {
                return Err(format!("QRWTLSTN or QRWTSRVR is not active ({started}):\n{out}"));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        let rdb = local_rdb(s)?;
        let mut report = requester(ddm_port(self)?, &rdb)?;

        // The guest's own requester, to itself by a second entry.
        let _ = s.command("RMVRDBDIRE RDB(LOOP)", CMD);
        let add = s.command("ADDRDBDIRE RDB(LOOP) RMTLOCNAME('127.0.0.1' *IP) TEXT('This system again')", CMD)?;
        ok_line(&add, "ADDRDBDIRE")?;
        let _ = s.command("CHGSYSVAL SYSVAL(QRETSVRSEC) VALUE('1')", CMD);
        let _ = s.command("RMVSVRAUTE USRPRF(QSECOFR) SERVER(LOOP)", CMD);
        let aute = s.command(
            &format!("ADDSVRAUTE USRPRF(QSECOFR) SERVER(LOOP) USRID(QSECOFR) PASSWORD('{SECOFR_PASSWORD}')"),
            CMD,
        )?;
        ok_line(&aute, "ADDSVRAUTE")?;
        let made = s.command(
            "CRTDDMF FILE(QGPL/RDDMT) RMTFILE(QSTEST/DDMT) RMTLOCNAME(*RDB) RDB(LOOP) TEXT('DDMT on LOOP')",
            CMD,
        )?;
        ok_line(&made, "CRTDDMF")?;
        let fd = s.command("DSPFD FILE(QGPL/RDDMT)", CMD)?;
        for want in ["DDM", "QSTEST/DDMT", "LOOP"] {
            if !fd.contains(want) {
                return Err(format!("DSPFD of the DDM file lacks {want}:\n{fd}"));
            }
        }
        let objd = s.command("DSPOBJD OBJ(QGPL/RDDMT) OBJTYPE(*FILE)", CMD)?;
        if !objd.contains("DDMF") {
            return Err(format!("DSPOBJD does not show attribute DDMF:\n{objd}"));
        }
        let list = s.command("WRKDDMF FILE(QGPL/*ALL)", CMD)?;
        if !list.contains("QGPL/RDDMT") {
            return Err(format!("WRKDDMF lacks QGPL/RDDMT:\n{list}"));
        }
        // The local file with the remote one's format, and the program.
        let _ = s.command("DLTF FILE(QGPL/DDMTL)", CMD);
        let local = s.command("svc.sql CREATE TABLE QGPL/DDMTL (NAME CHAR(10) NOT NULL, CITY CHAR(10) NOT NULL);", CMD)?;
        let _ = local;
        secofr_command_entry(s)?;
        let _ = entry_wait(s, "crtsrcpf FILE(QGPL/QCLSRC)", |l| l.contains("QCLSRC"));
        crate::qir::qemu::push(s, "DDMREADS", DDMREAD)?;
        let put = s.command("svc.srcput QGPL/QCLSRC DDMREAD SRCTYPE(CLLE) FROMSTMF(DDMREADS)", CMD)?;
        if !put.contains("written") {
            return Err(format!("srcput: {put}"));
        }
        entry_wait(s, "crtclpgm PGM(QGPL/DDMREAD) SRCFILE(QGPL/QCLSRC)", has(&["DDMREAD", "created"]))?;
        entry_wait(s, "ovrdbf FILE(DDMTL) TOFILE(QGPL/RDDMT)", |l| l.contains("DDMTL") || l.contains("verride"))?;
        let read = entry_wait(s, "call QGPL/DDMREAD", has(&["DDM rows read:"]))?;
        sign_off(s)?;
        for want in ["DDM row Ann Oslo", "DDM row Bob Lima", "DDM row Cy Rome", "DDM rows read: 3"] {
            if !read.contains(want) {
                return Err(format!("the program over the DDM file did not show '{want}':\n{read}"));
            }
        }
        let _ = s.command("DLTF FILE(QGPL/RDDMT)", CMD);
        report.push_str("; in the guest: ADDRDBDIRE LOOP, ADDSVRAUTE, CRTDDMF (DSPFD, DSPOBJD DDMF, WRKDDMF), and a CL program's RCVF over an override to the DDM file read the 3 rows through the requester and a QRWTSRVR job");
        Ok(report)
    }

    /// Step 101.
    pub(crate) fn wscst(&self, s: &mut Session) -> Result<String, String> {
        let _ = self;
        let shipped = s.command("DSPOBJD OBJ(QSYS/QWPHPLSR4) OBJTYPE(*WSCST)", CMD)?;
        if !shipped.contains("QWPHPLSR4") || !shipped.contains("TRANSFORM") {
            return Err(format!("QSYS/QWPHPLSR4 is not shipped as a TRANSFORM *WSCST:\n{shipped}"));
        }
        let _ = s.command("CRTSRCPF FILE(QGPL/QTXTSRC)", CMD);
        let r = s.command("RTVWSCST DEVTYPE(*TRANSFORM) MFRTYPMDL(*HP5) SRCMBR(HP5) SRCFILE(QGPL/QTXTSRC)", CMD)?;
        ok_line(&r, "RTVWSCST")?;
        let src = s.command("svc.srcget QGPL/QTXTSRC HP5", CMD)?;
        for want in [":WSCST DEVCLASS=TRANSFORM.", ":INITPRT", ":PAGSIZE", ":EWSCST."] {
            if !src.contains(want) {
                return Err(format!("RTVWSCST's source lacks {want}:\n{src}"));
            }
        }
        let _ = s.command("DLTWSCST WSCST(QGPL/MYHP5)", CMD);
        let c = s.command("CRTWSCST WSCST(QGPL/MYHP5) SRCFILE(QGPL/QTXTSRC) SRCMBR(HP5)", CMD)?;
        ok_line(&c, "CRTWSCST")?;
        let objd = s.command("DSPOBJD OBJ(QGPL/MYHP5) OBJTYPE(*WSCST)", CMD)?;
        if !objd.contains("TRANSFORM") {
            return Err(format!("the object's attribute is not TRANSFORM:\n{objd}"));
        }
        // A spooled file to this system's own LPD through the object, and
        // a missing object refused.
        let _ = s.command("STRTCPSVR SERVER(*LPD)", CMD);
        // A spooled file: a CL compile's listing.
        let _ = s.command("CRTSRCPF FILE(QGPL/QCLSRC)", CMD);
        crate::qir::qemu::push(s, "WSLISTS", "             PGM
             ENDPGM
")?;
        let _ = s.command("svc.srcput QGPL/QCLSRC WSLIST SRCTYPE(CLLE) FROMSTMF(WSLISTS)", CMD)?;
        let printed = s.command("CRTCLPGM PGM(QGPL/WSLIST) SRCFILE(QGPL/QCLSRC)", CMD)?;
        // The report and its job as the listing message names them.
        let words: Vec<&str> = printed.split_whitespace().collect();
        let after = |w: &str| words.iter().position(|x| *x == w).and_then(|i| words.get(i + 1)).map(|s| s.trim_end_matches(['.', ',']).to_string());
        let report = after("Report").unwrap_or_else(|| "QSYSPRT".into());
        let job = after("job").map_or_else(String::new, |j| format!(" JOB({j})"));
        let sent = s.command(
            &format!("SNDTCPSPLF RMTSYS('127.0.0.1') PRTQ('QPRINT') FILE({report}){job} SPLNBR(*LAST) TRANSFORM(*YES) MFRTYPMDL(*WSCST) WSCST(QGPL/MYHP5)"),
            CMD,
        )?;
        if !sent.contains(" sent ") {
            return Err(format!("SNDTCPSPLF through QGPL/MYHP5 did not send {report} ({printed}):
{sent}"));
        }
        let missing = s.command(
            "SNDTCPSPLF RMTSYS('127.0.0.1') PRTQ('QPRINT') FILE(QSYSPRT) SPLNBR(*LAST) TRANSFORM(*YES) MFRTYPMDL(*WSCST) WSCST(QGPL/NOSUCH)",
            CMD,
        )?;
        if !missing.contains("CPF9801") {
            return Err(format!("a missing *WSCST was not refused with CPF9801:\n{missing}"));
        }
        let d = s.command("DLTWSCST WSCST(QGPL/MYHP5)", CMD)?;
        if !d.contains("deleted") {
            return Err(format!("DLTWSCST: {d}"));
        }
        Ok(format!(
            "QSYS/QWPHPLSR4 shipped; RTVWSCST *HP5 wrote the tag source; CRTWSCST compiled it (TRANSFORM); SNDTCPSPLF through it: {}; a missing one: CPF9801; DLTWSCST deleted it",
            sent.lines().next().unwrap_or("").trim()
        ))
    }
}
