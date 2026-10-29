// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The syntax tree. Every node that can fail at run time carries a
//! [`Span`] so messages can name the line.

use std::sync::{Arc, OnceLock};

/// A source position: byte-free character offsets and the line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    /// First character.
    pub start: u32,
    /// One past the last character.
    pub end: u32,
    /// Line of the first character, from 1.
    pub line: u32,
}

/// A compound list: and-or lists separated by `;`, `&` or newlines.
#[derive(Clone, Debug, Default)]
pub struct List {
    /// The items in order.
    pub items: Vec<Item>,
}

/// One and-or list with its terminator.
#[derive(Clone, Debug)]
pub struct Item {
    /// The and-or list.
    pub and_or: AndOr,
    /// Ended with `&`.
    pub background: bool,
    /// The source text, for `jobs`.
    pub text: String,
}

/// Pipelines joined by `&&` and `||`.
#[derive(Clone, Debug)]
pub struct AndOr {
    /// The first pipeline.
    pub first: Pipeline,
    /// The rest with their connectors.
    pub rest: Vec<(Connector, Pipeline)>,
}

/// `&&` or `||`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connector {
    /// `&&`.
    And,
    /// `||`.
    Or,
}

/// Commands joined by `|`, optionally negated with `!`.
#[derive(Clone, Debug)]
pub struct Pipeline {
    /// Started with `!`.
    pub negate: bool,
    /// The commands, at least one.
    pub commands: Vec<Command>,
    /// Where it starts.
    pub span: Span,
}

/// One command of a pipeline.
#[derive(Clone, Debug)]
pub enum Command {
    /// Assignments, words and redirections.
    Simple(SimpleCommand),
    /// A compound command and its redirections.
    Compound(Compound, Vec<Redirect>),
    /// `name() compound-command`.
    Function(Arc<FunctionDef>),
}

/// A simple command.
#[derive(Clone, Debug, Default)]
pub struct SimpleCommand {
    /// Leading `NAME=value` words.
    pub assigns: Vec<Assign>,
    /// The command name and arguments.
    pub words: Vec<Word>,
    /// Redirections in source order.
    pub redirs: Vec<Redirect>,
    /// Where it starts.
    pub span: Span,
}

/// `NAME=value`.
#[derive(Clone, Debug)]
pub struct Assign {
    /// The variable.
    pub name: String,
    /// The value word (may be empty).
    pub value: Word,
}

/// A compound command.
#[derive(Clone, Debug)]
pub enum Compound {
    /// `{ list; }`.
    Brace(List),
    /// `( list )`.
    Subshell(List),
    /// `if` with its `elif` arms and optional `else`.
    If {
        /// Condition and body pairs: the `if` then each `elif`.
        arms: Vec<(List, List)>,
        /// The `else` body.
        else_body: Option<List>,
    },
    /// `while` or `until`.
    Loop {
        /// `until` rather than `while`.
        until: bool,
        /// The condition.
        cond: List,
        /// The body.
        body: List,
    },
    /// `for name [in words]; do body; done`.
    For {
        /// The loop variable.
        var: String,
        /// The words; `None` iterates over `"$@"`.
        words: Option<Vec<Word>>,
        /// The body.
        body: List,
    },
    /// `case word in arms esac`.
    Case {
        /// The word matched.
        subject: Word,
        /// The arms in order.
        arms: Vec<CaseArm>,
    },
}

/// One `pattern | pattern ) list ;;` arm.
#[derive(Clone, Debug)]
pub struct CaseArm {
    /// The patterns.
    pub patterns: Vec<Word>,
    /// The body.
    pub body: List,
}

/// A function definition.
#[derive(Debug)]
pub struct FunctionDef {
    /// The name.
    pub name: String,
    /// The body.
    pub body: Compound,
    /// Redirections applied at every call.
    pub redirs: Vec<Redirect>,
    /// The source text of the definition.
    pub text: String,
    /// Where it starts.
    pub span: Span,
}

/// A redirection.
#[derive(Clone, Debug)]
pub struct Redirect {
    /// The explicit descriptor number, if any.
    pub fd: Option<u32>,
    /// The operator.
    pub op: RedirOp,
    /// The file word, descriptor word or here-document.
    pub target: RedirTarget,
    /// Where it starts.
    pub span: Span,
}

/// A redirection operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedirOp {
    /// `<`.
    In,
    /// `>`.
    Out,
    /// `>|`.
    Clobber,
    /// `>>`.
    Append,
    /// `<>`.
    ReadWrite,
    /// `<&`.
    DupIn,
    /// `>&`.
    DupOut,
    /// `<<` (`strip_tabs` false) or `<<-`.
    Here {
        /// `<<-`: strip leading tabs.
        strip_tabs: bool,
    },
}

impl RedirOp {
    /// The descriptor used when none is written.
    pub fn default_fd(self) -> u32 {
        match self {
            Self::In | Self::ReadWrite | Self::DupIn | Self::Here { .. } => 0,
            Self::Out | Self::Clobber | Self::Append | Self::DupOut => 1,
        }
    }
}

/// A redirection's operand.
#[derive(Clone, Debug)]
pub enum RedirTarget {
    /// A file name or descriptor word.
    Word(Word),
    /// A here-document body, filled when the line ends.
    Here(Arc<OnceLock<HereDoc>>),
}

/// A here-document body.
#[derive(Clone, Debug)]
pub struct HereDoc {
    /// The body; unexpanded bodies are one quoted part.
    pub body: Word,
}

/// A word: parts concatenated before field splitting.
#[derive(Clone, Debug, Default)]
pub struct Word {
    /// The parts.
    pub parts: Vec<WordPart>,
    /// Where it is.
    pub span: Span,
}

/// One piece of a word.
#[derive(Clone, Debug)]
pub enum WordPart {
    /// Unquoted text.
    Lit(String),
    /// Single-quoted or backslash-escaped text.
    Quoted(String),
    /// A double-quoted string; its `Lit` parts are quoted text.
    Double(Vec<WordPart>),
    /// `$name` or `${...}`.
    Param(Box<Param>),
    /// `$(...)` or backquotes.
    Command(Arc<List>),
    /// `$((...))`.
    Arith(Word),
}

/// A parameter expansion.
#[derive(Clone, Debug)]
pub struct Param {
    /// The parameter.
    pub name: ParamName,
    /// What to do with it.
    pub op: ParamOp,
}

/// Which parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamName {
    /// A variable.
    Var(String),
    /// `$1`, `${10}`.
    Positional(usize),
    /// `$0`, `$?`, `$$`, `$!`, `$#`, `$-`, `$@`, `$*`.
    Special(char),
}

/// The four test forms of `${name<op>word}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestOp {
    /// `-`: use a default.
    Default,
    /// `=`: assign a default.
    Assign,
    /// `?`: fail when unset.
    Error,
    /// `+`: use an alternative.
    Alternate,
}

/// The operation of a parameter expansion.
#[derive(Clone, Debug)]
pub enum ParamOp {
    /// `$x`, `${x}`.
    Plain,
    /// `${#x}`.
    Length,
    /// `${x-w}` and friends; `colon` also treats null as unset.
    Test {
        /// Which test.
        op: TestOp,
        /// Written with `:`.
        colon: bool,
        /// The word.
        word: Word,
    },
    /// `${x%w}`, `${x%%w}`, `${x#w}`, `${x##w}`.
    Trim {
        /// `%` forms.
        suffix: bool,
        /// Doubled forms.
        longest: bool,
        /// The pattern.
        pattern: Word,
    },
}

impl Word {
    /// The text when the word is one unquoted literal.
    pub fn as_literal(&self) -> Option<&str> {
        match self.parts.as_slice() {
            [WordPart::Lit(s)] => Some(s),
            _ => None,
        }
    }

    /// Any quoting anywhere in the word.
    pub fn has_quotes(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, WordPart::Quoted(_) | WordPart::Double(_)))
    }

    /// The word with quotes removed and no expansions, as a
    /// here-document delimiter is read.
    pub fn unquoted_text(&self) -> String {
        fn walk(parts: &[WordPart], out: &mut String) {
            for p in parts {
                match p {
                    WordPart::Lit(s) | WordPart::Quoted(s) => out.push_str(s),
                    WordPart::Double(inner) => walk(inner, out),
                    WordPart::Param(p) => {
                        out.push('$');
                        match &p.name {
                            ParamName::Var(n) => out.push_str(n),
                            ParamName::Positional(n) => out.push_str(&n.to_string()),
                            ParamName::Special(c) => out.push(*c),
                        }
                    }
                    WordPart::Command(_) => out.push_str("$(...)"),
                    WordPart::Arith(_) => out.push_str("$((...))"),
                }
            }
        }
        let mut out = String::new();
        walk(&self.parts, &mut out);
        out
    }
}
