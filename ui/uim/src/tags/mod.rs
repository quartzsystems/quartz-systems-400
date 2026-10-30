// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The tag table: every tag the compiler knows, how it closes, which
//! attributes it takes and where it may appear. `docs/UIM.md` lists the
//! same table as the support matrix.

/// How an element closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Closed by its `:Ename.` tag.
    Container,
    /// Closed by the next tag its parent takes, or by its parent's end.
    Leaf,
    /// No content.
    Empty,
    /// A highlight inside text, closed by its `:Ename.` tag.
    Inline,
}

/// One tag.
#[derive(Debug, PartialEq, Eq)]
pub struct TagDef {
    /// Upper-case name.
    pub name: &'static str,
    /// How it closes.
    pub kind: Kind,
    /// Attributes it takes.
    pub attrs: &'static [&'static str],
    /// Attributes it needs.
    pub required: &'static [&'static str],
    /// Tags it may appear under; `*` for any element that takes text.
    pub parents: &'static [&'static str],
    /// Takes text (a title or content).
    pub text: bool,
}

/// Elements whose content is help-style blocks.
pub const DOC: &[&str] = &[
    "HELP", "INFO", "TEXT", "NT", "NOTE", "LI", "DD", "PD", "FIG", "PRTHEAD", "PRTTRAIL",
];

/// Where a `:DATAI.`-level tag may go.
const DATA_HOST: &[&str] = &["DATA", "DATAGRP"];

macro_rules! tag {
    ($name:literal, $kind:ident, [$($a:literal),*], [$($r:literal),*], $parents:expr, $text:expr) => {
        TagDef {
            name: $name,
            kind: Kind::$kind,
            attrs: &[$($a),*],
            required: &[$($r),*],
            parents: $parents,
            text: $text,
        }
    };
}

/// Every tag, in reference order.
pub static TAGS: &[TagDef] = &[
    // The group.
    tag!(
        "PNLGRP",
        Container,
        ["SCHIDX", "DFTMSGF", "SUBMSGF", "TXTCHRID", "HLPSHELF"],
        [],
        &["ROOT"],
        false
    ),
    tag!("COPYR", Leaf, [], [], &["PNLGRP"], true),
    tag!(
        "IMPORT",
        Empty,
        ["NAME", "PNLGRP"],
        ["NAME", "PNLGRP"],
        &["PNLGRP"],
        false
    ),
    tag!("HLPSHELF", Empty, ["NAME"], ["NAME"], &["PNLGRP"], false),
    tag!("KEYLSHLF", Empty, ["NAME"], ["NAME"], &["PNLGRP"], false),
    tag!(
        "DUMMY",
        Empty,
        [],
        [],
        &[
            "PNLGRP", "HELP", "INFO", "TEXT", "NT", "NOTE", "LI", "DD", "PD", "FIG", "PRTHEAD",
            "PRTTRAIL"
        ],
        false
    ),
    // Classes and variables.
    tag!(
        "CLASS",
        Container,
        [
            "NAME", "BASETYPE", "WIDTH", "SHIFT", "CASE", "CHRID", "ECPARAM"
        ],
        ["NAME", "BASETYPE"],
        &["PNLGRP"],
        false
    ),
    tag!("TL", Container, ["MSG"], [], &["CLASS"], false),
    tag!("TI", Leaf, ["VALUE"], ["VALUE"], &["TL"], true),
    tag!(
        "CHECK",
        Empty,
        ["TYPE", "MSG", "VAR", "MSGID"],
        ["TYPE"],
        &["CLASS"],
        false
    ),
    tag!(
        "VAR",
        Empty,
        ["NAME", "CLASS", "VALUE", "SIZE"],
        ["NAME", "CLASS"],
        &["PNLGRP"],
        false
    ),
    tag!(
        "VARRCD",
        Empty,
        ["NAME", "VARS", "NOGET", "NOPUT", "EXTVARS"],
        ["NAME", "VARS"],
        &["PNLGRP"],
        false
    ),
    tag!(
        "LISTDEF",
        Empty,
        ["NAME", "VARS", "MSGID"],
        ["NAME", "VARS"],
        &["PNLGRP"],
        false
    ),
    tag!(
        "COND",
        Empty,
        ["NAME", "EXPR", "MSG"],
        ["NAME", "EXPR"],
        &["PNLGRP"],
        false
    ),
    tag!(
        "TT",
        Container,
        ["NAME", "CONDS"],
        ["NAME", "CONDS"],
        &["PNLGRP"],
        false
    ),
    tag!("TTROW", Empty, ["VALUES"], ["VALUES"], &["TT"], false),
    // Key lists.
    tag!(
        "KEYL",
        Container,
        ["NAME", "HELP"],
        ["NAME"],
        &["PNLGRP"],
        false
    ),
    tag!(
        "KEYI",
        Leaf,
        ["KEY", "HELP", "ACTION", "VARUPD", "COND"],
        ["KEY", "ACTION"],
        &["KEYL"],
        true
    ),
    // Panels and their areas.
    tag!(
        "PANEL",
        Container,
        [
            "NAME",
            "HELP",
            "KEYL",
            "ENTER",
            "DEPTH",
            "WIDTH",
            "PANELID",
            "TOPSEP",
            "BOTSEP",
            "TOPINST",
            "CSRVAR",
            "CSRLST",
            "HELPTITLE",
            "PRVPNL",
            "NXTPNL",
            "ENBGUI",
            "KEYLSHLF"
        ],
        ["NAME", "KEYL"],
        &["PNLGRP"],
        true
    ),
    tag!("TOPINST", Leaf, [], [], &["PANEL", "PRTPNL"], true),
    tag!("BOTINST", Leaf, [], [], &["PANEL", "PRTPNL"], true),
    tag!("CMDLINE", Leaf, ["SIZE", "NAME"], [], &["PANEL"], true),
    tag!(
        "DATA",
        Container,
        [
            "DEPTH", "LAYOUT", "BOTSEP", "COMPACT", "SCROLL", "TOPINST", "HELP"
        ],
        ["DEPTH"],
        &["PANEL", "PRTPNL"],
        true
    ),
    tag!(
        "DATAGRP",
        Container,
        ["GRPSEP", "HELP", "COMPACT", "NOGUI"],
        [],
        DATA_HOST,
        true
    ),
    tag!("DATACOL", Empty, ["WIDTH"], ["WIDTH"], DATA_HOST, false),
    tag!(
        "DATAI",
        Leaf,
        [
            "VAR", "HELP", "USAGE", "NAME", "BASETYPE", "AUTOSKIP", "DISPLAY", "NOCHECK",
            "REQUIRED", "MAXWIDTH", "MINWIDTH", "SHIFT", "CSRLOC", "NOPUTOVR"
        ],
        ["VAR", "USAGE"],
        DATA_HOST,
        true
    ),
    tag!(
        "DATAIX",
        Empty,
        [
            "VAR", "USAGE", "NAME", "HELP", "MAXWIDTH", "MINWIDTH", "DISPLAY", "AUTOSKIP",
            "NOCHECK", "REQUIRED", "SHIFT", "CSRLOC", "NOPUTOVR"
        ],
        ["VAR", "USAGE"],
        DATA_HOST,
        false
    ),
    tag!("DATAIXT", Leaf, [], [], DATA_HOST, true),
    tag!(
        "DATAC",
        Leaf,
        ["NAME", "HELP", "DISPLAY", "WIDTH"],
        [],
        DATA_HOST,
        true
    ),
    tag!(
        "DATASLT",
        Container,
        [
            "VAR", "HELP", "USAGE", "NAME", "DISPLAY", "REQUIRED", "CSRLOC", "SLTTYPE", "NOCHECK"
        ],
        ["VAR"],
        DATA_HOST,
        true
    ),
    tag!(
        "DATASLTC",
        Leaf,
        ["VALUE", "HELP", "COND", "DISPLAY"],
        ["VALUE"],
        &["DATASLT"],
        true
    ),
    tag!(
        "LIST",
        Container,
        [
            "DEPTH", "LISTDEF", "VIEWS", "ACTOR", "EXTACT", "MAXHEAD", "PARMS", "SCROLL",
            "BOTINST", "ENTER", "LAYOUT", "SELECT", "ACTLEN", "HELP", "COMPACT", "HEADSEP"
        ],
        ["DEPTH", "LISTDEF"],
        &["PANEL", "PRTPNL"],
        true
    ),
    tag!(
        "LISTACT",
        Leaf,
        [
            "OPTION", "ENTER", "HELP", "CONFIRM", "USREXIT", "ACTFOR", "RETURN", "VARUPD",
            "ENTERMSG"
        ],
        [],
        &["LIST"],
        true
    ),
    tag!(
        "LISTCOL",
        Leaf,
        [
            "VAR", "USAGE", "MAXWIDTH", "HELP", "NAME", "DISPLAY", "DSPCOL", "NOGUI", "AUTOSKIP",
            "MINWIDTH"
        ],
        ["VAR", "USAGE"],
        &["LIST", "LISTGRP"],
        true
    ),
    tag!(
        "LISTGRP",
        Container,
        ["COL", "HELP", "NOGUI"],
        [],
        &["LIST"],
        true
    ),
    tag!(
        "LISTVIEW",
        Empty,
        ["COLS", "LAYOUT"],
        ["COLS"],
        &["LIST"],
        false
    ),
    tag!(
        "MENU",
        Container,
        ["DEPTH", "SCROLL", "BOTSEP", "TOPSEP", "OPTIONS", "HELP"],
        ["DEPTH"],
        &["PANEL"],
        true
    ),
    tag!("MENUGRP", Container, ["HELP"], [], &["MENU"], true),
    tag!(
        "MENUI",
        Leaf,
        ["OPTION", "ACTION", "HELP", "COND", "VARUPD"],
        ["OPTION", "ACTION"],
        &["MENU", "MENUGRP"],
        true
    ),
    tag!(
        "INFO",
        Container,
        ["DEPTH", "SCROLL", "BOTSEP", "HELP"],
        ["DEPTH"],
        &["PANEL", "PRTPNL"],
        true
    ),
    tag!(
        "TEXT",
        Container,
        ["DEPTH", "BOTSEP"],
        [],
        &["PANEL", "PRTPNL"],
        true
    ),
    // Print panels.
    tag!(
        "PRTPNL",
        Container,
        ["NAME", "HELP", "WIDTH", "DEPTH", "TOPSEP", "BOTSEP"],
        ["NAME"],
        &["PNLGRP"],
        true
    ),
    tag!("PRTHEAD", Container, [], [], &["PRTPNL"], true),
    tag!("PRTTRAIL", Container, [], [], &["PRTPNL"], true),
    // Help modules and the document tags.
    tag!(
        "HELP",
        Container,
        ["NAME", "TITLE", "WIDTH", "DEPTH"],
        ["NAME"],
        &["PNLGRP"],
        true
    ),
    tag!("H1", Leaf, [], [], DOC, true),
    tag!("H2", Leaf, [], [], DOC, true),
    tag!("H3", Leaf, [], [], DOC, true),
    tag!("H4", Leaf, [], [], DOC, true),
    tag!("P", Leaf, [], [], DOC, true),
    tag!("PC", Leaf, [], [], DOC, true),
    tag!("XMP", Container, [], [], DOC, true),
    tag!("LINES", Container, [], [], DOC, true),
    tag!("FIG", Container, [], [], DOC, true),
    tag!("UL", Container, ["COMPACT"], [], DOC, false),
    tag!("OL", Container, ["COMPACT"], [], DOC, false),
    tag!("SL", Container, ["COMPACT"], [], DOC, false),
    tag!("LI", Leaf, [], [], &["UL", "OL", "SL"], true),
    tag!(
        "DL",
        Container,
        ["COMPACT", "TSIZE", "BREAK"],
        [],
        DOC,
        false
    ),
    tag!("DTHD", Leaf, [], [], &["DL"], true),
    tag!("DDHD", Leaf, [], [], &["DL"], true),
    tag!("DT", Leaf, [], [], &["DL"], true),
    tag!("DD", Leaf, [], [], &["DL"], true),
    tag!("PARML", Container, [], [], DOC, false),
    tag!("PT", Leaf, [], [], &["PARML"], true),
    tag!("PD", Leaf, [], [], &["PARML"], true),
    tag!("NT", Container, [], [], DOC, true),
    tag!("NOTE", Container, [], [], DOC, true),
    tag!("IMHELP", Empty, ["NAME"], ["NAME"], DOC, false),
    tag!("ISCH", Leaf, ["ROOTS"], ["ROOTS"], &["HELP"], true),
    tag!("ISCHSYN", Container, ["ROOT"], ["ROOT"], &["PNLGRP"], true),
    // Highlights inside text.
    tag!("HP0", Inline, [], [], &["*"], true),
    tag!("HP1", Inline, [], [], &["*"], true),
    tag!("HP2", Inline, [], [], &["*"], true),
    tag!("HP3", Inline, [], [], &["*"], true),
    tag!("HP4", Inline, [], [], &["*"], true),
    tag!("HP5", Inline, [], [], &["*"], true),
    tag!("HP6", Inline, [], [], &["*"], true),
    tag!("HP7", Inline, [], [], &["*"], true),
    tag!("HP8", Inline, [], [], &["*"], true),
    tag!("HP9", Inline, [], [], &["*"], true),
    tag!("PK", Inline, ["DEF"], [], &["*"], true),
    tag!("PV", Inline, [], [], &["*"], true),
    tag!("RT", Inline, [], [], &["*"], true),
    tag!("LINK", Inline, ["PERFORM"], ["PERFORM"], &["*"], true),
];

/// The definition of a start tag.
#[must_use]
pub fn lookup(name: &str) -> Option<&'static TagDef> {
    TAGS.iter().find(|t| t.name == name)
}

/// The element an end tag closes: `EPANEL` closes `PANEL`.
#[must_use]
pub fn end_of(name: &str) -> Option<&'static TagDef> {
    let inner = name.strip_prefix('E')?;
    lookup(inner).filter(|t| matches!(t.kind, Kind::Container | Kind::Inline))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_ends_resolve() {
        for (i, t) in TAGS.iter().enumerate() {
            assert!(
                !TAGS[..i].iter().any(|u| u.name == t.name),
                "{} twice",
                t.name
            );
            assert_eq!(t.name, t.name.to_ascii_uppercase());
            assert!(lookup(t.name).is_some());
        }
        assert_eq!(end_of("EPANEL").unwrap().name, "PANEL");
        assert_eq!(end_of("EHP2").unwrap().name, "HP2");
        assert!(end_of("EP").is_none());
        assert!(end_of("EVAR").is_none());
        assert!(lookup("NOPE").is_none());
    }
}
