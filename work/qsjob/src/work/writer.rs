// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS/QSPWTR`, the print writer a writer job runs: it asks the
//! storage service where it prints and then, file by file, for the next
//! ready one on its queue. The capture device renders the file's text
//! and a PDF of it into the store; a LAN printer or a remote output
//! queue gets it through the `print` crate (transform, then the raw
//! port, IPP or LPR). A failed send is reported to the service, which
//! puts the file back and shows the writer in `MSGW`; the writer tries
//! again after a pause.

use std::sync::Arc;
use std::time::Duration;

use print::sink::{Stream, TlsConnector};
use stg::remote::status;
use stg::{Client, Shared};
use wm::job::Status;
use wm::obj::splf::SplfAttrs;
use wm::wtr::{WtrDest, WtrStep};

use crate::console::Console;
use crate::printmap::{send_network, target_of};

/// How long an idle writer waits before it looks again.
const IDLE: Duration = Duration::from_millis(500);

/// How long a writer waits after a failed send before it tries again.
const RETRY: Duration = Duration::from_secs(30);

/// The most SCS bytes in one printer record.
const PRINT_CHUNK: usize = 4000;

/// TLS for `ipps` printers. `BLOCKED(phase10)`: the client TLS engine
/// (`net/tls`) is not in this tree yet; until it is, a secure printer
/// connection fails with the TLS message.
struct NoTls;

impl TlsConnector for NoTls {
    fn connect(&self, host: &str, _tcp: std::net::TcpStream) -> std::io::Result<Box<dyn Stream>> {
        Err(std::io::Error::other(format!(
            "{host}: no TLS client is configured for printers yet"
        )))
    }
}

/// Send a spooled file's SCS to the printer session holding device
/// `name`, one record at a time, each answered by the client. A device
/// without a session (the capture device) has no socket: nothing to do.
/// `BLOCKED(phase8)`: the record framing follows RFC 2877 as read; no
/// printer client could verify it.
fn send_to_device(name: &str, scs: &[u8]) -> Result<(), String> {
    use std::os::unix::net::UnixStream;
    let path = wm::dev::socket(name);
    if !std::path::Path::new(&path).exists() {
        return Ok(());
    }
    let stream = UnixStream::connect(&path).map_err(|e| format!("{path}: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    let mut chan = ipc::chan::Chan::new(stream);
    for chunk in scs.chunks(PRINT_CHUNK) {
        chan.send(&wm::dev::ToDevice::Write {
            opcode: 0,
            data: chunk.to_vec(),
        })
        .map_err(|e| e.to_string())?;
        match chan.recv::<wm::dev::FromDevice>() {
            Ok(wm::dev::FromDevice::Record { .. } | wm::dev::FromDevice::Reset { .. }) => {}
            Ok(wm::dev::FromDevice::Gone) | Err(_) => return Err("the printer session went away".into()),
        }
    }
    let _ = chan.send(&wm::dev::ToDevice::Release);
    Ok(())
}

/// Run writer `name` until it is ended; the end code.
pub fn run(c: &mut Console, link: &Shared, name: &str) -> u16 {
    let dest = link
        .borrow_mut()
        .with(|l| l.wtr_target(name.to_string()))
        .unwrap_or(WtrDest::Capture);
    let target = target_of(&dest);
    let tls: Arc<dyn TlsConnector> = Arc::new(NoTls);
    loop {
        let step = link.borrow_mut().with(|l| l.wtr_next(name.to_string()));
        match step {
            WtrStep::End => return 0,
            WtrStep::Wait => {
                status(link, Status::Evtw, "");
                if !c.idle(IDLE) {
                    return 0;
                }
            }
            WtrStep::File { attrs, data } => {
                status(link, Status::Run, &format!("PRT-{}", attrs.name));
                let Some(t) = &target else {
                    capture(link, name, &attrs, &data);
                    continue;
                };
                match send_network(t, &attrs, &dest, &data, tls.clone()) {
                    Ok(_) => {
                        let r = link.borrow_mut().with(|l| {
                            l.wtr_printed(name.to_string(), attrs.id, String::new(), Vec::new())
                        });
                        if let Err(e) = r {
                            eprintln!("qsjob: writer {name}: {e}");
                        }
                    }
                    Err((msgid, detail, hold)) => {
                        eprintln!("qsjob: writer {name}: {msgid} {detail}");
                        let _ = link.borrow_mut().with(|l| {
                            l.wtr_failed(name.to_string(), attrs.id, msgid, detail, hold)
                        });
                        status(link, Status::Msgw, "");
                        if !c.idle(RETRY) {
                            return 0;
                        }
                    }
                }
            }
        }
    }
}

/// The capture device: the file to a TN5250E printer session when one
/// holds the device, and its text and PDF to the store.
fn capture(link: &Shared, name: &str, attrs: &SplfAttrs, data: &[u8]) {
    if let Err(e) = send_to_device(name, data) {
        eprintln!("qsjob: writer {name}: printer session: {e}");
    }
    let (pages, fmt) = spool::scs::decode(data);
    let text = spool::view::text(&pages);
    let pdf = spool::pdf::render(&pages, fmt);
    let r = link
        .borrow_mut()
        .with(|l| l.wtr_printed(name.to_string(), attrs.id, text, pdf));
    if let Err(e) = r {
        eprintln!("qsjob: writer {name}: {e}");
    }
}

