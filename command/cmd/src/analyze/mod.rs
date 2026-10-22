// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command analyzer: a command string to the command it names, its
//! parameters bound, converted and validated against the definition,
//! and its dependencies checked. The result is what the parameter
//! block (`block`) is built from and what the prompter edits.

mod bind;
mod conv;
mod dep;
pub mod expr;

pub use conv::{VarInfo, VarType};

use crate::def::{Allow, CmdDef, Mode, Parm, ParmType, Qual};
use crate::error::{CmdError, Diag, Result};
use crate::payload::CmdObject;
use crate::token::{self, Options, Sel, Tok, Tokens};
use crate::value::Value;

/// Where `*CMD` objects come from.
pub trait CmdSource {
    /// The command `name`, in `lib` or through the library list: the
    /// object and the library it was found in, with `*OBJOPR` and
    /// `*USE` authority checked (`CPF9820`-style failure), or `None`
    /// when there is no such command.
    fn lookup(&mut self, name: &str, lib: Option<&str>) -> Result<Option<(CmdObject, String)>>;
}

/// A fixed set of definitions, for tests and the compiler's own tables.
#[derive(Default)]
pub struct MemSource {
    /// The commands, by name.
    pub cmds: Vec<(String, CmdObject)>,
}

impl MemSource {
    /// Add a definition compiled from source, under `QSYS`.
    pub fn add(&mut self, name: &str, source: &str) -> std::result::Result<(), Vec<Diag>> {
        let def = crate::source::compile(name, source)?;
        self.cmds.push((
            name.to_ascii_uppercase(),
            CmdObject::new(def, source.to_string(), 0),
        ));
        Ok(())
    }

    /// Add a definition.
    pub fn put(&mut self, def: CmdDef) {
        let name = def.name.clone();
        self.cmds
            .push((name, CmdObject::new(def, String::new(), 0)));
    }
}

impl CmdSource for MemSource {
    fn lookup(&mut self, name: &str, lib: Option<&str>) -> Result<Option<(CmdObject, String)>> {
        if lib.is_some_and(|l| l != "QSYS" && l != "*LIBL") {
            return Ok(None);
        }
        Ok(self
            .cmds
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, o)| (o.clone(), "QSYS".to_string())))
    }
}

/// Where the command is being run from, for `ALLOW`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Setting {
    /// An interactive job (else batch).
    pub interactive: bool,
    /// Inside a CL program or module.
    pub program: bool,
    /// Through `QCMDEXC` or `QCAPCMD`.
    pub exec: bool,
    /// Compiling a CL module rather than a program.
    pub module: bool,
}

impl Setting {
    /// The `ALLOW` flags the setting needs.
    #[must_use]
    pub fn needs(self) -> Vec<u16> {
        let mut v = Vec::new();
        if self.program {
            v.push(match (self.interactive, self.module) {
                (true, true) => Allow::IMOD,
                (true, false) => Allow::IPGM,
                (false, true) => Allow::BMOD,
                (false, false) => Allow::BPGM,
            });
        } else {
            v.push(if self.interactive {
                Allow::INTERACT
            } else {
                Allow::BATCH
            });
        }
        if self.exec {
            v.push(Allow::EXEC);
        }
        v
    }
}

/// How a compiler answers for its variables.
pub type VarLookup = dyn Fn(&str) -> Option<VarInfo>;

/// What the analyzer knows about the job and the caller.
pub struct Env<'a> {
    /// The commands.
    pub source: &'a mut dyn CmdSource,
    /// Where the command runs.
    pub setting: Setting,
    /// The job's mode (`*PROD`, `*DEBUG`, `*SERVICE`).
    pub mode: u8,
    /// `LMTCPB(*YES)`: only commands with `ALWLMTUSR(*YES)` may run.
    pub limited: bool,
    /// Number formats.
    pub opts: Options,
    /// The job's `DATFMT`: `*MDY`, `*DMY`, `*YMD`, `*JUL`, `*ISO`...
    pub datfmt: String,
    /// The job's `DATSEP`.
    pub datsep: char,
    /// CL variables, when a compiler is analyzing: `&NAME` values may
    /// name them.
    pub vars: Option<&'a VarLookup>,
    /// Nesting depth of `*CMDSTR` analysis.
    pub nest: u32,
}

impl<'a> Env<'a> {
    /// An interactive job's environment over `source`.
    pub fn new(source: &'a mut dyn CmdSource) -> Self {
        Self {
            source,
            setting: Setting {
                interactive: true,
                ..Setting::default()
            },
            mode: Mode::PROD,
            limited: false,
            opts: Options::default(),
            datfmt: "*MDY".into(),
            datsep: '/',
            vars: None,
            nest: 0,
        }
    }
}

/// A parameter after binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    /// The keyword.
    pub kwd: String,
    /// The value; `None` when the parameter was omitted and has no
    /// default (a blank, zero or empty list is passed).
    pub value: Option<Value>,
    /// The caller gave the value (rather than the default).
    pub specified: bool,
    /// The selective-prompt prefix on the keyword.
    pub sel: Option<Sel>,
    /// The value as the caller wrote it, for the prompter; empty when
    /// the default was taken.
    pub given: Vec<Tok>,
}

/// A command string analyzed: the command and its bound parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// The command name.
    pub name: String,
    /// The library it was found in.
    pub lib: String,
    /// The definition, `CHGCMDDFT` applied, proxies followed.
    pub def: CmdDef,
    /// The label on the command string.
    pub label: Option<String>,
    /// `?` before the command: prompt the whole command.
    pub prompt: bool,
    /// The parameters in definition order.
    pub parms: Vec<Bound>,
}

impl Resolved {
    /// A bound parameter.
    #[must_use]
    pub fn get(&self, kwd: &str) -> Option<&Bound> {
        self.parms.iter().find(|b| b.kwd == kwd)
    }

    /// The command string as the analyzer would spell it: every
    /// specified parameter in keyword form.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = self.name.clone();
        for b in &self.parms {
            if let (true, Some(v)) = (b.specified, &b.value) {
                s.push(' ');
                s.push_str(&b.kwd);
                s.push('(');
                s.push_str(&v.parm_source());
                s.push(')');
            }
        }
        s
    }

    /// Whether any keyword carries a selective-prompt prefix.
    #[must_use]
    pub fn selective(&self) -> bool {
        self.parms.iter().any(|b| b.sel.is_some())
    }
}

/// The command length limit of `QCMDEXC`.
pub const MAX_LEN: usize = 32702;

/// Analyze a command string.
pub fn analyze(env: &mut Env<'_>, text: &str) -> Result<Resolved> {
    if text.chars().count() > MAX_LEN {
        return Err(CmdError::escape("CPF0005"));
    }
    let toks = token::tokenize(text, env.opts)?;
    analyze_tokens(env, &toks)
}

/// Analyze tokenized text.
pub fn analyze_tokens(env: &mut Env<'_>, toks: &Tokens) -> Result<Resolved> {
    if toks.name.is_empty() {
        return Err(CmdError::escape("CPF0006"));
    }
    let (object, lib) = resolve(env, &toks.name, toks.lib.as_deref())?;
    let def = object.effective();
    let name = def.name.clone();
    if env.limited && !def.alwlmtusr {
        return Err(Diag::new("CPD0043").data(&[(&name, 10)]).into());
    }
    let mut diags = Vec::new();
    for need in env.setting.needs() {
        if !def.allow.has(need) {
            diags.push(Diag::new("CPD0031").data(&[(&name, 10)]));
            break;
        }
    }
    if def.mode.0 & env.mode == 0 {
        diags.push(Diag::new("CPD0032").data(&[(&name, 10)]));
    }
    if !diags.is_empty() {
        return Err(CmdError::analysis(&name, diags));
    }
    let mut ctx = bind::Ctx {
        env,
        def: &def,
        diags: Vec::new(),
    };
    let parms = ctx.bind_all(&toks.toks);
    let mut diags = ctx.diags;
    if diags.is_empty() {
        dep::check(&def, &parms, &mut diags);
    }
    if !diags.is_empty() {
        return Err(CmdError::analysis(&name, diags));
    }
    Ok(Resolved {
        name,
        lib,
        def,
        label: toks.label.clone(),
        prompt: toks.prompt,
        parms,
    })
}

/// Follow proxies to the command `name` names; the object and its
/// library.
pub fn resolve(env: &mut Env<'_>, name: &str, lib: Option<&str>) -> Result<(CmdObject, String)> {
    let mut name = name.to_ascii_uppercase();
    let mut lib = lib.map(str::to_ascii_uppercase);
    for _ in 0..=5 {
        let Some((object, found)) = env.source.lookup(&name, lib.as_deref())? else {
            return Err(Diag::new("CPD0030")
                .data(&[(&name, 10), (lib.as_deref().unwrap_or("*LIBL"), 10)])
                .into());
        };
        match &object.proxy {
            None => return Ok((object, found)),
            Some(target) => {
                name.clone_from(&target.name);
                lib = (target.lib != "*LIBL").then(|| target.lib.clone());
            }
        }
    }
    Err(Diag::new("CPD0035").data(&[(&name, 10)]).into())
}

/// Whether a parameter may be given positionally at index `i`.
pub(crate) fn positional_ok(def: &CmdDef, i: usize) -> bool {
    def.maxpos.is_none_or(|m| i < usize::from(m)) && i < def.parms.len()
}

/// The processing program a resolved command calls.
#[must_use]
pub fn cpp(def: &CmdDef) -> &Qual {
    &def.cpp
}

/// Whether a parameter is a group of elements.
pub(crate) fn is_elem(parm: &Parm) -> bool {
    matches!(parm.ty, ParmType::Elem { .. })
}

/// Whether a parameter is a qualified name.
pub(crate) fn is_qual(parm: &Parm) -> bool {
    matches!(parm.ty, ParmType::Qual { .. })
}

/// The tokens of a default's source text.
pub(crate) fn default_tokens(text: &str, opts: Options) -> Vec<Tok> {
    token::tokenize_list(text, opts).unwrap_or_default()
}
