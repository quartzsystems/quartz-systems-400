// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Journal commands added for V1R1M0: `CMPJRNIMG`, and the entries `RCVJRNE` hands its exit program in the job
//! (panel data `JRNE`, each `*TYPE1` record in hex).

use std::fmt::Write as _;

use cmd::block::Parms;
use stg::proto::{CmdMsg, CmdReply};

use super::escape;
use crate::ipl::system::System;
use crate::svc::db::journal;

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    Some(match name {
        "CMPJRNIMG" => cmpjrnimg(system, parms),
        _ => return None,
    })
}

fn failed(name: &str, e: &str) -> CmdReply {
    let id = super::leading_msgid(e).unwrap_or("CPF9898").to_string();
    escape(&id, name, e.get(8..).unwrap_or(e).trim())
}

fn qualified(parms: &Parms, kwd: &str) -> String {
    let (n, l) = parms.qual(kwd);
    if l.is_empty() || l == "*LIBL" {
        n
    } else {
        format!("{l}/{n}")
    }
}

/// `CMPJRNIMG`: the differences printed to `QPCMPIMG`, `CPC7029`.
fn cmpjrnimg(system: &mut System, parms: &Parms) -> CmdReply {
    const NAME: &str = "CMPJRNIMG";
    // The receiver range and entry selection beyond the member and
    // record are the journal's whole chain here.
    for k in [
        "FROMENTLRG",
        "FROMENT",
        "FROMTIME",
        "TOENTLRG",
        "TOENT",
        "TOTIME",
        "JOB",
        "PGM",
        "USRPRF",
        "CCIDLRG",
        "CMTCYCID",
    ] {
        if parms.specified(k) {
            // BLOCKED(rel): CMPJRNIMG's entry selection (sequence numbers,
            // times, job, program, user, commit cycle) is not taken yet.
            return escape("CPF3C3C", k, "");
        }
    }
    if parms.specified("RCVRNG") && parms.name("RCVRNG") != "*CURRENT" {
        // BLOCKED(rel): a receiver range other than *CURRENT is not taken.
        return escape("CPF3C3C", "RCVRNG", "");
    }
    let file = qualified(parms, "FILE");
    let mbr = parms.name("MBR");
    let after = parms.name("CMPOPT") == "*AFTER";
    let rcd = parms
        .int("RCDNBR")
        .and_then(|n| u64::try_from(n).ok())
        .filter(|n| *n > 0);
    let hex = parms.name("OUTFMT") == "*HEX";
    let (changes, compared) = match journal::cmpjrnimg(system, (&file, &mbr), after, rcd) {
        Ok(r) => r,
        Err(e) => return failed(NAME, &e),
    };
    let member = if mbr.is_empty() {
        "*FIRST".to_string()
    } else {
        mbr
    };
    let shown = |b: &[u8]| -> String {
        if hex {
            b.iter().fold(String::new(), |mut s, x| {
                let _ = write!(s, "{x:02X}");
                s
            })
        } else {
            ccsid::from_ebcdic(b)
        }
    };
    let opt = if after { "*AFTER" } else { "*BOTH" };
    let fmt = if hex { "*HEX" } else { "*CHAR" };
    let mut lines = vec![
        txt(system, "TXT0B20", &[&file, &member]),
        txt(system, "TXT0B21", &[opt, fmt]),
        String::new(),
    ];
    for c in &changes {
        let (rrn, first, second) = (
            c.rrn.to_string(),
            c.seqs.0.to_string(),
            c.seqs.1.to_string(),
        );
        lines.push(txt(system, "TXT0B22", &[&rrn, &first, &second]));
        for (off, before, after) in &c.runs {
            lines.push(format!(
                "  {}",
                txt(system, "TXT0B23", &[&off.to_string(), &shown(before)])
            ));
            lines.push(format!("  {}", txt(system, "TXT0B24", &[&shown(after)])));
        }
    }
    if changes.is_empty() {
        lines.push(txt(system, "TXT0B25", &[]));
    }
    let mut reply = match crate::svc::security::print(system, "QPCMPIMG", &lines) {
        Ok(t) => CmdReply {
            text: t,
            ..CmdReply::default()
        },
        Err(e) => return failed(NAME, &e),
    };
    let count = compared.to_string();
    reply.messages.push(CmdMsg {
        msgid: "CPC7029".into(),
        kind: 4,
        data: cmd::error::data(&[
            ("", 10),
            ("", 10),
            (&member, 10),
            ("", 10),
            ("", 10),
            ("", 10),
            (&count, 10),
        ]),
    });
    reply
}

fn txt(system: &mut System, id: &str, values: &[&str]) -> String {
    crate::stgsvc::msgs::text(system, id, values)
}

/// The job's data (request 270): `JRNE`, the entries of the `dspjrn`
/// filter line `args[0]`, each a row `REC` (hex), and `LAST`, the last
/// sequence number.
pub fn panel(
    system: &mut System,
    what: &str,
    args: &[String],
) -> Option<Result<stg::proto::Sys2Data, String>> {
    if what != "JRNE" {
        return None;
    }
    let line = args.first().cloned().unwrap_or_default();
    Some(
        journal::type1_entries(system, &line).map(|(records, last)| {
            let rows = records
                .iter()
                .map(|r| {
                    let hex = r.iter().fold(String::new(), |mut s, b| {
                        let _ = write!(s, "{b:02X}");
                        s
                    });
                    vec![("REC".to_string(), hex)]
                })
                .collect();
            (
                vec![("LAST".to_string(), last.to_string())],
                vec![("ENTRIES".to_string(), rows)],
            )
        }),
    )
}
