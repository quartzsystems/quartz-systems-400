// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compile listing in the ILE RPG layout: the prologue of command
//! options, the source with statement numbers, sequence numbers and
//! change dates and each diagnostic under its statement, the additional
//! diagnostics, the cross-reference, the external references, the
//! message summary, the final summary and the end line. Each record
//! begins with its forms control (`1` new page, ` ` `0` `-` one to
//! three lines), as the spooled file holds it. The words are ours; the
//! layout and the section order are IBM's.

use std::collections::BTreeMap;

use crate::compile::Compiled;
use crate::ctl::Options;
use crate::files::Dev;
use crate::msgs;
use crate::reader_class;
use crate::syms::Kind;

/// Lines on a page before the next heading.
const PAGE: usize = 60;

struct Out {
    lines: Vec<String>,
    page: u32,
    on_page: usize,
    title: String,
}

impl Out {
    fn heading(&mut self) {
        self.page += 1;
        self.lines.push(format!(
            "1{:<64}{:<26}Page {:>5}",
            format!(" 5770WDS {}  QS/400  RN        QS/400 ILE RPG", release::RELEASE),
            self.title,
            self.page
        ));
        self.on_page = 1;
    }

    fn line(&mut self, cc: char, text: impl AsRef<str>) {
        let add = match cc {
            '0' => 2,
            '-' => 3,
            _ => 1,
        };
        if self.on_page + add > PAGE {
            self.heading();
        }
        self.on_page += add;
        self.lines.push(format!("{cc}{}", text.as_ref()));
    }

    fn section(&mut self, title: &str) {
        self.heading();
        self.line('0', format!("{:>26}{title}", ""));
    }
}

fn spaced(s: &str) -> String {
    s.chars().map(|c| c.to_string()).collect::<Vec<_>>().join(" ")
}

/// Build the listing.
#[must_use]
pub fn listing(c: &Compiled, o: &Options) -> Vec<String> {
    let mut out = Out {
        lines: Vec::new(),
        page: 0,
        on_page: 0,
        title: format!("{}/{}", o.lib, o.name),
    };
    prologue(&mut out, c, o);
    source(&mut out, c);
    additional(&mut out, c);
    xref(&mut out, c);
    external(&mut out);
    summary(&mut out, c);
    final_summary(&mut out, c, o);
    out.lines
}

fn opt(out: &mut Out, cc: char, label: &str, value: &str) {
    let dots = format!("{label} ");
    let mut d = dots;
    while d.len() < 34 {
        d.push_str(if d.len() % 2 == 0 { "." } else { " " });
    }
    out.line(cc, format!("  {d}:   {value}"));
}

fn prologue(out: &mut Out, c: &Compiled, o: &Options) {
    out.heading();
    opt(out, '0', "Command", o.command.name());
    opt(out, ' ', "  Issued by", "QSECOFR");
    opt(out, ' ', if o.command == crate::ctl::Command::CrtRpgMod { "Module" } else { "Program" }, &o.name);
    opt(out, ' ', "  Library", &o.lib);
    opt(out, ' ', "Text 'description'", if o.text.is_empty() { "*SRCMBRTXT" } else { &o.text });
    opt(out, '0', "Source Member", &o.src.2);
    opt(out, ' ', "Source File", &o.src.1);
    opt(out, ' ', "  Library", &o.src.0);
    opt(out, ' ', "  CCSID", "37");
    opt(out, '0', "Generation severity level", &c.ctl.genlvl.to_string());
    opt(
        out,
        ' ',
        "Default activation group",
        if c.ctl.dftactgrp { "*YES" } else { "*NO" },
    );
    if !c.ctl.dftactgrp {
        opt(out, ' ', "Activation group", &c.ctl.actgrp);
    }
    opt(out, ' ', "Debugging views", &o.dbgview);
    opt(out, ' ', "Output", &o.output);
    opt(out, ' ', "Optimization level", &o.optimize);
    opt(out, ' ', "Source listing indentation", &c.ctl.indent);
    opt(out, ' ', "Truncate numeric", if c.ctl.truncnbr { "*YES" } else { "*NO" });
    opt(out, ' ', "Target release", &o.tgtrls);
    opt(out, ' ', "Allow null values", &c.ctl.alwnull);
}

fn source(out: &mut Out, c: &Compiled) {
    out.heading();
    out.line(
        '0',
        "Line   <---------------------- Source Specifications ----------------------------------------------------->  Do  Change Src Seq",
    );
    out.line(
        ' ',
        "Number ....+....1....+....2....+....3....+....4....+....5....+....6....+....7....+....8....+....9....+...10  Num Date   Id  Number",
    );
    out.line('0', format!("{:>26}{}", "", spaced("Source Listing")));
    let mut by_line: BTreeMap<u32, Vec<&rpgfront::Diag>> = BTreeMap::new();
    for d in &c.diags {
        by_line.entry(d.at.line).or_default().push(d);
    }
    let mut first = true;
    for l in &c.read.lines {
        if reader_class::skipped(l) {
            continue;
        }
        let text = format!("{:<100}", l.text.chars().take(100).collect::<String>());
        let src_id = if l.member == 0 { "  " } else { "01" };
        out.line(
            if first { '0' } else { ' ' },
            format!("{:>6} {text}  {:>3} {}  {src_id}  {}", l.no, "", l.date, l.seq),
        );
        first = false;
        if let Some(ds) = by_line.get(&l.no) {
            for d in ds.iter().filter(|d| d.at.line > 0) {
                out.line(
                    ' ',
                    format!("*{} {:02} {:>6} {}  {}", d.msgid, d.severity, d.at.line, l.seq, msgs::text(d)),
                );
            }
        }
    }
    out.line('0', format!("{:>7}{}", "", "* * * * *   E N D   O F   S O U R C E   * * * * *"));
}

fn additional(out: &mut Out, c: &Compiled) {
    out.section(&spaced("Additional Diagnostic Messages"));
    out.line('0', " Msg id  Sv Number Seq     Message text");
    for d in c.diags.iter().filter(|d| d.at.line == 0) {
        out.line(' ', format!("*{} {:02} {:>6}         {}", d.msgid, d.severity, 0, msgs::text(d)));
    }
    out.line(
        '0',
        " * * * * *   E N D   O F   A D D I T I O N A L   D I A G N O S T I C   M E S S A G E S   * * * * *",
    );
}

fn xref(out: &mut Out, c: &Compiled) {
    out.section(&spaced("Cross Reference"));
    out.line('0', "     File and Record References:");
    out.line('0', "        File              Device             References (D=Defined)");
    out.line(' ', "          Record");
    if c.files.is_empty() {
        out.line('0', "        No references in the source.");
    }
    for f in &c.files {
        let refs = c
            .table
            .find(f.scope, &f.name)
            .and_then(|id| c.table.refs.get(&id))
            .map(|v| v.iter().map(|r| format!("{:>6}", r.line)).collect::<Vec<_>>().join("  "))
            .unwrap_or_default();
        out.line(
            ' ',
            format!("         {:<18}{:<20}{:>6}D  {refs}", f.name, f.dev.name(), f.def_line),
        );
        let _ = Dev::Disk;
    }
    out.line('0', "     Global Field References:");
    out.line('0', "        Field             Attributes         References (D=Defined M=Modified)");
    let mut names: Vec<(String, usize)> = c
        .table
        .syms
        .iter()
        .enumerate()
        .filter(|(_, s)| s.scope == 0 && matches!(s.kind, Some(Kind::Field | Kind::Ds(_) | Kind::Subfield | Kind::Const(_))))
        .filter(|(_, s)| !s.name.starts_with('_'))
        .map(|(i, s)| (s.name.clone(), i))
        .collect();
    names.sort();
    for (name, id) in names {
        let s = &c.table.syms[id];
        let attr = s.ty.xref(&|i| c.table.ds_len(i));
        let mut refs = vec![format!("{:>6}D", s.def_line)];
        if let Some(v) = c.table.refs.get(&id) {
            for r in v {
                refs.push(format!("{:>6}{}", r.line, if r.modified { "M" } else { " " }));
            }
        }
        out.line(' ', format!("         {name:<18}{attr:<19}{}", refs.join(" ")));
    }
    out.line('0', "     Indicator References:");
    out.line('0', "        Indicator                            References (D=Defined M=Modified)");
    out.line('0', format!("{:>7}{}", "", "* * * * *   E N D   O F   C R O S S   R E F E R E N C E   * * * * *"));
}

fn external(out: &mut Out) {
    out.section(&spaced("External References"));
    out.line('0', "     Statically bound procedures:");
    out.line(' ', "         Procedure                            References");
    out.line(' ', "         No references in the source.");
    out.line('0', "     Imported fields:");
    out.line(' ', "         Field             Attributes         Defined");
    out.line(' ', "         No references in the source.");
    out.line('0', "     Exported fields:");
    out.line(' ', "         Field             Attributes         Defined");
    out.line(' ', "         No references in the source.");
    out.line('0', format!("{:>3}{}", "", "* * * * *   E N D   O F   E X T E R N A L   R E F E R E N C E S   * * * * *"));
}

fn summary(out: &mut Out, c: &Compiled) {
    out.section(&spaced("Message Summary"));
    out.line('0', " Msg id  Sv Number Message text");
    let mut counts: BTreeMap<(&str, u8), usize> = BTreeMap::new();
    for d in &c.diags {
        *counts.entry((d.msgid, d.severity)).or_default() += 1;
    }
    for ((id, sev), n) in counts {
        out.line(' ', format!("*{id} {sev:02} {n:>6} {}", msgs::summary_text(id)));
    }
    out.line('0', format!("{:>7}{}", "", "* * * * *   E N D   O F   M E S S A G E   S U M M A R Y   * * * * *"));
}

fn final_summary(out: &mut Out, c: &Compiled, o: &Options) {
    out.section(&spaced("Final Summary"));
    let n = |lo: u8, hi: u8| c.diags.iter().filter(|d| d.severity >= lo && d.severity <= hi).count();
    out.line('0', " Message Totals:");
    out.line('0', format!("   Information  (00) . . . . . . . :  {:>7}", n(0, 9)));
    out.line(' ', format!("   Warning      (10) . . . . . . . :  {:>7}", n(10, 19)));
    out.line(' ', format!("   Error        (20) . . . . . . . :  {:>7}", n(20, 29)));
    out.line(' ', format!("   Severe Error (30+)  . . . . . . :  {:>7}", n(30, 99)));
    out.line(' ', "   ---------------------------------  -------");
    out.line(' ', format!("   Total . . . . . . . . . . . . . :  {:>7}", c.diags.len()));
    let records = c.read.lines.iter().filter(|l| l.member == 0).count();
    let (specs, comments) = reader_class::counts(&c.read);
    out.line('-', " Source Totals:");
    out.line('0', format!("   Records . . . . . . . . . . . . :  {records:>7}"));
    out.line(' ', format!("   Specifications  . . . . . . . . :  {specs:>7}"));
    out.line(' ', format!("   Data records  . . . . . . . . . :  {:>7}", reader_class::data_records(&c.read)));
    out.line(' ', format!("   Comments  . . . . . . . . . . . :  {comments:>7}"));
    out.line('0', format!("{:>9}{}", "", "* * * * *   E N D   O F   F I N A L   S U M M A R Y   * * * * *"));
    let what = if o.command == crate::ctl::Command::CrtRpgMod { "Module" } else { "Program" };
    if c.created {
        out.line(
            ' ',
            format!(
                "{what} {} placed in library {}. {:02} highest severity. Created on dd-dd-dd at tt:tt:tt.",
                o.name, o.lib, c.max_severity
            ),
        );
    } else {
        out.line(' ', format!("Compilation stopped. Severity {:02} errors found in program.", c.max_severity));
    }
    out.line(' ', format!("{:>10}{}", "", "* * * * *   E N D   O F   C O M P I L A T I O N   * * * * *"));
}
