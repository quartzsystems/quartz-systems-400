// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the listing needs to know of each source line: its
//! specification type (a change of type spaces the listing), the
//! statement it belongs to (a diagnostic prints after its statement),
//! the data structure or prototype group around it, and its nesting
//! level in the "Do Num" column.

use rpgfront::reader::{Class, Line};

/// A specification type, in the listing's sense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ty {
    /// `H` to `P`, by letter (`C` for free-form calculations).
    Spec(char),
    /// `END-PROC`: a `P` that does not space.
    EndProc,
    /// Comments, directives, blanks: no type.
    None,
}

/// One line's facts.
#[derive(Clone, Debug)]
pub(crate) struct Info {
    pub ty: Ty,
    /// The line where the statement this line ends (or is in) ends;
    /// for fixed form, the last line before the next statement.
    pub stmt_end: usize,
    /// The line a data structure or prototype group around this line
    /// ends on (as `stmt_end`).
    pub group_end: usize,
    /// The "Do Num" column.
    pub level: String,
    /// The first line of its statement, when a statement starts here.
    pub starts: bool,
}

/// A free-form statement: its lines, first word and text.
struct Stmt {
    first: usize,
    last: usize,
    word: String,
    text: String,
}

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '#' | '@' | '$' | '-' | '*' | '%')
}

/// Free-form statements of a run of lines, by line index.
fn free_stmts(lines: &[Line], idx: &[usize]) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut cur: Option<Stmt> = None;
    for &i in idx {
        let (_, area) = lines[i].free_area();
        let mut quote = false;
        let chars: Vec<char> = area.chars().collect();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k];
            if quote {
                if c == '\'' {
                    quote = false;
                }
                if let Some(s) = cur.as_mut() {
                    s.text.push(c);
                }
                k += 1;
                continue;
            }
            if c == '/' && chars.get(k + 1) == Some(&'/') {
                break;
            }
            if cur.is_none() {
                if c == ' ' || c == ';' {
                    k += 1;
                    continue;
                }
                let word: String = chars[k..].iter().take_while(|c| name_char(**c)).collect();
                cur = Some(Stmt {
                    first: i,
                    last: i,
                    word: word.to_ascii_lowercase(),
                    text: String::new(),
                });
            }
            let s = cur.as_mut().expect("a statement");
            s.last = i;
            if c == '\'' {
                quote = true;
            }
            if c == ';' {
                out.push(cur.take().expect("a statement"));
            } else {
                s.text.push(c.to_ascii_lowercase());
            }
            k += 1;
        }
        if let Some(s) = cur.as_mut() {
            s.last = i;
            s.text.push(' ');
        }
    }
    if let Some(s) = cur {
        out.push(s);
    }
    out
}

fn free_type(word: &str, in_group: bool) -> Ty {
    match word {
        "ctl-opt" => Ty::Spec('H'),
        "dcl-f" => Ty::Spec('F'),
        "dcl-s" | "dcl-c" | "dcl-ds" | "dcl-pr" | "dcl-pi" | "end-ds" | "end-pr" | "end-pi" | "dcl-subf"
        | "dcl-parm" => Ty::Spec('D'),
        "dcl-proc" => Ty::Spec('P'),
        "end-proc" => Ty::EndProc,
        _ if in_group => Ty::Spec('D'),
        _ => Ty::Spec('C'),
    }
}

/// Whether a free `DCL-DS`/`DCL-PR`/`DCL-PI` opens a group that an
/// `END-xx` closes.
fn opens_group(s: &Stmt) -> bool {
    let words: Vec<&str> = s.text.split(|c: char| !name_char(c)).filter(|w| !w.is_empty()).collect();
    match s.word.as_str() {
        "dcl-ds" => !words.iter().any(|w| matches!(*w, "end-ds" | "likeds" | "likerec")),
        "dcl-pr" => !words.contains(&"end-pr"),
        "dcl-pi" => !words.contains(&"end-pi"),
        _ => false,
    }
}

fn level_kind(op: &str) -> Option<char> {
    let op = op.split('(').next().unwrap_or("");
    let b = ["if", "dow", "dou", "for", "for-each", "select", "monitor", "do"];
    let x = ["else", "elseif", "when", "when-in", "when-is", "other", "on-error", "on-excp", "on-exit"];
    let e = ["end", "endif", "enddo", "endfor", "endsl", "endmon"];
    if b.contains(&op) || (op.len() == 4 && (op.starts_with("ifl") || op.starts_with("ifg") || op.starts_with("ife") || op.starts_with("ifn")))
        || (op.len() == 5 && (op.starts_with("dow") || op.starts_with("dou")))
    {
        return Some('B');
    }
    if x.contains(&op) || (op.len() == 6 && op.starts_with("when")) {
        return Some('X');
    }
    if e.contains(&op) {
        return Some('E');
    }
    None
}

/// Fixed-form columns `from..=to`, blank padded.
fn cols(t: &str, from: usize, to: usize) -> String {
    let mut s: String = t.chars().skip(from - 1).take(to + 1 - from).collect();
    while s.len() < to + 1 - from {
        s.push(' ');
    }
    s
}

/// Whether a fixed specification line continues the one before.
fn continues(letter: char, t: &str, prev_long_name: bool) -> bool {
    if prev_long_name {
        return true;
    }
    match letter {
        'F' => cols(t, 7, 42).trim().is_empty(),
        'D' => cols(t, 7, 42).trim().is_empty(),
        'C' => cols(t, 7, 35).trim().is_empty() && !cols(t, 36, 80).trim().is_empty(),
        _ => false,
    }
}

/// A statement in line order.
struct Unit {
    first: usize,
    last: usize,
    ty: Ty,
    op: String,
    /// It opens a data structure or prototype group.
    opens: bool,
    /// It closes one.
    closes: bool,
    in_group: bool,
    fixed: bool,
}

/// The facts of every line of the expanded source.
pub(crate) fn analyze(lines: &[Line]) -> Vec<Info> {
    let n = lines.len();
    let mut info: Vec<Info> = (0..n)
        .map(|i| Info {
            ty: Ty::None,
            stmt_end: i,
            group_end: i,
            level: String::new(),
            starts: false,
        })
        .collect();
    let free_idx: Vec<usize> = (0..n).filter(|&i| matches!(lines[i].class, Class::Free | Class::Sql)).collect();
    let stmts = free_stmts(lines, &free_idx);
    let mut units: Vec<Unit> = Vec::new();
    let mut si = 0;
    // Free-form and fixed-form groups are tracked apart.
    let mut free_group = false;
    let mut in_group = false;
    let mut long_name = false;
    for (i, line) in lines.iter().enumerate() {
        while si < stmts.len() && stmts[si].first == i {
            let s = &stmts[si];
            let ty = free_type(&s.word, free_group);
            let opens = opens_group(s);
            let closes = matches!(s.word.as_str(), "end-ds" | "end-pr" | "end-pi");
            units.push(Unit {
                first: s.first,
                last: s.last,
                ty,
                op: s.word.clone(),
                opens,
                closes,
                in_group: free_group,
                fixed: false,
            });
            if opens {
                free_group = true;
            }
            if closes {
                free_group = false;
            }
            si += 1;
        }
        if let Class::Spec(letter) = line.class {
            let letter = letter.to_ascii_uppercase();
            let t = &line.text;
            let cont = units.last().is_some_and(|u| u.fixed && u.ty == Ty::Spec(letter) && u.last + 1 >= i)
                && continues(letter, t, long_name);
            long_name = letter == 'D' && cols(t, 7, 21).trim_end().ends_with("...");
            if cont {
                if let Some(u) = units.last_mut() {
                    u.last = i;
                }
                continue;
            }
            let decl = cols(t, 24, 25).trim().to_ascii_uppercase();
            let (opens, sub) = if letter == 'D' {
                (matches!(decl.as_str(), "DS" | "PR" | "PI"), decl.is_empty())
            } else {
                (false, false)
            };
            if letter != 'D' || !sub {
                in_group = false;
            }
            let op = if letter == 'C' { cols(t, 26, 35).trim().to_ascii_lowercase() } else { String::new() };
            let ty = match (letter, cols(t, 24, 24).to_ascii_uppercase().as_str()) {
                ('P', "E") => Ty::EndProc,
                _ => Ty::Spec(letter),
            };
            units.push(Unit {
                first: i,
                last: i,
                ty,
                op,
                opens,
                closes: false,
                in_group: in_group && sub,
                fixed: true,
            });
            if opens {
                in_group = true;
            }
        }
    }
    // Where each statement's diagnostics print: free form after its
    // last line, fixed form just before the next statement.
    let next_first: Vec<usize> = units.iter().skip(1).map(|u| u.first).chain(std::iter::once(n)).collect();
    let mut level: usize = 0;
    let mut cas: Vec<usize> = Vec::new();
    let mut group_start: Option<usize> = None;
    for (k, u) in units.iter().enumerate() {
        // Comments and directives before the next statement belong to
        // a fixed-form one.
        let end = if u.fixed { next_first[k].saturating_sub(1).max(u.last) } else { u.last };
        let mut mark = match u.ty {
            Ty::Spec('C') => level_kind(&u.op),
            _ => None,
        };
        // A CASxx group takes no level of its own: its ENDCS, or the END
        // that closes it, prints none either.
        if u.ty == Ty::Spec('C') {
            let op = u.op.split('(').next().unwrap_or("");
            if op == "cas" || (op.len() == 5 && op.starts_with("cas")) {
                if cas.last() != Some(&level) {
                    cas.push(level);
                }
            } else if op == "endcs" || (op == "end" && cas.last() == Some(&level)) {
                cas.pop();
                mark = None;
            } else if cas.last() == Some(&level) {
                cas.pop();
            }
        }
        let text = match mark {
            Some('B') => {
                level += 1;
                format!("B{level:02}")
            }
            Some('X') if level > 0 => format!("X{level:02}"),
            Some('E') if level > 0 => {
                let s = format!("E{level:02}");
                level -= 1;
                s
            }
            _ if level > 0 => format!("{level:02}"),
            _ => String::new(),
        };
        for f in &mut info[u.first..=u.last] {
            if f.ty == Ty::None {
                f.ty = u.ty;
                f.level.clone_from(&text);
            }
            f.stmt_end = end;
            f.group_end = end;
        }
        info[u.first].starts = true;
        if u.opens {
            group_start = Some(k);
        }
        let group_over = if u.fixed {
            units.get(k + 1).is_none_or(|v| !v.in_group || !v.fixed)
        } else {
            u.closes
        };
        if let Some(g) = group_start
            && (group_over && (u.in_group || u.opens || u.closes))
        {
            for v in &units[g..=k] {
                for f in &mut info[v.first..=v.last] {
                    f.group_end = end;
                }
            }
            group_start = None;
        }
    }
    info
}
