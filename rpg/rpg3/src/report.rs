// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The conversion report `CVTRPGSRC CVTRPT(*YES)` prints, in IBM's
//! layout: the command's parameters, then per member the files, the
//! messages with the records they concern and the final summary of
//! message and record totals. Each record begins with its forms
//! control. The message texts are ours.

use crate::cvt::Conversion;

/// The command's parameters as the report shows them.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)] // the command's four *YES/*NO parameters
pub struct Params {
    /// The user who issued the command.
    pub user: String,
    /// `FROMFILE` library and file, `FROMMBR`.
    pub from: (String, String, String),
    /// `TOFILE` library and file, `TOMBR` as given.
    pub to: (String, String, String),
    /// `LOGFILE` (`*NONE` or `lib/file`), `LOGMBR`.
    pub log: (String, String),
    /// `EXPCPY`, `CVTRPT`, `SECLVL`, `INSRTPL`.
    pub expcpy: bool,
    /// `CVTRPT`.
    pub cvtrpt: bool,
    /// `SECLVL`.
    pub seclvl: bool,
    /// `INSRTPL`.
    pub insrtpl: bool,
    /// The heading's date and time.
    pub when: String,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            user: "QSECOFR".into(),
            from: ("QGPL".into(), "QRPGSRC".into(), "MAIN".into()),
            to: ("QGPL".into(), "QRPGLESRC".into(), "*FROMMBR".into()),
            log: ("*NONE".into(), "*FIRST".into()),
            expcpy: false,
            cvtrpt: true,
            seclvl: false,
            insrtpl: false,
            when: String::new(),
        }
    }
}

/// Our text of a conversion message.
#[must_use]
pub fn text(id: &str) -> &'static str {
    match id {
        "RNM0501" => "A data format of this kind has no ILE RPG equivalent.",
        "RNM0507" => "The file continuation option is not converted.",
        "RNM0511" => "The specification type is not known; the record is copied.",
        _ => "Conversion message.",
    }
}

fn yes(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

/// A page heading.
fn heading(p: &Params, page: u32) -> String {
    format!("1QS00RPG  {}  CVTRPGSRC                                      {}   Page {page:>4}", release::RELEASE, p.when)
}

/// The report's records for one member.
#[must_use]
pub fn report(c: &Conversion, p: &Params) -> Vec<String> {
    let (log_file, log_lib) = match p.log.0.split_once('/') {
        Some((l, f)) => (f.to_string(), l.to_string()),
        None => (p.log.0.clone(), String::new()),
    };
    let mut out = vec![
        heading(p, 1),
        "0  Command  . . . . . . . . . . . . :   CVTRPGSRC".to_string(),
        format!("     Issued by  . . . . . . . . . . :     {}", p.user),
        format!("0  From file  . . . . . . . . . . . :   {}", p.from.1),
        format!("     Library  . . . . . . . . . . . :     {}", p.from.0),
        format!("   From member  . . . . . . . . . . :   {}", p.from.2),
        format!("0  To file. . . . . . . . . . . . . :   {}", p.to.1),
        format!("     Library  . . . . . . . . . . . :     {}", p.to.0),
        format!("   To member  . . . . . . . . . . . :   {}", p.to.2),
        format!("0  Log file . . . . . . . . . . . . :   {log_file}"),
        format!("     Library  . . . . . . . . . . . :     {log_lib}").trim_end().to_string(),
        format!("   Log member . . . . . . . . . . . :   {}", p.log.1),
        format!("0  Expand copy members. . . . . . . :   {}", yes(p.expcpy)),
        format!("   Print conversion report  . . . . :   {}", yes(p.cvtrpt)),
        format!("   Include second level text. . . . :   {}", yes(p.seclvl)),
        format!("   Insert specification template. . :   {}", yes(p.insrtpl)),
        heading(p, 2),
    ];
    let to_mbr = if p.to.2 == "*FROMMBR" { p.from.2.clone() } else { p.to.2.clone() };
    out.push(format!("0  From file  . . . . . . . . . . . :   {}/{}({})", p.from.0, p.from.1, p.from.2));
    out.push(format!("   To file. . . . . . . . . . . . . :   {}/{}({to_mbr})", p.to.0, p.to.1));
    out.push(format!("   Log file . . . . . . . . . . . . :   {}", p.log.0));
    out.push("-                       C o n v e r s i o n    R e p o r t".to_string());
    for m in &c.messages {
        out.push(format!("0  {:>6}  *{} {:02}  {}", m.at + 1, m.id, m.severity, text(m.id)));
    }
    let count = |lo: u8, hi: u8| c.messages.iter().filter(|m| m.severity >= lo && m.severity <= hi).count();
    let (info, warn, severe) = (count(0, 9), count(10, 29), count(30, 99));
    let highest = c.messages.iter().map(|m| m.severity).max().unwrap_or(0);
    out.extend([
        "-                            F i n a l   S u m m a r y".to_string(),
        "0  Message Totals:".to_string(),
        format!("0    Information  (00) . . . . . . . :  {info:>7}"),
        format!("     Warning      (10) . . . . . . . :  {warn:>7}"),
        format!("     Severe Error (30+)  . . . . . . :  {severe:>7}"),
        "     ---------------------------------  -------".to_string(),
        format!("     Total . . . . . . . . . . . . . :  {:>7}", info + warn + severe),
        "-  Source Totals:".to_string(),
        format!("0    Original Records Read . . . . . . :  {:>7}", c.counts.records),
        format!("     Converted Records Written . . . . :  {:>7}", c.counts.written),
        format!("     Highest Severity Message Issued . :  {highest:>7}"),
        "-         * * * * *   E N D   O F   F I N A L   S U M M A R Y   * * * * *".to_string(),
        "-          * * * * *   E N D   O F   C O N V E R S I O N   * * * * *".to_string(),
    ]);
    out
}
