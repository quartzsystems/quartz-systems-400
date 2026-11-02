// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QAPCNN` in `QUSRWRK`, as `QUSER` until sign-on: one QAP connection.
//! It takes the socket `QAPD` passed it, runs TLS 1.3 with the
//! certificate assigned to `QS400_ACCESS_SERVER`, and serves the
//! connection with `net/qapd` until it closes.

use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::time::Duration;

use ipc::chan::Chan;
use rustls::{ServerConnection, StreamOwned};
use stg::Shared;
use wm::ctl::{FromJob, Start};

use super::host::JobHost;
use super::listener::APP_ID;

/// The connection's TLS stream.
struct Tls(StreamOwned<ServerConnection, TcpStream>);

impl Read for Tls {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for Tls {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl qapd::Transport for Tls {
    fn close(&mut self) {
        self.0.conn.send_close_notify();
        let _ = self.0.flush();
        let _ = self.0.sock.shutdown(Shutdown::Both);
    }
}

/// The socket the listener passed over the control channel.
fn take_socket(ctl: &Chan) -> Result<TcpStream, String> {
    let chan = ctl.try_clone().map_err(|e| e.to_string())?;
    let via = sockets::Fd::from(chan.into_fd());
    let fd = sockets::sys::recv_fd(&via).map_err(|e| format!("{e:?}"))?;
    Ok(TcpStream::from(fd.into_owned()))
}

/// Serve one connection. Never returns.
pub fn run(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let sock = match take_socket(&ctl) {
        Ok(s) => s,
        Err(e) => crate::start::end(&mut ctl, &format!("no connection: {e}")),
    };
    let config = match crate::work::tls::assigned_tls13(&mut link.borrow_mut(), APP_ID) {
        Ok(c) => c,
        Err(e) => crate::start::end(&mut ctl, &e),
    };
    let tls = match ServerConnection::new(Arc::clone(&config)) {
        Ok(c) => c,
        Err(e) => crate::start::end(&mut ctl, &format!("TLS: {e}")),
    };
    let _ = sock.set_read_timeout(Some(Duration::from_millis(10)));
    let _ = sock.set_nodelay(true);
    stg::remote::status(link, wm::job::Status::Timw, "");
    let mut host = JobHost::new(std::rc::Rc::clone(link), start.number, &ctl);
    let why = qapd::serve(Tls(StreamOwned::new(tls, sock)), &mut host, qapd::Config::default());
    let _ = ctl.send(&FromJob::Ending {
        reason: format!("{why:?}"),
    });
    std::process::exit(0)
}
