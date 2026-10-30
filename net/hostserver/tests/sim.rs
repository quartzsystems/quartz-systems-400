// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every server on 127.0.0.1 over the backends in memory, for running
//! the `JTOpen` suite against without a system:
//! `QS_HS_SIM_SECS=300 QS_HS_USER=.. QS_HS_PASS=.. cargo test -p hostserver --test sim -- --ignored`.

mod common;

use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use common::mem::{Host, MemQueues, MemSpool, ifs, session};
use hostserver::central::Central;
use hostserver::database::{Database, DbConfig};
use hostserver::dtaq::DtaqServer;
use hostserver::file::FileServer;
use hostserver::netprt::{NetPrint, Splf};
use hostserver::rmtcmd::{CallResult, Commands, JobMessage, RmtCmd};
use hostserver::signon::Signon;
use hostserver::{Handler, NoExits};

struct Job;

impl Commands for Job {
    fn run(&mut self, command: &str) -> (bool, Vec<JobMessage>) {
        if command.to_ascii_uppercase().starts_with("CHKOBJ OBJ(QSYS/QSYSOPR)") {
            (true, Vec::new())
        } else {
            (
                false,
                vec![JobMessage {
                    id: "CPF0001".into(),
                    kind: 15,
                    severity: 30,
                    file: ("QCPFMSG".into(), "QSYS".into()),
                    text: "The simulator runs no commands.".into(),
                    ..JobMessage::default()
                }],
            )
        }
    }
    fn call(&mut self, program: &str, _library: &str, _parms: &mut [Vec<u8>]) -> CallResult {
        let mut data = vec![0x02, 0x01];
        data.extend(hostserver::text::ebcdic_fixed(program, 30));
        data.extend([0; 16]);
        CallResult::Failed(vec![JobMessage {
            id: "MCH3401".into(),
            kind: 15,
            severity: 40,
            data,
            ..JobMessage::default()
        }])
    }
}

fn host() -> Host {
    let level = std::env::var("QS_HS_SIM_PWDLVL").ok().and_then(|l| l.parse().ok()).unwrap_or(0);
    let user = std::env::var("QS_HS_USER").unwrap_or_else(|_| "QSECOFR".into()).to_uppercase();
    let pass = std::env::var("QS_HS_PASS").unwrap_or_default();
    let mut h = Host::new(level, &user, &pass);
    h.seed = [0x31, 0x41, 0x59, 0x26, 0x53, 0x58, 0x97, 0x93];
    h
}

fn serve(port: u16, s: TcpStream) {
    let mut s = s;
    if port == 449 {
        let _ = hostserver::io::serve_svrmap(&mut s);
        return;
    }
    let h = host();
    let job = |n: &str| (n.to_string(), "QUSER".to_string(), "000123".to_string());
    let mut handler: Box<dyn Handler> = match port {
        8476 => Box::new(Signon::new(h, NoExits)),
        8470 => Box::new(Central::new(h, NoExits)),
        8471 => Box::new(Database::new(h, session(), NoExits, DbConfig { rdb: "QS400SIM".into(), job: job("QZDASOINIT") })),
        8472 => Box::new(DtaqServer::new(h, MemQueues::default(), NoExits)),
        8473 => Box::new(FileServer::new(h, ifs(), NoExits)),
        8474 => Box::new(NetPrint::new(h, MemSpool(Vec::<Splf>::new()), NoExits, job("QNPSERVS"))),
        _ => Box::new(RmtCmd::new(h, Job, NoExits)),
    };
    if let Err(e) = hostserver::io::serve(&mut s, &mut *handler) {
        eprintln!("port {port}: {e}");
    }
}

#[test]
#[ignore = "serves until QS_HS_SIM_SECS pass"]
fn sim() {
    let secs = std::env::var("QS_HS_SIM_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    let end = Instant::now() + Duration::from_secs(secs);
    for port in [449, 8470, 8471, 8472, 8473, 8474, 8475, 8476] {
        let l = TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|e| panic!("port {port}: {e}"));
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                std::thread::spawn(move || serve(port, s));
            }
        });
    }
    eprintln!("host servers on 127.0.0.1 for {secs}s");
    while Instant::now() < end {
        std::thread::sleep(Duration::from_millis(200));
    }
}
