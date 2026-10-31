// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security displays: `DSPUSRPRF`'s other types (the basic one is
//! `panels::system`), `DSPAUTLOBJ` and `DSPAUDJRNE`.

use cmd::Resolved;
use stg::Client;

use super::{NO_PRINT, dashed, dialog, fetch, fill, list_of, parm, service_text, set_vars, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// `DSPUSRPRF TYPE(*OBJAUT|*OBJOWN|*OBJPGP|*CMDAUT|*DEVAUT|*GRPMBR)`.
pub fn dspusrprf(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    let ty = parm(r, "TYPE");
    let (panel, list) = match ty.as_str() {
        "*OBJAUT" => ("OBJAUT", "AUTS"),
        "*OBJPGP" => ("OBJPGP", "AUTS"),
        "*OBJOWN" => ("OBJOWN", "OWNS"),
        "*CMDAUT" => ("CMDAUT", "CMDS"),
        "*DEVAUT" => ("DEVAUT", "DEVS"),
        "*GRPMBR" => ("GRPMBR", "MBRS"),
        _ => return Some(Err(Fail::later("DSPUSRPRF"))),
    };
    let user = parm(r, "USRPRF");
    let data = read!(&r.name, fetch(host, "UPRF", &[&user, &ty]));
    let mut d = match dialog(host, "QUIUPRFTYP") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    set_vars(&mut d, &data);
    d.set("OBJHEAD", &dashed("Object", 27));
    d.set("DTAHEAD", &dashed("Data", 37));
    d.set("AUTVIEW", "OBJ");
    fill(&mut d, list, list_of(&data, list));
    loop {
        match show(&mut d, surface, panel) {
            Ok(Response::Exit | Response::Cancel | Response::Gone | Response::Return(0)) => {
                return Some(Ok(String::new()));
            }
            Ok(Response::Return(11)) => {
                let data_view = d.view == 0;
                d.view = usize::from(data_view);
                d.set("AUTVIEW", if data_view { "DTA" } else { "OBJ" });
            }
            Ok(Response::Print) => d.message = Some((NO_PRINT.into(), true)),
            Ok(_) => {}
            Err(e) => return Some(Err(e)),
        }
    }
}

/// The objects a `DSPAUTLOBJ` report lists: name, library, type and
/// owner.
fn secured(text: &str) -> Vec<Vec<(String, String)>> {
    text.lines()
        .skip_while(|l| !l.trim_start().starts_with("Object"))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let cut = |a: usize, b: usize| {
                l.get(a..b.min(l.len()))
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };
            vec![
                ("OBJ".into(), cut(1, 11)),
                ("OBJLIB".into(), cut(13, 23)),
                ("OBJTYPE".into(), cut(25, 35)),
                ("OBJOWNER".into(), cut(37, l.len())),
                ("OBJTEXT".into(), String::new()),
            ]
        })
        .collect()
}

/// `DSPAUTLOBJ`: the objects an authorization list secures.
pub fn dspautlobj(host: &Host, surface: &mut Surface, r: &Resolved) -> Option<Result<String, Fail>> {
    let name = parm(r, "AUTL");
    let head = read!("DSPAUTLOBJ", host.link.borrow_mut().with(|l| l.autl_panel(name.clone())));
    let text = match service_text(host, &r.text()) {
        Ok(t) => t,
        Err(e) => return Some(Err(e)),
    };
    let mut d = match dialog(host, "QUIAUTLOBJ") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    d.set("AUTL", &name);
    d.set(
        "OWNER",
        head.first().and_then(|r| r.first()).map_or("", String::as_str),
    );
    fill(&mut d, "OBJS", secured(&text));
    loop {
        match show(&mut d, surface, "DSPAUTLOBJ") {
            Ok(Response::Exit | Response::Cancel | Response::Gone | Response::Return(0)) => {
                return Some(Ok(String::new()));
            }
            Ok(Response::Print) => d.message = Some((NO_PRINT.into(), true)),
            Ok(_) => {}
            Err(e) => return Some(Err(e)),
        }
    }
}

/// `DSPAUDJRNE`: the audit journal entries as the report lists them.
pub fn dspaudjrne(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let text = service_text(host, &r.text())?;
    super::programs::view_report(
        host,
        surface,
        "Display Audit Journal Entries",
        "QPAUDENT",
        &text,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secured_objects_from_the_report() {
        let text = " Authorization list PAYROLL\n Object      Library     Type        Owner\n PAYFILE     PAYLIB      *FILE       QPGMR\n";
        let rows = secured(text);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0].1, "PAYFILE");
        assert_eq!(rows[0][1].1, "PAYLIB");
        assert_eq!(rows[0][2].1, "*FILE");
        assert_eq!(rows[0][3].1, "QPGMR");
    }
}
