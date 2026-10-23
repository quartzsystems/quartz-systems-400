// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prompt session: the values on the display, what each key does,
//! and the command string the display stands for. Enter validates
//! through the analyzer; when the validation changes which parameters
//! show (`PMTCTL`, `KEYPARM`), the display comes back first, as IBM's
//! does, and runs on the next Enter.

use std::collections::HashMap;
use std::fmt::Write as _;

use super::layout::{
    self, DOTS_END, FIELD_COL, FIRST_ROW, LABEL_COL, Line, MARKER_COL, PAGE_ROWS, View,
};
use super::{COLS, Path, Screen};
use crate::analyze::{Env, Resolved};
use crate::def::{CmdDef, Parm, ParmType};
use crate::token::{Options, Sel, Tok};
use crate::value::Value;

/// What a key did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Show the display again.
    Show,
    /// The command string to run or return.
    Done(String),
    /// Cancelled (`F3`, `F12`): `CPF6801`.
    Cancel,
}

/// A window over the prompt: the choices of a field, the command
/// string, the errors, the help.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Window {
    /// The values a field accepts; a number picks one.
    Choices {
        /// The field.
        path: Path,
        /// The values.
        values: Vec<String>,
    },
    /// The command string as it would be submitted.
    Command(String),
    /// The messages of the last validation.
    Errors(Vec<String>),
    /// How to use the display.
    Help,
}

/// The session.
pub struct Prompter {
    /// The definition.
    pub def: CmdDef,
    /// The command name.
    pub name: String,
    /// The library it was found in.
    pub lib: String,
    texts: HashMap<Path, String>,
    initial: HashMap<Path, String>,
    defaults: HashMap<Path, String>,
    specified: Vec<bool>,
    sel: Vec<Option<Sel>>,
    /// The original values of parameters with `?&`, `?*` and `?-`.
    originals: Vec<Option<Value>>,
    page: usize,
    additional: bool,
    all: bool,
    keywords: bool,
    keyset: u8,
    keyparm_only: bool,
    expanded: HashMap<usize, usize>,
    message: String,
    error: Option<Path>,
    errors: Vec<String>,
    window: Option<Window>,
    label: Option<String>,
    /// A source line's label and comment (SEU's prompt of a CL line).
    source: Option<(String, String)>,
}

/// The parameter index of the label and comment fields of a source
/// line's prompt.
const SOURCE_PARM: usize = usize::MAX;

impl Prompter {
    /// A session for a resolved command.
    #[must_use]
    pub fn new(r: &Resolved) -> Self {
        let mut texts = HashMap::new();
        let mut defaults = HashMap::new();
        let mut originals = Vec::new();
        for (i, (p, b)) in r.def.parms.iter().zip(&r.parms).enumerate() {
            default_texts(p, &Path::parm(i), &mut defaults);
            if b.specified && !b.given.is_empty() {
                given_texts(p, &b.given, &Path::parm(i), &mut texts);
            } else if b.specified
                && let Some(v) = &b.value
            {
                value_texts(p, v, &Path::parm(i), &mut texts);
            }
            originals.push(b.value.clone());
        }
        // A parameter nobody gave shows its default.
        for (k, v) in &defaults {
            texts.entry(k.clone()).or_insert_with(|| v.clone());
        }
        let expanded = r
            .def
            .parms
            .iter()
            .enumerate()
            .filter(|(_, p)| p.is_list())
            .map(|(i, _)| {
                let n = texts
                    .keys()
                    .filter(|k| k.parm == i && k.steps.len() == 1)
                    .count();
                (i, n.max(1))
            })
            .collect();
        let keyparm_only = r.def.parms.iter().any(|p| p.keyparm);
        Self {
            def: r.def.clone(),
            name: r.name.clone(),
            lib: r.lib.clone(),
            initial: texts.clone(),
            texts,
            defaults,
            specified: r.parms.iter().map(|b| b.specified).collect(),
            sel: r.parms.iter().map(|b| b.sel).collect(),
            originals,
            page: 0,
            additional: false,
            all: false,
            keywords: false,
            keyset: 0,
            keyparm_only,
            expanded,
            message: String::new(),
            error: None,
            errors: Vec::new(),
            window: None,
            label: r.label.clone(),
            source: None,
        }
    }

    /// A session over a line of CL source (SEU's `F4`): the display
    /// gains a `Label` field first and a `Comment` field last, and the
    /// command string leaves them out ([`source_parts`](Self::source_parts)
    /// has them).
    #[must_use]
    pub fn for_source_line(r: &Resolved, comment: &str) -> Self {
        let mut p = Self::new(r);
        let label = p.label.take().unwrap_or_default();
        p.source = Some((label, comment.to_string()));
        p
    }

    /// The label and comment of a source line's prompt as they stand.
    #[must_use]
    pub fn source_parts(&self) -> Option<(String, String)> {
        self.source.clone()
    }

    /// The label and comment lines around a source line's parameters.
    fn with_source(&self, items: &mut Vec<Line>) {
        let Some((label, comment)) = &self.source else {
            return;
        };
        let line = |text: &str, value: &str, step: usize, width: usize| Line::Value {
            label: text.to_string(),
            indent: 0,
            field: Some((width, value.to_string())),
            protected: None,
            choices: String::new(),
            marker: false,
            path: Path::parm(SOURCE_PARM).step(step),
        };
        items.insert(0, line("Label", label, 0, 10));
        items.push(line("Comment", comment, 1, layout::MAX_WIDTH));
    }

    /// The display as it stands.
    #[must_use]
    #[allow(clippy::explicit_counter_loop)] // rows are positions, not counts
    pub fn screen(&self) -> Screen {
        if let Some(w) = &self.window {
            return self.window_screen(w);
        }
        let mut lines = self.lines();
        self.with_source(&mut lines.items);
        let mut s = Screen::default();
        s.center(1, &format!("{} ({})", self.def.prompt, self.name));
        s.put(3, LABEL_COL, "Type choices, press Enter.");
        let pages = lines.items.len().div_ceil(PAGE_ROWS).max(1);
        let page = self.page.min(pages - 1);
        let start = page * PAGE_ROWS;
        let mut row = FIRST_ROW;
        for line in lines.items.iter().skip(start).take(PAGE_ROWS) {
            Self::draw(&mut s, row, line);
            row += 1;
        }
        s.right(
            22,
            if page + 1 < pages {
                "More..."
            } else {
                "Bottom"
            },
        );
        let (k1, k2) = self.keys(lines.has_additional);
        s.put(23, LABEL_COL, &k1);
        s.put(24, LABEL_COL, &k2);
        if !self.message.is_empty() {
            s.message.clone_from(&self.message);
        }
        if let Some(e) = &self.error
            && let Some(f) = s.fields.iter_mut().find(|f| f.path.as_ref() == Some(e))
        {
            f.reverse = true;
            s.cursor = Some((f.row, f.col));
        }
        if s.cursor.is_none() {
            s.cursor = s.fields.first().map(|f| (f.row, f.col));
        }
        s
    }

    fn keys(&self, has_additional: bool) -> (String, String) {
        if self.keyset == 1 {
            return (
                "F9=All parameters   F11=Keywords   F14=Command string   F15=Error messages".into(),
                "F16=Command complete   F24=More keys".into(),
            );
        }
        let f10 = if has_additional {
            "   F10=Additional parameters"
        } else {
            ""
        };
        (
            format!("F3=Exit   F4=Prompt   F5=Refresh{f10}   F12=Cancel"),
            "F13=How to use this display        F24=More keys".into(),
        )
    }

    fn draw(s: &mut Screen, row: usize, line: &Line) {
        match line {
            Line::Blank => {}
            Line::Title(t) => s.center(row, t),
            Line::Head { label, indent } => s.put(row, LABEL_COL + indent, label),
            Line::Value {
                label,
                indent,
                field,
                protected,
                choices,
                marker,
                path,
            } => {
                if !label.is_empty() {
                    s.put(
                        row,
                        LABEL_COL + indent,
                        &layout::leaders(label, LABEL_COL + indent),
                    );
                }
                if *marker {
                    s.put(row, MARKER_COL + indent, ">");
                }
                let col = FIELD_COL + indent;
                let width = match (field, protected) {
                    (Some((w, v)), _) => {
                        s.field(row, col, *w, v, Some(path.clone()));
                        *w
                    }
                    (None, Some(t)) => {
                        s.put(row, col, t);
                        t.chars().count().max(10)
                    }
                    (None, None) => 10,
                };
                let ccol = (col + 14).max(col + width + 2);
                if ccol < COLS - 4 && !choices.is_empty() {
                    let room = COLS - ccol;
                    let mut c: String = choices.chars().take(room).collect();
                    if choices.chars().count() > room {
                        c.truncate(room.saturating_sub(3));
                        c.push_str("...");
                    }
                    s.put(row, ccol, &c);
                }
            }
            Line::More { path, indent } => {
                s.put(row, 24 + indent, "+ for more values");
                s.field(
                    row,
                    FIELD_COL + indent,
                    1,
                    "",
                    Some(Path::parm(path.parm).step(usize::MAX)),
                );
            }
        }
        let _ = DOTS_END;
    }

    #[allow(clippy::explicit_counter_loop)] // rows are positions, not counts
    fn window_screen(&self, w: &Window) -> Screen {
        let mut s = Screen::default();
        match w {
            Window::Choices { path, values } => {
                let p = &self.def.parms[path.parm];
                s.center(1, &format!("Specify Value for Parameter {}", p.kwd));
                s.put(3, LABEL_COL, "Type choice, press Enter.");
                let mut row = 5;
                for (i, v) in values.iter().enumerate().take(16) {
                    s.put(row, LABEL_COL + 2, &format!("{:>2}. {v}", i + 1));
                    row += 1;
                }
                s.put(21, LABEL_COL, "Selection");
                s.field(21, 12, 2, "", None);
                s.put(23, LABEL_COL, "F12=Cancel");
                s.cursor = Some((21, 12));
            }
            Window::Command(text) => {
                s.center(1, "Command String");
                let mut row = 3;
                for chunk in wrap(text, 76) {
                    if row > 21 {
                        break;
                    }
                    s.put(row, LABEL_COL, &chunk);
                    row += 1;
                }
                s.put(23, LABEL_COL, "Press Enter to continue.");
            }
            Window::Errors(lines) => {
                s.center(1, "Error Messages");
                let mut row = 3;
                if lines.is_empty() {
                    s.put(row, LABEL_COL, "No errors.");
                }
                for l in lines.iter().take(18) {
                    s.put(row, LABEL_COL, &l.chars().take(78).collect::<String>());
                    row += 1;
                }
                s.put(23, LABEL_COL, "Press Enter to continue.");
            }
            Window::Help => {
                s.center(1, "How to Use the Command Prompt");
                let help = [
                    "Type a value in each field you want to set and press Enter.",
                    "A value shown is the default; leave it to keep it.",
                    "A field with > before it was given on the command line.",
                    "F10 shows the parameters most calls leave at their defaults;",
                    "F9 shows every parameter the command has.",
                    "F11 shows the keyword of each parameter instead of its choices.",
                    "F14 shows the command string the display stands for.",
                    "F15 lists the errors the last Enter found.",
                    "F16 runs the command with the remaining parameters at their defaults.",
                    "F3 or F12 leaves without running the command.",
                ];
                for (i, l) in help.iter().enumerate() {
                    s.put(3 + i, LABEL_COL, l);
                }
                s.put(23, LABEL_COL, "Press Enter to continue.");
            }
        }
        s
    }

    fn view(&self) -> View<'_> {
        View {
            texts: &self.texts,
            specified: &self.specified,
            sel: &self.sel,
            additional: self.additional,
            all: self.all,
            keywords: self.keywords,
            keyparm_only: self.keyparm_only,
            expanded: &self.expanded,
        }
    }

    fn lines(&self) -> layout::Lines {
        layout::lines(&self.def, &self.view())
    }

    /// A key with the fields as the display returned them (in the
    /// order [`screen`](Self::screen) listed them).
    pub fn key(&mut self, key: u8, fields: &[String], env: &mut Env<'_>) -> Step {
        if let Some(w) = self.window.take() {
            return self.window_key(w, key, fields);
        }
        let shown = self.screen();
        let before = self.lines().visible;
        let mut more: Option<usize> = None;
        for (f, typed) in shown.fields.iter().zip(fields) {
            let Some(path) = &f.path else {
                continue;
            };
            if path.parm == SOURCE_PARM {
                if let Some(src) = self.source.as_mut() {
                    let v = typed.trim_end().to_string();
                    if path.steps.first() == Some(&0) {
                        src.0 = v.trim().to_ascii_uppercase();
                    } else {
                        src.1 = v;
                    }
                }
                continue;
            }
            if path.steps.last() == Some(&usize::MAX) {
                if !typed.trim().is_empty() {
                    more = Some(path.parm);
                }
                continue;
            }
            self.texts
                .insert(path.clone(), typed.trim_end().to_string());
        }
        self.message.clear();
        self.error = None;
        match key {
            0 => {
                if let Some(parm) = more {
                    *self.expanded.entry(parm).or_insert(1) += 1;
                    return Step::Show;
                }
                if self.keyparm_only {
                    self.keyparm_only = false;
                    return Step::Show;
                }
                self.validate(env, false, &before)
            }
            3 | 12 => Step::Cancel,
            4 => {
                let path = self
                    .error
                    .clone()
                    .or_else(|| {
                        shown
                            .fields
                            .iter()
                            .find_map(|f| f.path.clone().filter(|p| p.parm != SOURCE_PARM))
                    });
                if let Some(path) = path {
                    let values = self.choices_of(&path);
                    self.window = Some(Window::Choices { path, values });
                }
                Step::Show
            }
            5 => {
                self.texts = self.initial.clone();
                Step::Show
            }
            7 => {
                self.page = self.page.saturating_sub(1);
                Step::Show
            }
            8 => {
                self.page += 1;
                Step::Show
            }
            9 => {
                self.all = !self.all;
                Step::Show
            }
            10 => {
                self.additional = true;
                Step::Show
            }
            11 => {
                self.keywords = !self.keywords;
                Step::Show
            }
            13 => {
                self.window = Some(Window::Help);
                Step::Show
            }
            14 => {
                self.window = Some(Window::Command(self.command_text()));
                Step::Show
            }
            15 => {
                self.window = Some(Window::Errors(self.errors.clone()));
                Step::Show
            }
            16 => self.validate(env, true, &before),
            24 => {
                self.keyset ^= 1;
                Step::Show
            }
            _ => {
                self.message = "That key does nothing here.".into();
                Step::Show
            }
        }
    }

    fn window_key(&mut self, w: Window, key: u8, fields: &[String]) -> Step {
        if let Window::Choices { path, values } = w
            && key == 0
            && let Some(n) = fields.first().and_then(|f| f.trim().parse::<usize>().ok())
            && n >= 1
            && n <= values.len()
        {
            self.texts.insert(path, values[n - 1].clone());
        }
        Step::Show
    }

    /// The values a field accepts, for the choices window.
    fn choices_of(&self, path: &Path) -> Vec<String> {
        let p = item_def(&self.def.parms[path.parm], &path.steps);
        let mut v: Vec<String> = Vec::new();
        for s in &p.sngval {
            v.push(s.from.clone());
        }
        for s in &p.values {
            v.push(s.clone());
        }
        for s in &p.spcval {
            if !v.contains(&s.from) {
                v.push(s.from.clone());
            }
        }
        v
    }

    /// Validate the display's values; run when they stand, or show
    /// the display again when the parameters that show changed.
    fn validate(&mut self, env: &mut Env<'_>, complete: bool, before: &[bool]) -> Step {
        let text = self.command_text();
        match crate::analyze(env, &text) {
            Ok(_) => {
                // The values changed which parameters show: the display
                // comes back with them before the command runs.
                if !complete && !self.all && self.lines().visible != before {
                    return Step::Show;
                }
                Step::Done(text)
            }
            Err(e) => {
                self.errors = e
                    .diags
                    .iter()
                    .chain(std::iter::once(&e.escape))
                    .map(|d| {
                        if d.detail.is_empty() {
                            d.msgid.clone()
                        } else {
                            format!("{} {}", d.msgid, d.detail)
                        }
                    })
                    .collect();
                let first = e.diags.first().unwrap_or(&e.escape);
                self.message = if first.detail.is_empty() {
                    first.msgid.clone()
                } else {
                    format!("{} {}", first.msgid, first.detail)
                };
                self.error = first
                    .parm
                    .as_ref()
                    .and_then(|k| self.def.index(k))
                    .map(|i| self.first_path(i));
                Step::Show
            }
        }
    }

    /// The first field path of a parameter.
    fn first_path(&self, i: usize) -> Path {
        let p = &self.def.parms[i];
        let mut path = Path::parm(i);
        let mut d = p;
        loop {
            if d.is_list() {
                path = path.step(0);
            }
            if d.ty.is_group() && !d.items.is_empty() {
                path = path.step(0);
                d = &d.items[0];
                continue;
            }
            break;
        }
        if self.texts.contains_key(&path) {
            path
        } else {
            Path::parm(i)
        }
    }

    /// The command string the display stands for: every parameter
    /// whose value differs from its default or came from the command
    /// line, in keyword form.
    #[must_use]
    pub fn command_text(&self) -> String {
        let mut s = String::new();
        if let Some(l) = &self.label {
            s.push_str(l);
            s.push_str(": ");
        }
        s.push_str(&self.name);
        for (i, p) in self.def.parms.iter().enumerate() {
            if p.constant.is_some() {
                continue;
            }
            if let Some(Sel::Hidden | Sel::Show | Sel::Omit) = self.sel.get(i).copied().flatten() {
                if let Some(v) = self.originals.get(i).cloned().flatten()
                    && self.specified[i]
                {
                    let _ = write!(s, " {}({})", p.kwd, v.parm_source());
                }
                continue;
            }
            let text = self.assemble(p, &Path::parm(i));
            let dft = self.assemble_from(p, &Path::parm(i), &self.defaults);
            if text.is_empty() {
                continue;
            }
            if text != dft || self.specified[i] {
                let _ = write!(s, " {}({text})", p.kwd);
            }
        }
        s
    }

    fn assemble(&self, p: &Parm, path: &Path) -> String {
        self.assemble_from(p, path, &self.texts)
    }

    /// The source text of a parameter from field texts.
    fn assemble_from(&self, p: &Parm, path: &Path, texts: &HashMap<Path, String>) -> String {
        if p.is_list() {
            let shown = self.expanded.get(&path.parm).copied().unwrap_or(1).max(1);
            let mut items = Vec::new();
            for k in 0..shown {
                let entry = path.step(k);
                let t = if p.ty.is_group() {
                    let inner = self.assemble_group(p, &entry, texts);
                    if inner.trim().is_empty() {
                        String::new()
                    } else {
                        format!("({inner})")
                    }
                } else {
                    texts
                        .get(&entry)
                        .cloned()
                        .unwrap_or_default()
                        .trim()
                        .to_string()
                };
                if !t.is_empty() {
                    items.push(t);
                }
            }
            return items.join(" ");
        }
        if p.ty.is_group() {
            return self.assemble_group(p, path, texts);
        }
        texts
            .get(path)
            .cloned()
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    fn assemble_group(&self, p: &Parm, path: &Path, texts: &HashMap<Path, String>) -> String {
        match &p.ty {
            ParmType::Qual { .. } => {
                let mut parts: Vec<String> = p
                    .items
                    .iter()
                    .enumerate()
                    .map(|(k, q)| self.assemble_from(q, &path.step(k), texts))
                    .collect();
                while parts.len() > 1 && parts.last().is_some_and(|l| l.is_empty() || l == "*LIBL")
                {
                    parts.pop();
                }
                if parts.len() > 1 && parts[0].is_empty() {
                    return String::new();
                }
                parts.reverse();
                parts.join("/")
            }
            ParmType::Elem { .. } => {
                let parts: Vec<String> = p
                    .items
                    .iter()
                    .enumerate()
                    .map(|(k, e)| {
                        let t = self.assemble_from(e, &path.step(k), texts);
                        if e.ty.is_group() && !e.is_list() && !t.is_empty() {
                            format!("({t})")
                        } else if t.is_empty() {
                            "*N".into()
                        } else {
                            t
                        }
                    })
                    .collect();
                let mut parts = parts;
                while parts.last().is_some_and(|l| l == "*N") {
                    parts.pop();
                }
                parts.join(" ")
            }
            _ => texts.get(path).cloned().unwrap_or_default(),
        }
    }
}

/// The definition of the item a path leads to.
fn item_def<'a>(p: &'a Parm, steps: &[usize]) -> &'a Parm {
    let mut d = p;
    let mut i = 0;
    if d.is_list() {
        i += 1;
    }
    while i < steps.len() {
        if d.ty.is_group() {
            match d.items.get(steps[i]) {
                Some(next) => d = next,
                None => break,
            }
            i += 1;
            if d.is_list() {
                i += 1;
            }
        } else {
            break;
        }
    }
    d
}

/// The field texts of a parameter as the caller wrote it.
fn given_texts(p: &Parm, toks: &[Tok], path: &Path, out: &mut HashMap<Path, String>) {
    if p.is_list() {
        let entries: &[Tok] = match toks {
            [Tok::Paren(items)]
                if !p.ty.is_group() && items.iter().all(|t| !matches!(t, Tok::Paren(_))) =>
            {
                items
            }
            other => other,
        };
        for (k, t) in entries.iter().enumerate() {
            tok_texts(p, t, &path.step(k), out);
        }
        return;
    }
    match &p.ty {
        ParmType::Qual { .. } => match toks {
            [one] => tok_texts(p, one, path, out),
            many => tok_texts(p, &Tok::Paren(many.to_vec()), path, out),
        },
        ParmType::Elem { .. } => match toks {
            [Tok::Paren(_)] => tok_texts(p, &toks[0], path, out),
            [one] => tok_texts(p, one, path, out),
            many => tok_texts(p, &Tok::Paren(many.to_vec()), path, out),
        },
        _ => {
            out.insert(path.clone(), Tok::join(toks));
        }
    }
}

/// The field texts of a bound value.
fn value_texts(p: &Parm, v: &Value, path: &Path, out: &mut HashMap<Path, String>) {
    if p.is_list() {
        let items: Vec<&Value> = match v {
            Value::List(items) => items.iter().collect(),
            other => vec![other],
        };
        for (k, item) in items.iter().enumerate() {
            item_texts(p, item, &path.step(k), out);
        }
        return;
    }
    item_texts(p, v, path, out);
}

fn item_texts(p: &Parm, v: &Value, path: &Path, out: &mut HashMap<Path, String>) {
    match (&p.ty, v) {
        (ParmType::Qual { .. }, Value::Qual(parts)) => {
            for (k, part) in parts.iter().enumerate() {
                if let Some(q) = p.items.get(k) {
                    item_texts(q, part, &path.step(k), out);
                }
            }
        }
        (ParmType::Elem { .. }, Value::Elems(items)) => {
            for (k, item) in items.iter().enumerate() {
                if let (Some(e), Some(v)) = (p.items.get(k), item) {
                    value_texts(e, v, &path.step(k), out);
                }
            }
        }
        (ParmType::Qual { .. } | ParmType::Elem { .. }, other) => {
            // A single value for a group: it goes in the first part.
            out.insert(path.step(0), other.source());
            if matches!(p.ty, ParmType::Qual { .. }) && p.items.len() > 1 {
                out.insert(path.step(1), String::new());
            }
        }
        (_, other) => {
            out.insert(path.clone(), other.source());
        }
    }
}

/// The default texts of a parameter, element by element.
fn default_texts(p: &Parm, path: &Path, out: &mut HashMap<Path, String>) {
    let toks = p
        .dft
        .as_ref()
        .map(|d| crate::token::tokenize_list(d, Options::default()).unwrap_or_default())
        .unwrap_or_default();
    if p.is_list() {
        let entry = path.step(0);
        if let Some(first) = toks.first() {
            tok_texts(p, first, &entry, out);
        } else if p.ty.is_group() {
            for (k, item) in p.items.iter().enumerate() {
                default_texts(item, &entry.step(k), out);
            }
        }
        return;
    }
    match &p.ty {
        ParmType::Qual { .. } => {
            if let [one] = toks.as_slice() {
                tok_texts(p, one, path, out);
            }
            for (k, q) in p.items.iter().enumerate() {
                let sub = path.step(k);
                if !out.contains_key(&sub) {
                    default_texts(q, &sub, out);
                }
            }
        }
        ParmType::Elem { .. } => {
            match toks.as_slice() {
                [one] => tok_texts(p, one, path, out),
                many if !many.is_empty() => {
                    for (k, t) in many.iter().enumerate() {
                        if let Some(e) = p.items.get(k) {
                            tok_texts(e, t, &path.step(k), out);
                        }
                    }
                }
                _ => {}
            }
            for (k, e) in p.items.iter().enumerate() {
                let sub = path.step(k);
                if !out.contains_key(&sub) && !out.contains_key(&sub.step(0)) {
                    default_texts(e, &sub, out);
                }
            }
        }
        _ => {
            if let [one] = toks.as_slice() {
                out.insert(path.clone(), one.text());
            }
        }
    }
}

/// A default token into the texts of a parameter or group.
fn tok_texts(p: &Parm, t: &Tok, path: &Path, out: &mut HashMap<Path, String>) {
    match (&p.ty, t) {
        (ParmType::Qual { .. }, Tok::Word(w)) if w.contains('/') => {
            let mut parts: Vec<&str> = w.split('/').collect();
            parts.reverse();
            for (k, part) in parts.iter().enumerate() {
                out.insert(path.step(k), (*part).to_string());
            }
        }
        (ParmType::Qual { .. }, Tok::Paren(items)) => {
            for (k, item) in items.iter().enumerate() {
                out.insert(path.step(k), item.text());
            }
        }
        (ParmType::Qual { .. }, other) => {
            out.insert(path.step(0), other.text());
        }
        (ParmType::Elem { .. }, Tok::Paren(items)) => {
            for (k, item) in items.iter().enumerate() {
                if let Some(e) = p.items.get(k) {
                    tok_texts(e, item, &path.step(k), out);
                }
            }
        }
        (ParmType::Elem { .. }, other) => {
            out.insert(path.step(0), other.text());
        }
        (_, other) => {
            out.insert(path.clone(), other.text());
        }
    }
}

/// Break text into lines of at most `width`, at blanks where possible.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split(' ') {
        if cur.chars().count() + word.chars().count() + 1 > width && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
        while cur.chars().count() > width {
            let head: String = cur.chars().take(width).collect();
            cur = cur.chars().skip(width).collect();
            out.push(head);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}
