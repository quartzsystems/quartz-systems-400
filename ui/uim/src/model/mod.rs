// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The model a renderer walks: a [`PanelGroup`] with its classes,
//! variables, conditions, key lists, panels, print panels and help
//! modules, every attribute resolved and every reference checked.
//! Names are upper-case. The payload keeps the model as it is here.

pub mod class;
pub mod cond;
pub mod help;
pub mod keyl;
pub mod panel;
pub mod var;

pub use class::{BaseType, Case, Check, Class, Shift, TranslateItem};
pub use cond::{CmpOp, Condition, Expr, TruthTable, TtValue};
pub use help::{Block, DefItem, Document, HelpModule, ListKind, ParmItem, SearchTopic, Span};
pub use keyl::{Action, Key, KeyItem, KeyList};
pub use panel::{
    Area, CmdLine, CmdLineSize, DataArea, DataChoice, DataConst, DataExtender, DataField,
    DataGroup, DataNode, DataSelect, Depth, Display, InfoArea, Layout, ListAction, ListArea,
    ListColNode, ListColumn, ListGroup, ListView, MenuArea, MenuItem, MenuNode, MenuOption, Panel,
    PrintPanel, Scroll, Separator, TextArea, Usage, VarUpd,
};
pub use var::{ListDef, VarRecord, Variable};

/// A whole panel group.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PanelGroup {
    /// `SCHIDX`: the search index the help modules go to.
    pub schidx: Option<String>,
    /// `DFTMSGF`: the message file for `MSG` identifiers, `LIB/NAME` or a name.
    pub dftmsgf: Option<String>,
    /// `SUBMSGF`: the message file for substitution.
    pub submsgf: Option<String>,
    /// `TXTCHRID`: the character identifier of the text.
    pub txtchrid: Option<String>,
    /// `HLPSHELF`: the help shelf.
    pub hlpshelf: Option<String>,
    /// `:KEYLSHLF.` names.
    pub keylshlf: Vec<String>,
    /// The `:COPYR.` text.
    pub copyright: Option<String>,
    /// `:IMPORT.` declarations.
    pub imports: Vec<Import>,
    /// Classes in source order.
    pub classes: Vec<Class>,
    /// Variables in source order.
    pub variables: Vec<Variable>,
    /// Variable records.
    pub var_records: Vec<VarRecord>,
    /// List definitions.
    pub list_defs: Vec<ListDef>,
    /// Conditions.
    pub conditions: Vec<Condition>,
    /// Truth tables.
    pub truth_tables: Vec<TruthTable>,
    /// Key lists.
    pub key_lists: Vec<KeyList>,
    /// Panels.
    pub panels: Vec<Panel>,
    /// Print panels.
    pub print_panels: Vec<PrintPanel>,
    /// Help modules.
    pub helps: Vec<HelpModule>,
    /// `:ISCHSYN.` synonym lists for index search.
    pub synonyms: Vec<Synonyms>,
}

/// A name taken from another panel group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    /// The name, or `*` for everything the group exports.
    pub name: String,
    /// The other group, `LIB/NAME` or a name.
    pub pnlgrp: String,
    /// Source line.
    pub line: u32,
}

/// Words that search as one root word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Synonyms {
    /// The root.
    pub root: String,
    /// The synonyms.
    pub words: Vec<String>,
}

impl PanelGroup {
    /// A class by name.
    #[must_use]
    pub fn class(&self, name: &str) -> Option<&Class> {
        self.classes.iter().find(|c| c.name == name)
    }

    /// A variable by name.
    #[must_use]
    pub fn variable(&self, name: &str) -> Option<&Variable> {
        self.variables.iter().find(|v| v.name == name)
    }

    /// A condition by name.
    #[must_use]
    pub fn condition(&self, name: &str) -> Option<&Condition> {
        self.conditions.iter().find(|c| c.name == name)
    }

    /// A key list by name.
    #[must_use]
    pub fn key_list(&self, name: &str) -> Option<&KeyList> {
        self.key_lists.iter().find(|k| k.name == name)
    }

    /// A list definition by name.
    #[must_use]
    pub fn list_def(&self, name: &str) -> Option<&ListDef> {
        self.list_defs.iter().find(|l| l.name == name)
    }

    /// A panel by name.
    #[must_use]
    pub fn panel(&self, name: &str) -> Option<&Panel> {
        self.panels.iter().find(|p| p.name == name)
    }

    /// A help module by name.
    #[must_use]
    pub fn help(&self, name: &str) -> Option<&HelpModule> {
        self.helps.iter().find(|h| h.name == name)
    }

    /// The names another group may import: help modules and panels.
    #[must_use]
    pub fn exports(&self) -> Vec<String> {
        self.helps
            .iter()
            .map(|h| h.name.clone())
            .chain(self.panels.iter().map(|p| p.name.clone()))
            .collect()
    }

    /// Whether a help module name resolves here or through an import.
    #[must_use]
    pub fn has_help(&self, name: &str) -> bool {
        self.help(name).is_some() || self.imports.iter().any(|i| i.name == name || i.name == "*")
    }
}
