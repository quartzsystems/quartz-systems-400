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
