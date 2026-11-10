// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `smbtest HOST PORT USER PASSWORD SHARE SCRATCH [options]` runs the
//! `NetServer` scenarios and prints `ok`, `fail` and `skip` lines; `smbtest probe
//! HOST PORT` negotiates only (no sign-on). Exits 1 when
//! any scenario failed, 2 on a usage error.

use smbtest::{Error, Target, scenarios, target::USAGE};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [cmd, host, port] = args.as_slice()
        && cmd == "probe"
    {
        let Ok(port) = port.parse() else {
            eprintln!("bad port {port}");
            std::process::exit(2);
        };
        print!("{}", scenarios::probe::run(&Target::new(host, port, "", "", "", "")).await);
        return;
    }
    let target = match Target::from_args(&args) {
        Ok(Some(t)) => t,
        Ok(None) => {
            for n in scenarios::names() {
                println!("{n}");
            }
            return;
        }
        Err(e @ Error::Usage(_)) => {
            eprintln!("{e}\n{USAGE}");
            std::process::exit(2);
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let report = scenarios::run_all(&target).await;
    print!("{report}");
    if !report.failures().is_empty() {
        std::process::exit(1);
    }
}
