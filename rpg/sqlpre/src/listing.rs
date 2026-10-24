// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The precompile listing in IBM's layout: the command's options, the
//! source records, the cross-reference of the names the statements
//! use, the diagnostics (`MSG ID  SEV  RECORD  TEXT`) and the totals.
//! Each record starts with its forms control character; the message
//! texts are ours.

use crate::{Options, Precompiled, msgs};

/// What the listing's options page names.
#[derive(Clone, Debug, Default)]
pub struct Heading {
    /// `LIB/OBJ`.
    pub object: String,
    /// `LIB/FILE`.
    pub source_file: String,
    /// The member, `*OBJ` for the object's name.
    pub member: String,
    /// `*PGM`, `*MODULE` or `*SRVPGM`.
    pub objtype: String,
    /// The release, `V7R5M0`.
    pub release: String,
    /// The command's SQL options (`SET OPTION` does not change the
    /// page).
    pub options: Options,
    /// The `EXEC SQL` statements of the source.
    pub statements: usize,
    /// The lines of `SET OPTION` statements.
    pub set_options: Vec<u32>,
}

/// A source record as the precompiler lists it.
#[derive(Clone, Debug, Default)]
pub struct Record {
    /// The record text.
    pub text: String,
    /// Its sequence number.
    pub seq: String,
    /// Fully free source (listed whole); fixed form lists columns
    /// 1-80 and the comments after.
    pub free: bool,
}

/// A cross-reference entry.
#[derive(Clone, Debug, Default)]
pub struct XrefEntry {
    /// The name.
    pub name: String,
    /// The record that defines it, if any.
    pub define: Option<u32>,
    /// What it is (`INTEGER PRECISION(9,0)`, `COLUMN`, `TABLE`).
    pub attr: String,
    /// The records that refer to it.
    pub refs: Vec<u32>,
}

/// The overflow line.
const OVERFLOW: usize = 60;

fn advance(cc: char) -> usize {
    match cc {
        '0' => 2,
        '-' => 3,
        _ => 1,
    }
}

/// The printed pages: a heading on each, and the section's title again
/// after it.
struct Pager {
    lines: Vec<String>,
    page: u32,
    pos: usize,
    title: Option<String>,
    head: String,
}

impl Pager {
    fn heading(&mut self) {
        self.page += 1;
        self.lines.push(format!("1{:<110}Page {:>5}", self.head, self.page));
        self.pos = 1;
        if let Some(t) = self.title.clone() {
            self.pos += advance(t.chars().next().unwrap_or(' '));
            self.lines.push(t);
        }
    }

    /// A section on a new page, with the title it repeats.
    fn section(&mut self, title: Option<String>) {
        // A page left at the overflow line gets its overflow page first.
        if self.page > 0 && self.pos >= OVERFLOW {
            self.heading();
        }
        self.title = title;
        self.heading();
    }

    fn line(&mut self, cc: char, text: &str) {
        let mut cc = cc;
        if self.pos + advance(cc) > OVERFLOW {
            self.heading();
            cc = match cc {
                ' ' => '0',
                _ => '-',
            };
        }
        self.pos += advance(cc);
        self.lines.push(format!("{cc}{text}").trim_end().to_string());
    }
}

/// The options page: a row per option, as IBM's precompiler words it.
fn options(h: &Heading) -> Vec<String> {
    let o = &h.options;
    let row = |label: &str, value: &str| format!("{label:.<26}{value}");
    vec![
        row("Source type", "RPG"),
        row("Object name", &h.object),
        row("Source file", &h.source_file),
        row("Member", &h.member),
        row("To source file", "QTEMP/QSQLTEMP1"),
        row("Options", "*XREF"),
        row("RPG preprocessor options", "*NONE"),
        row("Listing option", "*PRINT"),
        row("Target release", &h.release),
        row("INCLUDE file", "*SRCFILE"),
        row("Commit", &o.commit),
        row("Allow copy of data", "*OPTIMIZE"),
        row("Close SQL cursor", &o.closqlcsr),
        row("Allow blocking", "*ALLREAD"),
        row("Delay PREPARE", "*NO"),
        "Concurrent access".into(),
        row("  resolution", "*DFT"),
        row("Generation level", &o.genlvl.to_string()),
        row("Printer file", "*LIBL/QSYSPRT"),
        row("Date format", &o.datfmt),
        row("Date separator", &o.datsep),
        row("Time format", &o.timfmt),
        row("Time separator ", &o.timsep),
        row("Replace", "*YES"),
        row("Relational database", "*LOCAL"),
        row("User ", "*CURRENT"),
        row("RDB connect method", "*DUW"),
        row("Default collection", "*NONE"),
        "Dynamic default".into(),
        row("  collection", "*NO"),
        row("Package name", "*OBJLIB/*OBJ"),
        row("Path", "*NAMING"),
        row("SQL rules", "*DB2"),
        row("Created object type", &h.objtype),
        row("Debugging view", "*NONE"),
        row("Debugging encryption key", "*NONE"),
        row("User profile ", "*NAMING"),
        row("Dynamic user profile", "*USER"),
        row("Sort sequence", "*JOB"),
        row("Language ID", "*JOB"),
        row("IBM SQL flagging", "*NOFLAG"),
        row("ANS flagging", "*NONE"),
        row("Text", "*SRCMBRTXT"),
        row("Source file CCSID", "37"),
        row("Job CCSID", "37"),
        "Decimal result options:".into(),
        row("  Maximum precision", "31"),
        row("  Maximum scale", "31"),
        row("  Minimum divide scale", "0"),
        row("DECFLOAT rounding mode", "*HALFEVEN"),
        row("Compiler options", "*NONE"),
    ]
}

const FREE_RULER: &str =
    "Record  *...+... 1 ...+... 2 ...+... 3 ...+... 4 ...+... 5 ...+... 6 ...+... 7 ...+... 8 ...+... 9 ...+... 0   SEQNBR  Last change";
const FIXED_RULER: &str = "Record  *...+... 1 ...+... 2 ...+... 3 ...+... 4 ...+... 5 ...+... 6 ...+... 7 ...+... 8   SEQNBR  Last change       Comments";

/// A record's line: free form whole, fixed form columns 1-80 and its
/// comment columns after the sequence number.
fn record(n: usize, r: &Record) -> String {
    if r.free {
        let text: String = r.text.chars().take(100).collect();
        format!("{n:>5}   {text:<100}   {}", r.seq)
    } else {
        let text: String = r.text.chars().take(80).collect();
        let cmt: String = r.text.chars().skip(80).take(20).collect();
        format!("{n:>5}   {text:<80}   {}{:14}{}", r.seq, "", cmt.trim_end())
    }
}

/// The listing; `record_of` gives the record a source line is listed
/// as.
#[must_use]
pub fn listing(p: &Precompiled, h: &Heading, records: &[Record], xref: &[XrefEntry], record_of: &dyn Fn(u32) -> u32) -> Vec<String> {
    let mut out = Pager {
        lines: Vec::new(),
        page: 0,
        pos: 0,
        title: None,
        head: format!(" 5770SS1 {}  QS/400 SQL precompiler  {}", h.release, h.object),
    };
    out.section(None);
    for (k, r) in options(h).iter().enumerate() {
        out.line(if k == 0 { '-' } else { ' ' }, r);
    }
    out.line('0', "Source member changed on 00-00-00  00:00:00");
    // The source, a page for each run of one form.
    let mut form: Option<bool> = None;
    for (i, r) in records.iter().enumerate() {
        if form != Some(r.free) {
            let ruler = if r.free { FREE_RULER } else { FIXED_RULER };
            out.section(Some(format!("0{ruler}")));
            form = Some(r.free);
            out.line('0', &record(i + 1, r));
            continue;
        }
        out.line(' ', &record(i + 1, r));
    }
    out.line('0', &format!("{:32}* * * * *  E N D  O F  S O U R C E  * * * * *", ""));
    // The cross-reference: a name without attributes has its
    // references on its line; one with no references leaves a line.
    out.section(Some(format!("0{:47}CROSS REFERENCE", "")));
    out.line('0', "Data Names                    Define    Reference");
    let mut cc = '0';
    for e in xref {
        let def = e.define.map_or_else(|| "****".to_string(), |d| d.to_string());
        let refs: Vec<String> = e.refs.iter().map(u32::to_string).collect();
        let chunks = wrap(&refs, 132 - 43);
        if e.attr.is_empty() {
            let first = chunks.first().cloned().unwrap_or_default();
            out.line(cc, &format!("{:<33}{def:<9}{first}", e.name));
            for c in chunks.iter().skip(1) {
                out.line(' ', &format!("{:42}{c}", ""));
            }
            cc = ' ';
            continue;
        }
        out.line(cc, &format!("{:<33}{def:<9}{}", e.name, e.attr));
        for c in &chunks {
            out.line(' ', &format!("{:42}{c}", ""));
        }
        cc = if chunks.is_empty() { '0' } else { ' ' };
    }
    // A syntax error in SET OPTION is read three times; a source
    // without statements is told so.
    let mut shown: Vec<(String, u8, String)> = Vec::new();
    for d in &p.diags {
        let times = if d.msgid == "SQL0104" && h.set_options.contains(&d.line) { 3 } else { 1 };
        for _ in 0..times {
            shown.push((d.msgid.to_string(), d.sev, format!("{:>6}  {}", record_of(d.line), msgs::text(d))));
        }
    }
    if h.statements == 0 {
        let d = crate::Diag { line: 0, msgid: "SQL0053", sev: 10, args: Vec::new() };
        shown.push((d.msgid.to_string(), d.sev, format!("{:6}  {}", "", msgs::text(&d))));
    }
    let count = |lo: u8, hi: u8| shown.iter().filter(|d| d.1 >= lo && d.1 <= hi).count();
    let max = shown.iter().map(|d| d.1).max().unwrap_or(0);
    if shown.is_empty() {
        out.line(if cc == '0' { '-' } else { '0' }, "No errors found in source");
    } else {
        out.section(Some(format!("0{:45}DIAGNOSTIC MESSAGES", "")));
        out.line('0', "MSG ID  SEV  RECORD  TEXT");
        for (k, (id, sev, rest)) in shown.iter().enumerate() {
            let (rec, body) = rest.split_at(6);
            let text = wrap_text(body.trim_start(), 58);
            let first = format!("{id}  {sev:>2}  {rec}  {}", text[0]);
            out.line(if k == 0 { ' ' } else { '0' }, &first);
            for t in &text[1..] {
                out.line(' ', &format!("{:21}{t}", ""));
            }
        }
        out.line('-', "");
        out.line(' ', &format!("{:20}Message Summary", ""));
        out.line(' ', "  Total     Info   Warning      Error   Severe  Terminal");
        out.line(
            ' ',
            &format!(
                "{:>7}  {:>7}   {:>7}    {:>7}  {:>7}   {:>7}",
                shown.len(),
                count(0, 9),
                count(10, 19),
                count(20, 29),
                count(30, 39),
                count(40, 99)
            ),
        );
        out.line('-', &format!("{max:02} level severity errors found in source"));
    }
    out.line('0', &format!("{:>5} Source records processed", records.len()));
    out.line(' ', &format!("{:31}* * * * *  E N D  O F  L I S T I N G  * * * * *", ""));
    out.lines
}

/// References in lines of `width` columns.
fn wrap(refs: &[String], width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for r in refs {
        if !cur.is_empty() && cur.len() + 1 + r.len() > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(r);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A message text in lines of `width` columns.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let words: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    let v = wrap(&words, width);
    if v.is_empty() { vec![String::new()] } else { v }
}
