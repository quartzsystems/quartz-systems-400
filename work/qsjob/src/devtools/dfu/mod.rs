// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Data File Utility in the job: `STRDFU` and its menus and entry
//! panels, DFU program definitions (`*PGM` objects of attribute `DFU`,
//! requests 361-363), `UPDDTA`'s temporary program, `CHGDTA` and
//! `DSPDTA` running a DFU program. Records go through the open data
//! path the storage service keeps for the job, as a program's file
//! operations do. The displays are the `dfu` crate's.

mod data;
mod menu;
mod temp;
pub(crate) mod ui;

use cmd::Resolved;
use mi::Job;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::devtools::pdm::{parm, qual};
use crate::display::surface::Surface;
use crate::panels::shows_panels;

/// A DFU command at a display.
pub fn run(job: &mut Job, host: &Host, r: &Resolved, out: &mut Outcome) -> Result<String, Fail> {
    let _ = out;
    if !shows_panels(host, r) {
        return Err(Fail::new(
            "CPF9898",
            format!("{} needs a display; it cannot run in this job.", r.name),
        ));
    }
    let mut surface = Surface::new(host.clone());
    match r.name.as_str() {
        "STRDFU" => {
            let option = parm(r, "OPTION").unwrap_or_else(|| "*SELECT".into());
            let (pgm, pgmlib) = qual(r, "DFUPGM");
            let (file, filelib) = qual(r, "FILE");
            let mut v = menu::Values::default();
            v.set("PGM", pgm.unwrap_or_default());
            v.set("PGMLIB", pgmlib.unwrap_or_else(|| "*CURLIB".into()));
            v.set("FILE", file.unwrap_or_default());
            v.set("FILELIB", filelib.unwrap_or_else(|| "*LIBL".into()));
            v.set("MBR", parm(r, "MBR").unwrap_or_else(|| "*FIRST".into()));
            menu::strdfu(job, host, &mut surface, &option, v);
            Ok(String::new())
        }
        "UPDDTA" => {
            let (file, lib) = qual(r, "FILE");
            let file = file.ok_or_else(|| Fail::new("CPD0071", "A data file is needed."))?;
            let lib = lib.unwrap_or_else(|| "*LIBL".into());
            let f = data::file(host, &lib, &file).ok_or_else(|| not_found(&file, &lib))?;
            temp::run(host, &mut surface, &f, member(r), None, false)
                .map_err(|m| Fail::new("CPF4101", m))?;
            Ok(String::new())
        }
        "DSPDTA" | "CHGDTA" => {
            let (pgm, pgmlib) = qual(r, "PGM");
            let pgm = pgm.ok_or_else(|| Fail::new("CPD0071", "A DFU program is needed."))?;
            let pgmlib = pgmlib.unwrap_or_else(|| "*LIBL".into());
            let (_, p) = data::program(host, &pgmlib, &pgm)
                .map_err(|m| Fail::new("CPF9898", m))?
                .ok_or_else(|| {
                    Fail::new(
                        "CPF9801",
                        format!("DFU program {pgm} in library {pgmlib} not found."),
                    )
                })?;
            let (file, lib) = qual(r, "FILE");
            let (file, lib) = match file.as_deref() {
                None | Some("*SAME") => (p.file.clone(), p.file_lib.clone()),
                Some(f) => (f.to_string(), lib.unwrap_or_else(|| "*LIBL".into())),
            };
            let f = data::file(host, &lib, &file).ok_or_else(|| not_found(&file, &lib))?;
            temp::run(host, &mut surface, &f, member(r), Some(&p), r.name == "DSPDTA")
                .map_err(|m| Fail::new("CPF4101", m))?;
            Ok(String::new())
        }
        other => Err(Fail::new("CPF9898", format!("{other} is not a DFU command."))),
    }
}

fn member(r: &Resolved) -> Option<String> {
    parm(r, "MBR").filter(|m| m != "*FIRST")
}

fn not_found(file: &str, lib: &str) -> Fail {
    Fail::new(
        "CPF9812",
        format!("Data file {file} in library {lib} not found."),
    )
}
