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
    /// The `DFT` value as written (`'ABC'`, `42`), for `INZ(*EXTDFT)`.
    pub default: Option<String>,
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
    /// A display record format's two buffers.
    pub dsp: Option<DspLayout>,
}

/// A display record format's buffers: the output buffer holds the
/// option indicators and the fields shown, the input buffer the
/// response indicators and the fields read (without `INDARA` the
/// indicators come first, one byte each).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DspLayout {
    /// The output buffer's length.
    pub out_len: u32,
    /// The input buffer's length.
    pub in_len: u32,
    /// The option indicators at the front of the output buffer.
    pub option_inds: Vec<u8>,
    /// The response indicators at the front of the input buffer.
    pub response_inds: Vec<u8>,
    /// Texts of response indicators (keys, error messages), for the
    /// listing.
    pub ind_texts: Vec<(u8, String)>,
    /// Each field's offset in the output and in the input buffer, by
    /// the format's field index.
    pub offsets: Vec<(Option<u32>, Option<u32>)>,
    /// A subfile record format (`SFL`).
    pub subfile: bool,
    /// The subfile a control record format controls (`SFLCTL`).
    pub sflctl: Option<String>,
    /// `SLNO(*VAR)`: the program gives the starting line (`SLN`).
    pub slno_var: bool,
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
    /// A display file with a separate indicator area (`INDARA`).
    pub indara: bool,
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
    /// The file its description comes from at compile time (`EXTDESC`).
    pub desc: String,
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
    /// `IGNORE` record formats.
    pub ignore: Vec<String>,
    /// `INCLUDE` record formats.
    pub include: Vec<String>,
    /// `BLOCK(*NO)`.
    pub noblock: bool,
    /// `TEMPLATE`.
    pub template: bool,
    /// A `LIKEFILE` parameter: the caller's file, bound at the call.
    pub param: bool,
    /// The scope it belongs to (0 global).
    pub scope: usize,
    /// Where it is declared.
    pub def_line: u32,
    /// Its IR file index.
    pub ir_index: u32,
    /// `FORMLEN` and `FORMOFL`, 0 when not given.
    pub form: (u16, u16),
    /// `EXTIND(*INUx)`: the file is opened only when the switch is on.
    pub extind: Option<String>,
    /// `PRTCTL(ds)`: the structure that controls the printer.
    pub prtctl: Option<String>,
    /// `PGMNAME`: the program a `SPECIAL` file calls.
    pub pgmname: Option<String>,
    /// Where `SLN` is given.
    pub sln_at: Option<rpgfront::ast::Pos>,
    /// `HANDLER`: an Open Access handler.
    pub handler: bool,
    /// `ALIAS`: the fields go by their alternative names.
    pub alias: bool,
    /// A program-described keyed file's key length (`KEYED(*CHAR:n)`,
    /// columns 29-33).
    pub keylen: u32,
    /// `LIKEFILE(parent)`.
    pub likefile: Option<String>,
    /// `RECNO(field)`: the relative record number of each record read.
    pub recno: Option<String>,
}

impl File {
    /// The IR usage keyword.
    #[must_use]
    pub fn ir_usage(&self) -> &'static str {
        match (self.input, self.output || self.update) {
            (true, false) => "*INPUT",
            (false, true) if !self.update => "*OUTPUT",
            (true, true) if !self.update => "*ALL",
            _ if self.update && self.output => "*ALL",
            _ => "*UPDATE",
        }
    }
}

/// The file keywords (free form adds the device keywords).
const FILE_KEYWORDS: &[&str] = &[
    "ALIAS", "BLOCK", "CCSID", "COMMIT", "DATA", "DATFMT", "DEVID", "EXTDESC", "EXTFILE", "EXTIND", "EXTMBR",
    "FORMLEN", "FORMOFL", "HANDLER", "IGNORE", "INCLUDE", "INDDS", "INFDS", "INFSR", "KEYED", "KEYLOC",
    "LIKEFILE", "MAXDEV", "OFLIND", "PASS", "PGMNAME", "PLIST", "PREFIX", "PRTCTL", "QUALIFIED", "RAFDATA",
    "RECNO", "RENAME", "SAVEDS", "SAVEIND", "SFILE", "SLN", "STATIC", "TEMPLATE", "TIMFMT", "USAGE", "USROPN",
];

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
    // A keyword area token that is not a word.
    for k in f.kws.iter().filter(|k| !k.name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '*')) {
        diags.push(Diag::new(rpgfront::diag::ids::BAD_KEYWORD, 30, k.at).arg(k.name.clone()));
    }
    // Each keyword once (the repeatable ones aside).
    for (i, k) in f.kws.iter().enumerate() {
        let again = f.kws[..i].iter().any(|x| x.name == k.name);
        if again && !matches!(k.name.as_str(), "RENAME" | "IGNORE" | "INCLUDE" | "SFILE" | "PREFIX") {
            diags.push(Diag::new(ids::FILE_KEYWORD_TWICE, 20, k.at).arg(k.name.clone()));
        }
    }
    // A keyword the compiler does not know is ignored.
    for k in f.kws.iter().filter(|k| k.name.starts_with(|c: char| c.is_ascii_alphabetic())) {
        let device = f.fixed.is_none() && matches!(k.name.as_str(), "DISK" | "PRINTER" | "WORKSTN" | "SEQ" | "SPECIAL");
        if !device && !FILE_KEYWORDS.contains(&k.name.as_str()) {
            diags.push(Diag::new(ids::UNKNOWN_FILE_KEYWORD, 30, k.at).arg(k.name.clone()));
        }
    }
    if let Some(c) = &f.fixed {
        out.dev = match c.device.as_str() {
            "PRINTER" => Dev::Printer,
            "WORKSTN" => Dev::Workstn,
            "SEQ" => Dev::Seq,
            "SPECIAL" => Dev::Special,
            "DISK" => Dev::Disk,
            _ => {
                diags.push(Diag::new(ids::BAD_DEVICE, 20, f.at));
                Dev::Disk
            }
        };
        // Entries IBM does not know take their defaults.
        let designation = if matches!(c.designation.as_str(), "" | "P" | "S" | "R" | "T" | "F") {
            c.designation.clone()
        } else {
            diags.push(Diag::new(ids::BAD_DESIGNATION, 20, f.at));
            String::new()
        };
        let format = if matches!(c.format.as_str(), "F" | "E") {
            c.format.as_str()
        } else {
            diags.push(Diag::new(ids::BAD_FILE_FORMAT, 20, f.at));
            "F"
        };
        if format == "F" && c.reclen.parse::<u32>().is_ok_and(|n| n == 0) {
            diags.push(Diag::new(ids::BAD_RECLEN, 20, f.at));
        }
        let ftype = if matches!(c.ftype.as_str(), "I" | "O" | "U" | "C") {
            c.ftype.as_str()
        } else {
            diags.push(Diag::new(ids::BAD_FILE_TYPE, 20, f.at));
            if designation.is_empty() { "O" } else { "I" }
        };
        match ftype {
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
            _ => {}
        }
        if c.addition == "A" {
            out.output = true;
        }
        out.designation = designation;
        out.eof = c.eof == "E";
        out.external = format == "E";
        out.reclen = c.reclen.parse().unwrap_or(0);
        out.keyed = c.rectype == "K" || c.keylen.parse::<u32>().is_ok();
        out.keylen = c.keylen.trim().parse().unwrap_or(0);
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
            if n < 1 {
                let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
                diags.push(Diag::new(ids::BAD_RECLEN, 20, at));
            }
        }
        // Keywords a printer does not take.
        if out.dev == Dev::Printer {
            for k in f.kws.iter().filter(|k| matches!(k.name.as_str(), "KEYED" | "BLOCK" | "COMMIT" | "RECNO" | "SFILE")) {
                diags.push(Diag::new(ids::NOT_FOR_PRINTER, 20, k.at).arg(k.name.clone()));
            }
        }
        // A USAGE value that is not one: as if none were given.
        let usage = kw(f, "USAGE").filter(|k| {
            let ok = usage_words(k).iter().all(|w| matches!(w.as_str(), "*INPUT" | "*OUTPUT" | "*UPDATE" | "*DELETE"));
            if !ok {
                let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
                diags.push(Diag::new(ids::USAGE_PARM, 20, at));
                diags.push(Diag::new(ids::TEXT_SKIPPED, 0, at));
            }
            ok
        });
        if matches!(out.dev, Dev::Seq | Dev::Special) {
            out.external = false;
        }
        match usage {
            Some(k) => {
                // A printer only writes.
                if out.dev == Dev::Printer && usage_words(k).iter().any(|w| w != "*OUTPUT") {
                    let at = k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at);
                    diags.push(Diag::new(ids::USAGE_DEVICE, 20, at));
                }
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
        out.keylen = kw(f, "KEYED").and_then(|k| crate::decl::int_arg(k, 1)).and_then(|v| u32::try_from(v).ok()).unwrap_or(0);
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
    let num = |n: &str| kw(f, n).and_then(|k| crate::decl::int_arg(k, 0)).and_then(|v| u16::try_from(v).ok()).unwrap_or(0);
    out.form = (num("FORMLEN"), num("FORMOFL"));
    out.extind = kw(f, "EXTIND").and_then(|k| word_arg(k, 0));
    out.prtctl = kw(f, "PRTCTL").and_then(|k| word_arg(k, 0));
    out.sln_at = kw(f, "SLN").map(|k| k.arg(0).and_then(|a| a.toks.first()).map_or(k.at, |t| t.at));
    out.handler = kw(f, "HANDLER").is_some();
    out.alias = kw(f, "ALIAS").is_some();
    out.recno = kw(f, "RECNO").and_then(|k| word_arg(k, 0));
    out.pgmname = kw(f, "PGMNAME").and_then(|k| crate::decl::str_arg(k, 0).or_else(|| word_arg(k, 0)));
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
    for k in f.kws.iter().filter(|k| k.name == "IGNORE" || k.name == "INCLUDE") {
        let v: Vec<String> = (0..k.args.len()).filter_map(|i| word_arg(k, i)).collect();
        if k.name == "IGNORE" {
            out.ignore.extend(v);
        } else {
            out.include.extend(v);
        }
    }
    out.noblock = kw(f, "BLOCK").and_then(|k| word_arg(k, 0)).as_deref() == Some("*NO");
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
    // Keywords of program-described files only.
    if out.external
        && let Some(k) = f.kws.iter().find(|k| matches!(k.name.as_str(), "RAFDATA" | "KEYLOC" | "FORMLEN" | "FORMOFL"))
    {
        diags.push(Diag::new(ids::NOT_FOR_EXTERNAL, 20, k.at).arg(k.name.clone()));
    }
    // LIKEFILE: the description is the parent's (`compile` copies it).
    // BLOCKED(phase9): a LIKEFILE parameter (a file passed to a
    // procedure) is not supported; the IR has no file handle to pass.
    out.likefile = kw(f, "LIKEFILE").and_then(|k| word_arg(k, 0));
    if out.external && out.likefile.is_none() {
        let desc_name = kw(f, "EXTDESC").and_then(|k| str_arg(k, 0)).unwrap_or_else(|| out.name.clone());
        let (lib, name) = match desc_name.split_once('/') {
            Some((l, n)) => (l.to_string(), n.to_string()),
            None => (out.obj.0.clone(), desc_name),
        };
        out.desc = name.to_ascii_uppercase();
        match ext.describe(&name, &lib) {
            Some(d) => out.ext = Some(d),
            None => diags.push(Diag::new(ids::FILE_EXT_NOT_FOUND, 40, f.at).arg(name)),
        }
    }
    out
}
