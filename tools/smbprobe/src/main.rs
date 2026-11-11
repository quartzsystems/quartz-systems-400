// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `smbprobe`, a second SMB test client for QS/400 `NetServer`
//! (`docs/NETSERVER.md` section 11). It is built on the vendored server
//! library's own test helpers (`net/netserver/tests/smbc/`, shared by path,
//! and `crypto.rs`, copied), so it shares authors with the server;
//! `tools/smbtest` is the independent client. It covers what `smbtest`'s
//! crate cannot express: leases, byte-range locks, durable reconnects,
//! print shares, spoolss, share enumeration over NDR 2.0, guest logons,
//! share limits, per-share encryption and the minimum dialect.
//!
//! Each scenario prints `ok NAME` with indented detail lines, `fail NAME:
//! why`, or `skip NAME: why` (an option it needs was not given). The exit
//! status is 1 when any failed.

#![allow(
    clippy::many_single_char_names,
    reason = "the scenarios' clients are a, b and c"
)]

// The helpers are the server library's tests' own, shared by path.
#[path = "../../../net/netserver/tests/smbc/mod.rs"]
mod smbc;

mod conn;
mod crypto;
mod report;
mod scenarios;

use std::io::Write as _;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use conn::Target;

/// How long one scenario may take.
const SCENARIO_TIME: Duration = Duration::from_secs(150);

const USAGE: &str = "usage: smbprobe HOST PORT USER PASSWORD SHARE SCRATCH [--only A,B] \
[--user2 NAME] [--print-share NAME] [--max-share NAME] [--enc-share NAME] [--guest-user NAME] \
[--min-dialect 2.1|3.0|3.0.2|3.1.1] [--driver-file PATH=TEXT] [--ifs-text TEXT] [--list]
PASSWORD '-' reads SMBPROBE_PASSWORD; --user2's password is SMBPROBE_PASSWORD2";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--list") {
        for s in scenarios::ALL {
            println!("{}", s.name);
        }
        return;
    }
    let (target, only) = match parse(&args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("smbprobe: {e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("smbprobe: runtime: {e}");
            std::process::exit(2);
        }
    };
    let target = Arc::new(target);
    let mut failed = false;
    for s in scenarios::ALL {
        if let Some(only) = &only
            && !only.iter().any(|o| o == s.name)
        {
            continue;
        }
        let outcome = match (s.needs)(&target) {
            Some(why) => report::Outcome::Skip(why),
            None => rt.block_on(run_one(s, target.clone())),
        };
        failed |= matches!(outcome, report::Outcome::Fail(_));
        report::print(s.name, &outcome);
        let _ = std::io::stdout().flush();
    }
    std::process::exit(i32::from(failed));
}

/// Runs one scenario on its own task, so a helper's panic is its failure.
async fn run_one(s: &scenarios::Scenario, t: Arc<Target>) -> report::Outcome {
    let task = tokio::spawn((s.run)(t));
    match tokio::time::timeout(SCENARIO_TIME, task).await {
        Err(_) => report::Outcome::Fail(format!("no end in {}s", SCENARIO_TIME.as_secs())),
        Ok(Err(e)) if e.is_panic() => report::Outcome::Fail(report::panic_text(&*e.into_panic())),
        Ok(Err(e)) => report::Outcome::Fail(e.to_string()),
        Ok(Ok(Ok(lines))) => report::Outcome::Ok(lines),
        Ok(Ok(Err(why))) => report::Outcome::Fail(why),
    }
}

/// The target and the `--only` list.
fn parse(args: &[String]) -> Result<(Target, Option<Vec<String>>), String> {
    let mut pos = Vec::new();
    let mut only = None;
    let mut t = conn::Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--only" => only = Some(value()?.split(',').map(str::to_owned).collect()),
            "--user2" => t.user2 = Some(value()?.to_ascii_uppercase()),
            "--print-share" => t.print_share = Some(value()?),
            "--max-share" => t.max_share = Some(value()?),
            "--enc-share" => t.enc_share = Some(value()?),
            "--guest-user" => t.guest_user = Some(value()?),
            "--min-dialect" => {
                let v = value()?;
                t.min_dialect = Some(conn::dialect(&v).ok_or(format!("dialect {v}"))?);
            }
            "--driver-file" => {
                let v = value()?;
                let (p, d) = v.split_once('=').ok_or("--driver-file PATH=TEXT")?;
                t.driver_file = Some((p.to_owned(), d.as_bytes().to_vec()));
            }
            "--ifs-text" => t.ifs_text = Some(value()?),
            s if s.starts_with("--") => return Err(format!("unknown option {s}")),
            s => pos.push(s.to_owned()),
        }
    }
    let [host, port, user, password, share, scratch] = pos.as_slice() else {
        return Err("six positional arguments".into());
    };
    let port: u16 = port.parse().map_err(|_| format!("port {port}"))?;
    let addr: SocketAddr = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|e| format!("{host}: {e}"))?
        .next()
        .ok_or(format!("{host}: no address"))?;
    let password = if password == "-" {
        std::env::var("SMBPROBE_PASSWORD").map_err(|_| "SMBPROBE_PASSWORD not set")?
    } else {
        password.clone()
    };
    if t.user2.is_some() {
        t.password2 =
            Some(std::env::var("SMBPROBE_PASSWORD2").map_err(|_| "SMBPROBE_PASSWORD2 not set")?);
    }
    let target = Target::new(
        addr,
        host,
        &user.to_ascii_uppercase(),
        &password,
        share,
        scratch,
        t,
    );
    Ok((target, only))
}
