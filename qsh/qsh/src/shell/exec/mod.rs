// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running lists, and-or lists and pipelines; traps between commands.

mod compound;
mod redirect;
mod simple;

use std::sync::Arc;
use std::sync::mpsc;

pub use simple::Found;

use super::options::Opt;
use super::traps::Action;
use super::{Flow, Res, Shell};
use crate::arith::ArithEnv;
use crate::msg::Msg;
use crate::signal;
use crate::syntax::{AndOr, Command, Connector, Item, List, Pipeline};

/// Variables seen by arithmetic.
pub(crate) struct ArithVars<'a>(pub &'a mut Shell);

impl ArithEnv for ArithVars<'_> {
    fn get(&mut self, name: &str) -> Option<String> {
        self.0.var(name)
    }

    fn set(&mut self, name: &str, value: i64) -> Result<(), String> {
        if self.0.vars.set(name, &value.to_string()).is_err() {
            return Err(format!("variable {name} is read-only"));
        }
        if self.0.opts.get(Opt::AllExport) {
            self.0.vars.entry(name).exported = true;
        }
        Ok(())
    }
}

/// Signals whose default action is to end the process.
fn default_ends(sig: i32) -> bool {
    !matches!(
        sig,
        signal::SIGCHLD | signal::SIGCONT | signal::SIGSTOP | signal::SIGTSTP | 23 | 27 | 21 | 22
    )
}

impl Shell {
    /// Run a compound list.
    pub fn run_list(&mut self, list: &List) -> Res {
        let mut st = 0;
        for item in &list.items {
            if self.opts.get(Opt::NoExec) && !self.interactive {
                return Ok(st);
            }
            st = if item.background {
                self.run_async(item)?
            } else {
                self.run_and_or(&item.and_or)?
            };
            self.check_traps()?;
        }
        Ok(st)
    }

    fn run_and_or(&mut self, ao: &AndOr) -> Res {
        let n = ao.rest.len();
        let mut st = self.run_checked(&ao.first, n == 0)?;
        for (i, (conn, p)) in ao.rest.iter().enumerate() {
            let go = match conn {
                Connector::And => st == 0,
                Connector::Or => st != 0,
            };
            if go {
                st = self.run_checked(p, i + 1 == n)?;
            }
        }
        Ok(st)
    }

    /// Run a pipeline; `last` is false for pipelines whose failure
    /// `set -e` ignores (all but the last of an and-or list).
    fn run_checked(&mut self, p: &Pipeline, last: bool) -> Res {
        let quiet = !last || p.negate;
        if quiet {
            self.cond += 1;
        }
        let r = self.run_pipeline(p);
        if quiet {
            self.cond -= 1;
        }
        let st = r?;
        self.status = st;
        if !quiet && st != 0 && self.cond == 0 && self.opts.get(Opt::ErrExit) {
            return Err(Flow::Exit(st));
        }
        Ok(st)
    }

    fn run_pipeline(&mut self, p: &Pipeline) -> Res {
        let st = if p.commands.len() == 1 {
            self.run_command(&p.commands[0])?
        } else {
            self.run_multi(&p.commands)?
        };
        Ok(if p.negate { i32::from(st == 0) } else { st })
    }

    /// A pipeline of several commands: each runs in a subshell started
    /// through `Sys::fork`, joined by pipes.
    fn run_multi(&mut self, cmds: &[Command]) -> Res {
        let mut children = Vec::new();
        let mut prev = None;
        for (i, cmd) in cmds.iter().enumerate() {
            let last = i + 1 == cmds.len();
            let mut next = None;
            let mut sub = self.subshell();
            if let Some(r) = prev.take() {
                sub.fds.set(0, r);
            }
            if !last {
                match self.sys.pipe() {
                    Ok((r, w)) => {
                        sub.fds.set(1, w);
                        next = Some(r);
                    }
                    Err(e) => {
                        self.report(&Msg::Failed("pipe".to_string(), e.to_string()));
                        break;
                    }
                }
            }
            let cmd = cmd.clone();
            match self.fork_with(sub, move |s| s.run_command(&cmd)) {
                Ok(c) => children.push(c),
                Err(e) => {
                    self.report(&Msg::Failed("fork".to_string(), e.to_string()));
                    break;
                }
            }
            prev = next;
        }
        drop(prev);
        let mut st = 1;
        for mut c in children {
            st = c.wait().map_or(1, crate::sys::WaitStatus::code);
        }
        Ok(st)
    }

    /// Start `sub` running `f` through `Sys::fork`; the subshell learns
    /// its own process ID before it starts.
    fn fork_with<F>(
        &self,
        mut sub: Shell,
        f: F,
    ) -> Result<Box<dyn crate::sys::Child>, crate::sys::SysError>
    where
        F: FnOnce(&mut Shell) -> Res + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<i32>();
        let child = self.sys.fork(
            "qsh",
            Box::new(move || {
                if let Ok(pid) = rx.recv() {
                    sub.self_pid = pid;
                }
                let r = f(&mut sub);
                sub.finish_child(r)
            }),
        )?;
        let _ = tx.send(child.pid());
        Ok(child)
    }

    fn run_async(&mut self, item: &Item) -> Res {
        let mut sub = self.subshell();
        if !self.opts.get(Opt::Monitor)
            && let Ok(null) = self.sys.open(
                "/dev/null",
                crate::sys::OpenFlags {
                    read: true,
                    ..Default::default()
                },
                0,
            )
        {
            sub.fds.set(0, null);
        }
        let ao = Arc::new(item.and_or.clone());
        match self.fork_with(sub, move |s| s.run_and_or(&ao)) {
            Ok(child) => {
                let pid = child.pid();
                self.last_bg = Some(pid);
                let id = self.jobs.add(child, item.text.clone());
                if self.interactive && self.opts.get(Opt::Monitor) {
                    self.err(&format!("[{id}] {pid}\n"));
                }
                Ok(0)
            }
            Err(e) => {
                self.report(&Msg::Failed("fork".to_string(), e.to_string()));
                Ok(1)
            }
        }
    }

    /// The status a subshell ends with, its `EXIT` trap run.
    pub(crate) fn finish_child(&mut self, r: Res) -> i32 {
        let st = match r {
            Ok(n) | Err(Flow::Exit(n) | Flow::Error(n) | Flow::Fail(n) | Flow::Return(n)) => n,
            Err(Flow::Break(_) | Flow::Continue(_)) => self.status,
        };
        self.status = st;
        self.run_exit_trap()
    }

    /// Run the `EXIT` trap once; the final status.
    pub fn run_exit_trap(&mut self) -> i32 {
        if let Some(Action::Run(cmd)) = self.traps.get(0).cloned() {
            self.traps.set(0, None);
            let saved = self.status;
            self.in_trap = true;
            let r = self.run_text(&cmd, self.lineno);
            self.in_trap = false;
            match r {
                Err(Flow::Exit(n)) => self.status = n,
                _ => self.status = saved,
            }
        }
        self.status
    }

    /// Run the traps of signals that arrived; a signal without a trap
    /// ends the shell as its default action would.
    pub(crate) fn check_traps(&mut self) -> Result<(), Flow> {
        if self.in_trap {
            return Ok(());
        }
        for sig in self.sys.take_signals(self.self_pid) {
            match self.traps.get(sig).cloned() {
                Some(Action::Run(cmd)) => {
                    let saved = self.status;
                    self.in_trap = true;
                    let r = self.run_text(&cmd, self.lineno);
                    self.in_trap = false;
                    match r {
                        Err(Flow::Exit(n)) => return Err(Flow::Exit(n)),
                        _ => self.status = saved,
                    }
                }
                Some(Action::Ignore) => {}
                None => {
                    let shielded = self.interactive
                        && matches!(sig, signal::SIGINT | signal::SIGTERM | signal::SIGQUIT);
                    if default_ends(sig) && !shielded {
                        return Err(Flow::Exit(signal::status(sig)));
                    }
                }
            }
        }
        Ok(())
    }
}
