// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The session's history and the lines being typed. A statement is kept
//! as the input lines it was typed on (leading blank lines too, as the
//! display shows them), its messages after it; the history and the
//! input lines scroll as one area. F9 retrieves earlier statements; F6,
//! F10, F14 and F15 insert, copy, delete and split input lines.

/// One history entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// A statement: the lines it was typed on.
    Statement(Vec<String>),
    /// A message: its identifier (blank for none) and text.
    Message {
        /// `SQLnnnn` or `CPFnnnn`; blank for a plain line.
        id: String,
        /// The first-level text.
        text: String,
    },
}

/// A line of the scrolling area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    /// A statement's line: the first shows after `>`.
    Statement {
        /// The text.
        text: String,
        /// The statement's first line.
        first: bool,
        /// The statement's index among the statements.
        index: usize,
    },
    /// A message line.
    Message {
        /// The text.
        text: String,
        /// The message's identifier, for F1.
        id: String,
    },
}

/// Wrap `text` at `width` on blanks, cutting words longer than a line.
#[must_use]
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut w = word.to_string();
        while w.chars().count() > width {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            let head: String = w.chars().take(width).collect();
            w = w.chars().skip(width).collect();
            out.push(head);
        }
        let need = cur.chars().count() + usize::from(!cur.is_empty()) + w.chars().count();
        if need > width && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&w);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

/// The lines of the scrolling area.
pub const AREA: usize = 17;
/// Input lines kept below a history that fills the area.
pub const MIN_INPUT: usize = 4;
/// The width of an input line.
pub const WIDTH: usize = 69;
/// The width of a message line.
pub const MSG_WIDTH: usize = 68;

/// The history, the input and where the area stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct History {
    /// What ran and what it said, oldest first.
    pub entries: Vec<Entry>,
    /// The input lines shown, each a field's text.
    pub input: Vec<String>,
    /// The first history line shown.
    pub top: usize,
    /// How many statements back F9 has gone.
    retrieved: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    /// Empty: the whole area is input.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            input: vec![String::new(); AREA],
            top: 0,
            retrieved: 0,
        }
    }

    /// The input lines as a statement's lines: trailing blank lines
    /// dropped, each line's trailing blanks too.
    #[must_use]
    pub fn typed(&self) -> Vec<String> {
        let mut lines: Vec<String> = self
            .input
            .iter()
            .map(|l| l.trim_end().to_string())
            .collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines
    }

    /// A statement entered, as typed.
    pub fn statement(&mut self, lines: Vec<String>) {
        self.entries.push(Entry::Statement(lines));
        self.retrieved = 0;
    }

    /// A message, cut to one line.
    pub fn message(&mut self, id: &str, text: &str) {
        self.entries.push(Entry::Message {
            id: id.to_string(),
            text: text.chars().take(MSG_WIDTH).collect(),
        });
    }

    /// Services option 3: the history emptied.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.retrieved = 0;
        self.top = 0;
        self.fit();
    }

    /// The input as one statement: the lines end to end, each as wide
    /// as its field, so a word may run on to the next line.
    #[must_use]
    pub fn text(&self) -> String {
        join(&self.input)
    }

    /// Put `lines` in the input, from its first line.
    pub fn set_lines(&mut self, lines: &[String]) {
        let n = self.input.len().max(lines.len());
        self.input = lines
            .iter()
            .map(|l| l.chars().take(WIDTH).collect())
            .collect();
        self.input.resize(n, String::new());
    }

    /// Put `text` in the input, cut to the lines' width.
    pub fn set_text(&mut self, text: &str) {
        let chars: Vec<char> = text.chars().collect();
        let lines: Vec<String> = chars.chunks(WIDTH).map(|c| c.iter().collect()).collect();
        self.set_lines(&lines);
    }

    /// Empty the input.
    pub fn clear_input(&mut self) {
        let n = self.input.len();
        self.input = vec![String::new(); n];
    }

    /// The statements, newest first.
    fn statements(&self) -> Vec<&Vec<String>> {
        self.entries
            .iter()
            .rev()
            .filter_map(|e| match e {
                Entry::Statement(s) => Some(s),
                Entry::Message { .. } => None,
            })
            .collect()
    }

    /// The statement with this index (oldest first).
    #[must_use]
    pub fn statement_at(&self, index: usize) -> Option<&Vec<String>> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                Entry::Statement(s) => Some(s),
                Entry::Message { .. } => None,
            })
            .nth(index)
    }

    /// F9: the next earlier statement into the input; false when there
    /// is none.
    pub fn retrieve(&mut self) -> bool {
        let all = self.statements();
        if all.is_empty() {
            return false;
        }
        let i = self.retrieved % all.len();
        let s = all[i].clone();
        self.retrieved += 1;
        self.clear_input();
        self.set_lines(&s);
        true
    }

    /// F6: a blank line after input line `at`.
    pub fn insert_line(&mut self, at: usize) {
        let at = (at + 1).min(self.input.len());
        self.input.insert(at, String::new());
    }

    /// F10: input line `at` copied after itself.
    pub fn copy_line(&mut self, at: usize) {
        if let Some(l) = self.input.get(at).cloned() {
            self.input.insert(at + 1, l);
        }
    }

    /// F14: input line `at` removed, a blank line added at the end.
    pub fn delete_line(&mut self, at: usize) {
        if at < self.input.len() {
            self.input.remove(at);
            self.input.push(String::new());
        }
    }

    /// F15: input line `at` split at column `col` (from 0); the rest
    /// goes to a new line after it.
    pub fn split_line(&mut self, at: usize, col: usize) {
        let Some(l) = self.input.get(at).cloned() else {
            return;
        };
        let head: String = l.chars().take(col).collect();
        let tail: String = l.chars().skip(col).collect();
        self.input[at] = head;
        self.input.insert(at + 1, tail);
    }

    /// Every history line of the scrolling area.
    #[must_use]
    pub fn lines(&self) -> Vec<Line> {
        let mut out = Vec::new();
        let mut index = 0;
        for e in &self.entries {
            match e {
                Entry::Statement(s) => {
                    if s.is_empty() {
                        out.push(Line::Statement {
                            text: String::new(),
                            first: true,
                            index,
                        });
                    }
                    for (i, l) in s.iter().enumerate() {
                        out.push(Line::Statement {
                            text: l.clone(),
                            first: i == 0,
                            index,
                        });
                    }
                    index += 1;
                }
                Entry::Message { id, text } => out.push(Line::Message {
                    text: text.clone(),
                    id: id.clone(),
                }),
            }
        }
        out
    }

    /// The last `top` the area can have: the history's end with
    /// [`MIN_INPUT`] input lines below it.
    #[must_use]
    pub fn max_top(&self) -> usize {
        self.lines().len().saturating_sub(AREA - MIN_INPUT)
    }

    /// Size the input to the rows the history leaves, never dropping a
    /// line that holds text.
    pub fn fit(&mut self) {
        let shown = self.lines().len().saturating_sub(self.top).min(AREA);
        let rows = AREA - shown;
        let mut keep = self.input.len();
        while keep > rows && self.input[keep - 1].trim().is_empty() {
            keep -= 1;
        }
        self.input.truncate(keep);
        self.input.resize(rows.max(keep), String::new());
    }

    /// The area after a statement ran: where it stood, scrolled only as
    /// far as the input's minimum needs.
    pub fn to_end(&mut self) {
        self.top = self.top.max(self.max_top());
        self.fit();
    }

    /// Scrolled so every input line shows (after F6, F10, F15).
    pub fn show_input(&mut self) {
        let n = self.lines().len();
        let need = (n + self.input.len()).saturating_sub(AREA);
        self.top = self.top.max(need).min(n);
    }

    /// The area with the last statement at its top (after a statement
    /// failed), as far as the input's minimum allows.
    pub fn to_last_statement(&mut self) {
        let lines = self.lines();
        let start = lines
            .iter()
            .rposition(|l| matches!(l, Line::Statement { first: true, .. }))
            .unwrap_or(0);
        self.top = start.max(self.max_top());
        self.fit();
    }

    /// Page Up: earlier history.
    pub fn page_up(&mut self) {
        self.top = self.top.saturating_sub(AREA - MIN_INPUT);
        self.fit();
    }

    /// Page Down: later history, back to the end.
    pub fn page_down(&mut self) {
        self.top = (self.top + AREA - MIN_INPUT).min(self.max_top());
        self.fit();
    }

    /// Whether the area shows the history's end.
    #[must_use]
    pub fn at_end(&self) -> bool {
        self.top >= self.max_top()
    }
}

/// Lines end to end, each as wide as an input field, trimmed.
#[must_use]
pub fn join(lines: &[String]) -> String {
    let mut s = String::new();
    for l in lines {
        let mut l: String = l.chars().take(WIDTH).collect();
        while l.chars().count() < WIDTH {
            l.push(' ');
        }
        s.push_str(&l);
    }
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps() {
        assert_eq!(wrap("select a from b", 8), vec!["select a", "from b"]);
        assert_eq!(wrap("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
        assert_eq!(wrap("", 4), vec![""]);
    }

    #[test]
    fn text_runs_across_lines() {
        let mut h = History::new();
        h.input[0] = "select * f".into();
        h.input[1] = "rom t".into();
        assert_eq!(
            h.text(),
            format!("select * f{}rom t", " ".repeat(WIDTH - 10))
        );
        h.clear_input();
        h.input[0] = "select *".into();
        h.input[1] = "from t".into();
        assert_eq!(h.typed(), vec!["select *", "from t"]);
    }

    #[test]
    fn retrieve_walks_back() {
        let mut h = History::new();
        assert!(!h.retrieve());
        h.statement(vec!["select 1".into()]);
        h.message("", "done");
        h.statement(vec![String::new(), "select 2".into()]);
        assert!(h.retrieve());
        assert_eq!(h.text(), "select 2");
        assert_eq!(h.input[1], "select 2");
        assert!(h.retrieve());
        assert_eq!(h.text(), "select 1");
        assert!(h.retrieve());
        assert_eq!(h.text(), "select 2");
    }

    #[test]
    fn line_edits() {
        let mut h = History::new();
        h.input[0] = "select a from t".into();
        h.split_line(0, 9);
        assert_eq!(h.input[0], "select a ");
        assert_eq!(h.input[1], "from t");
        h.copy_line(1);
        assert_eq!(h.input[2], "from t");
        h.delete_line(2);
        assert_eq!(h.input[2], "");
        h.insert_line(0);
        assert_eq!(h.input[1], "");
        assert_eq!(h.input[2], "from t");
    }

    #[test]
    fn area_scrolls() {
        let mut h = History::new();
        h.message("", "connected");
        h.fit();
        assert_eq!(h.input.len(), AREA - 1);
        for i in 0..8 {
            h.statement(vec![format!("select {i}")]);
            h.message("", "ran");
        }
        h.to_end();
        assert_eq!(h.input.len(), MIN_INPUT);
        assert_eq!(h.lines().len() - h.top, AREA - MIN_INPUT);
        h.page_up();
        assert!(!h.at_end());
        h.page_down();
        assert!(h.at_end());
        h.statement(vec![String::new(), String::new(), "bad".into()]);
        h.message("SQL0104", "no");
        h.to_last_statement();
        let first = &h.lines()[h.top];
        assert!(matches!(first, Line::Statement { first: true, .. }));
        assert_eq!(h.input.len(), AREA - 4);
    }
}
