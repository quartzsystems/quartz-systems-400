// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKHDWRSC` and `DSPHDWRSC` where no panel shows them (batch, the
//! service console, `OUTPUT(*PRINT)`, `OUTPUT(*OUTFILE)`): the list of
//! a type from the hardware resource service, as text, a spooled file
//! or records of IBM's model format `QRZALL` (file `QARZALLF`), field
//! for field as the V7R5M0 oracle's `DSPFFD` shows it
//! (`tools/conformance/golden/phase11/hwrsrc/dspffd-qarzallf-*.txt`).

use cmd::block::Parms;
use hwrsrc::view::{self, TypeSpec};
use stg::proto::CmdReply;

use super::direct::{failed, plain};
use super::savout::{Fld, Row, stamp, write};
use crate::ipl::system::{System, now_unix};

/// `QRZALL`, 70 fields, 425 bytes.
const QRZALL: &[Fld] = &[
    ("DORCEN", 'A', 1, 0), ("DORDAT", 'A', 6, 0), ("DORTIM", 'A', 6, 0),
    ("DOSNAM", 'A', 8, 0), ("DOSTYP", 'A', 4, 0), ("DOSMOD", 'A', 3, 0),
    ("DOSSER", 'A', 10, 0), ("DORECF", 'A', 1, 0), ("DOSVRM", 'A', 6, 0),
    ("DORSVD", 'A', 36, 0), ("DORSVF", 'A', 2, 0), ("DOCRPF", 'A', 1, 0),
    ("DOCSAF", 'A', 1, 0), ("DOCMNF", 'A', 1, 0), ("DOLWSF", 'A', 1, 0),
    ("DOSTGF", 'A', 1, 0), ("DOPRCF", 'A', 1, 0), ("DORLVL", 'A', 1, 0),
    ("DORDSC", 'A', 2, 0), ("DORNAM", 'A', 10, 0), ("DORPAR", 'A', 10, 0),
    ("DORTYP", 'A', 4, 0), ("DORMOD", 'A', 3, 0), ("DORPRT", 'A', 12, 0),
    ("DORSER", 'A', 10, 0), ("DORDSA", 'A', 4, 0), ("DORUAA", 'A', 8, 0),
    ("DORSTS", 'A', 1, 0), ("DORRID", 'A', 2, 0), ("DOREIA", 'A', 2, 0),
    ("DORCSL", 'A', 3, 0), ("DORDSL", 'A', 4, 0), ("DOCFGO", 'A', 10, 0),
    ("DOCFGP", 'A', 10, 0), ("DOREDS", 'A', 2, 0), ("DORSVC", 'A', 8, 0),
    ("DOSYTM", 'A', 8, 0), ("DOSMTP", 'A', 4, 0), ("DOSMDL", 'A', 3, 0),
    ("DOSSRN", 'A', 10, 0), ("DORSVA", 'A', 8, 0), ("DORKBD", 'A', 3, 0),
    ("DORCOL", 'A', 1, 0), ("DORSWD", 'A', 1, 0), ("DORIWS", 'A', 1, 0),
    ("DORPOR", 'A', 2, 0), ("DORSWT", 'A', 2, 0), ("DORSVL", 'A', 8, 0),
    ("DORMSZ", 'P', 10, 0), ("DORSVP", 'A', 8, 0), ("DORAFI", 'A', 4, 0),
    ("DORACP", 'A', 5, 0), ("DORADP", 'A', 5, 0), ("DORTTY", 'A', 2, 0),
    ("DORTF1", 'A', 4, 0), ("DORTF2", 'A', 4, 0), ("DORTF3", 'A', 4, 0),
    ("DORTFR", 'A', 8, 0), ("DORUAT", 'A', 2, 0), ("DORUA1", 'A', 4, 0),
    ("DORUA2", 'A', 4, 0), ("DORUA3", 'A', 4, 0), ("DORUA4", 'A', 4, 0),
    ("DORUA5", 'A', 4, 0), ("PRCFCD", 'A', 4, 0), ("PRCFD", 'A', 4, 0),
    ("PRCIFD", 'A', 4, 0), ("LOCCOD", 'A', 79, 0), ("SWTMOD", 'A', 1, 0),
    ("DORESR", 'A', 15, 0),
];

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "WRKHDWRSC" => listing(system, parms, false),
        "DSPHDWRSC" => listing(system, parms, true),
        _ => return None,
    };
    Some(match r {
        Ok(text) => plain(text),
        Err(e) => failed(name, &e),
    })
}

/// The type's list: text, printed, or (`DSPHDWRSC`) an outfile.
fn listing(system: &mut System, parms: &Parms, dsp: bool) -> Result<String, String> {
    let ty = parms.name("TYPE");
    let spec = view::spec(&ty).ok_or_else(|| format!("CPD0084 {ty} is not a type of resource."))?;
    let t = hwrsrc::client::table(false).map_err(|e| format!("CPF9898 hardware resource service: {e}"))?;
    let output = if dsp { parms.name("OUTPUT") } else { "*PRINT".into() };
    match output.as_str() {
        "*OUTFILE" => outfile(system, parms, &t, spec),
        "*PRINT" => {
            let file = if dsp { "DSPHDWRSC" } else { "WRKHDWRSC" };
            let lines = view::print_lines(&t, spec);
            crate::svc::security::print_as(system, "QSYSPRT", file, &lines, false)?;
            Ok(format!("{} resources printed.\n", spec.word))
        }
        _ => Ok(view::print_lines(&t, spec).join("\n") + "\n"),
    }
}

/// One `QRZALL` record per resource the list shows.
fn outfile(system: &mut System, parms: &Parms, t: &hwrsrc::Table, spec: &TypeSpec) -> Result<String, String> {
    let now = now_unix();
    let st = stamp(now);
    let (cen, date, time) = (st[..1].to_string(), st[1..7].to_string(), st[7..13].to_string());
    let sysname = system.system_name();
    let serial = crate::stgsvc::security::sysval_text(system, "QSRLNBR");
    let model = crate::stgsvc::security::sysval_text(system, "QMODEL");
    let rel = {
        let r = release::RELEASE;
        format!("V{}R{}M{}", r.v, r.r, r.m)
    };
    let flag = |s: &str| if spec.ty == "*AHW" || spec.ty == s { "1" } else { "0" }.to_string();
    let mut rows: Vec<Row> = Vec::new();
    for row in view::list(t, spec) {
        let Some(r) = t.get(&row.name) else { continue };
        let (ty, md) = r.type_model();
        let mib = if r.kind == hwrsrc::Kind::Memory {
            r.description.split_whitespace().find_map(|w| w.parse::<u64>().ok()).unwrap_or(0)
        } else {
            0
        };
        rows.push(vec![
            ("DORCEN", cen.clone()),
            ("DORDAT", date.clone()),
            ("DORTIM", time.clone()),
            ("DOSNAM", sysname.clone()),
            ("DOSTYP", "QS40".into()),
            ("DOSMOD", model.chars().take(3).collect()),
            ("DOSSER", serial.chars().take(10).collect()),
            ("DOSVRM", rel.clone()),
            ("DOCRPF", flag("*CRP")),
            ("DOCSAF", flag("*CSA")),
            ("DOCMNF", flag("*CMN")),
            ("DOLWSF", flag("*LWS")),
            ("DOSTGF", flag("*STG")),
            ("DOPRCF", flag("*PRC")),
            ("DORLVL", (row.depth + 1).min(9).to_string()),
            ("DORNAM", r.name.clone()),
            ("DORPAR", r.parent.clone()),
            ("DORTYP", ty),
            ("DORMOD", md),
            ("DORSER", r.serial.chars().take(10).collect()),
            ("DORSTS", if r.operational() { "1" } else { "2" }.into()),
            ("DORMSZ", mib.to_string()),
            ("LOCCOD", view::location(r)),
        ]);
    }
    let note = write(system, parms, "QRZALL", QRZALL, &rows)?;
    Ok(note)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_format_is_the_oracles() {
        let len: usize = QRZALL
            .iter()
            .map(|(_, t, l, _)| if *t == 'P' { usize::from(*l) / 2 + 1 } else { usize::from(*l) })
            .sum();
        assert_eq!(QRZALL.len(), 70);
        assert_eq!(len, 425);
    }
}
