// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `pr [+page] [-column] [-adFfmprt] [-e[c][n]] [-h header] [-i[c][n]]
//! [-l lines] [-n[c][n]] [-o offset] [-s[c]] [-w width] [file...]`:
//! paginate files for printing.
//!
//! A page is `-l` lines (66): a 5-line header (two blank lines, the
//! date, the header and `Page n`, two blank lines), the text, and a
//! 5-line trailer of blank lines; the last page is padded. `-t` drops
//! header, trailer and padding; `-F`/`-f` end a page with a form feed
//! instead. Columns fill down (`-a` across), each truncated to fit `-w`
//! (72) unless `-s` separates them; `-m` puts the files side by side.

use qsh::sys::UtilCtx;

use crate::common::io::{Lines, Out, input};
use crate::common::time::{Zone, strftime};
use crate::common::{complain, complain_sys, usage};

const NAME: &str = "pr";

#[derive(Clone, Debug)]
struct Opts {
    first_page: u64,
    columns: usize,
    across: bool,
    double: bool,
    expand: Option<(char, usize)>,
    formfeed: bool,
    header: Option<String>,
    unexpand: Option<(char, usize)>,
    length: usize,
    merge: bool,
    number: Option<(char, usize)>,
    offset: usize,
    quiet: bool,
    sep: Option<char>,
    no_header: bool,
    width: usize,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            first_page: 1,
            columns: 1,
            across: false,
            double: false,
            expand: None,
            formfeed: false,
            header: None,
            unexpand: None,
            length: 66,
            merge: false,
            number: None,
            offset: 0,
            quiet: false,
            sep: None,
            no_header: false,
            width: 72,
        }
    }
}

/// `[char][digits]` after `-e`, `-i` or `-n`.
fn char_num(rest: &str, dflt_c: char, dflt_n: usize) -> Result<(char, usize), String> {
    let mut c = dflt_c;
    let mut digits = rest;
    if let Some(first) = rest.chars().next()
        && !first.is_ascii_digit()
    {
        c = first;
        digits = &rest[first.len_utf8()..];
    }
    let n = if digits.is_empty() {
        dflt_n
    } else {
        digits
            .parse::<usize>()
            .map_err(|_| format!("{digits} is not a valid number."))?
    };
    Ok((c, if n == 0 { dflt_n } else { n }))
}

fn num(s: &str, what: char) -> Result<usize, String> {
    crate::common::number(s)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("The value {s} of option -{what} is not valid."))
}

fn parse(args: &[String]) -> Result<(Opts, Vec<String>), String> {
    let mut o = Opts::default();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        i += 1;
        if a == "--" {
            break;
        }
        if let Some(p) = a.strip_prefix('+') {
            o.first_page = crate::common::number(p)
                .filter(|n| *n > 0)
                .ok_or_else(|| format!("{a} is not a valid page number."))?;
            continue;
        }
        if a.len() < 2 || !a.starts_with('-') {
            i -= 1;
            break;
        }
        let body = &a[1..];
        let mut j = 0;
        while j < body.len() {
            let c = body[j..].chars().next().unwrap_or(' ');
            let after = &body[j + c.len_utf8()..];
            if c.is_ascii_digit() {
                let end = body[j..]
                    .find(|ch: char| !ch.is_ascii_digit())
                    .map_or(body.len(), |k| j + k);
                o.columns = num(&body[j..end], c)?.max(1);
                j = end;
                continue;
            }
            match c {
                'a' => o.across = true,
                'd' => o.double = true,
                'F' | 'f' => o.formfeed = true,
                'm' => o.merge = true,
                'p' => {}
                'r' => o.quiet = true,
                't' => o.no_header = true,
                'e' | 'i' | 'n' | 's' => {
                    match c {
                        'e' => o.expand = Some(char_num(after, '\t', 8)?),
                        'i' => o.unexpand = Some(char_num(after, '\t', 8)?),
                        'n' => o.number = Some(char_num(after, '\t', 5)?),
                        _ => o.sep = Some(after.chars().next().unwrap_or('\t')),
                    }
                    break;
                }
                'h' | 'l' | 'o' | 'w' => {
                    let v = if after.is_empty() {
                        if i >= args.len() {
                            return Err(format!("Option -{c} needs an argument."));
                        }
                        i += 1;
                        args[i - 1].clone()
                    } else {
                        after.to_string()
                    };
                    match c {
                        'h' => o.header = Some(v),
                        'l' => o.length = num(&v, c)?,
                        'o' => o.offset = num(&v, c)?,
                        _ => o.width = num(&v, c)?.max(1),
                    }
                    break;
                }
                other => return Err(format!("Option -{other} is not valid.")),
            }
            j += c.len_utf8();
        }
    }
    if o.merge && o.columns > 1 {
        return Err("Options -m and -column cannot be used together.".to_string());
    }
    Ok((o, args[i..].to_vec()))
}

/// Expand input tabs (`-e`).
fn expand(line: &str, tab: char, gap: usize) -> String {
    let mut out = String::new();
    let mut col = 0;
    for ch in line.chars() {
        if ch == tab {
            let n = gap - col % gap;
            out.extend(std::iter::repeat_n(' ', n));
            col += n;
        } else {
            out.push(ch);
            col += 1;
        }
    }
    out
}

/// Replace spaces reaching tab stops with `tab` (`-i`).
fn unexpand(line: &str, tab: char, gap: usize) -> String {
    let mut out = String::new();
    let mut spaces = 0;
    let mut col = 0;
    for ch in line.chars() {
        col += 1;
        if ch == ' ' {
            spaces += 1;
            if col % gap == 0 {
                if spaces > 1 {
                    out.push(tab);
                } else {
                    out.push(' ');
                }
                spaces = 0;
            }
            continue;
        }
        out.extend(std::iter::repeat_n(' ', spaces));
        spaces = 0;
        out.push(ch);
    }
    out.extend(std::iter::repeat_n(' ', spaces));
    out
}

struct Source {
    name: String,
    lines: Vec<String>,
    mtime: Option<i64>,
}

fn read_source(cx: &UtilCtx<'_>, o: &Opts, f: &str) -> Result<Source, qsh::sys::SysError> {
    let fd = input(cx, f)?;
    let mut l = Lines::new(fd);
    let mut lines = Vec::new();
    while let Some((t, _)) = l.next_text()? {
        let s = String::from_utf8_lossy(&t).into_owned();
        lines.push(match o.expand {
            Some((c, g)) => expand(&s, c, g),
            None => s,
        });
    }
    let mtime = if f == "-" {
        None
    } else {
        cx.sys.stat(&cx.path(f)).ok().map(|st| st.mtime)
    };
    Ok(Source {
        name: if f == "-" { String::new() } else { f.to_string() },
        lines,
        mtime,
    })
}

/// Lays pages out and writes them.
struct Printer<'o> {
    o: &'o Opts,
    out: Out,
    date: String,
    header: String,
    page: u64,
    body: usize,
    trailer: bool,
}

impl Printer<'_> {
    fn emit_page(&mut self, rows: &[String]) {
        let o = self.o;
        let show = self.page >= o.first_page;
        if show {
            let pad = " ".repeat(o.offset);
            if self.trailer {
                self.out.text(&format!(
                    "\n\n{pad}{} {} Page {}\n\n\n",
                    self.date, self.header, self.page
                ));
            }
            let mut used = 0;
            for r in rows {
                let mut line = if r.is_empty() {
                    String::new()
                } else {
                    format!("{pad}{r}")
                };
                if let Some((c, g)) = o.unexpand {
                    line = unexpand(&line, c, g);
                }
                self.out.text(&line);
                self.out.text("\n");
                used += 1;
            }
            if o.formfeed {
                self.out.text("\x0c");
            } else if self.trailer {
                for _ in used..self.body {
                    self.out.text("\n");
                }
                self.out.text("\n\n\n\n\n");
            }
        }
        self.page += 1;
    }
}

fn numbered(o: &Opts, n: usize, text: &str) -> String {
    match o.number {
        Some((c, w)) => {
            let s = n.to_string();
            let s = if s.len() > w { &s[s.len() - w..] } else { &s };
            format!("{s:>w$}{c}{text}")
        }
        None => text.to_string(),
    }
}

/// Join column cells into one output line.
fn join_cells(o: &Opts, cells: &[Option<String>], ncol: usize) -> String {
    if ncol == 1 {
        return cells[0].clone().unwrap_or_default();
    }
    let mut out = String::new();
    match o.sep {
        Some(sep) => {
            let last = cells.iter().rposition(Option::is_some).unwrap_or(0);
            for (i, c) in cells.iter().enumerate().take(last + 1) {
                if i > 0 {
                    out.push(sep);
                }
                out.push_str(c.as_deref().unwrap_or(""));
            }
        }
        None => {
            let w = (o.width.saturating_sub(ncol - 1) / ncol).max(1);
            for (i, c) in cells.iter().enumerate() {
                let t: String = c.as_deref().unwrap_or("").chars().take(w).collect();
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(&t);
                out.extend(std::iter::repeat_n(' ', w - t.chars().count()));
            }
            out = out.trim_end().to_string();
        }
    }
    out
}

fn date_of(cx: &UtilCtx<'_>, mtime: Option<i64>) -> String {
    let t = mtime.unwrap_or_else(|| i64::try_from(cx.sys.now().as_secs()).unwrap_or(0));
    strftime("%b %e %H:%M %Y", &Zone::of(cx).local(t))
}

fn rows_with_spacing(o: &Opts, rows: Vec<String>) -> Vec<String> {
    if !o.double {
        return rows;
    }
    let mut v = Vec::new();
    for r in rows {
        v.push(r);
        v.push(String::new());
    }
    v
}

/// Run `pr`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let (o, ops) = match parse(&cx.argv[1..]) {
        Ok(x) => x,
        Err(e) => return usage(cx, NAME, &e),
    };
    let files = crate::common::io::inputs(&ops);
    let trailer = !o.no_header && o.length > 10;
    let body = if trailer { o.length - 10 } else { o.length.max(1) };
    let rows_per_page = if o.double { (body / 2).max(1) } else { body };
    let mut status = 0;
    let mut sources = Vec::new();
    for f in &files {
        match read_source(cx, &o, f) {
            Ok(s) => sources.push(s),
            Err(e) => {
                if !o.quiet {
                    complain_sys(cx, NAME, f, e);
                }
                status = 1;
            }
        }
    }
    let mut p = Printer {
        o: &o,
        out: Out::stdout(cx),
        date: String::new(),
        header: String::new(),
        page: 1,
        body,
        trailer,
    };
    if o.merge {
        p.date = date_of(cx, None);
        p.header = o.header.clone().unwrap_or_default();
        let n = sources.len().max(1);
        let total = sources.iter().map(|s| s.lines.len()).max().unwrap_or(0);
        let mut row = 0;
        while row < total {
            let end = (row + rows_per_page).min(total);
            let rows: Vec<String> = (row..end)
                .map(|r| {
                    let cells: Vec<Option<String>> =
                        sources.iter().map(|s| s.lines.get(r).cloned()).collect();
                    numbered(&o, r + 1, &join_cells(&o, &cells, n))
                })
                .collect();
            p.emit_page(&rows_with_spacing(&o, rows));
            row = end;
        }
        return p.out.finish(cx, NAME, status);
    }
    for s in &sources {
        p.page = 1;
        p.date = date_of(cx, s.mtime);
        p.header = o.header.clone().unwrap_or_else(|| s.name.clone());
        let lines: Vec<String> = s
            .lines
            .iter()
            .enumerate()
            .map(|(i, l)| numbered(&o, i + 1, l))
            .collect();
        let k = o.columns;
        let per_page = rows_per_page * k;
        let mut at = 0;
        while at < lines.len() {
            let end = (at + per_page).min(lines.len());
            let chunk = &lines[at..end];
            let nrows = if chunk.len() == per_page {
                rows_per_page
            } else {
                chunk.len().div_ceil(k)
            };
            let rows: Vec<String> = (0..nrows)
                .map(|r| {
                    let cells: Vec<Option<String>> = (0..k)
                        .map(|c| {
                            let i = if o.across { r * k + c } else { c * nrows + r };
                            chunk.get(i).cloned()
                        })
                        .collect();
                    join_cells(&o, &cells, k)
                })
                .collect();
            p.emit_page(&rows_with_spacing(&o, rows));
            at = end;
        }
    }
    if p.page <= o.first_page && o.first_page > 1 && !sources.is_empty() {
        complain(cx, NAME, &format!("There is no page {}.", o.first_page));
        status = 1;
    }
    p.out.finish(cx, NAME, status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs() {
        assert_eq!(expand("a\tb", '\t', 4), "a   b");
        assert_eq!(unexpand("a       b", '\t', 8), "a\tb");
        assert_eq!(unexpand("abcdefg b", '\t', 8), "abcdefg b");
        assert_eq!(char_num("x3", '\t', 8).unwrap(), ('x', 3));
        assert_eq!(char_num("", '\t', 8).unwrap(), ('\t', 8));
    }
}
