// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `PRTSQLINF`: the SQL information of a program made with embedded SQL
//! as a printed report, in the layout pub400's shows
//! (`tools/conformance/oracle/sql-admin/prtsqlinf-pgm.txt`): the object,
//! the precompile options, then each statement with its tokens apart.
//! Lines start with their FCFC character.

use db::sqlinf::program::{StmtDef, StmtKind};

use crate::lex::{Tok, tokenize};

/// The statement as the report prints it: tokens one blank apart, words
/// upper case, host variables `: H`.
#[must_use]
pub fn statement_text(src: &str) -> String {
    let Ok(toks) = tokenize(src) else {
        return src.split_whitespace().collect::<Vec<_>>().join(" ");
    };
    let mut out: Vec<String> = Vec::new();
    for t in toks {
        match t.tok {
            Tok::End => break,
            Tok::Host { ind, .. } => {
                out.push(": H".into());
                if ind.is_some() {
                    out.push(": H".into());
                }
            }
            Tok::Word(w) => out.push(w),
            Tok::Delimited(d) => out.push(format!("\"{}\"", d.replace('"', "\"\""))),
            Tok::Str(s) => out.push(format!("'{}'", s.replace('\'', "''"))),
            Tok::Hex(b) => out.push(format!("X'{}'", hex(&b))),
            Tok::BinHex(b) => out.push(format!("BX'{}'", hex(&b))),
            Tok::Num(n) => out.push(n),
            Tok::Param => out.push("?".into()),
            Tok::Sym(s) => out.push(s.into()),
        }
    }
    out.join(" ")
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02X}");
        s
    })
}

/// What kind of statement `text` is, for its entry.
#[must_use]
pub fn statement_kind(text: &str) -> StmtKind {
    let w: Vec<String> = text
        .split_whitespace()
        .take(3)
        .map(str::to_ascii_uppercase)
        .collect();
    match w.first().map(String::as_str) {
        Some("DECLARE")
            if w.get(2).is_some_and(|x| x == "CURSOR") || w.iter().any(|x| x == "CURSOR") =>
        {
            StmtKind::Declare
        }
        Some("SELECT" | "VALUES" | "INSERT" | "UPDATE" | "DELETE" | "MERGE" | "WITH" | "SET") => {
            StmtKind::Planned
        }
        _ => StmtKind::Plain,
    }
}

/// The precompile as the report shows it.
#[derive(Clone, Debug, Default)]
pub struct Precompile {
    /// The command (`CRTSQLRPGI`).
    pub command: String,
    /// The object, `LIB/NAME`, as made.
    pub object: String,
    /// The source file, `LIB/FILE`, empty for a stream file.
    pub srcfile: String,
    /// The member.
    pub srcmbr: String,
    /// `COMMIT`.
    pub commit: String,
    /// `*SYS` or `*SQL`.
    pub naming: String,
    /// `DATFMT`, `DATSEP`, `TIMFMT`, `TIMSEP`.
    pub dates: [String; 4],
    /// `CLOSQLCSR`.
    pub closqlcsr: String,
    /// `TEXT`.
    pub text: String,
    /// `SET OPTION` values beyond those.
    pub other: Vec<(String, String)>,
    /// The release made for.
    pub release: String,
}

fn sep(v: &str) -> String {
    if v.starts_with('*') || v.starts_with('\'') {
        v.to_string()
    } else {
        format!("'{v}'")
    }
}

/// The options, keyword and value, in the report's order. The values
/// QS/400 holds every program to (`options.rs` refuses the others) are
/// printed as they are.
#[must_use]
pub fn options(p: &Precompile) -> Vec<(String, String)> {
    let other = |k: &str, dft: &str| -> String {
        p.other
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(k))
            .map_or_else(|| dft.to_string(), |(_, v)| v.clone())
    };
    let mut out = vec![("OBJ".into(), p.object.clone())];
    if !p.srcfile.is_empty() {
        out.push(("SRCFILE".into(), p.srcfile.clone()));
        out.push(("SRCMBR".into(), p.srcmbr.clone()));
    }
    out.extend([
        ("COMMIT".into(), p.commit.clone()),
        ("OPTION".into(), format!("{} *NOEXTIND *PERIOD", p.naming)),
        ("TGTRLS".into(), p.release.clone()),
        ("ALWCPYDTA".into(), other("ALWCPYDTA", "*OPTIMIZE")),
        ("CLOSQLCSR".into(), p.closqlcsr.clone()),
        ("RDB".into(), "*LOCAL".into()),
        ("DATFMT".into(), p.dates[0].clone()),
        ("DATSEP".into(), sep(&p.dates[1])),
        ("TIMFMT".into(), p.dates[2].clone()),
        ("TIMSEP".into(), sep(&p.dates[3])),
        ("DFTRDBCOL".into(), "*NONE".into()),
        ("DYNDFTCOL".into(), "*NO".into()),
        ("SQLCURRULE".into(), "*DB2".into()),
        ("ALWBLK".into(), other("ALWBLK", "*ALLREAD")),
        ("DLYPRP".into(), other("DLYPRP", "*NO")),
        ("DYNUSRPRF".into(), "*USER".into()),
        ("USRPRF".into(), "*NAMING".into()),
        ("SRTSEQ".into(), "*HEX".into()),
        ("RDBCNNMTH".into(), other("RDBCNNMTH", "*DUW")),
        ("TEXT".into(), format!("'{:<50}'", p.text)),
        ("SQLPATH".into(), "*LIBL".into()),
        ("DECRESULT".into(), "31 31 0".into()),
        ("DECFLTRND".into(), "*HALFEVEN".into()),
        ("CONACC".into(), "*DFT".into()),
    ]);
    out
}

/// The report's heading and object.
#[derive(Clone, Debug)]
pub struct Heading {
    /// The product in the page heading (`QS00SS1`).
    pub product: String,
    /// `VxRyMz`.
    pub release: String,
    /// `LIB/NAME`.
    pub object: String,
    /// `*PGM` or `*SRVPGM`.
    pub objtype: String,
    /// `yy-mm-dd`.
    pub date: String,
    /// `hh:mm:ss`.
    pub time: String,
    /// The command it was made with (`CRTSQLRPGI`).
    pub command: String,
    /// The statements' CCSID.
    pub ccsid: u16,
}

/// Lines on a page below the heading.
pub const PAGE_LINES: usize = 60;

/// Statement lines break before this many characters.
const WIDTH: usize = 72;

fn wrap(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for w in text.split(' ') {
        let limit = if lines.is_empty() { WIDTH } else { WIDTH - 4 };
        if !cur.is_empty() && cur.len() + 1 + w.len() > limit {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(w);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

struct Pages<'a> {
    h: &'a Heading,
    lines: Vec<String>,
    page: u32,
    used: usize,
}

impl Pages<'_> {
    fn heading(&mut self) {
        self.page += 1;
        let product = format!("{} {:<6}       ", self.h.product, self.h.release);
        let title = "Print SQL information";
        let objword = match self.h.objtype.as_str() {
            "*SRVPGM" => "Service program",
            _ => "Program",
        };
        self.lines.push(format!(
            "1{product:<26}{title:<31}{objword} {:<36}{} {}  Page   {:03}",
            self.h.object, self.h.date, self.h.time, self.page
        ));
        self.used = 0;
    }

    /// A group of lines kept on one page; the first's FCFC is `ctl`.
    fn group(&mut self, ctl: char, group: &[String]) {
        let gap = match ctl {
            '-' => 3,
            '0' => 2,
            _ => 1,
        };
        let mut ctl = ctl;
        if self.page == 0 || self.used + gap + group.len() > PAGE_LINES {
            if self.page > 0 {
                // A new page begins with a blank line, the group spaced after it.
                self.heading();
                self.lines.push("-".into());
                self.used += 3;
                ctl = '0';
            } else {
                self.heading();
            }
        }
        for (i, l) in group.iter().enumerate() {
            let c = if i == 0 { ctl } else { ' ' };
            self.lines.push(format!("{c}{l}"));
        }
        self.used += gap + group.len() - 1;
    }
}

/// The report, FCFC lines.
#[must_use]
pub fn render(h: &Heading, options: &[(String, String)], stmts: &[StmtDef]) -> Vec<String> {
    let mut p = Pages {
        h,
        lines: Vec::new(),
        page: 0,
        used: 0,
    };
    p.group(
        '-',
        &[
            format!("Object name...............{}", h.object),
            format!("Object type...............{}", h.objtype),
        ],
    );
    let mut opts = vec![String::new(), format!("  {}", h.command)];
    opts.extend(options.iter().map(|(k, v)| format!("      {k}({v})")));
    opts.push(format!("      STATEMENT TEXT CCSID({})", h.ccsid));
    p.group('-', &opts);
    // Declarations first, then the statements in source order.
    let order = stmts
        .iter()
        .filter(|s| s.kind == StmtKind::Declare)
        .chain(stmts.iter().filter(|s| s.kind != StmtKind::Declare));
    for s in order {
        let mut g: Vec<String> = wrap(&s.text)
            .into_iter()
            .enumerate()
            .map(|(i, l)| if i == 0 { l } else { format!("    {l}") })
            .collect();
        if s.kind != StmtKind::Plain {
            g.push(format!("  SQL5065  {}", crate::prtsqlinf::NO_PLAN));
        }
        p.group('-', &g);
    }
    p.lines.push(format!(
        " {:31}* * * * *  E N D  O F  L I S T I N G  * * * * *",
        ""
    ));
    p.lines
}

/// The text under a statement that has no access plan (`SQL5065`).
pub const NO_PLAN: &str = "No access plan is kept for the statement.";

/// Statements made into entries for a program's SQL information.
#[must_use]
pub fn entries(stmts: &[(u32, String)]) -> Vec<StmtDef> {
    stmts
        .iter()
        .filter(|(_, t)| {
            let w: Vec<String> = t
                .split_whitespace()
                .take(2)
                .map(str::to_ascii_uppercase)
                .collect();
            // SET OPTION, INCLUDE and WHENEVER are the precompiler's own.
            let decl = w.first().is_some_and(|x| x == "DECLARE")
                && !t.to_ascii_uppercase().contains("CURSOR");
            !(decl
                || w == ["SET", "OPTION"]
                || w.first().is_some_and(|x| {
                    x == "INCLUDE" || x == "WHENEVER" || x == "BEGIN" || x == "END"
                }))
        })
        .map(|(line, t)| StmtDef {
            line: *line,
            kind: statement_kind(t),
            text: statement_text(t),
        })
        .collect()
}

/// A `PRTSQLINF` form QS/400 does not print.
#[must_use]
pub fn refusal(what: &str) -> String {
    format!(
        "{what} is not supported: only programs and service programs made with embedded SQL are printed"
    )
}

/// `SQL0204`'s text: `name` in `lib` not found.
#[must_use]
pub fn not_found(name: &str, lib: &str) -> String {
    format!("{name} in {lib} not found.")
}

/// `SQL5062`'s text: the object has no SQL statements.
#[must_use]
pub fn no_statements(name: &str, lib: &str, ty: &str) -> String {
    format!("{name} in {lib} type {ty} has no SQL statements.")
}
