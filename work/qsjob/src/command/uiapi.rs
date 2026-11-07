// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The UIM APIs in the job (`sysapi::uiapi` over the job's display and
//! the panel groups the storage service opens), printing panels and
//! help to `QSYSPRT`, help search indexes, and the commands that make
//! them: `CRTSCHIDX`, `ADDSCHIDXE`, `RMVSCHIDXE`, and the displays over
//! them: `STRSCHIDX`, `DSPHLPDOC`.

use std::cell::RefCell;
use std::rc::Rc;

use cmd::Resolved;
use mi::Job;
use stg::Client;
use sysapi::uiapi::{SharedUimHost, UimHost};
use uim::engine::{IndexEntry, LineMsg, Terminal};

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
pub fn print_rows(host: &Host, _title: &str, rows: &[String]) -> Result<LineMsg, LineMsg> {
    use mi::io::spool::{PrinterState, SpoolSink};
    let mut sink = stg::remote::spool::RemoteSpool::new(host.link.clone());
    let open = sink
        .open("QSYSPRT", "*LIBL", "*DFTACTGRP")
        .map_err(|e| LineMsg::error(&e.msgid, &[]))?;
    let mut p = PrinterState::new(open);
    for line in rows {
        let rec: Vec<u8> = line
            .chars()
            .map(|c| ccsid::byte_of(c).unwrap_or(0x40))
            .collect();
        p.write(&mut sink, "", &rec)
            .map_err(|e| LineMsg::error(&e.msgid, &[]))?;
    }
    p.close(&mut sink)
        .map_err(|e| LineMsg::error(&e.msgid, &[]))?;
    Ok(LineMsg::info(uim::engine::msgid::PRINTED, &[]))
}

/// `CRTSCHIDX`, `ADDSCHIDXE` and `RMVSCHIDXE`.
pub fn command(host: &Host, r: &Resolved) -> Result<String, Fail> {
    let parms = parms_of(r);
    let (name, lib) = parms.qual("SCHIDX");
    let reply = if r.name == "CRTSCHIDX" {
        let title = parms.text("TITLE");
        let text = match parms.text("TEXT").as_str() {
            "*BLANK" => String::new(),
            "*TITLE" => title,
            t => t.to_string(),
        };
        host.link
            .borrow_mut()
            .with(|l| l.schidx_create(name.clone(), lib.clone(), text, false))
    } else if r.name == "RMVSCHIDXE" {
        let g = parms.name("PNLGRP");
        host.link
            .borrow_mut()
            .with(|l| l.schidx_remove(name.clone(), lib.clone(), g))
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

/// `STRSCHIDX` (the search index panels; `*USER` is the system's index,
/// `QHSS1`) and `DSPHLPDOC` (help module `HLPLBL` of panel group `DOC`
/// in library `FLR`, the library list when blank; `*FIRST` is the
/// group's first module), full screen.
pub fn display(host: &Host, r: &Resolved) -> Result<String, Fail> {
    if host.display.borrow().is_none() {
        return Err(Fail::new("CPF6A1E", format!("{} needs a display.", r.name)));
    }
    let parms = parms_of(r);
    let system = crate::display::dds::system_name(host);
    let mut term = Surface::new(host.clone());
    let mut e = uim::engine::Engine::new(Rc::new(uim::PanelGroup::default()), &system);
    e.full_screen_help = true;
    if r.name == "STRSCHIDX" {
        let (name, lib) = parms.qual("SCHIDX");
        let qualified = if name.is_empty() || name == "*USER" {
            "*LIBL/QHSS1".to_string()
        } else {
            format!("{}/{name}", if lib.is_empty() { "*LIBL" } else { &lib })
        };
        e.start_search(&mut term, &qualified);
        return Ok(String::new());
    }
    let doc = parms.name("DOC");
    let lib = match parms.text("FLR").trim() {
        "" => "*LIBL".to_string(),
        l => l.to_ascii_uppercase(),
    };
    let g = crate::panels::group(host, &lib, &doc).map_err(|f| Fail::new("CPF6E3B", f.text))?;
    let label = parms.text("HLPLBL").trim().to_ascii_uppercase();
    let module = if label.is_empty() || label == "*FIRST" {
        g.helps
            .first()
            .map(|h| h.name.clone())
            .ok_or_else(|| Fail::new("CPF6E3B", format!("Panel group {doc} has no help.")))?
    } else {
        label
    };
    let shown = e.show_help(&mut term, &[format!("{lib}/{doc}/{module}")], false, true);
    if shown == Some(false) {
        return Err(Fail::new(
            "CPF6E3B",
            format!("No help module {module} in {doc}."),
        ));
    }
    Ok(String::new())
}
