// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The debug session: the module list, the view, breakpoints, watches
//! and equates, and what each command line or key does to them. What
//! the session cannot do itself (resume the program, evaluate a
//! variable, show another display) comes back as an [`Effect`] for
//! the job's driver.

use crate::DbgError;
use crate::bkpt::{Breakpoint, Breakpoints, StmtMap, Watch, Watches};
use crate::command::{
    self, Amount, ClearWhat, Command, DisplayWhat, Equates, Format, Place, SetOption,
};
use crate::view::SourceView;

/// A module the session can show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    /// The program, `LIB/NAME`.
    pub program: String,
    /// The module.
    pub module: String,
    /// The program's type, `*PGM` or `*SRVPGM`.
    pub kind: String,
    /// The source lines; empty when not available.
    pub lines: Vec<String>,
    /// Statements by line.
    pub map: StmtMap,
}

impl Module {
    /// The program's library and name.
    #[must_use]
    pub fn lib_name(&self) -> (&str, &str) {
        self.program
            .split_once('/')
            .unwrap_or(("*LIBL", self.program.as_str()))
    }
}

/// Where the program stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopAt {
    /// The program, `LIB/NAME`.
    pub program: String,
    /// The procedure.
    pub procedure: String,
    /// The statement.
    pub stmt: u32,
    /// Why: a breakpoint, a step, a watch.
    pub reason: StopReason,
}

/// Why the program stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// A breakpoint.
    Breakpoint,
    /// A step ended.
    Step,
    /// A watched variable changed.
    Watch(usize),
}

/// What the driver must do after a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Nothing more; show the view again.
    None,
    /// A message on the message line.
    Message(String),
    /// Resume the program: `count` statements, into calls, or to the
    /// next breakpoint when `step` is false.
    Resume {
        /// Step rather than run.
        step: bool,
        /// Statements to step.
        count: usize,
        /// Into called programs.
        into: bool,
    },
    /// Evaluate (or assign) a variable of the stopped program.
    Eval {
        /// The variable.
        expr: String,
        /// The value assigned.
        assign: Option<String>,
        /// The format.
        format: Format,
        /// The length.
        len: Option<usize>,
    },
    /// A variable's attributes.
    Attr(String),
    /// Show another module (load its source first).
    ShowModule(String),
    /// The help display.
    Help,
    /// Work with Debugged Threads.
    Threads,
    /// Work with Watch.
    WatchList,
    /// `UPDPROD` changed.
    UpdProd(bool),
}

/// The session.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// The modules of the programs under debug.
    pub modules: Vec<Module>,
    /// The module shown.
    pub shown: usize,
    /// Its view; `None` until a module is shown.
    pub view: Option<SourceView>,
    /// Breakpoints.
    pub bkpts: Breakpoints,
    /// Watches.
    pub watches: Watches,
    /// `EQUATE` names.
    pub equates: Equates,
    /// Where the program stopped.
    pub stopped: Option<StopAt>,
    /// Production files may change.
    pub updprod: bool,
    /// OPM programs are debugged at their source.
    pub opmsrc: bool,
    /// The line `QUAL` set.
    pub qual: Option<usize>,
    /// The thread `TBREAK` uses.
    pub thread: u64,
    /// Lines a page shows and columns a row.
    pub page: (usize, usize),
}

impl Session {
    /// A session over `modules`, `page` lines by `width` columns of
    /// source.
    #[must_use]
    pub fn new(modules: Vec<Module>, page: usize, width: usize) -> Self {
        let mut s = Self {
            modules,
            page: (page, width),
            thread: 1,
            ..Self::default()
        };
        s.show(0);
        s
    }

    /// Show module `i`.
    pub fn show(&mut self, i: usize) {
        let Some(m) = self.modules.get(i) else {
            return;
        };
        self.shown = i;
        let mut v = SourceView::new(
            &m.program,
            &m.module,
            m.lines.clone(),
            self.page.0,
            self.page.1,
        );
        if let Some(old) = &self.view {
            v.match_case = old.match_case;
        }
        if let Some(st) = &self.stopped
            && st.program == m.program
            && let Some(l) = m.map.line_of(st.stmt)
        {
            v.stop_at(l);
        }
        self.view = Some(v);
    }

    /// The module shown.
    #[must_use]
    pub fn module(&self) -> Option<&Module> {
        self.modules.get(self.shown)
    }

    /// Show the module named, if the list has it.
    pub fn show_named(&mut self, name: &str) -> bool {
        match self
            .modules
            .iter()
            .position(|m| m.module.eq_ignore_ascii_case(name))
        {
            Some(i) => {
                self.show(i);
                true
            }
            None => false,
        }
    }

    /// The program stopped: show its module at the statement.
    pub fn stop(&mut self, at: StopAt) {
        let i = self
            .modules
            .iter()
            .position(|m| m.program == at.program)
            .unwrap_or(self.shown);
        self.stopped = Some(at);
        self.show(i);
    }

    /// The program runs again.
    pub fn resumed(&mut self) {
        self.stopped = None;
        if let Some(v) = self.view.as_mut() {
            v.current = None;
        }
    }

    /// The breakpoints as the debug core keeps them: program and
    /// statement.
    #[must_use]
    pub fn core_breakpoints(&self) -> Vec<(String, u32)> {
        self.bkpts
            .list
            .iter()
            .map(|b| (b.program.clone(), b.stmt))
            .collect()
    }

    /// The line a place names, from the cursor's line when it names
    /// none.
    fn line_of(at: &Place, cursor: Option<usize>) -> Result<usize, DbgError> {
        match at {
            Place::Cursor => cursor.ok_or(DbgError::Missing("BREAK")),
            Place::Line(n) | Place::ProcLine(_, n) => Ok(*n),
        }
    }

    /// Set or replace a breakpoint on a line of the shown module.
    pub fn add_break(
        &mut self,
        line: usize,
        when: Option<String>,
        thread: Option<u64>,
    ) -> Result<usize, DbgError> {
        let m = self.module().ok_or(DbgError::NoSource)?;
        let (line, stmt) = if m.lines.is_empty() {
            // No source view: the number is a statement's.
            (
                line,
                u32::try_from(line).map_err(|_| DbgError::NoLine(line))?,
            )
        } else {
            if line == 0 || line > m.lines.len() {
                return Err(DbgError::NoLine(line));
            }
            m.map.at_or_after(line).ok_or(DbgError::NoStatement(line))?
        };
        let b = Breakpoint {
            program: m.program.clone(),
            module: m.module.clone(),
            line,
            stmt,
            when,
            thread,
        };
        self.bkpts.set(b);
        Ok(line)
    }

    /// F6: a breakpoint on the line, or off when there is one.
    pub fn toggle(&mut self, line: usize) -> Result<String, DbgError> {
        let (p, md) = self
            .module()
            .map(|m| (m.program.clone(), m.module.clone()))
            .ok_or(DbgError::NoSource)?;
        if self.bkpts.clear(&p, &md, line) {
            return Ok(format!("Breakpoint removed from line {line}."));
        }
        let at = self.add_break(line, None, None)?;
        Ok(format!("Breakpoint set on line {at}."))
    }

    /// Run a debug command line; `cursor` is the source line the cursor
    /// is on, if any.
    pub fn line(&mut self, text: &str, cursor: Option<usize>) -> Result<Effect, DbgError> {
        let expanded = self.equates.expand(text);
        let cmd = command::parse(&expanded)?;
        self.apply(cmd, cursor)
    }

    /// Apply a parsed command.
    #[allow(clippy::too_many_lines)] // one arm per command
    pub fn apply(&mut self, cmd: Command, cursor: Option<usize>) -> Result<Effect, DbgError> {
        Ok(match cmd {
            Command::Break { at, when } => {
                let line = Self::line_of(&at, cursor)?;
                let set = self.add_break(line, when.clone(), None)?;
                Effect::Message(match when {
                    Some(_) => format!("Conditional breakpoint set on line {set}."),
                    None => format!("Breakpoint set on line {set}."),
                })
            }
            Command::TBreak { at, when } => {
                let line = Self::line_of(&at, cursor)?;
                let set = self.add_break(line, when, Some(self.thread))?;
                Effect::Message(format!(
                    "Thread breakpoint set on line {set} for thread {}.",
                    self.thread
                ))
            }
            Command::Clear(what) => self.clear(what, cursor)?,
            Command::Display(what) => match what {
                DisplayWhat::Breakpoints => Effect::Message(self.breakpoint_text()),
                DisplayWhat::Equates => Effect::Message(if self.equates.defs.is_empty() {
                    "No equates are defined.".into()
                } else {
                    self.equates
                        .defs
                        .iter()
                        .map(|(k, v)| format!("{k} = {v}"))
                        .collect::<Vec<_>>()
                        .join("; ")
                }),
                DisplayWhat::Watch => Effect::WatchList,
                DisplayWhat::Module(m) => Effect::ShowModule(m),
            },
            Command::Eval {
                expr,
                assign,
                format,
                len,
            } => {
                self.need_stop("EVAL")?;
                Effect::Eval {
                    expr,
                    assign,
                    format,
                    len,
                }
            }
            Command::Attr(v) => {
                self.need_stop("ATTR")?;
                Effect::Attr(v)
            }
            Command::Watch { expr, len } => {
                self.need_stop("WATCH")?;
                let st = self.stopped.clone().ok_or(DbgError::NotStopped("WATCH"))?;
                let module = self.module().map(|m| m.module.clone()).unwrap_or_default();
                let w = Watch {
                    n: 0,
                    expr: expr.clone(),
                    len: len.unwrap_or(0),
                    program: st.program,
                    module,
                    procedure: st.procedure,
                };
                match self.watches.add(w) {
                    Some(n) => Effect::Message(format!("Watch {n} set on {expr}.")),
                    None => Effect::Message(format!(
                        "{expr} is already watched, or no more watches fit."
                    )),
                }
            }
            Command::Step { count, into } => {
                self.need_stop("STEP")?;
                Effect::Resume {
                    step: true,
                    count,
                    into,
                }
            }
            Command::Qual(n) => {
                self.view_mut()?;
                self.qual = Some(n);
                Effect::Message(format!("Variables are now looked up at line {n}."))
            }
            Command::Find { what, back } => {
                let v = self.view_mut()?;
                if v.is_empty() {
                    return Err(DbgError::NoSource);
                }
                match v.find(&what, back) {
                    Some(l) => Effect::Message(format!("Found on line {l}.")),
                    None => Effect::Message(format!("{} was not found.", v.last_find)),
                }
            }
            Command::Up(a) => self.scroll(|v| v.up(a))?,
            Command::Down(a) => self.scroll(|v| v.down(a))?,
            Command::Left(a) => self.scroll(|v| v.left(a))?,
            Command::Right(a) => self.scroll(|v| v.right(a))?,
            Command::Top => self.scroll(SourceView::to_top)?,
            Command::Bottom => self.scroll(SourceView::to_bottom)?,
            Command::Next | Command::Previous => {
                let next = matches!(cmd, Command::Next);
                let (p, md) = self
                    .module()
                    .map(|m| (m.program.clone(), m.module.clone()))
                    .ok_or(DbgError::NoSource)?;
                let from = cursor
                    .or_else(|| self.view.as_ref().map(|v| v.top))
                    .unwrap_or(1);
                let to = if next {
                    self.bkpts.next_after(&p, &md, from)
                } else {
                    self.bkpts.prev_before(&p, &md, from)
                };
                match to {
                    Some(l) => {
                        self.view_mut()?.show_line(l);
                        Effect::None
                    }
                    None => Effect::Message("No breakpoint in that direction.".into()),
                }
            }
            Command::Help => Effect::Help,
            Command::Thread => Effect::Threads,
            Command::Set(o) => match o {
                SetOption::UpdProd(v) => {
                    self.updprod = v;
                    Effect::UpdProd(v)
                }
                SetOption::Case(m) => {
                    if let Some(v) = self.view.as_mut() {
                        v.match_case = m;
                    }
                    Effect::Message(if m {
                        "FIND now matches case.".into()
                    } else {
                        "FIND now ignores case.".into()
                    })
                }
                SetOption::OpmSrc(v) => {
                    self.opmsrc = v;
                    Effect::Message(format!(
                        "OPM source debug {}.",
                        if v { "on" } else { "off" }
                    ))
                }
            },
            Command::Equate { name, def } => {
                let gone = def.is_none();
                self.equates.set(&name, def);
                Effect::Message(if gone {
                    format!("{name} is no longer an equate.")
                } else {
                    format!("{name} is now an equate.")
                })
            }
        })
    }

    fn need_stop(&self, what: &'static str) -> Result<(), DbgError> {
        if self.stopped.is_none() {
            return Err(DbgError::NotStopped(what));
        }
        Ok(())
    }

    fn view_mut(&mut self) -> Result<&mut SourceView, DbgError> {
        self.view.as_mut().ok_or(DbgError::NoSource)
    }

    fn scroll(&mut self, f: impl FnOnce(&mut SourceView)) -> Result<Effect, DbgError> {
        f(self.view_mut()?);
        Ok(Effect::None)
    }

    fn clear(&mut self, what: ClearWhat, cursor: Option<usize>) -> Result<Effect, DbgError> {
        let (p, md) = self
            .module()
            .map(|m| (m.program.clone(), m.module.clone()))
            .ok_or(DbgError::NoSource)?;
        Ok(match what {
            ClearWhat::At(at) => {
                let line = Self::line_of(&at, cursor)?;
                let line = self
                    .module()
                    .and_then(|m| m.map.at_or_after(line))
                    .map_or(line, |(l, _)| l);
                if self.bkpts.clear(&p, &md, line) {
                    Effect::Message(format!("Breakpoint removed from line {line}."))
                } else {
                    Effect::Message(format!("No breakpoint on line {line}."))
                }
            }
            ClearWhat::Pgm => {
                let n = self.bkpts.clear_program(&p);
                Effect::Message(format!("{n} breakpoint(s) removed from the program."))
            }
            ClearWhat::Watch(n) => Effect::Message(if self.watches.remove(n) {
                format!("Watch {n} removed.")
            } else {
                format!("There is no watch {n}.")
            }),
            ClearWhat::WatchAll => {
                self.watches.clear();
                Effect::Message("Every watch removed.".into())
            }
        })
    }

    /// The shown module's breakpoints as one message.
    #[must_use]
    pub fn breakpoint_text(&self) -> String {
        let Some(m) = self.module() else {
            return "No breakpoints.".into();
        };
        let lines = self.bkpts.lines(&m.program, &m.module);
        if lines.is_empty() {
            return "No breakpoints are set in this module.".into();
        }
        let list: Vec<String> = lines.iter().map(ToString::to_string).collect();
        format!("Breakpoints on lines {}.", list.join(", "))
    }

    /// Page keys and shifts from the keyboard.
    pub fn key_scroll(&mut self, down: bool) {
        if let Some(v) = self.view.as_mut() {
            if down {
                v.down(Amount::Default);
            } else {
                v.up(Amount::Default);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Session {
        let lines: Vec<String> = (1..=30).map(|i| format!("stmt line {i}")).collect();
        let map = StmtMap::new((1..=30).step_by(2).zip(1..).collect());
        Session::new(
            vec![Module {
                program: "QGPL/P".into(),
                module: "P".into(),
                kind: "*PGM".into(),
                lines,
                map,
            }],
            15,
            70,
        )
    }

    #[test]
    fn breaks_move_to_a_statement() {
        let mut s = session();
        assert_eq!(
            s.line("break 4", None).unwrap(),
            Effect::Message("Breakpoint set on line 5.".into())
        );
        assert_eq!(s.core_breakpoints(), vec![("QGPL/P".to_string(), 3)]);
        assert_eq!(s.toggle(5).unwrap(), "Breakpoint removed from line 5.");
        assert!(s.core_breakpoints().is_empty());
        s.line("br", Some(7)).unwrap();
        s.line("br 9", None).unwrap();
        assert_eq!(s.breakpoint_text(), "Breakpoints on lines 7, 9.");
        s.line("clear 8", None).unwrap();
        assert_eq!(s.core_breakpoints().len(), 1);
        s.line("clear pgm", None).unwrap();
        assert!(s.core_breakpoints().is_empty());
        assert!(matches!(
            s.line("break 40", None),
            Err(DbgError::NoLine(40))
        ));
    }

    #[test]
    fn stopped_only() {
        let mut s = session();
        assert!(matches!(
            s.line("eval &x", None),
            Err(DbgError::NotStopped(_))
        ));
        assert!(matches!(s.line("step", None), Err(DbgError::NotStopped(_))));
        s.stop(StopAt {
            program: "QGPL/P".into(),
            procedure: "P".into(),
            stmt: 10,
            reason: StopReason::Breakpoint,
        });
        assert_eq!(s.view.as_ref().unwrap().current, Some(19));
        assert!(matches!(
            s.line("eval &x", None).unwrap(),
            Effect::Eval { .. }
        ));
        assert_eq!(
            s.line("s 2 into", None).unwrap(),
            Effect::Resume {
                step: true,
                count: 2,
                into: true
            }
        );
        assert_eq!(
            s.line("watch &x", None).unwrap(),
            Effect::Message("Watch 1 set on &x.".into())
        );
        s.resumed();
        assert!(s.view.as_ref().unwrap().current.is_none());
    }

    #[test]
    fn equates_and_moves() {
        let mut s = session();
        s.line("equate b5 break 5", None).unwrap();
        s.line("b5", None).unwrap();
        s.line("break 21", None).unwrap();
        s.line("next", Some(2)).unwrap();
        assert_eq!(s.view.as_ref().unwrap().top, 1);
        s.line("next", Some(10)).unwrap();
        assert_eq!(s.view.as_ref().unwrap().top, 21);
        s.line("bottom", None).unwrap();
        assert_eq!(s.view.as_ref().unwrap().top, 16);
        s.line("find 'LINE 3'", None).unwrap();
        assert_eq!(s.view.as_ref().unwrap().found, Some(30));
        assert_eq!(
            s.line("set updprod yes", None).unwrap(),
            Effect::UpdProd(true)
        );
    }
}
