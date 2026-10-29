// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Compound commands and function definitions.

use std::sync::Arc;

use crate::pattern;
use crate::shell::{Flow, Res, Shell};
use crate::syntax::{CaseArm, Command, Compound, List, Word};

/// What a loop does after its body.
enum Next {
    Go(i32),
    Stop,
}

impl Shell {
    /// Run one command of a pipeline.
    pub(crate) fn run_command(&mut self, cmd: &Command) -> Res {
        match cmd {
            Command::Simple(s) => self.run_simple(s),
            Command::Compound(c, redirs) => {
                let saved = match self.redirect(redirs) {
                    Ok(s) => s,
                    Err(Flow::Fail(n)) => return Ok(n),
                    Err(e) => return Err(e),
                };
                let r = self.run_compound(c);
                self.fds.restore(saved);
                r
            }
            Command::Function(f) => {
                self.funcs.insert(f.name.clone(), Arc::clone(f));
                Ok(0)
            }
        }
    }

    /// Run a compound command.
    pub(crate) fn run_compound(&mut self, c: &Compound) -> Res {
        match c {
            Compound::Brace(l) => self.run_list(l),
            Compound::Subshell(l) => {
                let mut sub = self.subshell();
                let r = sub.run_list(l);
                Ok(sub.finish_child(r))
            }
            Compound::If { arms, else_body } => {
                for (cond, body) in arms {
                    if self.run_cond(cond)? == 0 {
                        return self.run_list(body);
                    }
                }
                match else_body {
                    Some(b) => self.run_list(b),
                    None => Ok(0),
                }
            }
            Compound::Loop { until, cond, body } => self.run_loop(*until, cond, body),
            Compound::For { var, words, body } => self.run_for(var, words.as_deref(), body),
            Compound::Case { subject, arms } => self.run_case(subject, arms),
        }
    }

    fn run_cond(&mut self, l: &List) -> Res {
        self.cond += 1;
        let r = self.run_list(l);
        self.cond -= 1;
        r
    }

    /// Fold a loop body's outcome into what the loop does next.
    fn loop_step(r: Res) -> Result<Next, Flow> {
        match r {
            Ok(s) => Ok(Next::Go(s)),
            Err(Flow::Break(n)) if n > 1 => Err(Flow::Break(n - 1)),
            Err(Flow::Break(_)) => Ok(Next::Stop),
            Err(Flow::Continue(n)) if n > 1 => Err(Flow::Continue(n - 1)),
            Err(Flow::Continue(_)) => Ok(Next::Go(0)),
            Err(e) => Err(e),
        }
    }

    fn run_loop(&mut self, until: bool, cond: &List, body: &List) -> Res {
        self.loops += 1;
        let r = (|| {
            let mut st = 0;
            loop {
                let c = self.run_cond(cond);
                match Self::loop_step(c)? {
                    Next::Stop => return Ok(st),
                    Next::Go(c) => {
                        if (c == 0) == until {
                            return Ok(st);
                        }
                    }
                }
                let b = self.run_list(body);
                match Self::loop_step(b)? {
                    Next::Go(s) => st = s,
                    Next::Stop => return Ok(st),
                }
            }
        })();
        self.loops -= 1;
        r
    }

    fn run_for(&mut self, var: &str, words: Option<&[Word]>, body: &List) -> Res {
        let values = match words {
            Some(w) => self.expand_words(w)?,
            None => self.args.clone(),
        };
        self.loops += 1;
        let r = (|| {
            let mut st = 0;
            for v in values {
                self.assign(var, &v)?;
                let b = self.run_list(body);
                match Self::loop_step(b)? {
                    Next::Go(s) => st = s,
                    Next::Stop => break,
                }
            }
            Ok(st)
        })();
        self.loops -= 1;
        r
    }

    fn run_case(&mut self, subject: &Word, arms: &[CaseArm]) -> Res {
        let s = self.expand_string(subject)?;
        for arm in arms {
            for p in &arm.patterns {
                let pat = self.expand_pattern(p)?;
                if pattern::matches(&pat, &s, false) {
                    return self.run_list(&arm.body);
                }
            }
        }
        Ok(0)
    }
}
