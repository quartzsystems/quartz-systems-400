// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `CRTRPGPGM` listing in RPG/400's sections: the command options,
//! the source by statement number with each diagnostic after its
//! statement, the additional diagnostics, the diagnostics of the
//! cross-reference, the message summary and the final summary. Each
//! record begins with its forms control. The texts are ours.

use rpgfront::Member;

use super::{Compiled, Options, Place, text};

/// A heading line.
fn heading(o: &Options, page: u32) -> String {
    format!(
        "1 QS00RPG  {}  RPG/400  {}/{}  {:>50}Page {page:>4}",
        release::RELEASE,
        o.pgm.1,
        o.pgm.0,
        ""
    )
}

/// The listing.
#[allow(clippy::too_many_lines)]
pub(super) fn listing(main: &Member, c: &Compiled, o: &Options) -> Vec<String> {
    let mut out = Vec::new();
    let mut page = 1;
    out.push(heading(o, page));
    out.push("0 Compiler . . . . . . . . . . . . . :   QS/400 RPG/400".into());
    out.push("0 Command Options:".into());
    for (label, value) in [
        ("Program", format!("{}/{}", o.pgm.1, o.pgm.0)),
        ("Source file", format!("{}/{}", o.src.0, o.src.1)),
        ("Source member", o.src.2.clone()),
        ("Source listing options", o.option.join(" ")),
        ("Generation options", o.genopt.join(" ")),
        ("Generation severity level", o.genlvl.to_string()),
        (
            "Replace program",
            if o.replace { "*YES" } else { "*NO" }.to_string(),
        ),
        ("Target release", o.tgtrls.clone()),
        ("User profile", o.usrprf.clone()),
        ("Text", o.text.clone()),
        (
            "Ignore decimal data error",
            if o.igndecerr { "*YES" } else { "*NO" }.to_string(),
        ),
        (
            "Allow null values",
            if o.alwnull { "*YES" } else { "*NO" }.to_string(),
        ),
    ] {
        out.push(format!("    {:<33}:   {value}", dots(label)));
    }
    page += 1;
    out.push(heading(o, page));
    out.push("0                          S o u r c e   L i s t i n g".into());
    let mut in_data = false;
    for r in &main.records {
        let stmt = r.seq.trim().parse::<u32>().unwrap_or(0);
        if r.text.starts_with("**") {
            in_data = true;
        }
        let body: String = r.text.chars().skip(5).collect();
        if in_data {
            out.push(format!("            {}", r.text));
            continue;
        }
        out.push(format!(" {stmt:>9}  {body}"));
        for d in c
            .diags
            .iter()
            .filter(|d| d.place == Place::Source && d.stmt == stmt)
        {
            out.push(format!(" * {}", &d.msgid[3..]));
        }
    }
    out.push("-           * * * * *   E N D   O F   S O U R C E   * * * * *".into());
    let additional: Vec<_> = c
        .diags
        .iter()
        .filter(|d| d.place == Place::Additional)
        .collect();
    if !additional.is_empty() {
        out.push("-          A d d i t i o n a l   D i a g n o s t i c   M e s s a g e s".into());
        for d in additional {
            out.push(format!(
                "0* {}  {:>8}   {}",
                &d.msgid[3..],
                d.stmt,
                text(&d.msgid)
            ));
        }
    }
    page += 1;
    out.push(heading(o, page));
    out.push("-                         C r o s s   R e f e r e n c e".into());
    for d in c.diags.iter().filter(|d| d.place == Place::Xref) {
        let name = d.args.first().cloned().unwrap_or_default();
        out.push(format!(" * {}  {name:<10}  {:>6}", &d.msgid[3..], d.stmt));
    }
    out.push("       * * * * *   E N D   O F   C R O S S   R E F E R E N C E   * * * * *".into());
    page += 1;
    out.push(heading(o, page));
    out.push("-                         M e s s a g e   S u m m a r y".into());
    let mut ids: Vec<(String, u8, usize)> = Vec::new();
    for d in &c.diags {
        match ids.iter_mut().find(|(m, _, _)| *m == d.msgid) {
            Some(e) => e.2 += 1,
            None => ids.push((d.msgid.clone(), d.severity, 1)),
        }
    }
    ids.sort();
    for (id, sev, n) in &ids {
        out.push(format!("0* {id} Severity:  {sev:02}   Number: {n:>4}"));
        out.push(format!("           Message . . . . :   {}", text(id)));
    }
    out.push("0      * * * * *   E N D   O F   M E S S A G E   S U M M A R Y   * * * * *".into());
    page += 1;
    out.push(heading(o, page));
    out.push("0                           F i n a l   S u m m a r y".into());
    out.push(format!(
        "0 Program Source Totals: records {}, specifications {}, comments {}",
        c.conversion.counts.records, c.conversion.counts.specs, c.conversion.counts.comments
    ));
    if c.created {
        out.push(format!(
            "0 Program {} is placed in library {}. {:02} highest severity.",
            o.pgm.0, o.pgm.1, c.max_severity
        ));
    } else {
        out.push(format!(
            "0 Program {} was not created: {:02} highest severity.",
            o.pgm.0, c.max_severity
        ));
    }
    out.push("0          * * * * *   E N D   O F   C O M P I L A T I O N   * * * * *".into());
    out
}

/// A label with its leader dots, as the options page prints it.
fn dots(label: &str) -> String {
    let mut s = format!("{label} ");
    while s.len() < 33 {
        s.push_str(if s.len() % 2 == 0 { "." } else { " " });
    }
    s
}
