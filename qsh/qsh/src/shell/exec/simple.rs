// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Simple commands: expansion, lookup (special builtins, functions,
//! builtins, `PATH`), temporary assignments and running.

use std::sync::Arc;

use crate::builtins::{self, Builtin};
use crate::msg::Msg;
use crate::pattern::path::{absolute, join};
use crate::shell::jobs::{JobState, state_text};
use crate::shell::options::Opt;
use crate::shell::vars::Var;
use crate::shell::{Flow, MAX_NEST, Res, Shell};
use crate::syntax::{FunctionDef, SimpleCommand};
use crate::sys::{Access, Errno, FileType, SpawnRequest, WaitStatus, stream::read_to_end};

/// What a command name resolves to.
#[derive(Clone)]
pub enum Found {
    /// A special builtin.
    Special(Builtin),
    /// A function.
    Function(Arc<FunctionDef>),
    /// A regular builtin.
    Builtin(Builtin),
    /// A program at a path.
    External(String),
    /// Nothing.
    NotFound,
}

impl Shell {
    pub(crate) fn run_simple(&mut self, cmd: &SimpleCommand) -> Res {
        self.lineno = cmd.span.line;
        self.cmdsub_status = None;
        let argv = self.expand_words(&cmd.words)?;
        if argv.is_empty() {
            // Each assignment sees the ones before it.
            let mut done = Vec::with_capacity(cmd.assigns.len());
            for a in &cmd.assigns {
                let v = self.expand_assign_value(&a.value)?;
                self.assign(&a.name, &v)?;
                done.push((a.name.clone(), v));
            }
            if self.opts.get(Opt::XTrace) && !done.is_empty() {
                self.trace(&done, &[]);
            }
            let saved = match self.redirect(&cmd.redirs) {
                Ok(s) => s,
                Err(Flow::Fail(n)) => return Ok(n),
                Err(e) => return Err(e),
            };
            self.fds.restore(saved);
            return Ok(self.cmdsub_status.unwrap_or(0));
        }
        let mut assigns = Vec::with_capacity(cmd.assigns.len());
        for a in &cmd.assigns {
            let v = self.expand_assign_value(&a.value)?;
            assigns.push((a.name.clone(), v));
        }
        if self.opts.get(Opt::XTrace) {
            self.trace(&assigns, &argv);
        }
        let found = self.lookup(&argv[0], false);
        let saved = match self.redirect(&cmd.redirs) {
            Ok(s) => s,
            Err(Flow::Fail(n)) => {
                return if matches!(found, Found::Special(_)) && !self.interactive {
                    Err(Flow::Error(n))
                } else {
                    Ok(n)
                };
            }
            Err(e) => return Err(e),
        };
        let keep_redirs = argv.len() == 1 && argv[0] == "exec";
        let r = self.run_found(found, &argv, &assigns);
        if keep_redirs {
            drop(saved);
        } else {
            self.fds.restore(saved);
        }
        r
    }

    /// Write the `set -x` line.
    fn trace(&mut self, assigns: &[(String, String)], argv: &[String]) {
        let ps4 = self.var("PS4").unwrap_or_default();
        let mut line = self.expand_prompt(&ps4);
        let mut words: Vec<String> = assigns.iter().map(|(k, v)| format!("{k}={v}")).collect();
        words.extend(argv.iter().cloned());
        line.push_str(&words.join(" "));
        line.push('\n');
        self.err(&line);
    }

    /// Run a resolved command.
    pub(crate) fn run_found(
        &mut self,
        found: Found,
        argv: &[String],
        assigns: &[(String, String)],
    ) -> Res {
        match found {
            Found::Special(b) => {
                for (k, v) in assigns {
                    self.assign(k, v)?;
                }
                self.call_builtin(b, argv)
            }
            Found::Builtin(b) => {
                let saved = self.temp_assign(assigns)?;
                let r = self.call_builtin(b, argv);
                self.untemp(saved);
                r
            }
            Found::Function(f) => {
                let saved = self.temp_assign(assigns)?;
                let r = self.call_function(&f, argv);
                self.untemp(saved);
                r
            }
            Found::External(path) => self.run_external(&path, argv, assigns),
            Found::NotFound => {
                self.report(&Msg::NotFound(argv[0].clone()));
                Ok(127)
            }
        }
    }

    pub(crate) fn call_builtin(&mut self, b: Builtin, argv: &[String]) -> Res {
        match b(self, argv) {
            Err(Flow::Fail(n)) => Ok(n),
            r => r,
        }
    }

    fn temp_assign(
        &mut self,
        assigns: &[(String, String)],
    ) -> Result<Vec<(String, Option<Var>)>, Flow> {
        let mut saved = Vec::new();
        for (k, v) in assigns {
            saved.push((k.clone(), self.vars.var(k).cloned()));
            if let Err(e) = self.assign(k, v) {
                self.untemp(saved);
                return Err(e);
            }
            self.vars.entry(k).exported = true;
        }
        Ok(saved)
    }

    fn untemp(&mut self, saved: Vec<(String, Option<Var>)>) {
        for (k, v) in saved.into_iter().rev() {
            self.vars.restore(&k, v);
        }
    }

    /// Resolve a command name. `no_functions` skips functions (the
    /// `command` builtin).
    pub(crate) fn lookup(&mut self, name: &str, no_functions: bool) -> Found {
        if name.contains('/') {
            return Found::External(name.to_string());
        }
        if let Some(b) = builtins::special(name) {
            return Found::Special(b);
        }
        if !no_functions && let Some(f) = self.funcs.get(name) {
            return Found::Function(Arc::clone(f));
        }
        if let Some(b) = builtins::regular(name) {
            return Found::Builtin(b);
        }
        match self.search_path(name) {
            Some(p) => Found::External(p),
            None => Found::NotFound,
        }
    }

    /// Search `PATH` for an executable file.
    pub(crate) fn search_path(&mut self, name: &str) -> Option<String> {
        if let Some(p) = self.hash.get(name).cloned() {
            if self.is_executable(&p) {
                return Some(p);
            }
            self.hash.remove(name);
        }
        let path = self.var("PATH").unwrap_or_default();
        for dir in path.split(':') {
            let dir = if dir.is_empty() { "." } else { dir };
            let cand = join(dir, name);
            if self.is_executable(&cand) {
                self.hash.insert(name.to_string(), cand.clone());
                return Some(cand);
            }
        }
        None
    }

    fn is_executable(&self, p: &str) -> bool {
        let abs = absolute(&self.cwd, p);
        matches!(self.sys.stat(&abs), Ok(st) if st.ftype != FileType::Dir)
            && self.sys.access(&abs, Access::Exec).is_ok()
    }

    pub(crate) fn call_function(&mut self, f: &Arc<FunctionDef>, argv: &[String]) -> Res {
        if self.func_depth >= MAX_NEST {
            self.report(&Msg::Failed(
                f.name.clone(),
                "functions are nested too deeply.".to_string(),
            ));
            return Err(Flow::Error(2));
        }
        let saved_args = std::mem::replace(&mut self.args, argv[1..].to_vec());
        let saved_loops = std::mem::replace(&mut self.loops, 0);
        self.func_depth += 1;
        self.locals.push(Vec::new());
        let r = match self.redirect(&f.redirs) {
            Ok(saved) => {
                let r = self.run_compound(&f.body);
                self.fds.restore(saved);
                r
            }
            Err(Flow::Fail(n)) => Ok(n),
            Err(e) => Err(e),
        };
        if let Some(locals) = self.locals.pop() {
            for (name, old) in locals.into_iter().rev() {
                self.vars.restore(&name, old);
            }
        }
        self.func_depth -= 1;
        self.loops = saved_loops;
        self.args = saved_args;
        match r {
            Err(Flow::Return(n)) => Ok(n),
            Err(Flow::Break(_) | Flow::Continue(_)) => Ok(self.status),
            r => r,
        }
    }

    fn run_external(&mut self, path: &str, argv: &[String], assigns: &[(String, String)]) -> Res {
        let mut env = self.vars.environ();
        for (k, v) in assigns {
            env.retain(|(n, _)| n != k);
            env.push((k.clone(), v.clone()));
        }
        let abs = absolute(&self.cwd, path);
        let req = SpawnRequest {
            path: abs.clone(),
            argv: argv.to_vec(),
            env: env.clone(),
            cwd: self.cwd.clone(),
            umask: self.umask,
            fds: self.fds.snapshot(),
            background: false,
        };
        match self.sys.spawn(req) {
            Ok(mut child) => {
                let sys = Arc::clone(&self.sys);
                let jc = if self.interactive && self.opts.get(Opt::Monitor) {
                    sys.job_control()
                } else {
                    None
                };
                if let Some(jc) = jc {
                    let _ = jc.give_terminal(child.pid());
                }
                let st = child.wait();
                if let Some(jc) = jc {
                    let _ = jc.take_terminal();
                }
                match st {
                    Ok(WaitStatus::Stopped(sig)) => {
                        let text = argv.join(" ");
                        let id = self.jobs.add(child, text.clone());
                        if let Some(j) = self.jobs.get_mut(id) {
                            j.state = JobState::Stopped(sig);
                        }
                        self.err(&format!(
                            "[{id}] + {} {text}\n",
                            state_text(JobState::Stopped(sig))
                        ));
                        Ok(crate::signal::status(sig))
                    }
                    Ok(w) => Ok(w.code()),
                    Err(_) => Ok(1),
                }
            }
            Err(e) if e.errno == Errno::ENOEXEC => Ok(self.run_script_file(&abs, argv, env)),
            Err(e) if e.errno == Errno::ENOENT => {
                self.report(&Msg::NotFound(argv[0].clone()));
                Ok(127)
            }
            Err(e) => {
                self.report(&Msg::CannotRun(argv[0].clone(), e.to_string()));
                Ok(126)
            }
        }
    }

    /// Run a file that is not a program as a script in a new shell
    /// environment: exported variables only, no functions or aliases.
    fn run_script_file(&mut self, path: &str, argv: &[String], env: Vec<(String, String)>) -> i32 {
        let text = match self.read_file(path) {
            Ok(t) => t,
            Err(e) => {
                self.report(&Msg::CannotRun(argv[0].clone(), e.to_string()));
                return 126;
            }
        };
        let mut sub = self.subshell();
        sub.vars = crate::shell::vars::Vars::default();
        for (k, v) in env {
            let e = sub.vars.entry(&k);
            e.value = Some(v);
            e.exported = true;
        }
        let cwd = sub.cwd.clone();
        sub.defaults(&cwd);
        sub.getopts_pos = 0;
        sub.funcs.clear();
        sub.aliases.clear();
        sub.opts = crate::shell::options::Options::default();
        sub.set_args(&argv[0], argv[1..].to_vec());
        let r = sub.run_text(&text, 1);
        sub.finish_child(r)
    }

    /// Read a whole file as text.
    pub(crate) fn read_file(&self, path: &str) -> Result<String, crate::sys::SysError> {
        let abs = absolute(&self.cwd, path);
        let fd = self.sys.open(
            &abs,
            crate::sys::OpenFlags {
                read: true,
                ..Default::default()
            },
            0,
        )?;
        let bytes = read_to_end(&*fd)?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}
