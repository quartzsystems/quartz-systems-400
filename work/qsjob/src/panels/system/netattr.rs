// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPNETA` (`QUIDSPNETA`) and `DSPSFWRSC` (`QUISFWRSC`).

use stg::Client;

use super::{display};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// `LIB/NAME` as name and library; a special value has no library.
fn split_qual(v: &str) -> (String, String) {
    match v.split_once('/') {
        Some((l, n)) => (n.to_string(), l.to_string()),
        None => (v.to_string(), String::new()),
    }
}

/// `NETID.CPNAME` (or `NETID CPNAME`) as network ID and name.
fn split_net(v: &str) -> (String, String) {
    match v.split_once(['.', ' ']) {
        Some((n, c)) => (n.trim().to_string(), c.trim().to_string()),
        None => (v.to_string(), String::new()),
    }
}

/// `DSPNETA`.
pub fn dspneta(host: &Host, surface: &mut Surface) -> Option<Result<String, Fail>> {
    let attrs = read!("DSPNETA", host.link.borrow_mut().with(Client::neta_panel));
    let mut d = match crate::panels::dialog(host, "QUIDSPNETA") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    for (k, v) in &attrs {
        match k.as_str() {
            "PNDSYSNAME" => d.set(k, if v == "*NONE" { "" } else { v }),
            "MSGQ" | "OUTQ" | "ALRFTR" | "DDMACC" | "PCSACC" => {
                let (n, l) = split_qual(v);
                d.set(k, &n);
                d.set(&format!("{k}LIB"), &l);
            }
            "NETSERVER" => {
                let (n, c) = split_net(v);
                d.set("NETSRVID", &n);
                d.set("NETSRVCP", &c);
            }
            "ALRBCKFP" | "ALRRQSFP" => {
                let net = if k == "ALRBCKFP" { "ALRBCKNET" } else { "ALRRQSNET" };
                if v == "*NONE" {
                    d.set(k, "");
                    d.set(net, "*NONE");
                } else {
                    let (n, c) = split_net(v);
                    d.set(k, &c);
                    d.set(net, &n);
                }
            }
            _ => d.set(k, v),
        }
    }
    Some(display(surface, &mut d, "DSPNETA"))
}

/// `DSPSFWRSC`: the products installed; F11 switches the view.
pub fn dspsfwrsc(host: &Host, surface: &mut Surface) -> Option<Result<String, Fail>> {
    let rows = host.link.borrow_mut().with(Client::sfwrsc_panel);
    let mut d = match crate::panels::dialog(host, "QUISFWRSC") {
        Ok(d) => d,
        Err(e) => return Some(Err(e)),
    };
    for r in rows {
        let get = |i: usize| r.get(i).cloned().unwrap_or_default();
        d.add_row(
            "RESOURCES",
            vec![
                ("RSCID".into(), get(0)),
                ("RSCOPT".into(), get(1)),
                ("RSCFTR".into(), get(2)),
                ("RSCDESC".into(), get(3)),
                ("RSCTYPE".into(), get(4)),
                ("RSCLIB".into(), get(5)),
                ("RSCRLS".into(), get(6)),
            ],
        );
    }
    d.list_mut("RESOURCES").complete = true;
    let mut panel = "DSPSFWRSC";
    loop {
        match d.show(surface, panel) {
            Ok(Response::Return(11)) => {
                panel = if panel == "DSPSFWRSC" { "DSPSFWLIB" } else { "DSPSFWRSC" };
            }
            Ok(Response::Return(19)) => {
                d.message = Some(("This system shows no trademarks.".into(), false));
            }
            Ok(Response::Print) => {
                d.message = Some(("Print is not available in this release.".into(), true));
            }
            Ok(Response::Options(_) | Response::Command(_) | Response::Retrieve) => {}
            Ok(_) => return Some(Ok(String::new())),
            Err(e) => return Some(Err(crate::panels::failed(e.to_string()))),
        }
    }
}
