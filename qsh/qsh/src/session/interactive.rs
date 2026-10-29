// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The interactive session, fed a line at a time by the 5250 QSH
//! terminal or an SSH TTY.

use crate::msg::Msg;
use crate::shell::jobs::{JobState, state_text};
use crate::shell::options::Opt;
use crate::shell::{Flow, Shell};
use crate::syntax::Parser;

/// What the session wants after a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feed {
    /// Ready for a new command: show `PS1`.
    Ready,
    /// The command continues: show `PS2`.
    More,
    /// The shell exited with this status (its `EXIT` trap has run).
    Exit(i32),
}

/// An interactive shell and its partial input.
pub struct Interactive {
    sh: Shell,
    pending: String,
    line: u32,
}

impl Interactive {
    /// Wrap a shell (built with [`super::shell_for`]) as an interactive
    /// session.
    pub fn new(mut sh: Shell) -> Self {
        sh.set_interactive(true);
        Self {
            sh,
            pending: String::new(),
            line: 1,
        }
    }

    /// The shell.
    pub fn shell(&mut self) -> &mut Shell {
        &mut self.sh
    }

    /// The prompt to show now: `PS1`, or `PS2` inside a command, with
    /// parameters expanded.
    pub fn prompt(&mut self) -> String {
        let name = if self.pending.is_empty() {
            "PS1"
        } else {
            "PS2"
        };
        let p = self.sh.var(name).unwrap_or_default();
        self.sh.expand_prompt(&p)
    }

    /// Feed one line (without its newline).
    pub fn feed_line(&mut self, line: &str) -> Feed {
        self.pending.push_str(line);
        self.pending.push('\n');
        if self.sh.option(Opt::Verbose) {
            self.sh.err(&format!("{line}\n"));
        }
        // An odd number of trailing backslashes continues the line.
        if line.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1 {
            return Feed::More;
        }
        loop {
            let aliases = self.sh.aliases.clone();
            let src = std::mem::take(&mut self.pending);
            let mut parser = Parser::at_line(&src, self.line)
                .with_aliases(&aliases)
                .partial();
            match parser.next_command() {
                Ok(None) => {
                    self.line = parser.line();
                    return Feed::Ready;
                }
                Ok(Some(list)) => {
                    self.pending = parser.remaining();
                    self.line = parser.line();
                    match self.sh.run_list(&list) {
                        Ok(_) => {}
                        Err(Flow::Exit(n)) => {
                            self.sh.status = n;
                            return Feed::Exit(self.sh.run_exit_trap());
                        }
                        Err(Flow::Error(n)) => {
                            self.sh.status = n;
                            self.pending.clear();
                            return Feed::Ready;
                        }
                        Err(Flow::Fail(n) | Flow::Return(n)) => self.sh.status = n,
                        Err(Flow::Break(_) | Flow::Continue(_)) => {}
                    }
                    if self.pending.trim().is_empty() {
                        self.pending.clear();
                        return Feed::Ready;
                    }
                }
                Err(e) if e.is_incomplete() => {
                    self.pending = src;
                    return Feed::More;
                }
                Err(e) => {
                    self.sh.report(&Msg::Syntax(e.line, e.kind.to_string()));
                    self.sh.status = 2;
                    self.line = parser.line();
                    return Feed::Ready;
                }
            }
        }
    }

    /// End of input (the user closed the session or pressed the end key).
    pub fn feed_eof(&mut self) -> Feed {
        if self.sh.option(Opt::IgnoreEof) {
            self.sh.err("qsh: use exit to leave the shell.\n");
            return Feed::Ready;
        }
        self.pending.clear();
        Feed::Exit(self.sh.run_exit_trap())
    }

    /// Lines about background jobs that ended since the last call, for
    /// the session to show before the prompt.
    pub fn notices(&mut self) -> String {
        self.sh.jobs.poll();
        let mut out = String::new();
        let cur = self.sh.jobs.current();
        let prev = self.sh.jobs.previous();
        for j in self.sh.jobs.iter() {
            if matches!(j.state, JobState::Done(_)) {
                let mark = if Some(j.id) == cur {
                    '+'
                } else if Some(j.id) == prev {
                    '-'
                } else {
                    ' '
                };
                out.push_str(&format!(
                    "[{}] {mark} {} {}\n",
                    j.id,
                    state_text(j.state),
                    j.text
                ));
            }
        }
        self.sh.jobs.reap();
        out
    }
}
