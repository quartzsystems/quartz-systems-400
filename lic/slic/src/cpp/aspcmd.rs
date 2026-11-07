// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! ASP balancing and spool storage: `STRASPBAL`, `ENDASPBAL`,
//! `CHKASPBAL` and `RCLSPLSTG`, with the primary's parameters
//! (`tools/conformance/oracle/rel5/commands-primary.txt`). The system
//! has ASP 1 only and no independent ASPs.
//!
//! - `TYPE(*CAPACITY)` moves objects off the fuller units until every
//!   unit is within a percent of the ASP's average (`store`'s
//!   `balance_step`), a few objects every pass of `QSYSARB5` until it
//!   is done, `TIMLMT` runs out or `ENDASPBAL` ends it.
//! - `*USAGE` needs the trace `TRCASPBAL` gathers and `*HSM` and `*MP`
//!   units of two kinds: there is no trace and every unit is one kind,
//!   so they end with `CPD18AA` reasons 6 and 7, as IBM i answers.
//! - `BLOCKED(rel)`: `*ENDALC`, `*RSMALC` and `*MOVDTA` need an end of
//!   allocation the configuration keeps across an IPL; the store's
//!   allocator keeps it only while a unit is removed (`CPF18BB`).

use cmd::block::Parms;
use stg::proto::{CmdMsg, CmdReply};

use crate::ipl::system::{System, now_unix};

/// The ASP's name in the messages: the system ASP has none.
const ASP_NAME: &str = "*SYSBAS";

/// A command of this module, or `None` for another.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    // Balancing is the security officer's: *ALLOBJ, as on IBM i.
    if matches!(name, "STRASPBAL" | "ENDASPBAL")
        && let Err(e) = crate::stgsvc::security::require(sec::profile::SpecialAuth::ALLOBJ, name)
    {
        return Some(super::direct::plain(e));
    }
    Some(match name {
        "STRASPBAL" => straspbal(system, parms),
        "ENDASPBAL" => endaspbal(system, parms),
        "CHKASPBAL" => chkaspbal(system),
        "RCLSPLSTG" => rclsplstg(system, parms),
        _ => return None,
    })
}

fn msg(msgid: &str, kind: u8, fields: &[(&str, usize)]) -> CmdMsg {
    CmdMsg {
        msgid: msgid.into(),
        kind,
        data: cmd::error::data(fields),
    }
}

/// A reply that is one escape.
fn escape(msgid: &str, fields: &[(&str, usize)]) -> CmdReply {
    CmdReply {
        escape: Some(msg(msgid, 7, fields)),
        ..CmdReply::default()
    }
}

/// A reply with messages and no escape.
fn messages(m: Vec<CmdMsg>) -> CmdReply {
    CmdReply {
        messages: m,
        ..CmdReply::default()
    }
}

/// `CPD18AA` for ASP 1 with a reason code.
fn balance_error(reason: &str) -> CmdReply {
    escape("CPD18AA", &[(reason, 2), ("1", 5), (ASP_NAME, 10)])
}

/// The ASPs `ASP` and `ASPDEV` name: ASP 1, the only one, or none;
/// the escape when they do not.
fn names_asp1(parms: &Parms) -> Option<CmdReply> {
    let asps = parms.list("ASP");
    let devs = parms.list("ASPDEV");
    if asps.is_empty() && devs.is_empty() {
        return Some(escape("CPD18A3", &[]));
    }
    let asp_ok = asps
        .iter()
        .all(|v| matches!(v.source().to_ascii_uppercase().as_str(), "*ALL" | "0" | "1"));
    let dev_ok = devs
        .iter()
        .all(|v| v.source().eq_ignore_ascii_case("*ALLAVL"));
    (!(asp_ok && dev_ok)).then(|| escape("CPD18AC", &[]))
}

fn straspbal(system: &mut System, parms: &Parms) -> CmdReply {
    let ty = parms.name("TYPE");
    if ty.is_empty() {
        return escape("CPD18A2", &[]);
    }
    let unit = !parms.list("UNIT").is_empty();
    match ty.as_str() {
        "*ENDALC" | "*RSMALC" => {
            if !unit {
                return escape("CPD18A5", &[]);
            }
            return escape("CPF18BB", &[]);
        }
        "*MOVDTA" => {
            if unit || !parms.list("ASP").is_empty() || !parms.list("ASPDEV").is_empty() {
                return escape("CPD18A4", &[]);
            }
            return escape("CPF18BB", &[]);
        }
        _ => {}
    }
    if unit {
        return escape("CPD18A8", &[]);
    }
    if let Some(r) = names_asp1(parms) {
        return r;
    }
    let limit = parms.text("TIMLMT");
    if limit.is_empty() {
        return escape("CPD18A6", &[]);
    }
    if system.aspbal.is_some() {
        return escape("CPF18B2", &[("1", 5), (ASP_NAME, 10)]);
    }
    match ty.as_str() {
        "*USAGE" => return balance_error("6"),
        "*HSM" | "*MP" => return balance_error("7"),
        _ => {}
    }
    let spans = system
        .storage
        .as_ref()
        .map_or(0, |s| s.store.span_fill().len());
    if spans < 2 {
        return balance_error("5");
    }
    let until = match limit.to_ascii_uppercase().as_str() {
        "*NOMAX" | "-1" => None,
        m => Some(now_unix() + m.parse::<u64>().unwrap_or(1) * 60),
    };
    system.aspbal = Some((store::BalanceCursor::default(), until));
    messages(vec![msg(
        "CPC18A5",
        4,
        &[("1", 5), (ASP_NAME, 10), ("*CAPACITY", 10)],
    )])
}

fn endaspbal(system: &mut System, parms: &Parms) -> CmdReply {
    if parms.list("ASP").is_empty() && parms.list("ASPDEV").is_empty() {
        return escape("CPD18A1", &[]);
    }
    if let Some(r) = names_asp1(parms) {
        return r;
    }
    if system.aspbal.take().is_none() {
        return balance_error("4");
    }
    messages(vec![msg("CPC18A6", 4, &[("1", 5), (ASP_NAME, 10)])])
}

fn chkaspbal(system: &System) -> CmdReply {
    let bal = if system.aspbal.is_some() {
        msg("CPI18A5", 4, &[("*CAPACITY", 10), ("1", 5), (ASP_NAME, 10)])
    } else {
        msg("CPI18A6", 4, &[("1", 5), (ASP_NAME, 10)])
    };
    messages(vec![bal, msg("CPI18A8", 4, &[("1", 5), (ASP_NAME, 10)])])
}

/// One pass of a running `*CAPACITY` balance, from `QSYSARB5`'s watch;
/// `CPC18A6` to the history log when it ends.
pub fn balance_tick(system: &mut System) {
    let Some((mut cursor, until)) = system.aspbal else {
        return;
    };
    let expired = until.is_some_and(|u| now_unix() >= u);
    let done = expired
        || system.storage.as_mut().is_none_or(|s| {
            s.store
                .balance_step(&mut cursor, 64)
                .map_or(true, |step| step.done)
        });
    if done {
        system.aspbal = None;
        let who = ("QSYSARB5".to_string(), "QSYS".to_string(), 0);
        let data = cmd::error::data(&[("1", 5), (ASP_NAME, 10)]);
        crate::stgsvc::hist::system_message(system, "CPC18A6", &data, "04", who);
    } else {
        system.aspbal = Some((cursor, until));
    }
}

fn rclsplstg(system: &mut System, parms: &Parms) -> CmdReply {
    let days = match parms.name("DAYS").as_str() {
        "*NONE" | "0" => None,
        d => match d.parse::<u32>() {
            Ok(n) if (1..=366).contains(&n) => Some(n),
            _ => return escape("CPD0084", &[]),
        },
    };
    match parms.name("ASPGRP").as_str() {
        "" | "*SYSBAS" | "*ALL" | "*" => {}
        "*CURASPGRP" => return escape("CPFB8E9", &[("*CURASPGRP", 10), ("*", 10)]),
        dev => return escape("CPF9825", &[(dev, 10)]),
    }
    let _ = crate::stgsvc::spool::reclaim(system, days);
    CmdReply::default()
}
