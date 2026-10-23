// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS/QSPWTR`, the print writer a writer job runs: it asks the
//! storage service for the next ready file on its queue, renders it,
//! and hands back what it printed. The capture device's rendering is
//! the file's text and a PDF of it.

use std::time::Duration;

use stg::remote::status;
use stg::{Client, Shared};
use wm::job::Status;
use wm::wtr::WtrStep;

use crate::console::Console;

/// How long an idle writer waits before it looks again.
const IDLE: Duration = Duration::from_millis(500);

/// The most SCS bytes in one printer record.
const PRINT_CHUNK: usize = 4000;

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
                // A TN5250E printer session listens on the device's
                // socket: the SCS goes to the client as printer records.
                if let Err(e) = send_to_device(name, &data) {
                    eprintln!("qsjob: writer {name}: printer session: {e}");
                }
                let (pages, fmt) = spool::scs::decode(&data);
                let text = spool::view::text(&pages);
                let pdf = spool::pdf::render(&pages, fmt);
                let r = link
                    .borrow_mut()
                    .with(|l| l.wtr_printed(name.to_string(), attrs.id, text, pdf));
                if let Err(e) = r {
                    eprintln!("qsjob: writer {name}: {e}");
                }
            }
        }
    }
}
