// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A definition flattened for prompting elsewhere (QAP's
//! `DescribeCommand`, web pages): one node per parameter, element and
//! qualifier, in the order the prompter shows them, each child right
//! after its owner. Parameters `F4` shows at once come first, then the
//! ones under Additional Parameters (`PMTCTL`), then the constants it
//! never shows, so another prompter cannot disagree with this one.

use crate::def::{CmdDef, Parm, ParmType, PmtCtlRef};

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A parameter.
    Parameter,
    /// An element of a list.
    Element,
    /// A part of a qualified name.
    Qualifier,
}

/// A value's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `*NAME`, `*SNAME`, `*CNAME`, `*VARNAME`; a qualified name.
    Name,
    /// `*CHAR`, `*X`; an element list.
    Character,
    /// `*DEC`.
    Decimal,
    /// `*INT2`, `*INT4`, `*UINT2`, `*UINT4`.
    Integer,
    /// `*LGL`.
    Logical,
    /// `*DATE`.
    Date,
    /// `*TIME`.
    Time,
    /// `*GENERIC`.
    Generic,
    /// `*PNAME`.
    Path,
    /// `*CMD`, `*CMDSTR`.
    Command,
}

/// When the prompter shows a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Show {
    /// On the first display.
    Always,
    /// After `F10`, or when its `PMTCTL` condition holds.
    Additional,
    /// Never: a constant.
    Never,
}

/// One node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Its index.
    pub index: usize,
    /// Its owner's index.
    pub parent: Option<usize>,
    /// What it is.
    pub role: Role,
    /// The keyword; empty for elements and qualifiers.
    pub keyword: String,
    /// The prompt text.
    pub prompt: String,
    /// The value's kind.
    pub kind: Kind,
    /// Length (digits for `*DEC`).
    pub length: u32,
    /// Decimal positions.
    pub decimals: u8,
    /// `MIN` above 0.
    pub required: bool,
    /// `MIN`.
    pub min: u16,
    /// `MAX`.
    pub max: u16,
    /// `DFT`.
    pub default: Option<String>,
    /// `SPCVAL` and `SNGVAL` as typed.
    pub special: Vec<String>,
    /// `VALUES` when `RSTD(*YES)`.
    pub values: Vec<String>,
    /// `RANGE`.
    pub range: Option<(String, String)>,
    /// When it shows.
    pub show: Show,
}

fn kind_of(t: &ParmType) -> Kind {
    match t {
        ParmType::Name | ParmType::SName | ParmType::CName | ParmType::VarName | ParmType::Qual { .. } => Kind::Name,
        ParmType::Generic => Kind::Generic,
        ParmType::Dec => Kind::Decimal,
        ParmType::Lgl => Kind::Logical,
        ParmType::Int2 | ParmType::Int4 | ParmType::Uint2 | ParmType::Uint4 => Kind::Integer,
        ParmType::Date => Kind::Date,
        ParmType::Time => Kind::Time,
        ParmType::PName => Kind::Path,
        ParmType::Cmd | ParmType::CmdStr => Kind::Command,
        ParmType::Char | ParmType::X | ParmType::ZeroElem | ParmType::Null | ParmType::Elem { .. } => {
            Kind::Character
        }
    }
}

/// `PMTCTL(*PMTRQS)` and conditional parameters alike wait for `F10`
/// or their condition.
fn show_of(p: &Parm) -> Show {
    if p.constant.is_some() {
        Show::Never
    } else if matches!(p.pmtctl, Some(PmtCtlRef::PmtRqs | PmtCtlRef::Label { .. })) {
        Show::Additional
    } else {
        Show::Always
    }
}

/// A parameter, element or qualifier and what it holds, depth first.
fn push(out: &mut Vec<Node>, p: &Parm, role: Role, parent: Option<usize>, show: Show) {
    let index = out.len();
    let len = p.len.as_ref();
    out.push(Node {
        index,
        parent,
        role,
        keyword: if role == Role::Parameter { p.kwd.clone() } else { String::new() },
        prompt: p.prompt.as_ref().map(|(t, _)| t.clone()).unwrap_or_default(),
        kind: kind_of(&p.ty),
        length: len.map_or(0, |l| l.len),
        decimals: len.map_or(0, |l| l.dec),
        required: p.min > 0,
        min: p.min,
        max: p.max,
        default: p.dft.clone(),
        special: p.spcval.iter().chain(&p.sngval).map(|s| s.from.clone()).collect(),
        values: if p.rstd { p.values.clone() } else { Vec::new() },
        range: p.range.clone(),
        show,
    });
    let child = match p.ty {
        ParmType::Elem { .. } => Role::Element,
        ParmType::Qual { .. } => Role::Qualifier,
        _ => return,
    };
    for item in &p.items {
        push(out, item, child, Some(index), show);
    }
}

/// `def` flattened in the prompter's order.
#[must_use]
pub fn nodes(def: &CmdDef) -> Vec<Node> {
    let mut out = Vec::new();
    for want in [Show::Always, Show::Additional, Show::Never] {
        for p in def.parms.iter().filter(|p| show_of(p) == want) {
            push(&mut out, p, Role::Parameter, None, want);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::compile;

    const SRC: &str = "
             CMD        PROMPT('Test')
             PARM       KWD(OBJ) TYPE(Q1) MIN(1) PROMPT('Object')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          VALUES(*FILE *PGM) DFT(*FILE) PROMPT('Type')
             PARM       KWD(EXTRA) TYPE(*DEC) LEN(5 2) PMTCTL(*PMTRQS) +
                          RANGE(1 100) PROMPT('Extra')
             PARM       KWD(SIZE) TYPE(E1) PROMPT('Size')
             PARM       KWD(CONST) TYPE(*CHAR) LEN(1) CONSTANT('X')
 Q1:         QUAL       TYPE(*NAME) LEN(10)
             QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
 E1:         ELEM       TYPE(*INT4) PROMPT('Records')
             ELEM       TYPE(*INT4) PROMPT('Increment')
";

    #[test]
    fn the_prompters_order() {
        let def = compile("TEST", SRC).unwrap();
        let n = nodes(&def);
        let kw: Vec<(&str, Role, Option<usize>, Show)> =
            n.iter().map(|x| (x.keyword.as_str(), x.role, x.parent, x.show)).collect();
        assert_eq!(
            kw,
            [
                ("OBJ", Role::Parameter, None, Show::Always),
                ("", Role::Qualifier, Some(0), Show::Always),
                ("", Role::Qualifier, Some(0), Show::Always),
                ("TYPE", Role::Parameter, None, Show::Always),
                ("SIZE", Role::Parameter, None, Show::Always),
                ("", Role::Element, Some(4), Show::Always),
                ("", Role::Element, Some(4), Show::Always),
                ("EXTRA", Role::Parameter, None, Show::Additional),
                ("CONST", Role::Parameter, None, Show::Never),
            ]
        );
        assert!(n.iter().enumerate().all(|(i, x)| x.index == i));
        assert!(n[0].required && n[0].kind == Kind::Name);
        assert_eq!(n[2].prompt, "Library");
        assert_eq!(n[2].default.as_deref(), Some("*LIBL"));
        assert_eq!(n[2].special, ["*LIBL", "*CURLIB"]);
        assert_eq!(n[3].values, ["*FILE", "*PGM"]);
        assert_eq!((n[5].kind, n[5].prompt.as_str()), (Kind::Integer, "Records"));
        assert_eq!((n[7].kind, n[7].length, n[7].decimals), (Kind::Decimal, 5, 2));
        assert_eq!(n[7].range, Some(("1".into(), "100".into())));
    }
}
