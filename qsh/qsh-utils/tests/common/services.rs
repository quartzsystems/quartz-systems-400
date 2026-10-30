// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The seeded system's services: DNS records, a small CL interpreter
//! that logs every command to `/tmp/cl.log`, canned SQL results, an
//! `rexec` server, system values, data areas and queues, spooled files
//! and IPC objects.

use std::collections::BTreeSet;
use std::io::Read as _;
use std::sync::{Arc, Mutex};

use qsh::sys::services::rtype;
use qsh::sys::stream::write_all;
use qsh::sys::{
    ClMessage, ClOutcome, ClRequest, Credentials, DataArea, DtaaraType, Errno, IpcInfo, IpcKind,
    MemSys, MsgKind, SplfInfo, Spooled, SqlColumn, SqlOutcome, SqlRequest, SysError, duplex,
};

/// The password the `rexec` server takes for `QSHUSER`.
pub const REXEC_PASSWORD: &str = "secret";

fn msg(id: &str, kind: MsgKind, text: &str) -> ClMessage {
    ClMessage {
        id: id.to_string(),
        kind,
        text: text.to_string(),
    }
}

/// `KEY(value)` of a command string, or the first positional value.
fn param(cmd: &str, key: &str) -> Option<String> {
    let up = cmd.to_ascii_uppercase();
    let pat = format!("{key}(");
    if let Some(i) = up.find(&pat) {
        let rest = &cmd[i + pat.len()..];
        let mut depth = 0;
        let mut quoted = false;
        for (j, c) in rest.char_indices() {
            match c {
                '\'' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted && depth == 0 => {
                    return Some(rest[..j].trim_matches('\'').to_string());
                }
                ')' if !quoted => depth -= 1,
                _ => {}
            }
        }
        return None;
    }
    None
}

fn positional(cmd: &str) -> Option<String> {
    let mut it = cmd.split_whitespace();
    it.next();
    it.next()
        .filter(|w| !w.contains('('))
        .map(str::to_ascii_uppercase)
}

fn cl(libs: &Mutex<BTreeSet<String>>, req: &ClRequest) -> ClOutcome {
    let cmd = req.command.trim();
    let verb = cmd
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let lib = || {
        param(cmd, "LIB")
            .map(|s| s.to_ascii_uppercase())
            .or_else(|| positional(cmd))
            .unwrap_or_default()
    };
    let mut out = ClOutcome::default();
    match verb.as_str() {
        "CRTLIB" => {
            let l = lib();
            let mut set = libs.lock().unwrap();
            if set.insert(l.clone()) {
                out.messages.push(msg(
                    "CPC2102",
                    MsgKind::Completion,
                    &format!("Library {l} created."),
                ));
            } else {
                out.messages.push(msg(
                    "CPF2111",
                    MsgKind::Escape,
                    &format!("Library {l} exists."),
                ));
                out.failed = true;
            }
        }
        "DLTLIB" => {
            let l = lib();
            if libs.lock().unwrap().remove(&l) {
                out.messages.push(msg(
                    "CPC2194",
                    MsgKind::Completion,
                    &format!("Library {l} deleted."),
                ));
            } else {
                out.messages.push(msg(
                    "CPF2110",
                    MsgKind::Escape,
                    &format!("Library {l} not found."),
                ));
                out.failed = true;
            }
        }
        "DSPLIBL" => {
            let mut page = vec!["Library list of the job".to_string()];
            for l in ["QSYS", "QSYS2", "QHLPSYS", "QUSRSYS", "QGPL", "QTEMP"] {
                page.push(format!("  {l}"));
            }
            out.spooled.push(Spooled {
                name: "QPRTSPLF".to_string(),
                pages: vec![page],
            });
        }
        "DSPJOB" => {
            let job = if req.same_job {
                "000123/QSHUSER/QZSHSH"
            } else {
                "000200/QSHUSER/QP0ZSPWP"
            };
            out.spooled.push(Spooled {
                name: "QPDSPJOB".to_string(),
                pages: vec![vec![format!("Job {job}")], vec!["page two".to_string()]],
            });
        }
        "WRKENVVAR" => {
            let lines: Vec<String> = req.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
            out.spooled.push(Spooled {
                name: "QPRTENV".to_string(),
                pages: vec![lines],
            });
        }
        "SNDPGMMSG" => {
            let text = param(cmd, "MSG").unwrap_or_default();
            let ty = param(cmd, "MSGTYPE")
                .unwrap_or_else(|| "*INFO".to_string())
                .to_ascii_uppercase();
            let kind = match ty.as_str() {
                "*COMP" => MsgKind::Completion,
                "*DIAG" => MsgKind::Diagnostic,
                "*ESCAPE" => MsgKind::Escape,
                _ => MsgKind::Informational,
            };
            out.failed = kind == MsgKind::Escape;
            out.messages.push(msg("CPF9898", kind, &format!("{text}.")));
        }
        "SNDMSG" | "CHGJOB" | "OVRDBF" | "DLTOVR" => {}
        _ => {
            out.messages.push(msg(
                "CPD0030",
                MsgKind::Diagnostic,
                &format!("Command {verb} in library *LIBL not found."),
            ));
            out.messages.push(msg(
                "CPF0006",
                MsgKind::Escape,
                "Errors occurred in command.",
            ));
            out.failed = true;
        }
    }
    out
}

fn col(name: &str, width: usize, numeric: bool) -> SqlColumn {
    SqlColumn {
        name: name.to_string(),
        width,
        numeric,
    }
}

#[allow(clippy::unnecessary_wraps)]
fn v(s: &str) -> Option<String> {
    Some(s.to_string())
}

fn sql(req: &SqlRequest) -> SqlOutcome {
    let st: String = req
        .statement
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase();
    let st = st.trim_end_matches(';').trim().to_string();
    let bad = |tok: &str| SqlOutcome::Error {
        sqlstate: "42601".to_string(),
        code: -104,
        text: format!("Token {tok} was not valid."),
    };
    if !req.system_naming && st.contains('/') {
        return bad("/");
    }
    if req.system_naming && st.contains("QGPL.") {
        return SqlOutcome::Error {
            sqlstate: "42833".to_string(),
            code: -5016,
            text: "Qualified object name PEOPLE not valid.".to_string(),
        };
    }
    let people = ["PEOPLE", "QGPL.PEOPLE", "QGPL/PEOPLE"];
    let from = st.split(" FROM ").nth(1).unwrap_or_default().to_string();
    let table = from.split_whitespace().next().unwrap_or_default().to_string();
    if st.starts_with("SELECT COUNT(*)") && people.contains(&table.as_str()) {
        return SqlOutcome::Rows {
            columns: vec![col("00001", 11, true)],
            rows: vec![vec![v("4")]],
        };
    }
    if st.starts_with("SELECT") && people.contains(&table.as_str()) {
        return SqlOutcome::Rows {
            columns: vec![
                col("NAME", 10, false),
                col("AGE", 5, true),
                col("CITY", 2, false),
            ],
            rows: vec![
                vec![v("bob"), v("25"), v("ny")],
                vec![v("alice"), v("30"), v("la")],
                vec![v("carol"), v("25"), None],
                vec![v("dave"), v("19"), v("ny")],
            ],
        };
    }
    if st.starts_with("SELECT") && table == "EMPTY" {
        return SqlOutcome::Rows {
            columns: vec![col("ID", 11, true), col("DESCRIPTION", 30, false)],
            rows: Vec::new(),
        };
    }
    if st.starts_with("SELECT") && table == "WIDE" {
        return SqlOutcome::Rows {
            columns: vec![col("A", 1, false), col("LONG_COLUMN_NAME", 3, true)],
            rows: vec![vec![v("x"), v("-1.5")], vec![v("longer value"), v("123456")]],
        };
    }
    if st.starts_with("SELECT") {
        let name = table.rsplit(['.', '/']).next().unwrap_or_default().to_string();
        return SqlOutcome::Error {
            sqlstate: "42704".to_string(),
            code: -204,
            text: format!("{name} in QGPL type *FILE not found."),
        };
    }
    if st.starts_with("INSERT") {
        return SqlOutcome::Done { count: Some(1) };
    }
    if st.starts_with("UPDATE") || st.starts_with("DELETE") {
        return SqlOutcome::Done { count: Some(3) };
    }
    for w in ["CREATE", "DROP", "COMMIT", "ROLLBACK", "SET", "LABEL", "GRANT"] {
        if st.starts_with(w) {
            return SqlOutcome::Done { count: None };
        }
    }
    bad(st.split_whitespace().next().unwrap_or("?"))
}

fn read_field(fd: &qsh::sys::Fd) -> Option<String> {
    let mut out = Vec::new();
    let mut b = [0u8; 1];
    loop {
        match fd.read(&mut b) {
            Ok(1) if b[0] == 0 => return Some(String::from_utf8_lossy(&out).into_owned()),
            Ok(1) => out.push(b[0]),
            _ => return None,
        }
    }
}

/// An `rexec` server on one end of a connection: it reads the port,
/// user, password and command, then answers.
fn rexecd(end: &qsh::sys::Fd) {
    let (Some(_port), Some(user), Some(pass), Some(cmd)) = (
        read_field(end),
        read_field(end),
        read_field(end),
        read_field(end),
    ) else {
        return;
    };
    if !user.eq_ignore_ascii_case("QSHUSER") || pass != REXEC_PASSWORD {
        let _ = write_all(&**end, b"\x01Login incorrect.\n");
        return;
    }
    let _ = write_all(&**end, b"\0");
    let _ = write_all(&**end, format!("ran: {cmd}\n").as_bytes());
    if cmd == "cat" {
        // Echo what the client sends until it stops.
        let mut buf = Vec::new();
        let mut chunk = [0u8; 256];
        let mut src = Reader(end.clone());
        while let Ok(n) = src.read(&mut chunk) {
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        let _ = write_all(&**end, &buf);
    }
}

struct Reader(qsh::sys::Fd);

impl std::io::Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

fn ipc(kind: IpcKind, id: i32, key: i32, mode: u32, size: u64, used: u64, count: u64) -> IpcInfo {
    IpcInfo {
        kind,
        id,
        key,
        mode,
        uid: 100,
        gid: 0,
        cuid: 100,
        cgid: 0,
        size,
        used,
        count,
        pids: (101, 0),
        times: [1_780_000_000, 0, 1_779_000_000],
    }
}

fn page(lines: &[&str]) -> Vec<String> {
    lines.iter().map(ToString::to_string).collect()
}

/// Seed the services of a system.
pub fn seed(s: &MemSys) {
    let a = rtype::A;
    for (name, ty, data) in [
        ("www.example.com", a, "93.184.216.34"),
        (
            "www.example.com",
            rtype::AAAA,
            "2606:2800:220:1:248:1893:25c8:1946",
        ),
        ("example.com", a, "93.184.216.34"),
        ("example.com", rtype::MX, "10 mail.example.com."),
        ("example.com", rtype::MX, "20 backup.example.com."),
        ("example.com", rtype::NS, "ns1.example.com."),
        ("example.com", rtype::NS, "ns2.example.com."),
        ("example.com", rtype::TXT, "\"v=spf1 -all\""),
        (
            "example.com",
            rtype::SOA,
            "ns1.example.com. hostmaster.example.com. 2026102901 7200 3600 1209600 300",
        ),
        ("mail.example.com", a, "192.0.2.25"),
        ("ns1.example.com", a, "192.0.2.53"),
        ("alias.example.com", rtype::CNAME, "www.example.com."),
        ("v6only.example.com", rtype::AAAA, "2001:db8::1"),
        ("multi.example.com", a, "192.0.2.1"),
        ("multi.example.com", a, "192.0.2.2"),
        ("34.216.184.93.in-addr.arpa", rtype::PTR, "www.example.com."),
        (
            "1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.8.b.d.0.1.0.0.2.ip6.arpa",
            rtype::PTR,
            "v6only.example.com.",
        ),
        (
            "_ldap._tcp.example.com",
            rtype::SRV,
            "0 5 389 ldap.example.com.",
        ),
        ("rhost.example.com", a, "192.0.2.80"),
    ] {
        s.add_dns(name, ty, 300, data);
    }
    s.set_host_name("QS400.EXAMPLE.COM");

    let libs = Arc::new(Mutex::new(BTreeSet::from(["QGPL".to_string()])));
    let me = s.clone();
    s.set_cl(Arc::new(move |req: &ClRequest| {
        let mut log = me.read_file("/tmp/cl.log").unwrap_or_default();
        let mut line = req.command.clone();
        if req.same_job {
            line.push_str(" [same job]");
        }
        log.extend_from_slice(line.as_bytes());
        log.push(b'\n');
        me.write_file("/tmp/cl.log", log);
        cl(&libs, req)
    }));
    s.set_sql(Arc::new(sql));
    s.set_connector(Arc::new(|host: &str, port: u16| {
        let known = ["rhost", "rhost.example.com", "192.0.2.80"];
        if !known.iter().any(|h| h.eq_ignore_ascii_case(host)) {
            return Err(SysError::new(Errno::EHOSTUNREACH));
        }
        if port != 512 {
            return Err(SysError::new(Errno::ECONNREFUSED));
        }
        let (client, server) = duplex();
        std::thread::spawn(move || rexecd(&server));
        Ok(client)
    }));
    s.set_credentials(Credentials {
        user: "QSHUSER".to_string(),
        uid: 100,
        gid: 0,
        groups: Vec::new(),
    });
    s.set_process_cwd(101, "/home/QSHUSER/data");

    for (k, val) in [
        ("QCCSID", "37"),
        ("QDATFMT", "MDY"),
        ("QSECURITY", "40"),
        ("QMODEL", "8286"),
        ("QSRLNBR", "0000000"),
        ("QSYSLIBL", "QSYS QSYS2 QHLPSYS QUSRSYS"),
        ("QTIMZON", "QN0500EST"),
    ] {
        s.set_sysval(k, val);
    }
    for (k, val) in [("SYSNAME", "QS400"), ("LCLNETID", "APPN"), ("LCLCPNAME", "QS400")] {
        s.set_netattr(k, val);
    }

    s.set_data_area(
        "/QSYS.LIB/QGPL.LIB/CFG.DTAARA",
        DataArea {
            kind: DtaaraType::Char,
            length: 20,
            value: format!("{:<20}", "CONFIG"),
        },
    );
    for (name, area) in [
        (
            "COUNT",
            DataArea {
                kind: DtaaraType::Dec(7, 2),
                length: 7,
                value: "12.50".to_string(),
            },
        ),
        (
            "FLAG",
            DataArea {
                kind: DtaaraType::Logical,
                length: 1,
                value: "1".to_string(),
            },
        ),
    ] {
        let p = format!("/QSYS.LIB/QSHLIB.LIB/{name}.DTAARA");
        s.make_object(&p, 512);
        s.set_mtime(&p, super::DATA_MTIME);
        s.set_data_area(&p, area);
    }
    s.make_object("/QSYS.LIB/QSHLIB.LIB/WORK.DTAQ", 1024);
    s.set_mtime("/QSYS.LIB/QSHLIB.LIB/WORK.DTAQ", super::DATA_MTIME);
    s.make_data_queue("/QSYS.LIB/QSHLIB.LIB/WORK.DTAQ");

    let info = |name: &str, number: u32, pages: u32| SplfInfo {
        name: name.to_string(),
        number,
        pages,
        status: "RDY".to_string(),
    };
    s.add_spooled_file(
        "*",
        info("QPRINT", 1, 2),
        vec![
            page(&["first page line 1", "first page line 2"]),
            page(&["second page"]),
        ],
    );
    s.add_spooled_file("*", info("QSYSPRT", 1, 1), vec![page(&["system print"])]);
    s.add_spooled_file(
        "*",
        info("QPRINT", 2, 1),
        vec![page(&["second QPRINT", "", "after blank"])],
    );
    s.add_spooled_file(
        "000124/QSHUSER/QP0ZSPWP",
        info("QPJOBLOG", 1, 1),
        vec![page(&["job log of the child"])],
    );

    s.add_ipc(ipc(IpcKind::Queue, 0, 0x1234, 0o660, 16384, 12, 2));
    s.add_ipc(ipc(IpcKind::Memory, 1, 0, 0o600, 4096, 1, 0));
    s.add_ipc(ipc(IpcKind::Semaphore, 2, 0x5678, 0o666, 3, 0, 0));
}
