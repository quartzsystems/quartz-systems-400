// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The interpreter: shell state ([`Shell`]) and execution ([`exec`]).

pub mod exec;
pub mod fds;
pub mod jobs;
pub mod options;
pub mod traps;
pub mod vars;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::msg::{self, Msg};
use crate::syntax::{FunctionDef, Parser};
use crate::sys::{Fd, Sys, stream::write_all};
use fds::FdTable;
use jobs::Jobs;
use options::{Opt, Options};
use traps::Traps;
use vars::{ReadOnlyError, Var, Vars};

/// How control leaves a command other than by finishing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// `break n`.
    Break(u32),
    /// `continue n`.
    Continue(u32),
    /// `return n`.
    Return(i32),
    /// `exit n`, or `set -e`: leave the shell or subshell.
    Exit(i32),
    /// A shell error: a non-interactive shell exits with the status, an
    /// interactive one abandons the command.
    Error(i32),
    /// A builtin failed after reporting why; its status.
    Fail(i32),
}

/// A command's outcome: its status or a change of control.
pub type Res = Result<i32, Flow>;

/// The default `PATH` (Qshell's).
pub const DEFAULT_PATH: &str = "/usr/bin:.";

/// A shell: variables, functions, options, descriptors, jobs.
pub struct Shell {
    pub(crate) sys: Arc<dyn Sys>,
    pub(crate) vars: Vars,
    pub(crate) funcs: HashMap<String, Arc<FunctionDef>>,
    pub(crate) aliases: BTreeMap<String, String>,
    pub(crate) opts: Options,
    pub(crate) arg0: String,
    pub(crate) args: Vec<String>,
    pub(crate) status: i32,
    pub(crate) last_bg: Option<i32>,
    pub(crate) fds: FdTable,
    pub(crate) cwd: String,
    pub(crate) umask: u32,
    pub(crate) traps: Traps,
    pub(crate) jobs: Jobs,
    pub(crate) hash: BTreeMap<String, String>,
    /// `$$`: the shell's process ID, the same in subshells.
    pub(crate) pid: i32,
    /// This (sub)shell's own process ID, for signals.
    pub(crate) self_pid: i32,
    pub(crate) interactive: bool,
    pub(crate) loops: u32,
    pub(crate) func_depth: u32,
    pub(crate) locals: Vec<Vec<(String, Option<Var>)>>,
    /// Inside a condition: `set -e` does not apply.
    pub(crate) cond: u32,
    pub(crate) lineno: u32,
    pub(crate) dot_depth: u32,
    pub(crate) getopts_pos: usize,
    pub(crate) start: Duration,
    pub(crate) random: u32,
    pub(crate) cmdsub_status: Option<i32>,
    pub(crate) in_trap: bool,
    pub(crate) eval_depth: u32,
    /// Dynamic variables the user has unset.
    pub(crate) dyn_off: HashSet<String>,
}

/// Deepest nesting of function calls, `eval` and `.`.
pub(crate) const MAX_NEST: u32 = 200;

impl Shell {
    /// A new shell over `sys` with descriptors 0, 1 and 2 as given. The
    /// environment becomes exported variables; Qshell's defaults fill
    /// `IFS`, `PATH`, `PS1`, `PS2`, `PS4`, `PWD`, `PPID`, `OPTIND` and
    /// `QSH_VERSION`.
    pub fn new(
        sys: Arc<dyn Sys>,
        stdin: Option<Fd>,
        stdout: Option<Fd>,
        stderr: Option<Fd>,
    ) -> Self {
        let mut vars = Vars::default();
        for (k, v) in sys.environ() {
            if crate::syntax::is_name(&k) {
                let e = vars.entry(&k);
                e.value = Some(v);
                e.exported = true;
            }
        }
        let mut fds = FdTable::default();
        for (n, fd) in [stdin, stdout, stderr].into_iter().enumerate() {
            if let Some(fd) = fd {
                fds.set(u32::try_from(n).unwrap_or(0), fd);
            }
        }
        let cwd = sys.initial_cwd();
        let pid = sys.pid();
        let now = sys.now();
        let mut sh = Self {
            vars,
            funcs: HashMap::new(),
            aliases: BTreeMap::new(),
            opts: Options::default(),
            arg0: "qsh".to_string(),
            args: Vec::new(),
            status: 0,
            last_bg: None,
            fds,
            cwd: cwd.clone(),
            umask: 0o022,
            traps: Traps::default(),
            jobs: Jobs::default(),
            hash: BTreeMap::new(),
            pid,
            self_pid: pid,
            interactive: false,
            loops: 0,
            func_depth: 0,
            locals: Vec::new(),
            cond: 0,
            lineno: 0,
            dot_depth: 0,
            getopts_pos: 0,
            start: now,
            random: (now.subsec_nanos() ^ pid.unsigned_abs()) | 1,
            cmdsub_status: None,
            in_trap: false,
            eval_depth: 0,
            dyn_off: HashSet::new(),
            sys,
        };
        sh.defaults(&cwd);
        sh
    }

    pub(crate) fn defaults(&mut self, cwd: &str) {
        let set_default = |vars: &mut Vars, k: &str, v: &str| {
            if vars.get(k).is_none() {
                let _ = vars.set(k, v);
            }
        };
        let _ = self.vars.set("IFS", " \t\n");
        set_default(&mut self.vars, "PATH", DEFAULT_PATH);
        set_default(&mut self.vars, "PS1", "$ ");
        set_default(&mut self.vars, "PS2", "> ");
        set_default(&mut self.vars, "PS4", "+ ");
        let _ = self.vars.set("OPTIND", "1");
        let _ = self.vars.set("PPID", &self.sys.ppid().to_string());
        let _ = self.vars.set("QSH_VERSION", &release::RELEASE.to_string());
        let _ = self.vars.set("PWD", cwd);
        self.vars.entry("PWD").exported = true;
        if self.vars.get("HOME").is_none()
            && let Some(user) = self.vars.get("LOGNAME").map(str::to_string)
            && let Some(h) = self.sys.home_of(&user)
        {
            let _ = self.vars.set("HOME", &h);
            self.vars.entry("HOME").exported = true;
        }
    }

    /// The system.
    pub fn sys(&self) -> &Arc<dyn Sys> {
        &self.sys
    }

    /// `$?`.
    pub fn status(&self) -> i32 {
        self.status
    }

    /// Set `$0` and the positional parameters.
    pub fn set_args(&mut self, arg0: &str, args: Vec<String>) {
        self.arg0 = arg0.to_string();
        self.args = args;
    }

    /// Mark the shell interactive (prompts, error recovery, `-i`).
    pub fn set_interactive(&mut self, on: bool) {
        self.interactive = on;
        self.opts.set(Opt::Interactive, on);
    }

    /// Turn an option on or off.
    pub fn set_option(&mut self, o: Opt, on: bool) {
        self.opts.set(o, on);
    }

    /// Is an option on?
    pub fn option(&self, o: Opt) -> bool {
        self.opts.get(o)
    }

    /// The working directory.
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// A descriptor of the shell.
    pub fn fd(&self, n: u32) -> Option<Fd> {
        self.fds.get(n)
    }

    /// Point a descriptor of the shell at a stream.
    pub fn set_fd(&mut self, n: u32, fd: Fd) {
        self.fds.set(n, fd);
    }

    /// A variable's value, dynamic variables included.
    pub fn var(&mut self, name: &str) -> Option<String> {
        if !self.dyn_off.contains(name) {
            match name {
                "RANDOM" => {
                    self.random = self.random.wrapping_mul(1_103_515_245).wrapping_add(12345);
                    return Some(((self.random >> 16) & 0x7fff).to_string());
                }
                "SECONDS" => {
                    let base: u64 = self
                        .vars
                        .get("SECONDS")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0);
                    let el = self.sys.now().saturating_sub(self.start).as_secs();
                    return Some((base + el).to_string());
                }
                "LINENO" => return Some(self.lineno.to_string()),
                _ => {}
            }
        }
        self.vars.get(name).map(str::to_string)
    }

    /// Assign a variable as an assignment word does: `integer` variables
    /// evaluate arithmetic, `set -a` exports.
    pub(crate) fn assign(&mut self, name: &str, value: &str) -> Result<(), Flow> {
        let value = if self.vars.var(name).is_some_and(|v| v.integer) {
            match crate::arith::eval(value, &mut exec::ArithVars(self)) {
                Ok(n) => n.to_string(),
                Err(e) => {
                    self.report(&Msg::Arith(e.to_string()));
                    return Err(Flow::Error(1));
                }
            }
        } else {
            value.to_string()
        };
        if let Err(ReadOnlyError(n)) = self.vars.set(name, &value) {
            self.report(&Msg::ReadOnly(n));
            return Err(Flow::Error(1));
        }
        if self.opts.get(Opt::AllExport) {
            self.vars.entry(name).exported = true;
        }
        match name {
            "OPTIND" => self.getopts_pos = 0,
            "RANDOM" => {
                self.random = value.parse().unwrap_or(1);
            }
            "SECONDS" => self.start = self.sys.now(),
            "PATH" => self.hash.clear(),
            _ => {}
        }
        Ok(())
    }

    /// Write a diagnostic to standard error.
    pub(crate) fn report(&mut self, m: &Msg) {
        let line = msg::line(m);
        if let Some(fd) = self.fds.get(2) {
            let _ = write_all(&*fd, line.as_bytes());
        }
    }

    /// Write bytes to a descriptor. A pipe with no reader ends the
    /// (sub)shell as `SIGPIPE` would; other errors fail the builtin.
    pub(crate) fn write_fd(&mut self, n: u32, data: &[u8]) -> Result<(), Flow> {
        let Some(fd) = self.fds.get(n) else {
            return Err(Flow::Fail(1));
        };
        match write_all(&*fd, data) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                Err(Flow::Exit(crate::signal::status(crate::signal::SIGPIPE)))
            }
            Err(_) => Err(Flow::Fail(1)),
        }
    }

    /// Write text to standard output.
    pub(crate) fn out(&mut self, s: &str) -> Result<(), Flow> {
        self.write_fd(1, s.as_bytes())
    }

    /// Write text to standard error, ignoring failures.
    pub(crate) fn err(&mut self, s: &str) {
        if let Some(fd) = self.fds.get(2) {
            let _ = write_all(&*fd, s.as_bytes());
        }
    }

    /// A copy of this shell for a subshell: same variables, functions,
    /// descriptors; no jobs; trap commands reset.
    pub(crate) fn subshell(&self) -> Self {
        Self {
            sys: Arc::clone(&self.sys),
            vars: self.vars.clone(),
            funcs: self.funcs.clone(),
            aliases: self.aliases.clone(),
            opts: self.opts.clone(),
            arg0: self.arg0.clone(),
            args: self.args.clone(),
            status: self.status,
            last_bg: self.last_bg,
            fds: self.fds.clone(),
            cwd: self.cwd.clone(),
            umask: self.umask,
            traps: self.traps.for_subshell(),
            jobs: Jobs::default(),
            hash: self.hash.clone(),
            pid: self.pid,
            self_pid: self.self_pid,
            interactive: false,
            loops: self.loops,
            func_depth: self.func_depth,
            locals: Vec::new(),
            cond: self.cond,
            lineno: self.lineno,
            dot_depth: self.dot_depth,
            getopts_pos: self.getopts_pos,
            start: self.start,
            random: self.random.wrapping_add(7919),
            cmdsub_status: None,
            in_trap: false,
            eval_depth: self.eval_depth,
            dyn_off: self.dyn_off.clone(),
        }
    }

    /// Parse and run source text one complete command at a time, so an
    /// alias defined by one command applies to the next. A syntax error
    /// is reported and is a shell error.
    pub fn run_text(&mut self, src: &str, first_line: u32) -> Res {
        let aliases = self.aliases.clone();
        let mut parser = Parser::at_line(src, first_line).with_aliases(&aliases);
        let mut status = 0;
        let seen = self.aliases.clone();
        let mut rest_len = src.chars().count();
        loop {
            // Aliases changed by the last command apply from here on.
            if seen != self.aliases {
                let rest = parser.remaining();
                let line = parser.line();
                return self.continue_text(&rest, line, status);
            }
            let list = match parser.next_command() {
                Ok(Some(l)) => l,
                Ok(None) => break,
                Err(e) => {
                    self.report(&Msg::Syntax(e.line, e.kind.to_string()));
                    self.status = 2;
                    return Err(Flow::Error(2));
                }
            };
            let now = parser.remaining().chars().count();
            if self.opts.get(Opt::Verbose) || self.opts.get(Opt::Trace) {
                let total = src.chars().count();
                let text: String = src
                    .chars()
                    .skip(total.saturating_sub(rest_len))
                    .take(rest_len.saturating_sub(now))
                    .collect();
                if self.opts.get(Opt::Verbose) {
                    self.err(&text);
                }
                if self.opts.get(Opt::Trace) {
                    self.trace_file(&text);
                }
            }
            rest_len = now;
            if self.opts.get(Opt::NoExec) && !self.interactive {
                continue;
            }
            status = self.run_list(&list)?;
        }
        Ok(status)
    }

    /// `set -o trace`: append command text to `$TRACEFILE` (default
    /// `$HOME/qsh_trace`).
    fn trace_file(&mut self, text: &str) {
        let path = match self.var("TRACEFILE") {
            Some(p) if !p.is_empty() => p,
            _ => crate::pattern::path::join(
                &self.var("HOME").unwrap_or_else(|| "/".to_string()),
                "qsh_trace",
            ),
        };
        let abs = crate::pattern::path::absolute(&self.cwd, &path);
        let flags = crate::sys::OpenFlags {
            write: true,
            create: true,
            append: true,
            ..Default::default()
        };
        if let Ok(fd) = self.sys.open(&abs, flags, 0o600 & !self.umask) {
            let _ = write_all(&*fd, text.as_bytes());
        }
    }

    fn continue_text(&mut self, rest: &str, line: u32, status: i32) -> Res {
        let r = self.run_text(rest, line)?;
        if rest.trim().is_empty() {
            Ok(status)
        } else {
            Ok(r)
        }
    }
}
