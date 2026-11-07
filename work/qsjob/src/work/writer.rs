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
//! again after a pause. A TN5250E printer session's device gets each
//! file as one RFC 2877 print job; a file completes only when the client
//! answered every record. A refusal leaves the file ready and the writer
//! in `MSGW` on an inquiry; a lost session leaves the file ready and the
//! writer waiting for a session to hold the device again (the session
//! that ends ends the writer).

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

/// TLS for `ipps` printers: the printer's chain is checked against the
/// shipped roots and the trusted CAs of `*SYSTEM` (a printer with a
/// self-signed certificate needs it imported there as a CA, `QSIMPCRT`),
/// and its name against the host the device names.
struct PrinterTls(Result<Arc<rustls::ClientConfig>, String>);

/// A TLS connection to a printer.
struct TlsPrinter(tls::stream::TlsStream<std::net::TcpStream>);

impl std::io::Read for TlsPrinter {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl std::io::Write for TlsPrinter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl Stream for TlsPrinter {
    fn finish(&mut self) -> std::io::Result<()> {
        self.0.close().map_err(std::io::Error::other)?;
        self.0.get_ref().shutdown(std::net::Shutdown::Write)
    }
}

impl TlsConnector for PrinterTls {
    fn connect(&self, host: &str, tcp: std::net::TcpStream) -> std::io::Result<Box<dyn Stream>> {
        let config = self
            .0
            .clone()
            .map_err(|e| std::io::Error::other(format!("{host}: no TLS client: {e}")))?;
        let name = tls::config::server_name(host).map_err(std::io::Error::other)?;
        let s = tls::stream::TlsStream::connect(config, name, tcp)
            .map_err(|e| std::io::Error::other(format!("{host}: {e}")))?;
        Ok(Box::new(TlsPrinter(s)))
    }
}

/// How long a printer session's client has to answer a record.
const ANSWER: Duration = Duration::from_secs(120);

/// The printer session's socket, as `tn5250::print::deliver` sees it.
struct SessionDevice(ipc::chan::Chan);

impl tn5250::print::Device for SessionDevice {
    fn send(&mut self, record: Vec<u8>) -> Result<(), String> {
        self.0
            .send(&wm::dev::ToDevice::Print { record })
            .map_err(|_| "the printer session went away".to_string())
    }

    fn recv(&mut self) -> Result<Vec<u8>, String> {
        match self.0.recv::<wm::dev::FromDevice>() {
            Ok(wm::dev::FromDevice::Printed { record }) => Ok(record),
            Ok(wm::dev::FromDevice::Record { .. } | wm::dev::FromDevice::Reset { .. }) => {
                Ok(Vec::new())
            }
            Ok(wm::dev::FromDevice::Gone) => Err("the printer session went away".into()),
            Err(_) => Err("the printer session did not answer".into()),
        }
    }
}

/// Send a spooled file to the printer session holding `device` as one
/// print job (RFC 2877): its records, each answered, then the null
/// record. `Ok` only when the client took all of it.
fn send_to_session(device: &str, scs: &[u8]) -> Result<(), tn5250::print::Failure> {
    use std::os::unix::net::UnixStream;
    use tn5250::print::Failure;
    let path = wm::dev::socket(device);
    let stream = UnixStream::connect(&path).map_err(|e| Failure::Lost(format!("{path}: {e}")))?;
    stream
        .set_read_timeout(Some(ANSWER))
        .map_err(|e| Failure::Lost(e.to_string()))?;
    let mut dev = SessionDevice(ipc::chan::Chan::new(stream));
    let mut sent = 0;
    let r = tn5250::print::deliver(&mut dev, scs, PRINT_CHUNK, &mut sent);
    let _ = dev.0.send(&wm::dev::ToDevice::Release);
    r
}

/// Whether a printer session holds `device` now.
fn session_up(device: &str) -> bool {
    std::path::Path::new(&wm::dev::socket(device)).exists()
}

/// Run writer `name` until it is ended; the end code.
pub fn run(c: &mut Console, link: &Shared, name: &str) -> u16 {
    let dest = link
        .borrow_mut()
        .with(|l| l.wtr_target(name.to_string()))
        .unwrap_or(WtrDest::Capture);
    let target = target_of(&dest);
    // Only a secure printer needs the trusted CAs (and the service's work).
    let config = if matches!(dest, WtrDest::Lan { secure: true, .. }) {
        super::tls::client_config(&mut link.borrow_mut())
    } else {
        Err("the device does not ask for TLS".into())
    };
    let tls: Arc<dyn TlsConnector> = Arc::new(PrinterTls(config));
    let session = match &dest {
        WtrDest::Session { device } => Some(device.clone()),
        _ => None,
    };
    loop {
        // A printer session's writer takes no file while no session
        // holds its device.
        if session.as_deref().is_some_and(|d| !session_up(d)) {
            status(link, Status::Evtw, "");
            if !c.idle(IDLE) {
                return 0;
            }
            let ending = link
                .borrow_mut()
                .with(Client::wtr_list)
                .iter()
                .any(|w| w.0 == name && w.2 == "END");
            if ending {
                return 0;
            }
            continue;
        }
        let step = link.borrow_mut().with(|l| l.wtr_next(name.to_string()));
        match step {
            WtrStep::End => return 0,
            WtrStep::Wait => {
                status(link, Status::Evtw, "");
                if !c.idle(IDLE) {
                    return 0;
                }
            }
            WtrStep::MsgWait => {
                status(link, Status::Msgw, "");
                if !c.idle(IDLE) {
                    return 0;
                }
            }
            WtrStep::File { attrs, data } => {
                status(link, Status::Run, &format!("PRT-{}", attrs.name));
                if let Some(device) = &session {
                    // After a lost session, give its end time to show.
                    if print_on_session(link, name, device, &attrs, &data) && !c.idle(LOST) {
                        return 0;
                    }
                    continue;
                }
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

/// How long a writer waits after its printer session went away.
const LOST: Duration = Duration::from_secs(2);

/// A file to a printer session: complete only when the client took all
/// of it; otherwise ready again, with a message or an inquiry. Whether
/// the session was lost.
fn print_on_session(
    link: &Shared,
    name: &str,
    device: &str,
    attrs: &SplfAttrs,
    data: &[u8],
) -> bool {
    use tn5250::print::Failure;
    let mut was_lost = false;
    let r = match send_to_session(device, data) {
        Ok(()) => link
            .borrow_mut()
            .with(|l| l.wtr_printed(name.to_string(), attrs.id, String::new(), Vec::new())),
        Err(f) => {
            eprintln!("qsjob: writer {name}: {device}: {}", f.text());
            let lost = matches!(f, Failure::Lost(_));
            was_lost = lost;
            let r = link.borrow_mut().with(|l| {
                l.wtr_device_error(name.to_string(), attrs.id, lost, f.text(), f.diag_hex())
            });
            if !lost {
                status(link, Status::Msgw, "");
            }
            r
        }
    };
    if let Err(e) = r {
        eprintln!("qsjob: writer {name}: {e}");
    }
    was_lost
}

/// The capture device: the file's text and PDF to the store.
fn capture(link: &Shared, name: &str, attrs: &SplfAttrs, data: &[u8]) {
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
