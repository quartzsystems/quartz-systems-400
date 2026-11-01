// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `httpd.conf`: the parsed tree, its printing, and the edits the
//! administration pages make. Unedited nodes print their source text, so
//! comments and layout survive a form's change.

use std::fmt::Write as _;

pub mod parse;
pub mod rules;

pub use parse::parse;

/// A configuration error: where and what (`HTP8100`).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("line {line}, column {col}: {text}")]
pub struct ConfError {
    /// 1-based line.
    pub line: u32,
    /// 1-based column.
    pub col: u32,
    /// What is wrong.
    pub text: String,
}

impl ConfError {
    /// An error at `line`, `col`.
    pub fn new(line: u32, col: u32, text: &str) -> Self {
        Self {
            line,
            col,
            text: text.to_string(),
        }
    }
}

/// A directive line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directive {
    /// As written.
    pub name: String,
    /// The arguments, quotes removed.
    pub args: Vec<String>,
    /// 1-based line (0 for one added since parsing).
    pub line: u32,
    /// The source text with its line end; `None` once edited.
    pub raw: Option<String>,
}

impl Directive {
    /// A new directive.
    pub fn new(name: &str, args: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            args: args.iter().map(ToString::to_string).collect(),
            line: 0,
            raw: None,
        }
    }

    /// Whether this is directive `name` (case-blind).
    pub fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }

    /// The arguments joined by spaces.
    pub fn value(&self) -> String {
        self.args.join(" ")
    }
}

/// A `<Section args>` with what is inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    /// `Directory`, `VirtualHost`, ...
    pub name: String,
    /// Its arguments.
    pub args: Vec<String>,
    /// 1-based line of the start.
    pub line: u32,
    /// The nodes inside.
    pub items: Vec<Node>,
    /// The start line's text.
    pub raw: Option<String>,
    /// The end line's text.
    pub close_raw: Option<String>,
}

/// A line or a section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// A blank line (its text).
    Blank(String),
    /// A comment (its text).
    Comment(String),
    /// A directive.
    Directive(Directive),
    /// A section.
    Section(Section),
}

/// Directives and sections shared by the file and its sections.
pub trait Scope {
    /// The nodes.
    fn items(&self) -> &Vec<Node>;
    /// The nodes, to change.
    fn items_mut(&mut self) -> &mut Vec<Node>;

    /// Every directive `name` directly in this scope.
    fn all(&self, name: &str) -> Vec<&Directive> {
        self.items()
            .iter()
            .filter_map(|n| match n {
                Node::Directive(d) if d.is(name) => Some(d),
                _ => None,
            })
            .collect()
    }

    /// The last directive `name` (the one that wins).
    fn get(&self, name: &str) -> Option<&Directive> {
        self.all(name).into_iter().last()
    }

    /// The last directive `name`'s arguments joined, or "".
    fn value(&self, name: &str) -> String {
        self.get(name).map(Directive::value).unwrap_or_default()
    }

    /// Set directive `name` to `args`: the first one is changed, any
    /// others removed; a new one goes after the last directive.
    fn set(&mut self, name: &str, args: &[&str]) {
        let mut done = false;
        self.items_mut().retain_mut(|n| match n {
            Node::Directive(d) if d.is(name) => {
                if done {
                    return false;
                }
                done = true;
                d.args = args.iter().map(ToString::to_string).collect();
                d.raw = None;
                true
            }
            _ => true,
        });
        if !done {
            self.add(Directive::new(name, args));
        }
    }

    /// Remove every directive `name`; how many went.
    fn unset(&mut self, name: &str) -> usize {
        self.remove_where(|d| d.is(name))
    }

    /// Remove the directives `f` picks; how many went.
    fn remove_where(&mut self, f: impl Fn(&Directive) -> bool) -> usize {
        let before = self.items().len();
        self.items_mut().retain(|n| !matches!(n, Node::Directive(d) if f(d)));
        before - self.items().len()
    }

    /// Add `d` after the last directive of the same name, else after the
    /// last directive in this scope, else at the end.
    fn add(&mut self, d: Directive) {
        let items = self.items();
        let same = items.iter().rposition(|n| matches!(n, Node::Directive(x) if x.is(&d.name)));
        let any = items.iter().rposition(|n| matches!(n, Node::Directive(_)));
        let at = same.or(any).map_or(items.len(), |i| i + 1);
        self.items_mut().insert(at, Node::Directive(d));
    }

    /// Every section `name` directly in this scope.
    fn sections(&self, name: &str) -> Vec<&Section> {
        self.items()
            .iter()
            .filter_map(|n| match n {
                Node::Section(s) if s.name.eq_ignore_ascii_case(name) => Some(s),
                _ => None,
            })
            .collect()
    }

    /// Section `name` whose first argument is `arg`.
    fn section(&self, name: &str, arg: &str) -> Option<&Section> {
        self.sections(name).into_iter().find(|s| s.args.first().map(String::as_str) == Some(arg))
    }

    /// Section `name` `arg`, to change; created at the end if missing.
    fn section_mut(&mut self, name: &str, arg: &str) -> &mut Section {
        let at = self.items().iter().position(
            |n| matches!(n, Node::Section(s) if s.name.eq_ignore_ascii_case(name) && s.args.first().map(String::as_str) == Some(arg)),
        );
        let at = at.unwrap_or_else(|| {
            let items = self.items_mut();
            items.push(Node::Section(Section::new(name, &[arg])));
            items.len() - 1
        });
        match &mut self.items_mut()[at] {
            Node::Section(s) => s,
            _ => unreachable!("found a section"),
        }
    }

    /// Remove section `name` `arg`; whether one went.
    fn remove_section(&mut self, name: &str, arg: &str) -> bool {
        let before = self.items().len();
        self.items_mut().retain(
            |n| !matches!(n, Node::Section(s) if s.name.eq_ignore_ascii_case(name) && s.args.first().map(String::as_str) == Some(arg)),
        );
        before != self.items().len()
    }
}

impl Section {
    /// A new, empty section.
    pub fn new(name: &str, args: &[&str]) -> Self {
        Self {
            name: name.to_string(),
            args: args.iter().map(ToString::to_string).collect(),
            line: 0,
            items: Vec::new(),
            raw: None,
            close_raw: None,
        }
    }
}

impl Scope for Section {
    fn items(&self) -> &Vec<Node> {
        &self.items
    }
    fn items_mut(&mut self) -> &mut Vec<Node> {
        &mut self.items
    }
}

/// A parsed configuration file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conf {
    /// The top-level nodes.
    pub items: Vec<Node>,
}

impl Scope for Conf {
    fn items(&self) -> &Vec<Node> {
        &self.items
    }
    fn items_mut(&mut self) -> &mut Vec<Node> {
        &mut self.items
    }
}

/// Quote an argument if it needs it.
pub fn quote(a: &str) -> String {
    if a.is_empty() || a.contains(char::is_whitespace) || a.contains('"') {
        format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        a.to_string()
    }
}

fn line_of(name: &str, args: &[String], indent: usize) -> String {
    let mut s = " ".repeat(indent);
    s.push_str(name);
    for a in args {
        s.push(' ');
        s.push_str(&quote(a));
    }
    s.push('\n');
    s
}

fn print_nodes(items: &[Node], depth: usize, out: &mut String) {
    for n in items {
        match n {
            Node::Blank(t) | Node::Comment(t) => {
                out.push_str(t);
                if !t.ends_with('\n') {
                    out.push('\n');
                }
            }
            Node::Directive(d) => match &d.raw {
                Some(r) => {
                    out.push_str(r);
                    if !r.ends_with('\n') {
                        out.push('\n');
                    }
                }
                None => out.push_str(&line_of(&d.name, &d.args, depth * 3)),
            },
            Node::Section(s) => {
                if let Some(r) = &s.raw { out.push_str(r) } else {
                    let mut l = line_of(&s.name, &s.args, depth * 3);
                    l.insert(depth * 3, '<');
                    l.insert(l.len() - 1, '>');
                    out.push_str(&l);
                }
                print_nodes(&s.items, depth + 1, out);
                if let Some(r) = &s.close_raw {
                    out.push_str(r);
                    if !r.ends_with('\n') {
                        out.push('\n');
                    }
                } else {
                    out.push_str(&" ".repeat(depth * 3));
                    let _ = writeln!(out, "</{}>", s.name);
                }
            }
        }
    }
}

impl Conf {
    /// The file's text.
    pub fn print(&self) -> String {
        let mut out = String::new();
        print_nodes(&self.items, 0, &mut out);
        out
    }

    /// Lines in the printed file.
    pub fn line_count(&self) -> usize {
        self.print().lines().count()
    }
}

#[cfg(test)]
mod tests;
