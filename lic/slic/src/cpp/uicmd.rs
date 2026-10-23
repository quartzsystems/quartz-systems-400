// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The user-interface object commands: `CRTPNLGRP` compiles a UIM
//! source member into a `*PNLGRP`, `CRTMNU` makes a `*MENU` over a
//! panel group (or a display file or program, kept as attributes).

use cmd::block::Parms;
use qsname::Name;
use stg::proto::CmdReply;

use super::direct::{failed, in_store, lib_or, library, member_text, plain};
use crate::ipl::system::{System, now_unix};
use crate::svc::ui;

/// Run one of the commands.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> CmdReply {
    let r = match name {
        "CRTPNLGRP" => crtpnlgrp(system, parms),
        "CRTMNU" => crtmnu(system, parms),
        _ => Err(format!("CPF0006 {name} has no processing program")),
    };
    match r {
        Ok(text) => plain(text),
        Err(e) => failed(name, &e),
    }
}

fn stamp(system: &System) -> wm::obj::Stamp {
    wm::obj::Stamp {
        owner: system.svc_user.clone().unwrap_or_else(|| "QSYS".into()),
        system: system.system_name(),
        now: now_unix(),
    }
}

fn crtpnlgrp(system: &mut System, parms: &Parms) -> Result<String, String> {
    let (n, l) = parms.qual("PNLGRP");
    let (sf, sfl) = parms.qual("SRCFILE");
    let mbr = match parms.name("SRCMBR").as_str() {
        "" | "*PNLGRP" => n.clone(),
        m => m.to_string(),
    };
    let text = parms.text("TEXT");
    let replace = parms.name("REPLACE") != "*NO";
    let st = stamp(system);
    in_store(system, |txn, qsys, names| {
        let lib = library(txn, qsys, names, &l)?;
        let name = Name::parse(&n).map_err(|e| format!("CPD0078 PNLGRP({n}): {e}"))?;
        let source = member_text(txn, qsys, names, &sf, &sfl, &mbr)?;
        let (payload, _) = ui::compile(&source, &n, &lib_or(&l))
            .map_err(|listing| format!("CPF6301 Panel group {n} not created.\n{listing}"))?;
        let text = if text.is_empty() || text == "*SRCMBRTXT" || text == "*BLANK" {
            String::new()
        } else {
            text
        };
        ui::create_pnlgrp(txn, lib, name, &text, &st, &payload, replace)?;
        Ok(format!(
            "Panel group {n} created in library {}.\n",
            lib_or(&l)
        ))
    })
}

fn crtmnu(system: &mut System, parms: &Parms) -> Result<String, String> {
    let (n, l) = parms.qual("MENU");
    let kind = menus::MenuKind::parse(&parms.name("TYPE")).unwrap_or_default();
    let qual = |kwd: &str| -> Option<menus::Qualified> {
        let (qn, ql) = parms.qual(kwd);
        match qn.as_str() {
            "" | "*MENU" => Some(menus::Qualified::new(&lib_or(&l), &n)),
            _ => Some(menus::Qualified::new(&lib_or(&ql), &qn)),
        }
    };
    let mut attrs = menus::MenuAttrs::uim(
        qual("PNLGRP").unwrap_or_else(|| menus::Qualified::new(&lib_or(&l), &n)),
        &parms.text("TEXT"),
    );
    attrs.kind = kind;
    match kind {
        menus::MenuKind::Uim => {}
        menus::MenuKind::Dspf => {
            attrs.pnlgrp = None;
            attrs.dspf = qual("DSPF");
            attrs.msgf = qual("MSGF");
        }
        menus::MenuKind::Pgm => {
            attrs.pnlgrp = None;
            attrs.pgm = qual("PGM");
        }
    }
    attrs.cmdlin = menus::CmdLin::parse(&parms.name("CMDLIN")).unwrap_or_default();
    attrs.curlib = menus::CurLib::parse(&parms.name("CURLIB"));
    attrs.prdlib = menus::PrdLib::parse(&parms.name("PRDLIB"));
    if attrs.text == "*BLANK" {
        attrs.text.clear();
    }
    attrs
        .validate()
        .map_err(|e| format!("{e}"))?;
    let replace = parms.name("REPLACE") != "*NO";
    let st = stamp(system);
    in_store(system, |txn, qsys, names| {
        let lib = library(txn, qsys, names, &l)?;
        let name = Name::parse(&n).map_err(|e| format!("CPD0078 MENU({n}): {e}"))?;
        ui::create_menu(txn, lib, name, &attrs, &st, replace)?;
        Ok(format!("Menu {n} created in library {}.\n", lib_or(&l)))
    })
}
