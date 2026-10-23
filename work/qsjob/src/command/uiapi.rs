// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The UIM APIs in the job (`sysapi::uiapi` over the job's display and
//! the panel groups the storage service opens), printing panels and
//! help to `QSYSPRT`, help search indexes, and the commands that make
//! them: `CRTSCHIDX`, `ADDSCHIDXE`.

use std::cell::RefCell;
use std::rc::Rc;

use cmd::Resolved;
use mi::Job;
use stg::Client;
use sysapi::uiapi::{SharedUimHost, UimHost};
use uim::engine::{IndexEntry, Terminal};

use super::{Fail, parms_of};
use crate::console::host::Host;
use crate::display::surface::Surface;

/// The APIs' view of the job.
struct JobUim {
    host: Host,
}

impl UimHost for JobUim {
    fn panel_group(
        &mut self,
        lib: &str,
        name: &str,
    ) -> Result<(Rc<uim::PanelGroup>, String), String> {
        let lib = if lib.is_empty() { "*LIBL" } else { lib };
        crate::panels::group(&self.host, lib, name)
            .map(|g| (g, lib.to_string()))
            .map_err(|f| f.text)
    }

    fn terminal(&mut self) -> Option<Box<dyn Terminal>> {
        // Only a job that owns a 5250 device shows panels.
        if self.host.display.borrow().is_none() {
            return None;
        }
        Some(Box::new(Surface::new(self.host.clone())))
    }

    fn system_name(&mut self) -> String {
        crate::display::dds::system_name(&self.host)
    }
}

/// Install the UIM APIs into the job.
pub fn install(job: &mut Job, host: &Host) {
    let h: SharedUimHost = Rc::new(RefCell::new(JobUim { host: host.clone() }));
    sysapi::uiapi::install(job, &h);
}

/// A search index's topics through the storage service.
pub fn search_index(host: &Host, lib: &str, name: &str) -> Option<Vec<IndexEntry>> {
    let rows = host
        .link
        .borrow_mut()
        .with(|l| l.schidx_entries(name.to_string(), lib.to_string()))
        .ok()?;
    Some(
        rows.into_iter()
            .map(|(title, roots, pnlgrp, module)| IndexEntry {
                title,
                roots,
                pnlgrp,
                module,
            })
            .collect(),
    )
}

/// Rows printed into a new spooled file of `QSYSPRT`: the message the
/// display shows.
pub fn print_rows(host: &Host, _title: &str, rows: &[String]) -> Result<String, String> {
    use mi::io::spool::{PrinterState, SpoolSink};
    let mut sink = stg::remote::spool::RemoteSpool::new(host.link.clone());
    let open = sink
        .open("QSYSPRT", "*LIBL", "*DFTACTGRP")
        .map_err(|e| format!("{} ended the print.", e.msgid))?;
    let mut p = PrinterState::new(open);
    for line in rows {
        let rec: Vec<u8> = line
            .chars()
            .map(|c| ccsid::byte_of(c).unwrap_or(0x40))
            .collect();
        p.write(&mut sink, "", &rec)
            .map_err(|e| format!("{} ended the print.", e.msgid))?;
    }
    p.close(&mut sink)
        .map_err(|e| format!("{} ended the print.", e.msgid))?;
    Ok("Display printed to QSYSPRT.".into())
}

/// `CRTSCHIDX` and `ADDSCHIDXE`.
pub fn command(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let parms = parms_of(r);
    let (name, lib) = parms.qual("SCHIDX");
    let reply = if r.name == "CRTSCHIDX" {
        let text = match parms.text("TEXT").as_str() {
            "*BLANK" => String::new(),
            t => t.to_string(),
        };
        let replace = parms.name("REPLACE") == "*YES";
        host.link
            .borrow_mut()
            .with(|l| l.schidx_create(name.clone(), lib.clone(), text, replace))
    } else {
        let (g, gl) = parms.qual("PNLGRP");
        host.link
            .borrow_mut()
            .with(|l| l.schidx_add(name.clone(), lib.clone(), g, gl))
    };
    reply.map_err(|e| {
        let id = e
            .split_whitespace()
            .next()
            .filter(|w| w.len() == 7)
            .unwrap_or("CPF6E12")
            .to_string();
        Fail::new(&id, e)
    })
}
