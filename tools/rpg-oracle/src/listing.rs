// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Compile listings as `CPYSPLF ... CTLCHAR(*FCFC)` gives them: one
//! line per record, the first character the carriage control. The
//! diagnostics are read out of the source and additional-diagnostic
//! sections; the listing is masked (page headings, dates and times,
//! system and library names, the oracle's page footer) so that two
//! compiles of one member compare equal.

/// One `RNF` diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Diag {
    /// The listing statement (line) number it belongs to; 0 for none.
    pub line: u32,
    /// `RNFnnnn`.
    pub msgid: String,
    /// Severity, 0 to 99.
    pub severity: u8,
    /// The source sequence number, as the listing prints it.
    pub seq: String,
}

/// The diagnostics of a listing, sorted by line then identifier.
#[must_use]
pub fn diags(lines: &[String]) -> Vec<Diag> {
    let mut out = Vec::new();
    let mut current: u32 = 0;
    let mut in_source = false;
    let mut in_additional = false;
    let mut in_sql = false;
    for raw in lines {
        let body = raw.get(1..).unwrap_or("");
        let t = body.trim_start();
        // The SQL precompile listing: `SQLnnnn sev record text`.
        if t.starts_with("DIAGNOSTIC MESSAGES") {
            in_sql = true;
            continue;
        }
        if in_sql {
            if t.starts_with("Message Summary") {
                in_sql = false;
            } else if let Some(d) = sql_diag_line(t) {
                out.push(d);
            }
            continue;
        }
        if t.starts_with("S o u r c e   L i s t i n g") {
            in_source = true;
            continue;
        }
        if t.starts_with("A d d i t i o n a l   D i a g n o s t i c") {
            in_additional = true;
            continue;
        }
        if t.starts_with("* * * * *   E N D   O F") {
            in_source = false;
            in_additional = false;
            continue;
        }
        if in_source && let Some(n) = source_line_number(body) {
            current = n;
            continue;
        }
        if (in_source || in_additional)
            && let Some(d) = diag_line(t, current)
        {
            out.push(d);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The statement number of a source line: a number right-aligned in
/// the first six columns, then a blank.
fn source_line_number(body: &str) -> Option<u32> {
    let head: String = body.chars().take(6).collect();
    if head.trim().is_empty() || body.chars().nth(6).is_some_and(|c| c != ' ') {
        return None;
    }
    head.trim().parse().ok()
}

/// `SQLnnnn ss record text`, as the precompile listing prints it.
fn sql_diag_line(t: &str) -> Option<Diag> {
    let mut w = t.split_whitespace();
    let msgid = w.next()?;
    if msgid.len() != 7 || !msgid.starts_with("SQL") || !msgid[3..].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let severity: u8 = w.next()?.parse().ok()?;
    let line: u32 = w.next()?.parse().ok()?;
    Some(Diag {
        line,
        msgid: msgid.to_string(),
        severity,
        seq: String::new(),
    })
}

/// `*RNFnnnn ss [x|n] seq text`.
fn diag_line(t: &str, current: u32) -> Option<Diag> {
    let rest = t.strip_prefix('*')?;
    let msgid: String = rest.chars().take(7).collect();
    if msgid.len() != 7 || !msgid.starts_with("RNF") && !msgid.starts_with("RNS") {
        return None;
    }
    let after = rest.get(7..)?;
    let mut words = after.split_whitespace();
    let severity: u8 = words.next()?.parse().ok()?;
    // Then a marker letter or a statement number, then the sequence.
    let mut line = current;
    let mut seq = String::new();
    for w in words.by_ref().take(2) {
        if w.len() == 1 && w.chars().all(|c| c.is_ascii_lowercase()) {
            continue;
        }
        if w.len() == 6 && w.chars().all(|c| c.is_ascii_digit()) {
            seq = w.to_string();
            break;
        }
        if w.chars().all(|c| c.is_ascii_digit()) {
            line = w.parse().ok()?;
            continue;
        }
        break;
    }
    Some(Diag {
        line,
        msgid,
        severity,
        seq,
    })
}

/// What masking replaces, besides the fixed patterns.
#[derive(Clone, Debug, Default)]
pub struct Mask {
    /// Library names to show as `{lib}`.
    pub libs: Vec<String>,
    /// User names to show as `{user}`.
    pub users: Vec<String>,
    /// System names to show as `{sys}`.
    pub systems: Vec<String>,
}

/// Mask a listing: the page heading becomes `1{heading}`, the oracle's
/// page footer goes, dates and times become `dd-dd-dd` and `dd:dd:dd`,
/// and the names in `m` become placeholders. Trailing blanks go.
#[must_use]
pub fn mask(lines: &[String], m: &Mask) -> Vec<String> {
    let mut out = Vec::new();
    for raw in lines {
        if raw.contains("https://pub400.com") {
            continue;
        }
        let mut l = raw.trim_end().to_string();
        if l.starts_with('1') && (l.contains("5770WDS") || l.contains("Page")) {
            let page = l
                .rsplit_once("Page")
                .map(|(_, p)| p.trim().to_string())
                .unwrap_or_default();
            out.push(format!("1{{heading}} page {page}"));
            continue;
        }
        for (names, ph) in [(&m.libs, "{lib}"), (&m.users, "{user}"), (&m.systems, "{sys}")] {
            for n in names.iter().filter(|n| !n.is_empty()) {
                l = l.replace(n.as_str(), ph);
            }
        }
        out.push(dates_and_times(&l));
    }
    out
}

/// `nn-nn-nn` and `nn:nn:nn` (and `nn/nn/nn`, `nn.nn.nn`) as patterns.
fn dates_and_times(l: &str) -> String {
    let c: Vec<char> = l.chars().collect();
    let mut out = c.clone();
    let mut i = 0;
    while i + 8 <= c.len() {
        let w = &c[i..i + 8];
        let d = |k: usize| w[k].is_ascii_digit();
        let sep = w[2];
        let shape = d(0) && d(1) && d(3) && d(4) && d(6) && d(7) && w[5] == sep;
        let edge_l = i == 0 || !c[i - 1].is_ascii_digit();
        let edge_r = i + 8 == c.len() || !c[i + 8].is_ascii_digit();
        if shape && edge_l && edge_r && matches!(sep, '-' | ':' | '/' | '.') {
            let p = if sep == ':' || sep == '.' { 't' } else { 'd' };
            for (k, slot) in out[i..i + 8].iter_mut().enumerate() {
                if k != 2 && k != 5 {
                    *slot = p;
                }
            }
            i += 8;
            continue;
        }
        i += 1;
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_both_kinds_of_diagnostic() {
        let lines: Vec<String> = [
            "0                          S o u r c e   L i s t i n g",
            "0     1 0000000**FREE",
            " *RNF0257 30 a      000100  Form-Type entry ...",
            "      2        dcl-f",
            " *RNF5501 20      5 000500  The comment entry ...",
            "0       * * * * *   E N D   O F   S O U R C E   * * * * *",
            "0          A d d i t i o n a l   D i a g n o s t i c   M e s s a g e s",
            " *RNF7030 30      2 000200  The name ...",
            " *RNF7023 40      0         The Compiler ...",
            "0 * * * * *   E N D   O F   A D D I T I O N A L   D I A G N O S T I C   M E S S A G E S",
            "0                          M e s s a g e   S u m m a r y",
            " *RNF2318 00      1 No overflow ...",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let d = diags(&lines);
        let got: Vec<(u32, &str, u8, &str)> =
            d.iter().map(|x| (x.line, x.msgid.as_str(), x.severity, x.seq.as_str())).collect();
        assert_eq!(
            got,
            vec![
                (0, "RNF7023", 40, ""),
                (1, "RNF0257", 30, "000100"),
                (2, "RNF7030", 30, "000200"),
                (5, "RNF5501", 20, "000500"),
            ]
        );
    }

    #[test]
    fn reads_the_precompile_diagnostics() {
        let lines: Vec<String> = [
            "0                                             DIAGNOSTIC MESSAGES",
            "0MSG ID  SEV  RECORD  TEXT",
            " SQL0312  30       7  Position 28 ...",
            "                      for reason code 1.",
            "0SQL0104  30       8  Position 10 ...",
            "-",
            "                     Message Summary",
            "   Total     Info   Warning      Error   Severe  Terminal",
            "       2        0         0          0        2         0",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let d = diags(&lines);
        let got: Vec<(u32, &str, u8)> = d.iter().map(|x| (x.line, x.msgid.as_str(), x.severity)).collect();
        assert_eq!(got, vec![(7, "SQL0312", 30), (8, "SQL0104", 30)]);
    }

    #[test]
    fn masks_headings_dates_and_names() {
        let lines = vec![
            "1 5770WDS V7R5M0  220415 RN        IBM ILE RPG     XB/HELLO   PUB400     26-10-23 19:02:28        Page      1".to_string(),
            "   Last Change  . . . :   26-10-23  19:02:28".to_string(),
            "     Library  . . . :     XB".to_string(),
            "                     -=* https://pub400.com *=-".to_string(),
        ];
        let m = Mask {
            libs: vec!["XB".into()],
            ..Mask::default()
        };
        assert_eq!(
            mask(&lines, &m),
            vec![
                "1{heading} page 1".to_string(),
                "   Last Change  . . . :   dd-dd-dd  tt:tt:tt".to_string(),
                "     Library  . . . :     {lib}".to_string(),
            ]
        );
    }
}
