// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the edit and browse display shows: the header, a row per
//! line, the prompt, the keys, the message.

use super::{DATA_COL, FIRST_ROW, Item, Mode, Session, Target};
use crate::prompt::Prompt;
use crate::srctype::{FREE, format_line};

/// The first three rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// `Columns`: the first and last column shown.
    pub columns: String,
    /// `Edit` or `Browse`.
    pub title: String,
    /// The file (`LIB/FILE`) or the pending command.
    pub right: String,
    /// The member; blank while a command is pending.
    pub member: String,
    /// The format line's label (`FMT xx`).
    pub fmt_label: String,
    /// The format line's ruler.
    pub fmt_ruler: String,
}

/// What a row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// A record: sequence and data can be typed over.
    Record,
    /// An insert line.
    Insert,
    /// The beginning or end of data, excluded records, a format line.
    Marker,
    /// Nothing.
    Blank,
}

/// One row of records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// The screen row.
    pub row: usize,
    /// The sequence area (7 characters).
    pub seq: String,
    /// The data shown (the window's width).
    pub data: String,
    /// The record's date and its screen column, when the window's end
    /// leaves room.
    pub date: Option<(usize, String)>,
    /// What it is.
    pub kind: RowKind,
}

/// The prompt area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptView {
    /// The `Prompt type` row.
    pub top: usize,
    /// The format.
    pub format: String,
    /// The sequence number of the line prompted.
    pub seq: String,
    /// Labels: row, column, text.
    pub labels: Vec<(usize, usize, String)>,
    /// Fields: row, column, length, value.
    pub fields: Vec<(usize, usize, usize, String)>,
    /// The free format's ruler: row, column, text.
    pub ruler: Option<(usize, usize, String)>,
}

/// The whole display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// Browse or edit.
    pub mode: Mode,
    /// Rows 1 to 3.
    pub header: Header,
    /// The command line's text.
    pub cmdline: String,
    /// The record rows.
    pub rows: Vec<Row>,
    /// The prompt, when one is shown.
    pub prompt: Option<PromptView>,
    /// The function key set (0, 1, 2); `None` in full screen mode.
    pub keyset: Option<u8>,
    /// The message line.
    pub message: Option<String>,
    /// The copyright on the last row.
    pub copyright: bool,
    /// The cursor.
    pub cursor: (usize, usize),
}

/// A row of a character repeated.
fn run(c: char, n: usize) -> String {
    std::iter::repeat_n(c, n).collect()
}

/// A marker line: `lead` stars or dashes, the words, the rest to the
/// window's width.
fn marker(fill: char, lead: usize, words: &str, width: usize) -> String {
    let mut s = run(fill, lead);
    s.push(' ');
    s.push_str(words);
    s.push(' ');
    let n = width.saturating_sub(s.chars().count());
    s.push_str(&run(fill, n));
    s
}

/// The columns `from..from+n` of a record.
fn window_of(data: &str, from: usize, n: usize) -> String {
    let s: String = data.chars().skip(from - 1).take(n).collect();
    format!("{s:<n$}")
}

impl Session {
    /// The display as it stands; remembers what each row showed, to
    /// read the operator's changes against.
    pub fn view(&mut self) -> View {
        let items = self.items();
        let rows_n = self.body_rows();
        self.top = self.top.min(items.len().saturating_sub(1));
        let w = self.window();
        let mut rows = Vec::new();
        self.shown.clear();
        for k in 0..rows_n {
            let row = FIRST_ROW + k;
            let Some(item) = items.get(self.top + k) else {
                rows.push(Row {
                    row,
                    seq: String::new(),
                    data: String::new(),
                    date: None,
                    kind: RowKind::Blank,
                });
                continue;
            };
            let r = self.row_for(row, item, w);
            self.shown
                .push((row, item.clone(), r.seq.clone(), r.data.clone()));
            rows.push(r);
        }
        let header = self.header(&items, w);
        let prompt = self.prompt_view();
        let cursor = self.cursor.unwrap_or((2, DATA_COL));
        View {
            mode: self.mode,
            header,
            cmdline: self.cmdline.clone(),
            rows,
            prompt,
            keyset: (!self.defaults.full || self.prompt.is_some()).then_some(self.keyset),
            message: self.message.clone(),
            copyright: self.copyright,
            cursor,
        }
    }

    /// The pending command as typed (the first one).
    fn pending_text(&self) -> Option<String> {
        self.pending.first().map(|p| p.cmd.text.clone())
    }

    fn header(&self, items: &[Item], w: usize) -> Header {
        let right_col = self.left + w - 1;
        let fmt = self.header_format(items);
        let (fmt_label, fmt_ruler) = format_line(&fmt, self.left, right_col);
        let (right, member) = match self.pending_text() {
            Some(t) => (format!("Pending . . . . . :   {t}"), String::new()),
            None => (format!("{:>26}", format!("{}/{}", self.lib, self.file)), format!("{:>10}", self.member)),
        };
        Header {
            columns: format!("{:>5}{:>4}", self.left, right_col),
            title: match self.mode {
                Mode::Edit => "Edit",
                Mode::Browse => "Browse",
            }
            .into(),
            right,
            member,
            fmt_label,
            fmt_ruler,
        }
    }

    /// The format the format line shows: the cursor's record's, else
    /// the first record shown, else the free format.
    fn header_format(&self, items: &[Item]) -> String {
        let st = self.srctype();
        let rec_on = |row: usize| {
            self.shown.iter().find(|s| s.0 == row).and_then(|s| match &s.1 {
                Item::Rec(id) => Some(*id),
                _ => None,
            })
        };
        if let Some((r, _)) = self.cursor
            && let Some(id) = rec_on(r)
        {
            let data = self.target_data(Target::Rec(id));
            // On a record the cursor names, a comment shows its record's
            // specification.
            let f = st.format_of(&data);
            if f == "*" {
                let mut d: Vec<char> = data.chars().collect();
                if d.len() >= 7 {
                    d[6] = ' ';
                }
                return st.format_of(&d.into_iter().collect::<String>()).to_string();
            }
            return f.to_string();
        }
        let first = items.iter().skip(self.top).find_map(|i| match i {
            Item::Rec(id) => Some(*id),
            _ => None,
        });
        first.map_or_else(|| "**".into(), |id| st.format_of(&self.target_data(Target::Rec(id))).to_string())
    }

    fn row_for(&self, row: usize, item: &Item, w: usize) -> Row {
        let mk = |seq: String, data: String, date, kind| Row {
            row,
            seq,
            data,
            date,
            kind,
        };
        match item {
            Item::Begin => mk(String::new(), marker('*', 15, "Beginning of data", super::WINDOW), None, RowKind::Marker),
            Item::End => mk(String::new(), marker('*', 18, "End of data", super::WINDOW), None, RowKind::Marker),
            Item::Excl(ids) => mk(
                "- - - -".into(),
                marker('-', 13, &format!("{} data records excluded", ids.len()), super::WINDOW),
                None,
                RowKind::Marker,
            ),
            Item::Fmt(_, f) => {
                let (l, r) = format_line(f, self.left, self.left + w - 1);
                mk(l, r, None, RowKind::Marker)
            }
            Item::Ins(_) => mk("'''''''".into(), String::new(), None, RowKind::Insert),
            Item::Rec(id) => {
                let Some(r) = self.buf.index_of(*id).and_then(|i| self.buf.get(i)) else {
                    return mk(String::new(), String::new(), None, RowKind::Blank);
                };
                let data = window_of(&r.data, self.left, w);
                let col = DATA_COL + w + 5;
                let date = (col <= 79).then(|| {
                    let t = format!("{:06}", r.date);
                    (col, t.chars().take(80 - col).collect())
                });
                // A pending command shows where it was typed.
                let seq = self
                    .pending
                    .iter()
                    .find(|p| p.id == *id)
                    .map_or_else(|| crate::seq_text(r.seq), |p| p.cmd.text.clone());
                mk(seq, data, date, RowKind::Record)
            }
        }
    }

    fn prompt_view(&self) -> Option<PromptView> {
        let p = self.prompt.as_ref()?;
        let data = self.target_data(p.target);
        let pr = Prompt::new(&p.format, &data, self.buf.width);
        let top = pr.layout.top;
        let labels = pr
            .layout
            .labels
            .iter()
            .map(|(r, c, t)| (top + r, *c, (*t).to_string()))
            .collect();
        let fields = pr
            .layout
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| (top + f.row, f.col, pr.field_len(i), pr.values[i].clone()))
            .collect();
        let ruler = (pr.layout == FREE).then(|| {
            let text: String = (1..=79)
                .map(|c| if c == 1 { '.' } else { crate::srctype::ruler_char(c) })
                .collect();
            (top + 3, 2, text)
        });
        Some(PromptView {
            top,
            format: p.format.clone(),
            seq: self.target_seq(p.target),
            labels,
            fields,
            ruler,
        })
    }

    /// What a screen row showed at the last view: the item, sequence
    /// text, data text.
    #[must_use]
    pub fn shown_at(&self, row: usize) -> Option<(&Item, &str, &str)> {
        self.shown
            .iter()
            .find(|s| s.0 == row)
            .map(|s| (&s.1, s.2.as_str(), s.3.as_str()))
    }
}
