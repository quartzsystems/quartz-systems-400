// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ddm-trace`: record DDM/DRDA traffic on port 446 through the
//! recording proxy, as `OUT/<scenario>.trace`. The client is either the
//! driver's own requester (`ddm::client`) running SQL statements, or an
//! external program (a `JTOpen` record-level access test) that reads
//! `QS_HS_HOST`, `QS_HS_USER` and `QS_HS_PASS`.

// Short names for the reader, writer, field and value.
#![allow(clippy::many_single_char_names)]

pub mod proxy;

use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use ddm::client::{ConnectOpts, Connection};
use ddm::sec::Mechanism;

use crate::Target;
use crate::env::Env;
use proxy::{Proxy, ProxyConfig};

/// What a recording run needs.
#[derive(Clone, Debug)]
pub struct TraceRun {
    /// The oracle.
    pub target: Target,
    /// Where the trace goes.
    pub out: PathBuf,
    /// The scenario's name (the file's stem).
    pub scenario: String,
    /// The DDM port.
    pub port: u16,
    /// Ports forwarded without recording.
    pub forward: Vec<u16>,
    /// How long open connections get to close after the client is done.
    pub settle: Duration,
    /// The RDB name for the driver's own requester.
    pub rdb: String,
    /// The security mechanism to ask for first.
    pub mechanism: Option<u16>,
    /// SQL statements for the driver's requester: queries are prepared,
    /// opened, fetched and closed; anything else runs immediately.
    pub sql: Vec<String>,
    /// The package collection (`{user}` stands for the user).
    pub collection: Option<String>,
    /// Bind the package with this many dynamic sections first.
    pub bind: u16,
    /// Only ask for this `SECMEC` and report what the server offers.
    pub offer: Option<u16>,
    /// An external client instead (after `--`).
    pub command: Vec<String>,
}

/// Run the client through the proxy and write the trace.
pub fn run(r: &TraceRun) -> Result<PathBuf> {
    let e = Env::load()?;
    let creds = e.credentials(r.target, None)?;
    let cfg = ProxyConfig {
        upstream: creds.host.clone(),
        port: r.port,
        forward: r.forward.clone(),
        forbidden: crate::hostsvr::proxy::forbidden_forms(&creds.password),
        user: creds.user.clone(),
        password: creds.password.clone(),
    };
    let p = Proxy::start(&cfg)?;
    if r.command.is_empty() {
        if let Err(err) = requester(r, &creds.user, &creds.password) {
            println!("requester: {err}");
        }
    } else {
        let status = Command::new(&r.command[0])
            .args(&r.command[1..])
            .env("QS_HS_HOST", "127.0.0.1")
            .env("QS_HS_USER", &creds.user)
            .env("QS_HS_PASS", &creds.password)
            .status()
            .with_context(|| format!("run {}", r.command[0]))?;
        println!("client exited: {status}");
    }
    let t = p.finish(r.settle, &r.scenario)?;
    let path = r.out.join(format!("{}.trace", r.scenario));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, t.write()).with_context(|| format!("write {}", path.display()))?;
    let chunks: usize = t.connections.iter().map(|c| c.chunks().count()).sum();
    println!("{}: {} connections, {chunks} chunks", path.display(), t.connections.len());
    Ok(path)
}

fn requester(r: &TraceRun, user: &str, password: &str) -> Result<()> {
    let s = TcpStream::connect(("127.0.0.1", r.port)).context("connect to the proxy")?;
    s.set_read_timeout(Some(Duration::from_secs(30)))?;
    if let Some(code) = r.offer {
        // Ask for a mechanism and stop at the answer: what is offered.
        let mut c = Connection::raw(s, &r.rdb);
        let ask = ddm::msg::Attributes {
            extnam: Some("QS400DDM".into()),
            srvnam: Some("QS400".into()),
            srvclsnm: Some(ddm::client::server_class()),
            srvrlslv: Some(ddm::client::release_level()),
            managers: ddm::msg::Attributes::requester_managers(),
        };
        let accsec = ddm::msg::Accsec {
            secmec: code,
            rdbnam: Some(r.rdb.clone()),
            sectkn: None,
            encalg: None,
        };
        let replies = c.exchange(vec![
            ddm::Dss::request(1, ask.to_excsat()),
            ddm::Dss::request(2, accsec.to_object()),
        ])?;
        for d in &replies {
            if let Some(o) = d.first().filter(|o| o.cp == ddm::cp::rpy::ACCSECRD) {
                let rd = ddm::msg::Accsecrd::from_object(o);
                println!("SECMEC {code}: offered {:?}, SECCHKCD {:?}", rd.secmec, rd.secchkcd);
            }
        }
        return Ok(());
    }
    let mut o = ConnectOpts::new(&r.rdb, user, password);
    if let Some(c) = &r.collection {
        o.collection = c.replace("{user}", &user.to_ascii_uppercase());
        o.package = "QSDDMPKG".into();
    }
    if let Some(code) = r.mechanism {
        let Some(m) = Mechanism::from_code(code) else {
            bail!("SECMEC {code} is not one QS/400 speaks");
        };
        o.mechanisms.retain(|x| *x != m);
        o.mechanisms.insert(0, m);
    }
    let mut c = Connection::connect(s, &o).context("connect")?;
    println!(
        "connected: {:?}, SQLAM {}, server {:?} {:?}",
        c.mechanism, c.sqlam, c.server.srvclsnm, c.server.srvrlslv
    );
    if r.bind > 0 {
        let ca = c.bind_dynamic(r.bind)?;
        println!("bind: {} {} {}", ca.sqlcode, ca.sqlstate, ca.sqlerrmc);
    }
    for stmt in &r.sql {
        let upper = stmt.trim_start().to_ascii_uppercase();
        if upper == "ROLLBACK" {
            let ca = c.rollback()?;
            println!("rollback: {}", ca.sqlcode);
        } else if upper == "COMMIT" {
            let ca = c.commit()?;
            println!("commit: {}", ca.sqlcode);
        } else if upper.starts_with("SELECT") || upper.starts_with("WITH") || upper.starts_with("VALUES") {
            let p = c.prepare(stmt)?;
            println!("prepared: {} columns, {:?}", p.columns.len(), p.sqlca.as_ref().map(|s| s.sqlcode));
            for col in &p.columns {
                println!("  {} {}", col.name, col.ty.sql_name());
            }
            if p.sqlca.as_ref().is_some_and(ddm::sql::Sqlca::is_error) {
                continue;
            }
            let rows = c.query(&p)?;
            for row in &rows.rows {
                let cells: Vec<String> = row.iter().map(ddm::sql::Value::render).collect();
                println!("  | {}", cells.join(" | "));
            }
            println!("end: {:?}", rows.end.as_ref().map(|s| (s.sqlcode, s.sqlstate.clone())));
            let ca = c.close(&p, &rows)?;
            println!("close: {}", ca.sqlcode);
        } else {
            let ca = c.execute_immediate(stmt)?;
            println!("immediate: {} {}", ca.sqlcode, ca.sqlstate);
        }
    }
    let ca = c.commit()?;
    println!("commit: {}", ca.sqlcode);
    Ok(())
}
