// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Help modules and the document tree that help text, information
//! areas and print headings share.

/// A `:HELP.` module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelpModule {
    /// Name.
    pub name: String,
    /// The title: the text after the tag, or `TITLE`.
    pub title: Vec<Span>,
    /// `WIDTH`.
    pub width: Option<u32>,
    /// `DEPTH`.
    pub depth: Option<u32>,
    /// The text.
    pub body: Document,
    /// `:ISCH.` index search topics.
    pub topics: Vec<SearchTopic>,
    /// Source line.
    pub line: u32,
}

/// Blocks in order.
pub type Document = Vec<Block>;

/// One block of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// `:H1.` to `:H4.`.
    Heading {
        /// 1 to 4.
        level: u8,
        /// The text.
        text: Vec<Span>,
    },
    /// `:P.`: a paragraph.
    Para(Vec<Span>),
    /// `:PC.`: a paragraph that continues the one before.
    ParaCont(Vec<Span>),
    /// `:XMP.`: lines kept as written, in example style.
    Example(Vec<Vec<Span>>),
    /// `:LINES.`: lines kept as written.
    Lines(Vec<Vec<Span>>),
    /// `:FIG.`: a figure, lines kept as written.
    Figure(Vec<Vec<Span>>),
    /// `:UL.`, `:OL.`, `:SL.`.
    List {
        /// Which list.
        kind: ListKind,
        /// `COMPACT`.
        compact: bool,
        /// The items.
        items: Vec<Document>,
    },
    /// `:DL.`.
    DefList {
        /// `COMPACT`.
        compact: bool,
        /// `:DTHD.` and `:DDHD.`.
        heads: Option<(Vec<Span>, Vec<Span>)>,
        /// The items.
        items: Vec<DefItem>,
    },
    /// `:PARML.`.
    ParmList(Vec<ParmItem>),
    /// `:NT.` or `:NOTE.`.
    Note(Document),
    /// `:IMHELP.`: another help module's text goes here.
    ImHelp(String),
}

/// Which list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    /// `:UL.`: bullets.
    Unordered,
    /// `:OL.`: numbers.
    Ordered,
    /// `:SL.`: plain.
    Simple,
}

/// A `:DT.` with its `:DD.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefItem {
    /// The term.
    pub term: Vec<Span>,
    /// The definition.
    pub def: Document,
}

/// A `:PT.` with its `:PD.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParmItem {
    /// The parameter.
    pub term: Vec<Span>,
    /// Its description.
    pub def: Document,
}

/// A run of inline text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Span {
    /// Literal text.
    Text(String),
    /// `&name.`: a dialog variable's value.
    Var(String),
    /// `:HPn.`: highlight `n`.
    Hp(u8, Vec<Span>),
    /// `:PK.`: a program keyword.
    Pk(Vec<Span>),
    /// `:PV.`: a program variable.
    Pv(Vec<Span>),
    /// `:RT.`: reverse text.
    Rt(Vec<Span>),
    /// `:LINK.`: text that performs an action when chosen.
    Link {
        /// `PERFORM`: the action.
        perform: String,
        /// The text.
        text: Vec<Span>,
    },
}

impl Span {
    /// The plain text of spans, highlights dropped and variables as `&name.`.
    #[must_use]
    pub fn plain(spans: &[Span]) -> String {
        let mut out = String::new();
        for s in spans {
            match s {
                Self::Text(t) => out.push_str(t),
                Self::Var(v) => {
                    out.push('&');
                    out.push_str(v);
                    out.push('.');
                }
                Self::Hp(_, inner) | Self::Pk(inner) | Self::Pv(inner) | Self::Rt(inner) => {
                    out.push_str(&Self::plain(inner));
                }
                Self::Link { text, .. } => out.push_str(&Self::plain(text)),
            }
        }
        out
    }

    /// Every variable the spans reference.
    #[must_use]
    pub fn variables(spans: &[Span]) -> Vec<&str> {
        let mut out = Vec::new();
        for s in spans {
            match s {
                Self::Text(_) => {}
                Self::Var(v) => out.push(v.as_str()),
                Self::Hp(_, inner) | Self::Pk(inner) | Self::Pv(inner) | Self::Rt(inner) => {
                    out.extend(Self::variables(inner));
                }
                Self::Link { text, .. } => out.extend(Self::variables(text)),
            }
        }
        out
    }
}

/// An `:ISCH.` topic of a help module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchTopic {
    /// `ROOTS`: the root words.
    pub roots: Vec<String>,
    /// The topic text.
    pub text: Vec<Span>,
}
