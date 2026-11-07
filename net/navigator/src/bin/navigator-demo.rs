// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The product over the sample system on a local port, to click
//! through in a browser: `cargo run -p navigator --bin navigator-demo
//! [PORT]`, then <http://127.0.0.1:2002/qs400/navigator/>, user
//! `QSECOFR`, password `QS400`.

use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use navigator::Navigator;
use navigator::fixture::fixture;
use qsweb::http::{Peer, serve};

fn main() -> std::io::Result<()> {
    let port: u16 = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(2002);
    let nav = Arc::new(Navigator::new(fixture().env));
    let l = TcpListener::bind(("127.0.0.1", port))?;
    println!(
        "http://127.0.0.1:{port}/qs400/navigator/  (QSECOFR / {})",
        navigator::fixture::PASSWORD
    );
    for s in l.incoming() {
        let Ok(mut s) = s else { continue };
        let nav = nav.clone();
        std::thread::spawn(move || {
            let _ = s.set_read_timeout(Some(Duration::from_secs(30)));
            let Ok(addr) = s.peer_addr() else { return };
            serve(
                &mut s,
                &Peer {
                    addr,
                    secure: false,
                },
                nav.as_ref(),
            );
        });
    }
    Ok(())
}
