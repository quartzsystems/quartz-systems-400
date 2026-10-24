// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A corpus member and the directives in its header comments. A
//! directive is a comment that starts with `;`: `//; key: value` in
//! free form, `*; key: value` with the `*` in column 7 in fixed form.
//!
//! - `cmd:` the compile command, with `{lib}` and `{name}`; the
//!   default is `CRTBNDRPG` with `DBGVIEW(*NONE) OUTPUT(*PRINT)`;
//!   `cmd+:` lines go on with it.
//! - `call:` the call command; `run: no` compiles only.
//! - `rcdlen:` the source file's record length (112 by default).
//! - `setup:` a command run before the compile (several allowed);
//!   one that compiles (`CRT...`) keeps the compile gap.
//! - `with:` other members the case needs (copy members, modules of a
//!   service program): `NAME.rpginc` files beside it, uploaded under
//!   their own names.
//! - `post:` a command run after the `dds:` and `sql:` steps, before
//!   the compile (several allowed): `CHGPF`, `STRJRNPF` on the files
//!   they made.
//! - `cleanup:` a command run after the call (several allowed): the
//!   objects the setup made.
//! - `dds: NAME` a physical, logical, display or printer file created
//!   from the DDS in `rpg/dds/NAME.pf`, `.lf`, `.dspf` or `.prtf` (beside the
//!   batch folders) before the compile, in the work library, and
//!   deleted after the run.
//! - `sql: STATEMENT` an SQL statement run before the compile, after
//!   the files above it (`{lib}` is the work library): `CREATE TABLE
//!   {lib}.T (...)` (character columns `CCSID 37`), `INSERT INTO
//!   {lib}.T VALUES ...`. Tables it creates are deleted after the run.
//! - `sqlfile: NAME` the statements of `rpg/dds/NAME.sql`, each as a
//!   `sql:` step (cut at semicolons).
//! - `target:` `pub400`, the only system with a compiler.
//! - `keys:` the operator's answer to one screen (several, in order):
//!   `R,C=TEXT` types `TEXT` (quoted with `'` when it holds blanks)
//!   into the field at row `R`, column `C` and presses Field Exit
//!   (`R,C:TEXT` types without it),
//!   `@R,C` puts the cursor there, and the last word is the key
//!   (`enter`, `f1`-`f24`, `pgdn`, `pgup`, `help`). A member with
//!   `keys:` is called at a display (`rpg-oracle` signs on through the
//!   5250 driver) and every screen it reads is kept in the outcome.
//!
//! `dds:` and `sql:` steps run in order, the same on the oracle and in
//! the kit, so both sides hold the same files and records.

use std::path::Path;

/// One member of the corpus.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Case {
    /// Member name, upper case, at most 10 characters.
    pub name: String,
    /// The source lines.
    pub lines: Vec<String>,
    /// The compile command template.
    pub cmd: String,
    /// The call command template; empty when the case only compiles.
    pub call: String,
    /// Source record length.
    pub rcdlen: usize,
    /// Commands before the compile.
    pub setup: Vec<String>,
    /// Commands after the database steps, before the compile.
    pub post: Vec<String>,
    /// Commands after the call.
    pub cleanup: Vec<String>,
    /// Other members: name and lines.
    pub with: Vec<(String, Vec<String>)>,
    /// The names `with:` asked for, before they are loaded.
    pub with_names: Vec<String>,
    /// Database files and SQL statements, in order.
    pub db: Vec<DbStep>,
    /// The operator's answers, one per screen read.
    pub keys: Vec<String>,
}

/// What a DDS source describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdsKind {
    /// A physical file (`.pf`).
    Pf,
    /// A logical file (`.lf`).
    Lf,
    /// A display file (`.dspf`).
    Dspf,
    /// A printer file (`.prtf`).
    Prtf,
}

impl DdsKind {
    /// The source file's extension.
    #[must_use]
    pub fn ext(self) -> &'static str {
        match self {
            Self::Pf => "pf",
            Self::Lf => "lf",
            Self::Dspf => "dspf",
            Self::Prtf => "prtf",
        }
    }

    /// The command that creates the file.
    #[must_use]
    pub fn command(self) -> &'static str {
        match self {
            Self::Pf => "CRTPF",
            Self::Lf => "CRTLF",
            Self::Dspf => "CRTDSPF",
            Self::Prtf => "CRTPRTF",
        }
    }
}

/// One database setup step of a member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbStep {
    /// A file from DDS: its name, what it is, the source.
    Dds {
        /// The file name.
        name: String,
        /// What the DDS describes.
        kind: DdsKind,
        /// The DDS source lines.
        source: Vec<String>,
    },
    /// An SQL statement, `{lib}` not yet filled.
    Sql(String),
    /// `sqlfile: NAME` before loading: the statements of `dds/NAME.sql`.
    SqlFile(String),
}

/// The statements of an SQL script: cut at semicolons outside
/// literals, `--` comments dropped, blanks collapsed.
#[must_use]
pub fn statements(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        let mut quote = false;
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\'' {
                quote = !quote;
            }
            if !quote && ch == '-' && chars.peek() == Some(&'-') {
                break;
            }
            if !quote && ch == ';' {
                let s = cur.split_whitespace().collect::<Vec<_>>().join(" ");
                if !s.is_empty() {
                    out.push(s);
                }
                cur.clear();
                continue;
            }
            cur.push(ch);
        }
        cur.push(' ');
    }
    let s = cur.split_whitespace().collect::<Vec<_>>().join(" ");
    if !s.is_empty() {
        out.push(s);
    }
    out
}

impl DbStep {
    /// The file this step creates, if any: a DDS file, or the table
    /// or view of a `CREATE`.
    #[must_use]
    pub fn creates(&self) -> Option<String> {
        match self {
            Self::Dds { name, .. } => Some(name.clone()),
            Self::SqlFile(_) => None,
            Self::Sql(s) => {
                let w: Vec<String> = s.split_whitespace().take(3).map(str::to_ascii_uppercase).collect();
                if w.len() == 3 && w[0] == "CREATE" && matches!(w[1].as_str(), "TABLE" | "VIEW" | "INDEX") {
                    let n = w[2].rsplit(['.', '/']).next().unwrap_or("");
                    let n: String = n.chars().take_while(|c| c.is_ascii_alphanumeric() || "$#@_".contains(*c)).collect();
                    (!n.is_empty()).then_some(n)
                } else {
                    None
                }
            }
        }
    }
}

/// The default compile command.
pub const DEFAULT_CMD: &str =
    "CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT)";

/// The default call command.
pub const DEFAULT_CALL: &str = "CALL PGM({lib}/{name})";

impl Case {
    /// Read a member from its text.
    #[must_use]
    pub fn parse(name: &str, text: &str) -> Self {
        let mut c = Self {
            name: name.to_ascii_uppercase(),
            lines: text.replace("\r\n", "\n").lines().map(str::to_string).collect(),
            cmd: DEFAULT_CMD.into(),
            call: DEFAULT_CALL.into(),
            rcdlen: 112,
            setup: Vec::new(),
            post: Vec::new(),
            cleanup: Vec::new(),
            with: Vec::new(),
            with_names: Vec::new(),
            db: Vec::new(),
            keys: Vec::new(),
        };
        for l in c.lines.clone() {
            let Some((key, value)) = directive(&l) else {
                continue;
            };
            match key.as_str() {
                "cmd" => c.cmd = value,
                // A command too long for one record goes on.
                "cmd+" => {
                    c.cmd.push(' ');
                    c.cmd.push_str(&value);
                }
                "call" => c.call = value,
                "run" if value.eq_ignore_ascii_case("no") => c.call.clear(),
                "rcdlen" => c.rcdlen = value.parse().unwrap_or(112),
                "setup" => c.setup.push(value),
                "post" => c.post.push(value),
                "cleanup" => c.cleanup.push(value),
                "with" => c
                    .with_names
                    .extend(value.split([',', ' ']).filter(|s| !s.is_empty()).map(str::to_ascii_uppercase)),
                "dds" => c.db.push(DbStep::Dds {
                    name: value.to_ascii_uppercase(),
                    kind: DdsKind::Pf,
                    source: Vec::new(),
                }),
                "sql" => c.db.push(DbStep::Sql(value)),
                "sqlfile" => c.db.push(DbStep::SqlFile(value.to_ascii_uppercase())),
                "keys" => c.keys.push(value),
                _ => {}
            }
        }
        c
    }

    /// Read a member from a file named `NAME.rpgle` (or `.sqlrpgle`),
    /// with the `.rpginc` members its `with:` names and the DDS its
    /// `dds:` steps name from `../dds/`.
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut c = Self::parse(&name, &text);
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        for w in c.with_names.clone() {
            let p = dir.join(format!("{w}.rpginc"));
            let p = if p.exists() { p } else { dir.join(format!("{}.rpginc", w.to_ascii_lowercase())) };
            let t = std::fs::read_to_string(&p)?;
            c.with.push((w, t.replace("\r\n", "\n").lines().map(str::to_string).collect()));
        }
        let dir = path
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."))
            .join("dds");
        let read = |p: &Path| {
            std::fs::read_to_string(p).map_err(|e| std::io::Error::new(e.kind(), format!("{}: {e}", p.display())))
        };
        let mut steps = Vec::new();
        for step in std::mem::take(&mut c.db) {
            match step {
                DbStep::Dds { name, .. } => {
                    let kind = [DdsKind::Pf, DdsKind::Dspf, DdsKind::Prtf, DdsKind::Lf]
                        .into_iter()
                        .find(|k| dir.join(format!("{name}.{}", k.ext())).exists())
                        .unwrap_or(DdsKind::Lf);
                    let t = read(&dir.join(format!("{name}.{}", kind.ext())))?;
                    let source = t.lines().map(|l| l.trim_end_matches('\r').to_string()).collect();
                    steps.push(DbStep::Dds { name, kind, source });
                }
                DbStep::SqlFile(name) => {
                    let t = read(&dir.join(format!("{name}.sql")))?;
                    steps.extend(statements(&t).into_iter().map(DbStep::Sql));
                }
                s @ DbStep::Sql(_) => steps.push(s),
            }
        }
        c.db = steps;
        Ok(c)
    }

    /// A template with `{lib}` and `{name}` filled.
    #[must_use]
    pub fn fill(&self, template: &str, lib: &str) -> String {
        template.replace("{lib}", lib).replace("{name}", &self.name)
    }

    /// The source's data width: the record length less the sequence
    /// and date fields.
    #[must_use]
    pub fn width(&self) -> usize {
        self.rcdlen.saturating_sub(12)
    }

    /// Whether the member is too wide for its record length.
    #[must_use]
    pub fn too_wide(&self) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| l.chars().count() > self.width())
            .map(|i| i + 1)
    }
}

/// A directive comment: `//; key: value` or `*; key: value` in column 7.
fn directive(line: &str) -> Option<(String, String)> {
    let t = line.trim_start();
    let rest = if let Some(r) = t.strip_prefix("//;") {
        r
    } else if line.chars().nth(6) == Some('*') && line.chars().nth(7) == Some(';') {
        &line[line.char_indices().nth(8)?.0..]
    } else {
        return None;
    };
    let (k, v) = rest.split_once(':')?;
    Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_directives_in_both_forms() {
        let c = Case::parse(
            "hello",
            "**FREE\n//; cmd: CRTRPGMOD MODULE({lib}/{name})\n//; run: no\n",
        );
        assert_eq!(c.name, "HELLO");
        assert_eq!(c.fill(&c.cmd, "L"), "CRTRPGMOD MODULE(L/HELLO)");
        assert!(c.call.is_empty());
        let f = Case::parse("x", "      *; rcdlen: 252\n      *; setup: DLTF A\n      *; post: STRJRNPF A\n");
        assert_eq!(f.post, vec!["STRJRNPF A".to_string()]);
        assert_eq!(f.rcdlen, 252);
        assert_eq!(f.setup, vec!["DLTF A".to_string()]);
        let d = Case::parse("x", "//; dds: CUST\n//; sql: CREATE TABLE {lib}.T2 (A CHAR(1))\n");
        assert_eq!(d.db.len(), 2);
        assert_eq!(d.db[0].creates().as_deref(), Some("CUST"));
        assert_eq!(d.db[1].creates().as_deref(), Some("T2"));
    }
}
