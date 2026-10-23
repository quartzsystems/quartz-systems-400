// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Breakpoints and watches as the debug session keeps them: by source
//! line, each mapped to the statement the debug core stops at
//! ([`StmtMap`]: a line with no statement moves to the next one that
//! has, as IBM's debugger moves it).

/// Statement numbers by first source line, in line order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StmtMap {
    /// `(line, statement)`, lines 1-based and ascending.
    pub pairs: Vec<(usize, u32)>,
}

impl StmtMap {
    /// From `(line, statement)` pairs in any order.
    #[must_use]
    pub fn new(mut pairs: Vec<(usize, u32)>) -> Self {
        pairs.sort_unstable();
        pairs.dedup_by_key(|p| p.0);
        Self { pairs }
    }

    /// One statement per line (a view whose lines are statements).
    #[must_use]
    pub fn identity(lines: usize) -> Self {
        Self {
            pairs: (1..=lines)
                .map(|l| (l, u32::try_from(l).unwrap_or(u32::MAX)))
                .collect(),
        }
    }

    /// The statement on `line`, or the first one after it, with its line.
    #[must_use]
    pub fn at_or_after(&self, line: usize) -> Option<(usize, u32)> {
        self.pairs.iter().copied().find(|(l, _)| *l >= line)
    }

    /// The line a statement starts on.
    #[must_use]
    pub fn line_of(&self, stmt: u32) -> Option<usize> {
        self.pairs.iter().find(|(_, s)| *s == stmt).map(|(l, _)| *l)
    }
}

/// One breakpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Breakpoint {
    /// The program, `LIB/NAME`.
    pub program: String,
    /// The module.
    pub module: String,
    /// The source line.
    pub line: usize,
    /// The statement the core stops at.
    pub stmt: u32,
    /// A condition (`WHEN`).
    pub when: Option<String>,
    /// A thread breakpoint (`TBREAK`): the thread it is for.
    pub thread: Option<u64>,
}

/// The session's breakpoints.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Breakpoints {
    /// In the order they were set.
    pub list: Vec<Breakpoint>,
}

impl Breakpoints {
    /// Set one; a breakpoint on the same line is replaced.
    pub fn set(&mut self, b: Breakpoint) {
        self.list
            .retain(|o| !(o.program == b.program && o.module == b.module && o.line == b.line));
        self.list.push(b);
    }

    /// Remove the one on `line`; whether there was one.
    pub fn clear(&mut self, program: &str, module: &str, line: usize) -> bool {
        let before = self.list.len();
        self.list
            .retain(|o| !(o.program == program && o.module == module && o.line == line));
        before != self.list.len()
    }

    /// Remove every breakpoint of a program; how many went.
    pub fn clear_program(&mut self, program: &str) -> usize {
        let before = self.list.len();
        self.list.retain(|o| o.program != program);
        before - self.list.len()
    }

    /// The one on a line.
    #[must_use]
    pub fn at(&self, program: &str, module: &str, line: usize) -> Option<&Breakpoint> {
        self.list
            .iter()
            .find(|o| o.program == program && o.module == module && o.line == line)
    }

    /// A module's breakpoint lines, ascending.
    #[must_use]
    pub fn lines(&self, program: &str, module: &str) -> Vec<usize> {
        let mut v: Vec<usize> = self
            .list
            .iter()
            .filter(|o| o.program == program && o.module == module)
            .map(|o| o.line)
            .collect();
        v.sort_unstable();
        v
    }

    /// The first breakpoint line after `line` (`NEXT`).
    #[must_use]
    pub fn next_after(&self, program: &str, module: &str, line: usize) -> Option<usize> {
        self.lines(program, module).into_iter().find(|l| *l > line)
    }

    /// The last breakpoint line before `line` (`PREVIOUS`).
    #[must_use]
    pub fn prev_before(&self, program: &str, module: &str, line: usize) -> Option<usize> {
        self.lines(program, module)
            .into_iter()
            .rev()
            .find(|l| *l < line)
    }

    /// A module's breakpoints by line, for the Work with Module
    /// Breakpoints list.
    #[must_use]
    pub fn of_module(&self, program: &str, module: &str) -> Vec<&Breakpoint> {
        let mut v: Vec<&Breakpoint> = self
            .list
            .iter()
            .filter(|o| o.program == program && o.module == module)
            .collect();
        v.sort_by_key(|o| o.line);
        v
    }
}

/// One watch condition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Watch {
    /// Its number, from 1.
    pub n: usize,
    /// The variable as typed.
    pub expr: String,
    /// The bytes watched.
    pub len: usize,
    /// The program.
    pub program: String,
    /// The module.
    pub module: String,
    /// The procedure.
    pub procedure: String,
}

/// The session's watches.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Watches {
    /// By number.
    pub list: Vec<Watch>,
}

impl Watches {
    /// The most a session watches at once.
    pub const MAX: usize = 256;

    /// Add one; its number, or `None` when the table is full or the
    /// variable is already watched.
    pub fn add(&mut self, mut w: Watch) -> Option<usize> {
        if self.list.len() >= Self::MAX
            || self
                .list
                .iter()
                .any(|o| o.expr.eq_ignore_ascii_case(&w.expr) && o.program == w.program)
        {
            return None;
        }
        w.n = (1..=Self::MAX).find(|n| self.list.iter().all(|o| o.n != *n))?;
        let n = w.n;
        self.list.push(w);
        self.list.sort_by_key(|o| o.n);
        Some(n)
    }

    /// Remove by number.
    pub fn remove(&mut self, n: usize) -> bool {
        let before = self.list.len();
        self.list.retain(|o| o.n != n);
        before != self.list.len()
    }

    /// Remove all.
    pub fn clear(&mut self) {
        self.list.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bp(line: usize) -> Breakpoint {
        Breakpoint {
            program: "QGPL/P".into(),
            module: "P".into(),
            line,
            stmt: u32::try_from(line).unwrap(),
            when: None,
            thread: None,
        }
    }

    #[test]
    fn map_moves_to_next_statement() {
        let m = StmtMap::new(vec![(5, 2), (1, 1), (9, 3)]);
        assert_eq!(m.at_or_after(1), Some((1, 1)));
        assert_eq!(m.at_or_after(2), Some((5, 2)));
        assert_eq!(m.at_or_after(10), None);
        assert_eq!(m.line_of(3), Some(9));
        assert_eq!(StmtMap::identity(3).at_or_after(2), Some((2, 2)));
    }

    #[test]
    fn breakpoints() {
        let mut b = Breakpoints::default();
        b.set(bp(9));
        b.set(bp(3));
        b.set(bp(9));
        assert_eq!(b.lines("QGPL/P", "P"), vec![3, 9]);
        assert_eq!(b.next_after("QGPL/P", "P", 3), Some(9));
        assert_eq!(b.prev_before("QGPL/P", "P", 9), Some(3));
        assert_eq!(b.next_after("QGPL/P", "P", 9), None);
        assert!(b.clear("QGPL/P", "P", 3));
        assert!(!b.clear("QGPL/P", "P", 3));
        assert_eq!(b.clear_program("QGPL/P"), 1);
    }

    #[test]
    fn watches() {
        let mut w = Watches::default();
        let mk = |e: &str| Watch {
            n: 0,
            expr: e.into(),
            len: 4,
            program: "QGPL/P".into(),
            module: "P".into(),
            procedure: "P".into(),
        };
        assert_eq!(w.add(mk("&A")), Some(1));
        assert_eq!(w.add(mk("&B")), Some(2));
        assert_eq!(w.add(mk("&a")), None);
        assert!(w.remove(1));
        assert_eq!(w.add(mk("&C")), Some(1));
        w.clear();
        assert!(w.list.is_empty());
    }
}
