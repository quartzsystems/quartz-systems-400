// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The driver: source, tokens, tree, model, checks, listing. [`Build`]
//! carries the diagnostics and the name references the checks resolve
//! at the end; the builders are `group`, `panel`, `text` and `attrs`.

mod attrs;
mod check;
mod group;
mod panel;
mod text;

use std::collections::{BTreeMap, BTreeSet};

use crate::diag::{Diagnostics, ids};
use crate::model::PanelGroup;
use crate::payload::{self, Payload};
use crate::source::{Imbed, Source};
use crate::{lex, listing, tree};

/// What the caller tells the compiler about the object.
#[derive(Clone, Debug, Default)]
pub struct CompileOpts {
    /// The panel group's name.
    pub name: Option<String>,
    /// Its library.
    pub library: Option<String>,
    /// `LIB/FILE(MBR)` for the listing header.
    pub source: Option<String>,
    /// Timestamp text for the listing header.
    pub timestamp: Option<String>,
}

/// The names other panel groups export, for `:IMPORT.` checks.
pub trait Imports {
    /// The exported names of a group (`LIB/NAME` or a name), or `None`
    /// when the compiler cannot know and should not check.
    fn exports(&self, pnlgrp: &str) -> Option<Vec<String>>;
}

/// No knowledge of other groups: imports are taken on trust.
pub struct NoImports;

impl Imports for NoImports {
    fn exports(&self, _pnlgrp: &str) -> Option<Vec<String>> {
        None
    }
}

/// Exports from a map, keyed by the group as the source names it.
#[derive(Default)]
pub struct MapImports {
    groups: BTreeMap<String, Vec<String>>,
}

impl MapImports {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a group's exports under the name sources use for it.
    pub fn add(&mut self, pnlgrp: &str, exports: Vec<String>) {
        self.groups.insert(pnlgrp.to_ascii_uppercase(), exports);
    }
}

impl Imports for MapImports {
    fn exports(&self, pnlgrp: &str) -> Option<Vec<String>> {
        self.groups.get(&pnlgrp.to_ascii_uppercase()).cloned()
    }
}

/// The outcome of one compilation.
#[derive(Debug)]
pub struct Compilation {
    /// The source, imbeds expanded.
    pub source: Source,
    /// The model, when a `:PNLGRP.` was found, errors or not.
    pub group: Option<PanelGroup>,
    /// Every message, in line order.
    pub diagnostics: Diagnostics,
    /// The listing.
    pub listing: String,
    opts: CompileOpts,
}

impl Compilation {
    /// No message of severity 20 or more: the object can be created.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.diagnostics.ok() && self.group.is_some()
    }

    /// The `*PNLGRP` payload, when the compilation is usable.
    #[must_use]
    pub fn payload(&self) -> Option<Vec<u8>> {
        if !self.ok() {
            return None;
        }
        let p = Payload {
            name: self.opts.name.clone().unwrap_or_default(),
            library: self.opts.library.clone().unwrap_or_default(),
            group: self.group.clone()?,
            source: self.source.lines().to_vec(),
        };
        Some(payload::encode(&p))
    }
}

/// Compile one source member.
#[must_use]
pub fn compile(
    text: &str,
    imbeds: &dyn Imbed,
    imports: &dyn Imports,
    opts: &CompileOpts,
) -> Compilation {
    let (source, mut diags) = Source::read(text, imbeds);
    let (tokens, d) = lex::lex(&source);
    diags.extend(d);
    let (root, d) = tree::build(tokens);
    diags.extend(d);
    let mut group = None;
    match root.named("PNLGRP").next() {
        None => diags.push(ids::no_pnlgrp()),
        Some(el) => {
            let mut b = Build::new();
            let g = b.group(el);
            check::run(&mut b, &g, imports);
            diags.extend(b.diags);
            group = Some(g);
        }
    }
    diags.sort();
    let listing = listing::render(&source, group.as_ref(), diags.items(), opts);
    Compilation {
        source,
        group,
        diagnostics: diags,
        listing,
        opts: opts.clone(),
    }
}

/// A name used somewhere, resolved at the end.
struct Ref {
    /// `Class`, `Variable`, `Help module`, ... as the message names it.
    what: &'static str,
    name: String,
    line: usize,
}

/// State shared by the builders.
struct Build {
    diags: Diagnostics,
    refs: Vec<Ref>,
    defined: BTreeSet<(&'static str, String)>,
}

impl Build {
    fn new() -> Self {
        Self {
            diags: Diagnostics::new(),
            refs: Vec::new(),
            defined: BTreeSet::new(),
        }
    }

    /// Record a reference to resolve later.
    fn refer(&mut self, what: &'static str, name: &str, line: usize) {
        self.refs.push(Ref {
            what,
            name: name.to_string(),
            line,
        });
    }

    /// Record an optional reference.
    fn refer_opt(&mut self, what: &'static str, name: Option<&String>, line: usize) {
        if let Some(n) = name {
            self.refer(what, n, line);
        }
    }

    /// Record a definition; a repeat is a diagnostic.
    fn define(&mut self, what: &'static str, name: &str, line: usize) {
        if !self.defined.insert((what, name.to_string())) {
            self.diags.push(ids::duplicate_name(line, what, name));
        }
    }
}

/// A line index as the model stores it.
fn line_u32(line: usize) -> u32 {
    u32::try_from(line).unwrap_or(u32::MAX)
}
