// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product over the sample system on a local port, to click
//! through in a browser: `cargo run -p httpadmin --bin httpadmin-demo
//! [PORT]`, then <http://127.0.0.1:2001/qs400/httpadmin/>, user
//! `QSECOFR`, password `QS400`.

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use httpadmin::Admin;
use httpadmin::fixture::fixture;
use qsweb::http::{Peer, serve};

fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(2001);
    let admin = Arc::new(Admin::new(fixture().env));
    let l = TcpListener::bind(("127.0.0.1", port))?;
    println!("http://127.0.0.1:{port}/qs400/httpadmin/  (QSECOFR / {})", httpadmin::fixture::PASSWORD);
    for s in l.incoming() {
        let Ok(mut s) = s else { continue };
        let admin = admin.clone();
        std::thread::spawn(move || {
            let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
            let Ok(addr) = s.peer_addr() else { return };
            serve(&mut s, &Peer { addr, secure: false }, admin.as_ref());
        });
    }
    Ok(())
}
