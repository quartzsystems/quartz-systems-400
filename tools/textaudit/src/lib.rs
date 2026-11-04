// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The text audit: every string a user reads at a 5250 display or in a
//! printed report comes from a message file (`*MSGF`), a panel group
//! or a display file source, never from a Rust literal, so that a
//! secondary language library can translate it.
//!
//! `scope.toml` names the display paths (the sources whose output a
//! panel, a display file or a spooled file shows) by crate. In those
//! sources every string literal outside tests, attributes, logging,
//! assertions and `.expect(...)` is a candidate; it is text when it
//! holds words ([`prose::is_prose`]) and is not an interface token
//! the allow-list names (IBM's names, keywords, special values). Each
//! is a site. `baseline.txt` lists the sites not yet moved; a site not
//! in it fails the audit, so the count never grows.

pub mod prose;
pub mod scan;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The scope, relative to the repository root.
pub const SCOPE: &str = "tools/textaudit/scope.toml";
/// The baseline, relative to the repository root.
pub const BASELINE: &str = "tools/textaudit/baseline.txt";

/// Errors of the audit.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file could not be read.
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    /// The scope did not parse.
    #[error("{SCOPE}: {0}")]
    Scope(String),
    /// A source did not tokenize.
    #[error("{0} does not tokenize")]
    Parse(PathBuf),
}

/// A result of the audit.
pub type Result<T> = std::result::Result<T, Error>;

/// `scope.toml`.
#[derive(Debug, Deserialize)]
pub struct Scope {
    /// The display paths, by crate.
    #[serde(rename = "crate")]
    pub crates: Vec<CrateScope>,
    /// Interface tokens.
    #[serde(default)]
    pub allow: Allow,
}

/// One crate's display paths.
#[derive(Debug, Deserialize)]
pub struct CrateScope {
    /// The crate.
    pub name: String,
    /// Files or directories (every `.rs` under them), from the root.
    pub paths: Vec<String>,
    /// Files or directories under `paths` that are not display paths.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// What is not text.
#[derive(Debug, Default, Deserialize)]
pub struct Allow {
    /// Literals that are interface tokens, exactly.
    #[serde(default)]
    pub exact: Vec<String>,
    /// Calls whose literal arguments are not text (keys, names).
    #[serde(default)]
    pub calls: Vec<String>,
}

/// What a site's text is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A message's text written beside its identifier (`CPF9801 ...`,
    /// `Fail::new(id, text)`): the message file has it.
    Message,
    /// An error's detail (`Err(...)`, `map_err`): the function check's
    /// `CPD9898` in the job log.
    Detail,
    /// Text drawn on a display or printed in a report.
    Display,
}

impl Kind {
    /// Its name in the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Detail => "detail",
            Self::Display => "display",
        }
    }
}

/// Calls whose text argument is a message's, beside its identifier.
const MESSAGE_CALLS: &[&str] = &["Fail::new", "LineMsg::error", "LineMsg::info", "with_text"];
/// Calls whose text argument is an error's detail.
const DETAIL_CALLS: &[&str] = &[
    "Err",
    "map_err",
    "ok_or",
    "ok_or_else",
    "failed",
    "Fail::check",
    "Fail::read",
    "bail!",
    "anyhow!",
];

/// Whether `s` starts with a message identifier (`CPF9801`, `CPD0084`).
fn starts_with_msgid(s: &str) -> bool {
    let t = s.trim_start();
    // `{CREATED} ...`: an identifier's constant.
    if let Some(rest) = t.strip_prefix('{')
        && let Some(end) = rest.find('}')
        && end > 2
        && rest[..end].chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
    {
        return true;
    }
    let b = t.as_bytes();
    b.len() >= 7
        && b[..3].iter().all(u8::is_ascii_uppercase)
        && b[3..7].iter().all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(c))
        && b.get(7).is_none_or(|c| !c.is_ascii_alphanumeric())
}

/// The kind of a literal by its text and the calls around it.
#[must_use]
pub fn kind_of(text: &str, calls: &[String]) -> Kind {
    // The innermost function call, through `format!` and the like.
    let call = calls.iter().rev().find(|c| !c.ends_with('!') || c.as_str() == "bail!");
    if starts_with_msgid(text) || call.is_some_and(|c| MESSAGE_CALLS.contains(&c.as_str())) {
        Kind::Message
    } else if calls.iter().any(|c| DETAIL_CALLS.contains(&c.as_str())) {
        Kind::Detail
    } else {
        Kind::Display
    }
}

/// One site: a literal of a display path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Site {
    /// The source, from the root, with `/`.
    pub file: String,
    /// The literal's line.
    pub line: usize,
    /// The literal.
    pub text: String,
    /// What it is.
    pub kind: Kind,
}

impl Site {
    /// Its baseline key: the file and the text, escaped to one line.
    #[must_use]
    pub fn key(&self) -> String {
        format!("{}\t{}", self.file, self.text.escape_default())
    }
}

/// The audit's result.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Every site, by crate.
    pub sites: BTreeMap<String, Vec<Site>>,
    /// Sites not in the baseline.
    pub new: Vec<Site>,
    /// Baseline entries no longer found (moved to a message file).
    pub stale: Vec<String>,
    /// The baseline's size.
    pub baseline: usize,
}

impl Report {
    /// The sites in all.
    #[must_use]
    pub fn total(&self) -> usize {
        self.sites.values().map(Vec::len).sum()
    }

    /// New literals fail the audit.
    #[must_use]
    pub fn fails(&self) -> bool {
        !self.new.is_empty()
    }

    /// One line per crate, then the total.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut s = format!("{:<14} {:>6} {:>8} {:>7} {:>8}\n", "crate", "sites", "message", "detail", "display");
        let count = |v: &[Site], k: Kind| v.iter().filter(|x| x.kind == k).count();
        for (c, v) in &self.sites {
            let _ = writeln!(
                s,
                "{c:<14} {:>6} {:>8} {:>7} {:>8}",
                v.len(),
                count(v, Kind::Message),
                count(v, Kind::Detail),
                count(v, Kind::Display)
            );
        }
        let _ = writeln!(
            s,
            "{:<14} {:>6}  (baseline {}, new {}, moved {})",
            "total",
            self.total(),
            self.baseline,
            self.new.len(),
            self.stale.len()
        );
        s
    }

    /// The summary and the new sites.
    #[must_use]
    pub fn details(&self, limit: usize) -> String {
        let mut s = self.summary();
        if !self.new.is_empty() {
            let _ = writeln!(s, "\nnew literals on display paths ({}):", self.new.len());
            for n in self.new.iter().take(limit) {
                let _ = writeln!(s, "  {}:{}  {:?}", n.file, n.line, n.text);
            }
            if self.new.len() > limit {
                let _ = writeln!(s, "  ... and {} more", self.new.len() - limit);
            }
        }
        s
    }

    /// The baseline file for these sites.
    #[must_use]
    pub fn baseline_text(&self) -> String {
        let mut keys: Vec<String> = self.sites.values().flatten().map(Site::key).collect();
        keys.sort();
        let mut s = String::from(
            "# textaudit baseline: literals on display paths not yet moved to a message\n\
             # file or panel source. `cargo run -p textaudit -- baseline` rewrites it.\n",
        );
        for k in keys {
            s.push_str(&k);
            s.push('\n');
        }
        s
    }
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| Error::Io(path.to_path_buf(), e))
}

/// Every `.rs` under `p`, in order.
fn sources(p: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if p.is_file() {
        out.push(p.to_path_buf());
        return Ok(());
    }
    let mut ents: Vec<PathBuf> = std::fs::read_dir(p)
        .map_err(|e| Error::Io(p.to_path_buf(), e))?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .collect();
    ents.sort();
    for e in ents {
        if e.is_dir() {
            if e.file_name().is_some_and(|n| n == "tests") {
                continue;
            }
            sources(&e, out)?;
        } else if e.extension().is_some_and(|x| x == "rs") && e.file_name().is_none_or(|n| n != "tests.rs") {
            out.push(e);
        }
    }
    Ok(())
}

/// The scope of the tree at `root`.
pub fn scope(root: &Path) -> Result<Scope> {
    toml::from_str(&read(&root.join(SCOPE))?).map_err(|e| Error::Scope(e.to_string()))
}

fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// The sites of the tree at `root`, by crate.
pub fn sites(root: &Path) -> Result<BTreeMap<String, Vec<Site>>> {
    let sc = scope(root)?;
    let mut out = BTreeMap::new();
    for c in &sc.crates {
        let mut files = Vec::new();
        for p in &c.paths {
            sources(&root.join(p), &mut files)?;
        }
        files.dedup();
        let mut v = Vec::new();
        for f in files {
            let r = rel(root, &f);
            if c.exclude.iter().any(|x| r == *x || r.starts_with(&format!("{x}/"))) {
                continue;
            }
            let lits = scan::literals(&read(&f)?).ok_or_else(|| Error::Parse(f.clone()))?;
            for l in lits {
                if !prose::is_prose(&l.text)
                    || sc.allow.exact.contains(&l.text)
                    || l.calls.last().is_some_and(|k| sc.allow.calls.contains(k))
                {
                    continue;
                }
                v.push(Site {
                    file: r.clone(),
                    line: l.line,
                    kind: kind_of(&l.text, &l.calls),
                    text: l.text,
                });
            }
        }
        out.insert(c.name.clone(), v);
    }
    Ok(out)
}

/// The baseline's keys, each as often as it occurs.
pub fn baseline(root: &Path) -> Result<BTreeMap<String, usize>> {
    let p = root.join(BASELINE);
    let text = if p.exists() { read(&p)? } else { String::new() };
    let mut m = BTreeMap::new();
    for l in text.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
        *m.entry(l.to_string()).or_insert(0) += 1;
    }
    Ok(m)
}

/// Run the audit over the tree at `root`.
pub fn audit(root: &Path) -> Result<Report> {
    let sites = sites(root)?;
    let mut left = baseline(root)?;
    let total = left.values().sum();
    let mut new = Vec::new();
    for s in sites.values().flatten() {
        match left.get_mut(&s.key()) {
            Some(n) if *n > 0 => *n -= 1,
            _ => new.push(s.clone()),
        }
    }
    let stale = left
        .into_iter()
        .flat_map(|(k, n)| std::iter::repeat_n(k, n))
        .collect();
    Ok(Report {
        sites,
        new,
        stale,
        baseline: total,
    })
}

/// The repository root above `dir`.
#[must_use]
pub fn find_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors()
        .find(|d| d.join(SCOPE).is_file())
        .map(Path::to_path_buf)
}
