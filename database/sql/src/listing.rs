// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `RUNSQLSTM OUTPUT(*PRINT)`'s listing, laid out as pub400 prints it
//! (`tools/conformance/sql/oracle/listing/expected/`): a page of the
//! options, the source records numbered, then the statements'
//! messages with their severity and record and a summary by severity.
//! Every page starts with the product, release, title, source name,
//! date, time and page number. The words are `QSTXTMSG` texts the
//! caller resolves; the lines start with their forms control (`1` a
//! new page, blank the next line).

use std::fmt::Write as _;

use crate::run::{RunEntry, RunReport};

/// A `QSTXTMSG` (or message file) text with its values substituted.
pub type Words<'a> = dyn Fn(&str, &[&str]) -> String + 'a;

/// The options page: each line's label (`QSTXTMSG`) and the keyword
/// whose value follows its dot leaders.
const OPTIONS: &[(&str, &str)] = &[
    ("TXT0945", "TGTRLS"),
    ("TXT0946", "COMMIT"),
    ("TXT0947", "NAMING"),
    ("TXT0948", "ERRLVL"),
    ("TXT0949", "DATFMT"),
    ("TXT094A", "DATSEP"),
    ("TXT094B", "TIMFMT"),
    ("TXT094C", "TIMSEP"),
    ("TXT094D", "MARGINS"),
    ("TXT094E", "DFTRDBCOL"),
    ("TXT094F", "SAAFLAG"),
    ("TXT0950", "FLAGSTD"),
    ("TXT0951", "DECMPT"),
    ("TXT0952", "SRTSEQ"),
    ("TXT0953", "LANGID"),
    ("TXT0954", "PRTFILE"),
    ("TXT0955", "*SRCCCSID"),
    ("TXT0956", "*JOBCCSID"),
    ("TXT0957", "PROCESS"),
    ("TXT0958", "ALWCPYDTA"),
    ("TXT0959", "ALWBLK"),
    ("TXT095A", "SQLCURRULE"),
    ("TXT095B", ""),
    ("TXT095C", "*DECPREC"),
    ("TXT095D", "*DECSCALE"),
    ("TXT095E", "*DECDIV"),
    ("TXT095F", ""),
    ("TXT0960", "CONACC"),
    ("TXT0961", "SYSTIME"),
];

/// The labels' width with their dot leaders.
const LABEL: usize = 26;
/// Lines on a page under its heading.
const PER_PAGE: usize = 56;
/// Where a message's text starts, and how wide it runs.
const MSG_COL: usize = 21;
const MSG_WIDTH: usize = 56;

/// What the listing needs beyond the run.
pub struct Listing<'a> {
    /// The product identifier (`QS00SS1`).
    pub product: &'a str,
    /// The release (`V1R1M0`).
    pub release: &'a str,
    /// The source's name: the member, or the stream file's.
    pub name: &'a str,
    /// The source: `(label id, value)` lines, the stream file or the
    /// file and member.
    pub source: Vec<(&'a str, String)>,
    /// The options' values by keyword (and the `*` items).
    pub value: &'a dyn Fn(&str) -> String,
    /// The date and time of the run, as the job writes them.
    pub stamp: (String, String),
    /// When the source was last changed, when known.
    pub changed: Option<(String, String)>,
    /// The source records.
    pub records: Vec<String>,
}

/// The listing's lines, each starting with its forms control.
#[must_use]
pub fn listing(l: &Listing<'_>, report: &RunReport, text: &str, w: &Words<'_>) -> Vec<String> {
    let mut pages: Vec<Vec<String>> = Vec::new();
    // Page 1: the options.
    let mut p1 = Vec::new();
    for (id, value) in &l.source {
        p1.push(dotted(&w(id, &[]), value));
    }
    for (id, kwd) in OPTIONS {
        let label = w(id, &[]);
        p1.push(if kwd.is_empty() {
            label
        } else {
            dotted(&label, &(l.value)(kwd))
        });
    }
    if let Some((d, t)) = &l.changed {
        p1.push(w("TXT0962", &[d, t]));
    }
    pages.push(p1);
    // The source, numbered.
    let ruler = w("TXT0963", &[]);
    let mut src: Vec<String> = l
        .records
        .iter()
        .enumerate()
        .map(|(i, r)| format!("{:>7} {r}", i + 1))
        .collect();
    src.push(format!("{:33}{}", "", w("TXT0964", &[])));
    let msgs = messages(report, text, w);
    let worst = msgs.iter().map(|m| m.1).max().unwrap_or(0);
    let end = format!("{:32}{}", "", w("TXT0903", &[]));
    if msgs.is_empty() {
        src.push(if report.errors == 0 {
            w("TXT0969", &[])
        } else {
            w("TXT0968", &[&worst.to_string()])
        });
        src.push(end.clone());
    }
    for chunk in src.chunks(PER_PAGE) {
        let mut p = vec![ruler.clone()];
        p.extend(chunk.iter().cloned());
        pages.push(p);
    }
    // The messages and the summary.
    if !msgs.is_empty() {
        let mut m = vec![w("TXT0965", &[])];
        for (id, sev, record, t) in &msgs {
            let wrapped = wrap(t, MSG_WIDTH);
            for (i, part) in wrapped.iter().enumerate() {
                if i == 0 {
                    m.push(format!("{id:<7}  {sev:>2}{record:>8}  {part}"));
                } else {
                    m.push(format!("{:MSG_COL$}{part}", ""));
                }
            }
        }
        m.push(format!("{:20}{}", "", w("TXT0966", &[])));
        m.push(w("TXT0967", &[]));
        let count = |lo: u8, hi: u8| msgs.iter().filter(|x| (lo..=hi).contains(&x.1)).count();
        m.push(format!(
            "{:>7}{:>9}{:>10}{:>11}{:>9}{:>10}",
            msgs.len(),
            count(0, 9),
            count(10, 19),
            count(20, 29),
            count(30, 39),
            count(40, 99)
        ));
        m.push(if worst == 0 {
            w("TXT0969", &[])
        } else {
            w("TXT0968", &[&worst.to_string()])
        });
        m.push(end);
        for chunk in m.chunks(PER_PAGE) {
            let mut p = vec![ruler.clone()];
            p.extend(chunk.iter().cloned());
            pages.push(p);
        }
    }
    let mut out = Vec::new();
    for (n, p) in pages.iter().enumerate() {
        out.push(format!("1{}", heading(l, n + 1, w)));
        out.extend(p.iter().map(|x| format!(" {x}")));
    }
    out
}

/// A page's heading. The first page's number is four wide, the others'
/// six, as pub400 prints them.
fn heading(l: &Listing<'_>, page: usize, w: &Words<'_>) -> String {
    let mut s = format!("{:<8}{:<7}", l.product, l.release);
    let _ = write!(s, "{:21}{:<31}{:<35}", "", w("TXT0940", &[]), l.name);
    let _ = write!(s, "{} {}   {}", l.stamp.0, l.stamp.1, w("TXT0904", &[]));
    if page == 1 {
        let _ = write!(s, "{page:>4}");
    } else {
        let _ = write!(s, "{page:>6}");
    }
    s
}

/// `label` with dot leaders to the value's column, then the value.
fn dotted(label: &str, value: &str) -> String {
    let mut s = label.to_string();
    while s.chars().count() < LABEL {
        s.push('.');
    }
    s + value
}

/// Each statement's message: identifier, severity, record and text.
fn messages(report: &RunReport, text: &str, w: &Words<'_>) -> Vec<(String, u8, usize, String)> {
    let starts = statement_records(text);
    let mut out = Vec::new();
    for e in &report.entries {
        let record = starts.get(e.index).copied().unwrap_or(1);
        let (id, sev, body) = if e.sqlcode < 0 {
            (e.msgid.clone(), 30, e.text.clone())
        } else if let Some((id, body)) = done(e, w) {
            (id.to_string(), 0, body)
        } else {
            continue;
        };
        out.push((id, sev, record, w("TXT096A", &["1", &body])));
    }
    out
}

/// The completion message of a statement that ran, as `RUNSQLSTM`
/// lists it.
fn done(e: &RunEntry, w: &Words<'_>) -> Option<(&'static str, String)> {
    let (schema, name) = e.target.clone()?;
    let lib = schema.unwrap_or_else(|| "*LIBL".into());
    let n = e.count.to_string();
    let id = match e.name.as_str() {
        "INSERT" => "SQL7956",
        "UPDATE" => "SQL7957",
        "DELETE" => "SQL7955",
        n if n.starts_with("DROP") => "SQL7953",
        _ => return None,
    };
    Some((id, w(id, &[&name, &lib, &n])))
}

/// Text cut at word boundaries to `width`.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

/// The record (line, from 1) each statement of `text` starts on: the
/// first character after the previous statement's `;` that is not a
/// blank or in a comment.
#[must_use]
pub fn statement_records(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut line = 1;
    let mut want = true;
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            line += 1;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        if c == '-' && chars.peek() == Some(&'-') {
            while chars.peek().is_some_and(|&n| n != '\n') {
                chars.next();
            }
            continue;
        }
        if c.is_whitespace() {
            continue;
        }
        if want {
            out.push(line);
            want = false;
        }
        match c {
            '\'' | '"' => quote = Some(c),
            ';' => want = true,
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(id: &str, v: &[&str]) -> String {
        let t = match id {
            "TXT0940" => "Run SQL Statements",
            "TXT0904" => "PAGE",
            "TXT096A" => "Position &1 &2",
            "TXT0968" => "&1 level severity errors found in source",
            "SQL7956" => "&3 rows inserted in &1 in &2.",
            _ => id,
        };
        let mut t = t.to_string();
        for (i, x) in v.iter().enumerate() {
            t = t.replace(&format!("&{}", i + 1), x);
        }
        t
    }

    #[test]
    fn records_of_statements() {
        let t = "CREATE TABLE T (A INT);\nINSERT INTO T VALUES(1);\n-- a note; here\nUPDATE T SET A = 2\n  WHERE A = 1;\nDROP TABLE T;\n";
        assert_eq!(statement_records(t), vec![1, 2, 4, 6]);
    }

    #[test]
    fn as_pub400_lays_it_out() {
        let report = RunReport {
            statements: 2,
            errors: 1,
            severity: 30,
            entries: vec![
                RunEntry {
                    index: 0,
                    name: "INSERT".into(),
                    count: 1,
                    target: Some((Some("LIB".into()), "T".into())),
                    ..RunEntry::default()
                },
                RunEntry {
                    index: 1,
                    name: "INSERT".into(),
                    sqlcode: -204,
                    msgid: "SQL0204".into(),
                    text: "X in LIB type *FILE not found.".into(),
                    ..RunEntry::default()
                },
            ],
            ..RunReport::default()
        };
        let text = "INSERT INTO LIB.T VALUES(1);\nINSERT INTO LIB.X VALUES(1);\n";
        let value = |k: &str| k.to_string();
        let l = Listing {
            product: "5770SS1",
            release: "V7R5M0",
            name: "SQL14",
            source: vec![],
            value: &value,
            stamp: ("26-11-05".into(), "20:29:13".into()),
            changed: None,
            records: text.lines().map(str::to_string).collect(),
        };
        let lines = listing(&l, &report, text, &words);
        let head = &lines[0][1..];
        assert_eq!(head.find("Run SQL Statements"), Some(36));
        assert_eq!(head.find("SQL14"), Some(67));
        assert_eq!(head.find("26-11-05"), Some(102));
        assert_eq!(head.find("PAGE"), Some(122));
        assert!(head.ends_with("PAGE   1"));
        assert!(
            lines
                .iter()
                .any(|x| x == " SQL7956   0       1  Position 1 1 rows inserted in T in LIB.")
        );
        assert!(
            lines
                .iter()
                .any(|x| x == " SQL0204  30       2  Position 1 X in LIB type *FILE not found.")
        );
        assert!(
            lines
                .iter()
                .any(|x| x == "       2        1         0          0        1         0")
        );
        assert!(
            lines
                .iter()
                .any(|x| x == " 30 level severity errors found in source")
        );
        assert!(
            lines
                .iter()
                .filter(|x| x.starts_with('1'))
                .nth(1)
                .is_some_and(|x| x.ends_with("PAGE     2"))
        );
    }
}
