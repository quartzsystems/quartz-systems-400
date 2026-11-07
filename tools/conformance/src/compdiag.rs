// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a compile listing says its messages are, as the keys
//! `compdiag::Diagnostic::key` gives: the member (upper case, empty
//! for none), the line in it and the message identifier. Each
//! compiler's listing is read here on its own terms, so that a test can
//! hold the diagnostics a compile reports against what it printed.

use std::collections::{BTreeMap, BTreeSet};

use compdiag::{Diagnostic, Where};

/// A member (or nothing), a line in it, a message identifier.
pub type Key = (String, u32, String);

/// The keys of a compile's diagnostics.
#[must_use]
pub fn keys(diags: &[Diagnostic]) -> BTreeSet<Key> {
    diags
        .iter()
        .map(|d| {
            let m = match &d.at {
                Where::Member(m) => m.member.to_ascii_uppercase(),
                Where::Path(p) => p.clone(),
                Where::None => String::new(),
            };
            (m, d.line, d.id.clone())
        })
        .collect()
}

/// A message identifier: three letters (or `DDS`), four hexadecimal
/// characters.
fn is_msgid(w: &str) -> bool {
    let b = w.as_bytes();
    b.len() == 7
        && b[..3].iter().all(u8::is_ascii_uppercase)
        && b[3..].iter().all(u8::is_ascii_hexdigit)
}

/// The members of an RPG compile: name (upper case) and the sequence
/// number of each record, in order.
pub type Members = BTreeMap<String, Vec<String>>;

/// An ILE RPG listing's messages (`CRTBNDRPG`, `CRTRPGMOD`): each
/// `*RNFnnnn` line in the source and additional-diagnostic sections at
/// the line it names (or the source line above it), that line read back
/// to its member through the source id and the `/COPY` member table,
/// and its sequence number to the member's line. `main` is the compiled
/// member.
#[must_use]
pub fn rpg_listing(lines: &[String], main: &str, members: &Members) -> BTreeSet<Key> {
    // Listing line -> (source id, sequence number).
    let mut rows: BTreeMap<u32, (String, String)> = BTreeMap::new();
    let mut copies: BTreeMap<String, String> = BTreeMap::new();
    let mut found: Vec<(Names, String)> = Vec::new();
    let mut section = "";
    let mut current = 0;
    // The precompile's records: number -> (member, line).
    let mut records: BTreeMap<u32, (String, u32)> = BTreeMap::new();
    // The members an SQL INCLUDE brought in, open: name, lines so far.
    let mut includes: Vec<(String, u32)> = vec![(main.to_ascii_uppercase(), 0)];
    let mut sql_found: Vec<(u32, String)> = Vec::new();
    for raw in lines {
        let body = raw.get(1..).unwrap_or("");
        let t = body.trim_start();
        if t.starts_with("S o u r c e   L i s t i n g") {
            section = "source";
            continue;
        }
        if t.starts_with("A d d i t i o n a l   D i a g n o s t i c") {
            section = "additional";
            continue;
        }
        if t.starts_with("/ C o p y   M e m b e r s") {
            section = "copy";
            continue;
        }
        if t.starts_with("* * * * *   E N D   O F") || t.starts_with("* * * * *  E N D  O F") {
            section = "";
            continue;
        }
        // The precompile listing (`CRTSQLRPGI`): its records, then its
        // messages by record.
        if t.starts_with("Record  *...+...") {
            section = "sqlsrc";
            continue;
        }
        if t.starts_with("DIAGNOSTIC MESSAGES") {
            section = "sqlmsg";
            continue;
        }
        if t.starts_with("Message Summary") {
            section = "";
            continue;
        }
        match section {
            "source" => {
                if let Some((no, at)) = rpg_row(body) {
                    current = no;
                    if let Some(at) = at {
                        rows.insert(no, at);
                    }
                    continue;
                }
                if let Some(m) = rpg_message(t, current) {
                    found.push(m);
                }
            }
            "additional" => {
                if let Some(m) = rpg_message(t, 0) {
                    found.push(m);
                }
            }
            "copy" => {
                let w: Vec<&str> = t.split_whitespace().collect();
                if w.len() >= 6 && w[0].chars().all(|c| c.is_ascii_digit()) {
                    copies.insert(w[1].to_string(), w[5].to_ascii_uppercase());
                }
            }
            "sqlsrc" => sql_record(t, members, &mut includes, &mut records),
            "sqlmsg" => {
                let w: Vec<&str> = t.split_whitespace().collect();
                if w.len() >= 3
                    && is_msgid(w[0])
                    && w[0].starts_with("SQL")
                    && let Ok(r) = w[2].parse::<u32>()
                {
                    sql_found.push((r, w[0].to_string()));
                }
            }
            _ => {}
        }
    }
    let mut out: BTreeSet<Key> = sql_found
        .into_iter()
        .map(|(r, id)| {
            let (m, l) = records.get(&r).cloned().unwrap_or_default();
            (m, l, id)
        })
        .collect();
    let at = Places {
        rows: &rows,
        copies: &copies,
        main,
        members,
    };
    out.extend(found.into_iter().map(|(names, id)| at.key(names, id)));
    out
}

/// What reads a message's place back to its member and line.
struct Places<'a> {
    rows: &'a BTreeMap<u32, (String, String)>,
    copies: &'a BTreeMap<String, String>,
    main: &'a str,
    members: &'a Members,
}

impl Places<'_> {
    fn key(&self, names: Names, id: String) -> Key {
        let row = match names {
            Names::Line(no) => self.rows.get(&no),
            Names::Seq(src, seq) => self.rows.values().find(|r| r.0 == src && r.1 == seq),
        };
        let Some((src, seq)) = row else {
            return (String::new(), 0, id);
        };
        let member = if src.is_empty() {
            self.main.to_ascii_uppercase()
        } else {
            self.copies.get(src).cloned().unwrap_or_default()
        };
        let line = self
            .members
            .get(&member)
            .and_then(|r| r.iter().position(|s| s == seq))
            .map_or(0, |i| u32::try_from(i + 1).unwrap_or(0));
        (member, line, id)
    }
}

/// A record of the precompile listing: `n   text   seq [date]`. A
/// record of an `INCLUDE`d member follows its statement; the markers
/// set around a member (`**END-FREE`, `**FREE`, sequence 0) are no one's.
fn sql_record(
    t: &str,
    members: &Members,
    includes: &mut Vec<(String, u32)>,
    records: &mut BTreeMap<u32, (String, u32)>,
) {
    let w: Vec<&str> = t.split_whitespace().collect();
    let Some(no) = w.first().and_then(|n| n.parse::<u32>().ok()) else {
        return;
    };
    let six = |x: &&&str| x.len() == 6 && x.chars().all(|c| c.is_ascii_digit());
    let Some(seq) = w.iter().skip(1).rev().find(six) else {
        return;
    };
    if *seq == "000000" {
        return;
    }
    // A member ended: its records are all listed.
    while includes.len() > 1
        && includes
            .last()
            .is_some_and(|(m, n)| members.get(m).is_none_or(|r| *n as usize >= r.len()))
    {
        includes.pop();
    }
    let Some(top) = includes.last_mut() else {
        return;
    };
    top.1 += 1;
    records.insert(no, (top.0.clone(), top.1));
    let upper = t.to_ascii_uppercase();
    if upper.contains("SQL")
        && let Some(at) = upper.find("INCLUDE ")
    {
        let name: String = upper[at + 8..]
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || "$#@_".contains(*c))
            .collect();
        if members.contains_key(&name) {
            includes.push((name, 0));
        }
    }
}

/// A source row: its listing number and, for a record of a member, the
/// source id (empty for the main member) and the sequence number.
fn rpg_row(body: &str) -> Option<(u32, Option<(String, String)>)> {
    let head: String = body.chars().take(6).collect();
    let next = body.chars().nth(6)?;
    if head.trim().is_empty() || next.is_ascii_digit() {
        return None;
    }
    let no: u32 = head.trim().parse().ok()?;
    let last = body.split_whitespace().last().unwrap_or("");
    let at =
        (last.len() >= 6 && last.chars().all(|c| c.is_ascii_digit()) && next != '=').then(|| {
            let (src, seq) = last.split_at(last.len() - 6);
            (src.to_string(), seq.to_string())
        });
    Some((no, at))
}

/// What a message names: a listing line, or a source id and sequence
/// number (`OPTION(*SRCSTMT)` and the additional diagnostics).
enum Names {
    Line(u32),
    Seq(String, String),
}

/// `*RNFnnnn ss [x|n] [seq] text`: what it names, else `current`.
fn rpg_message(t: &str, current: u32) -> Option<(Names, String)> {
    let rest = t.strip_prefix('*')?;
    let id: String = rest.chars().take(7).collect();
    if !is_msgid(&id) {
        return None;
    }
    let mut w = rest.get(7..)?.split_whitespace();
    let _sev: u8 = w.next()?.parse().ok()?;
    let digits = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit());
    let mut names = Names::Line(current);
    for n in w.take(2) {
        if n.len() == 1 && n.chars().all(|c| c.is_ascii_lowercase()) {
            continue;
        }
        if digits(n) && n.len() < 6 {
            names = Names::Line(n.parse().ok()?);
            break;
        }
        if digits(n) {
            let (src, seq) = n.split_at(n.len() - 6);
            names = Names::Seq(src.to_string(), seq.to_string());
        }
        break;
    }
    Some((names, id))
}

/// A CL listing's messages (`CRTBNDCL`, `CRTCLMOD`): each `* CPDnnnn`
/// line under the source line it belongs to, the line counted from
/// the first source row; one about the whole program only in the
/// message summary, at line 0.
#[must_use]
pub fn cl_listing(lines: &[String], member: &str) -> BTreeSet<Key> {
    let mut out = BTreeSet::new();
    let mut section = "";
    let mut n: u32 = 0;
    for l in lines {
        if l == "Statement  Nesting  Source" {
            section = "source";
            continue;
        }
        if l == "Message summary" {
            section = "summary";
            continue;
        }
        match section {
            "source" if l.is_empty() => section = "",
            "source" => {
                let rest = l.get(20..).unwrap_or("");
                let id = rest.strip_prefix("* ").and_then(|r| r.get(..7));
                match id
                    .filter(|id| is_msgid(id) && l.get(..20).is_some_and(|h| h.trim().is_empty()))
                {
                    Some(id) => {
                        out.insert((member.to_ascii_uppercase(), n, id.to_string()));
                    }
                    None => n += 1,
                }
            }
            "summary" => {
                let t = l.trim_start();
                let Some(id) = t.get(..7).filter(|id| is_msgid(id)) else {
                    continue;
                };
                if t.ends_with("(statement at line 0)") {
                    out.insert((String::new(), 0, id.to_string()));
                }
            }
            _ => {}
        }
    }
    out
}

/// A DDS listing's messages (`CRTPF`, `CRTLF`, `CRTDSPF`, `CRTPRTF`):
/// each `**  DDSnnnn` line under the source row it belongs to, the line
/// counted from the first row; one about the file as a whole only in
/// the messages at the end, with no sequence number, at line 0.
#[must_use]
pub fn dds_listing(listing: &str, member: &str) -> BTreeSet<Key> {
    let mut out = BTreeSet::new();
    let mut section = "";
    let mut n: u32 = 0;
    for l in listing.lines() {
        if l.starts_with("  SEQNBR  ") {
            section = "source";
            continue;
        }
        if l == "  Messages" {
            section = "messages";
            continue;
        }
        match section {
            "source" if l.is_empty() => section = "",
            "source" => {
                if let Some(rest) = l.strip_prefix("      **  ") {
                    if let Some(id) = rest.get(..7).filter(|id| is_msgid(id)) {
                        out.insert((member.to_ascii_uppercase(), n, id.to_string()));
                    }
                } else if !l.starts_with("       *  ") {
                    n += 1;
                }
            }
            "messages" => {
                let w: Vec<&str> = l.split_whitespace().collect();
                // `ID sev seq text`, the sequence number blank when none.
                if w.len() >= 2
                    && is_msgid(w[0])
                    && l.get(16..24).is_some_and(|s| s.trim().is_empty())
                {
                    out.insert((String::new(), 0, w[0].to_string()));
                }
            }
            _ => {}
        }
    }
    out
}

/// What `CRTCMD` printed for a definition that did not compile (it
/// spools no listing): `CPDnnnn Line N: detail`, or no line for a rule
/// over the definition as a whole.
#[must_use]
pub fn cmd_report(lines: &[String], member: &str) -> BTreeSet<Key> {
    lines
        .iter()
        .filter_map(|l| {
            let (id, rest) = l.split_once(' ').unwrap_or((l, ""));
            if !is_msgid(id) {
                return None;
            }
            let line = rest
                .strip_prefix("Line ")
                .and_then(|r| r.split_once(':'))
                .and_then(|(n, _)| n.parse::<u32>().ok());
            Some(match line {
                Some(n) => (member.to_ascii_uppercase(), n, id.to_string()),
                None => (String::new(), 0, id.to_string()),
            })
        })
        .collect()
}

/// A `RUNSQLSTM` listing's messages: after the messages heading
/// (`heading`, the text `TXT0965` gives), each `SQLnnnn sv record text`
/// row at its record of the source.
#[must_use]
pub fn sql_listing(lines: &[String], heading: &str, member: &str) -> BTreeSet<Key> {
    let mut out = BTreeSet::new();
    let mut on = false;
    for l in lines {
        let body = l.get(1..).unwrap_or("");
        if body == heading {
            on = true;
            continue;
        }
        if !on {
            continue;
        }
        let w: Vec<&str> = body.split_whitespace().collect();
        if w.len() >= 3
            && is_msgid(w[0])
            && body.starts_with(w[0])
            && let (Ok(_), Ok(r)) = (w[1].parse::<u8>(), w[2].parse::<u32>())
        {
            out.insert((member.to_ascii_uppercase(), r, w[0].to_string()));
        }
    }
    out
}
