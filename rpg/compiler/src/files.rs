// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! File declarations: an `F` spec's columns or a `DCL-F`'s keywords
//! into one description (device, usage, program or external
//! description, keys), and the external descriptions the compiler
//! reads from the `*FILE` objects at compile time.

use rpgfront::Diag;
use rpgfront::ast::{FileDecl, Keyword};

use crate::decl::{int_arg, str_arg, word_arg};
use crate::diag::ids;
use crate::types::Ty;

/// A file's device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Dev {
    /// `DISK`.
    #[default]
    Disk,
    /// `PRINTER`.
    Printer,
    /// `WORKSTN`.
    Workstn,
    /// `SEQ`.
    Seq,
    /// `SPECIAL`.
    Special,
}

impl Dev {
    /// The device as the listing names it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Disk => "DISK",
            Self::Printer => "PRINTER",
            Self::Workstn => "WORKSTN",
            Self::Seq => "SEQ",
            Self::Special => "SPECIAL",
        }
    }
}

/// A field of an external record format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtField {
    /// The field name.
    pub name: String,
    /// Its RPG type.
    pub ty: Ty,
    /// Its offset in the record buffer.
    pub offset: u32,
    /// `I` input, `O` output, `B` both, `H` hidden, `P` program to
    /// system (display files).
    pub usage: char,
    /// The `ALIAS` name.
    pub alias: String,
    /// Null capable.
    pub nullable: bool,
}

/// An external record format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtFormat {
    /// The format name.
    pub name: String,
    /// The 13-character level identifier.
    pub level_id: String,
    /// The record length.
    pub len: u32,
    /// The fields.
    pub fields: Vec<ExtField>,
    /// Key field names, in key order.
    pub keys: Vec<String>,
}

/// An external file description.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtFile {
    /// The library it was found in.
    pub lib: String,
    /// What kind of file.
    pub dev: Dev,
    /// Its formats.
    pub formats: Vec<ExtFormat>,
}

/// Where external descriptions come from at compile time.
pub trait ExtFiles {
    /// The description of `lib/name` (`*LIBL` for the library list).
    fn describe(&mut self, name: &str, lib: &str) -> Option<ExtFile>;
}

/// No external files.
pub struct NoExtFiles;

impl ExtFiles for NoExtFiles {
    fn describe(&mut self, _name: &str, _lib: &str) -> Option<ExtFile> {
        None
    }
}

/// A file as the compiler sees it.
#[derive(Clone, Debug, Default)]
pub struct File {
    /// Upper-case name.
    pub name: String,
    /// The object it opens (`EXTFILE`), library and name.
    pub obj: (String, String),
    /// `EXTMBR`.
    pub member: String,
    /// The device.
    pub dev: Dev,
    /// Input.
    pub input: bool,
    /// Output (or add, for disk files).
    pub output: bool,
    /// Update.
    pub update: bool,
    /// Delete.
    pub delete: bool,
    /// Keyed access.
    pub keyed: bool,
    /// Externally described.
    pub external: bool,
    /// Program-described record length.
    pub reclen: u32,
    /// The external description, when external.
    pub ext: Option<ExtFile>,
    /// `OFLIND`.
    pub oflind: Option<String>,
    /// The compiler assigned the overflow indicator, with an automatic
    /// skip at overflow.
    pub auto_overflow: bool,
    /// `INFDS`.
    pub infds: Option<String>,
    /// `INFSR`.
    pub infsr: Option<String>,
    /// `USROPN`.
    pub usropn: bool,
    /// Primary (`P`), secondary (`S`), full procedural (`F`) or
    /// output only (blank).
    pub designation: String,
    /// End-of-file (`E` in column 19).
    pub eof: bool,
    /// `PREFIX(text:n)`.
    pub prefix: Option<(String, u32)>,
    /// `RENAME(ext:int)` pairs.
    pub renames: Vec<(String, String)>,
    /// `INDDS`.
    pub indds: Option<String>,
    /// `SFILE(fmt:rrn)` pairs.
    pub sfiles: Vec<(String, String)>,
    /// `QUALIFIED`.
    pub qualified: bool,
    /// `TEMPLATE`.
    pub template: bool,
    /// The scope it belongs to (0 global).
    pub scope: usize,
    /// Where it is declared.
    pub def_line: u32,
    /// Its IR file index.
    pub ir_index: u32,
}

impl File {
    /// The IR usage keyword.
    #[must_use]
    pub fn ir_usage(&self) -> &'static str {
        match (self.input, self.output || self.update) {
            (true, false) => "*INPUT",
            (false, true) if !self.update => "*OUTPUT",
            (true, true) if !self.update => "*ALL",
            _ => "*UPDATE",
        }
    }
}

fn kw<'a>(f: &'a FileDecl, name: &str) -> Option<&'a Keyword> {
    f.kws.iter().find(|k| k.name == name)
}

/// The usage words of `USAGE(...)`.
fn usage_words(k: &Keyword) -> Vec<String> {
    (0..k.args.len()).filter_map(|i| word_arg(k, i)).collect()
}

/// Describe a file declaration; external formats come from `ext`.
pub fn file(f: &FileDecl, scope: usize, ext: &mut dyn ExtFiles, diags: &mut Vec<Diag>) -> File {
    let mut out = File {
        name: f.name.to_ascii_uppercase(),
        scope,
        def_line: f.at.line,
        ..File::default()
    };
    if let Some(c) = &f.fixed {
        out.dev = match c.device.as_str() {
            "PRINTER" => Dev::Printer,
            "WORKSTN" => Dev::Workstn,
            "SEQ" => Dev::Seq,
            "SPECIAL" => Dev::Special,
            _ => Dev::Disk,
        };
        match c.ftype.as_str() {
            "I" => out.input = true,
            "O" => out.output = true,
            "U" => {
                out.input = true;
                out.update = true;
                out.delete = true;
            }
            "C" => {
                out.input = true;
                out.output = true;
            }
            _ => diags.push(Diag::new(ids::BAD_KEYWORD_VALUE, 30, f.at).arg(c.ftype.clone())),
        }
        if c.addition == "A" {
            out.output = true;
        }
        out.designation.clone_from(&c.designation);
        out.eof = c.eof == "E";
        out.external = c.format == "E";
        out.reclen = c.reclen.parse().unwrap_or(0);
        out.keyed = c.rectype == "K" || c.keylen.parse::<u32>().is_ok();
    } else {
        // Free form: the device keyword, then USAGE.
        out.external = true;
        let mut dev_kw = None;
        for name in ["DISK", "PRINTER", "WORKSTN", "SEQ", "SPECIAL"] {
            if let Some(k) = kw(f, name) {
                dev_kw = Some(k);
                out.dev = match name {
                    "PRINTER" => Dev::Printer,
                    "WORKSTN" => Dev::Workstn,
                    "SEQ" => Dev::Seq,
                    "SPECIAL" => Dev::Special,
                    _ => Dev::Disk,
                };
            }
        }
        if let Some(k) = dev_kw
            && let Some(n) = int_arg(k, 0)
        {
            out.external = false;
            out.reclen = u32::try_from(n).unwrap_or(0);
        }
        if matches!(out.dev, Dev::Seq | Dev::Special) {
            out.external = false;
        }
        match kw(f, "USAGE") {
            Some(k) => {
                for w in usage_words(k) {
                    match w.as_str() {
                        "*INPUT" => out.input = true,
                        "*OUTPUT" => out.output = true,
                        "*UPDATE" => {
                            out.input = true;
                            out.update = true;
                        }
                        "*DELETE" => {
                            out.input = true;
                            out.update = true;
                            out.delete = true;
                        }
                        _ => diags.push(Diag::new(ids::BAD_KEYWORD_VALUE, 30, k.at).arg(w)),
                    }
                }
            }
            None => match out.dev {
                Dev::Printer => out.output = true,
                Dev::Workstn => {
                    out.input = true;
                    out.output = true;
                }
                _ => out.input = true,
            },
        }
        out.keyed = kw(f, "KEYED").is_some();
        out.designation = "F".into();
        if out.dev == Dev::Printer {
            out.designation.clear();
        }
    }
    out.oflind = kw(f, "OFLIND").and_then(|k| word_arg(k, 0));
    out.infds = kw(f, "INFDS").and_then(|k| word_arg(k, 0));
    out.infsr = kw(f, "INFSR").and_then(|k| word_arg(k, 0));
    out.indds = kw(f, "INDDS").and_then(|k| word_arg(k, 0));
    out.usropn = kw(f, "USROPN").is_some();
    out.qualified = kw(f, "QUALIFIED").is_some();
    out.template = kw(f, "TEMPLATE").is_some();
    out.prefix = kw(f, "PREFIX").and_then(|k| {
        let p = k.arg(0)?.toks.iter().map(rpgfront::lex::Tok::text).collect::<String>().to_ascii_uppercase();
        Some((p, int_arg(k, 1).and_then(|v| u32::try_from(v).ok()).unwrap_or(0)))
    });
    for k in f.kws.iter().filter(|k| k.name == "RENAME") {
        if let (Some(a), Some(b)) = (word_arg(k, 0), word_arg(k, 1)) {
            out.renames.push((a, b));
        }
    }
    for k in f.kws.iter().filter(|k| k.name == "SFILE") {
        if let (Some(a), Some(b)) = (word_arg(k, 0), word_arg(k, 1)) {
            out.sfiles.push((a, b));
        }
    }
    out.obj = match kw(f, "EXTFILE") {
        Some(k) => {
            let s = str_arg(k, 0).or_else(|| word_arg(k, 0)).unwrap_or_default().to_ascii_uppercase();
            match s.split_once('/') {
                Some((l, n)) => (l.to_string(), n.to_string()),
                None => ("*LIBL".into(), s),
            }
        }
        None => ("*LIBL".into(), out.name.clone()),
    };
    out.member = kw(f, "EXTMBR")
        .and_then(|k| str_arg(k, 0).or_else(|| word_arg(k, 0)))
        .unwrap_or_else(|| "*FIRST".into());
    if out.external {
        let desc_name = kw(f, "EXTDESC").and_then(|k| str_arg(k, 0)).unwrap_or_else(|| out.obj.1.clone());
        let (lib, name) = match desc_name.split_once('/') {
            Some((l, n)) => (l.to_string(), n.to_string()),
            None => (out.obj.0.clone(), desc_name),
        };
        match ext.describe(&name, &lib) {
            Some(d) => out.ext = Some(d),
            None => diags.push(Diag::new(ids::EXT_NOT_FOUND, 40, f.at).arg(name)),
        }
    }
    out
}
