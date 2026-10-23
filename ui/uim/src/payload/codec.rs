// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`Wire`] for every model type: structs field by field in declaration
//! order, enums as a one-byte tag and the variant's fields. The tags
//! are part of the payload format and never change meaning.

use ipc::{Dec, Wire, WireError, wire_enum, wire_struct};

use crate::model::{
    Action, Area, BaseType, Block, Case, Check, Class, CmdLine, CmdLineSize, CmpOp, Condition,
    DataArea, DataChoice, DataConst, DataExtender, DataField, DataGroup, DataNode, DataSelect,
    DefItem, Depth, Display, Expr, HelpModule, InfoArea, Key, KeyItem, KeyList, Layout, ListAction,
    ListArea, ListColNode, ListColumn, ListDef, ListGroup, ListKind, ListView, MenuArea, MenuItem,
    MenuNode, MenuOption, Panel, PanelGroup, ParmItem, PrintPanel, Scroll, SearchTopic, Separator,
    Shift, Span, TextArea, TranslateItem, TruthTable, TtValue, Usage, VarRecord, VarUpd, Variable,
};
use crate::model::{Import, Synonyms};
use crate::source::SourceLine;

wire_struct!(SourceLine { seq, text, imbed });

wire_struct!(PanelGroup {
    schidx,
    dftmsgf,
    submsgf,
    txtchrid,
    hlpshelf,
    keylshlf,
    copyright,
    imports,
    classes,
    variables,
    var_records,
    list_defs,
    conditions,
    truth_tables,
    key_lists,
    panels,
    print_panels,
    helps,
    synonyms
});
wire_struct!(Import { name, pnlgrp, line });
wire_struct!(Synonyms { root, words });

wire_struct!(Class {
    name,
    basetype,
    width,
    shift,
    case,
    chrid,
    ecparam,
    translations,
    tl_msg,
    checks,
    line
});
wire_struct!(TranslateItem { value, text });
wire_struct!(Check {
    kind,
    msg,
    var,
    msgid
});
wire_enum!(Shift {
    0 => None, 1 => Alpha, 2 => Numeric, 3 => DigitsOnly, 4 => NumericOnly, 5 => Signed,
    6 => Katakana
});
wire_enum!(Case { 0 => Mixed, 1 => Upper });

wire_struct!(Variable {
    name,
    class,
    value,
    size,
    line
});
wire_struct!(VarRecord {
    name,
    vars,
    noget,
    noput,
    extvars,
    line
});
wire_struct!(ListDef {
    name,
    vars,
    msgid,
    line
});

wire_struct!(Condition {
    name,
    expr,
    msg,
    line
});
wire_enum!(CmpOp { 0 => Eq, 1 => Ne, 2 => Gt, 3 => Lt, 4 => Ge, 5 => Le });
wire_struct!(TruthTable {
    name,
    conds,
    rows,
    line
});
wire_enum!(TtValue { 0 => True, 1 => False, 2 => Any });

wire_struct!(KeyList {
    name,
    help,
    keys,
    line
});
wire_struct!(KeyItem {
    key,
    help,
    action,
    varupd,
    cond,
    text,
    line
});

wire_struct!(Panel {
    name,
    title,
    help,
    keyl,
    enter,
    depth,
    width,
    panelid,
    topsep,
    botsep,
    topinst,
    csrvar,
    csrlst,
    helptitle,
    prvpnl,
    nxtpnl,
    enbgui,
    keylshlf,
    areas,
    line
});
wire_struct!(PrintPanel {
    name,
    title,
    help,
    width,
    depth,
    topsep,
    botsep,
    head,
    trail,
    areas,
    line
});
wire_enum!(Separator { 0 => Space, 1 => Rule, 2 => None });
wire_enum!(Layout { 0 => One, 1 => Two, 2 => Three });
wire_enum!(Scroll { 0 => Yes, 1 => No });
wire_enum!(Usage { 0 => Input, 1 => Output, 2 => Inout });
wire_enum!(Display { 0 => Yes, 1 => No });
wire_enum!(VarUpd { 0 => Yes, 1 => No });
wire_enum!(CmdLineSize { 0 => Short, 1 => Long });
wire_enum!(ListKind { 0 => Unordered, 1 => Ordered, 2 => Simple });

wire_struct!(DataArea {
    title,
    depth,
    layout,
    botsep,
    compact,
    scroll,
    topinst,
    help,
    items,
    line
});
wire_struct!(DataGroup {
    title,
    grpsep,
    help,
    compact,
    nogui,
    items,
    line
});
wire_struct!(DataField {
    var,
    prompt,
    help,
    usage,
    name,
    basetype,
    autoskip,
    display,
    nocheck,
    required,
    maxwidth,
    minwidth,
    shift,
    csrloc,
    noputovr,
    extenders,
    line
});
wire_struct!(DataExtender {
    var,
    usage,
    name,
    help,
    maxwidth,
    minwidth,
    display,
    text,
    line
});
wire_struct!(DataConst {
    text,
    name,
    help,
    display,
    width,
    line
});
wire_struct!(DataSelect {
    var,
    prompt,
    help,
    usage,
    name,
    display,
    required,
    csrloc,
    slttype,
    nocheck,
    choices,
    line
});
wire_struct!(DataChoice {
    value,
    help,
    cond,
    display,
    text,
    line
});

wire_struct!(ListArea {
    title,
    depth,
    listdef,
    views,
    actor,
    extact,
    maxhead,
    parms,
    scroll,
    botinst,
    enter,
    layout,
    select,
    actlen,
    help,
    actions,
    columns,
    list_views,
    line
});
wire_struct!(ListAction {
    option,
    enter,
    help,
    confirm,
    usrexit,
    actfor,
    ret,
    varupd,
    entermsg,
    text,
    line
});
wire_struct!(ListGroup {
    heading,
    col,
    help,
    nogui,
    columns,
    line
});
wire_struct!(ListColumn {
    var,
    usage,
    maxwidth,
    minwidth,
    help,
    name,
    display,
    dspcol,
    nogui,
    autoskip,
    heading,
    line
});
wire_struct!(ListView { cols, layout, line });

wire_struct!(MenuArea {
    title,
    depth,
    scroll,
    botsep,
    topsep,
    options,
    help,
    items,
    line
});
wire_struct!(MenuItem {
    option,
    action,
    help,
    cond,
    varupd,
    text,
    line
});
wire_struct!(InfoArea {
    depth,
    scroll,
    botsep,
    help,
    body,
    line
});
wire_struct!(TextArea {
    depth,
    botsep,
    body,
    line
});
wire_struct!(CmdLine {
    size,
    name,
    prompt,
    line
});

wire_struct!(HelpModule {
    name,
    title,
    width,
    depth,
    body,
    topics,
    line
});
wire_struct!(DefItem { term, def });
wire_struct!(ParmItem { term, def });
wire_struct!(SearchTopic { roots, text });

fn tag_err(what: &'static str, tag: u8) -> WireError {
    WireError::Tag { what, tag }
}

impl Wire for BaseType {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Char(n) => {
                out.push(0);
                n.put(out);
            }
            Self::Igc(n) => {
                out.push(1);
                n.put(out);
            }
            Self::Bin(n) => {
                out.push(2);
                n.put(out);
            }
            Self::Packed(d, p) => {
                out.push(3);
                d.put(out);
                p.put(out);
            }
            Self::Zoned(d, p) => {
                out.push(4);
                d.put(out);
                p.put(out);
            }
            Self::Float(n) => {
                out.push(5);
                n.put(out);
            }
            Self::Date => out.push(6),
            Self::Time => out.push(7),
            Self::Ptr => out.push(8),
            Self::ObjName => out.push(9),
            Self::ObjNameQ => out.push(10),
            Self::Name => out.push(11),
            Self::SName => out.push(12),
            Self::Dec => out.push(13),
            Self::Action => out.push(14),
            Self::CmdLine => out.push(15),
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Char(u32::get(d)?),
            1 => Self::Igc(u32::get(d)?),
            2 => Self::Bin(u32::get(d)?),
            3 => Self::Packed(u32::get(d)?, u32::get(d)?),
            4 => Self::Zoned(u32::get(d)?, u32::get(d)?),
            5 => Self::Float(u32::get(d)?),
            6 => Self::Date,
            7 => Self::Time,
            8 => Self::Ptr,
            9 => Self::ObjName,
            10 => Self::ObjNameQ,
            11 => Self::Name,
            12 => Self::SName,
            13 => Self::Dec,
            14 => Self::Action,
            15 => Self::CmdLine,
            t => return Err(tag_err("BaseType", t)),
        })
    }
}

impl Wire for Expr {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Var(s) => {
                out.push(0);
                s.put(out);
            }
            Self::Str(s) => {
                out.push(1);
                s.put(out);
            }
            Self::Num(s) => {
                out.push(2);
                s.put(out);
            }
            Self::List(s) => {
                out.push(3);
                s.put(out);
            }
            Self::Node(s) => {
                out.push(4);
                s.put(out);
            }
            Self::Cmp(op, a, b) => {
                out.push(5);
                op.put(out);
                a.put(out);
                b.put(out);
            }
            Self::And(a, b) => {
                out.push(6);
                a.put(out);
                b.put(out);
            }
            Self::Or(a, b) => {
                out.push(7);
                a.put(out);
                b.put(out);
            }
            Self::Not(a) => {
                out.push(8);
                a.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Var(String::get(d)?),
            1 => Self::Str(String::get(d)?),
            2 => Self::Num(String::get(d)?),
            3 => Self::List(String::get(d)?),
            4 => Self::Node(String::get(d)?),
            5 => Self::Cmp(CmpOp::get(d)?, Box::get(d)?, Box::get(d)?),
            6 => Self::And(Box::get(d)?, Box::get(d)?),
            7 => Self::Or(Box::get(d)?, Box::get(d)?),
            8 => Self::Not(Box::get(d)?),
            t => return Err(tag_err("Expr", t)),
        })
    }
}

impl Wire for Key {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Enter => out.push(0),
            Self::Help => out.push(1),
            Self::Print => out.push(2),
            Self::Home => out.push(3),
            Self::PageUp => out.push(4),
            Self::PageDown => out.push(5),
            Self::F(n) => {
                out.push(6);
                out.push(*n);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Enter,
            1 => Self::Help,
            2 => Self::Print,
            3 => Self::Home,
            4 => Self::PageUp,
            5 => Self::PageDown,
            6 => Self::F(u8::get(d)?),
            t => return Err(tag_err("Key", t)),
        })
    }
}

impl Wire for Action {
    fn put(&self, out: &mut Vec<u8>) {
        let (tag, name): (u8, Option<&String>) = match self {
            Self::Return(n) => {
                out.push(0);
                n.put(out);
                return;
            }
            Self::Enter => (1, None),
            Self::Help => (2, None),
            Self::ExtHelp => (3, None),
            Self::KeysHelp => (4, None),
            Self::About => (5, None),
            Self::Exit => (6, None),
            Self::Cancel => (7, None),
            Self::Print => (8, None),
            Self::CmdLine => (9, None),
            Self::PrvPnl => (10, None),
            Self::NxtPnl => (11, None),
            Self::PageUp => (12, None),
            Self::PageDown => (13, None),
            Self::PullDown => (14, None),
            Self::Actions => (15, None),
            Self::Retrieve => (16, None),
            Self::MoreKeys => (17, None),
            Self::FldHelp => (18, None),
            Self::Top => (19, None),
            Self::Bottom => (20, None),
            Self::Home => (21, None),
            Self::PrvPage => (22, None),
            Self::NxtPage => (23, None),
            Self::Call(s) => (24, Some(s)),
            Self::Panel(s) => (25, Some(s)),
            Self::Menu(s) => (26, Some(s)),
            Self::Msg(s) => (27, Some(s)),
            Self::HelpModule(s) => (28, Some(s)),
        };
        out.push(tag);
        if let Some(s) = name {
            s.put(out);
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Return(u32::get(d)?),
            1 => Self::Enter,
            2 => Self::Help,
            3 => Self::ExtHelp,
            4 => Self::KeysHelp,
            5 => Self::About,
            6 => Self::Exit,
            7 => Self::Cancel,
            8 => Self::Print,
            9 => Self::CmdLine,
            10 => Self::PrvPnl,
            11 => Self::NxtPnl,
            12 => Self::PageUp,
            13 => Self::PageDown,
            14 => Self::PullDown,
            15 => Self::Actions,
            16 => Self::Retrieve,
            17 => Self::MoreKeys,
            18 => Self::FldHelp,
            19 => Self::Top,
            20 => Self::Bottom,
            21 => Self::Home,
            22 => Self::PrvPage,
            23 => Self::NxtPage,
            24 => Self::Call(String::get(d)?),
            25 => Self::Panel(String::get(d)?),
            26 => Self::Menu(String::get(d)?),
            27 => Self::Msg(String::get(d)?),
            28 => Self::HelpModule(String::get(d)?),
            t => return Err(tag_err("Action", t)),
        })
    }
}

impl Wire for Depth {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Lines(n) => {
                out.push(0);
                n.put(out);
            }
            Self::Fill => out.push(1),
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Lines(u32::get(d)?),
            1 => Self::Fill,
            t => return Err(tag_err("Depth", t)),
        })
    }
}

impl Wire for MenuOption {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Number(n) => {
                out.push(0);
                out.push(*n);
            }
            Self::Star => out.push(1),
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Number(u8::get(d)?),
            1 => Self::Star,
            t => return Err(tag_err("MenuOption", t)),
        })
    }
}

impl Wire for Area {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Data(a) => {
                out.push(0);
                a.put(out);
            }
            Self::List(a) => {
                out.push(1);
                a.put(out);
            }
            Self::Menu(a) => {
                out.push(2);
                a.put(out);
            }
            Self::Info(a) => {
                out.push(3);
                a.put(out);
            }
            Self::Text(a) => {
                out.push(4);
                a.put(out);
            }
            Self::CmdLine(a) => {
                out.push(5);
                a.put(out);
            }
            Self::TopInst(s) => {
                out.push(6);
                s.put(out);
            }
            Self::BotInst(s) => {
                out.push(7);
                s.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Data(Wire::get(d)?),
            1 => Self::List(Wire::get(d)?),
            2 => Self::Menu(Wire::get(d)?),
            3 => Self::Info(Wire::get(d)?),
            4 => Self::Text(Wire::get(d)?),
            5 => Self::CmdLine(Wire::get(d)?),
            6 => Self::TopInst(Wire::get(d)?),
            7 => Self::BotInst(Wire::get(d)?),
            t => return Err(tag_err("Area", t)),
        })
    }
}

impl Wire for DataNode {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Group(g) => {
                out.push(0);
                g.put(out);
            }
            Self::Field(f) => {
                out.push(1);
                f.put(out);
            }
            Self::Const(c) => {
                out.push(2);
                c.put(out);
            }
            Self::Select(s) => {
                out.push(3);
                s.put(out);
            }
            Self::Column(w) => {
                out.push(4);
                w.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Group(Wire::get(d)?),
            1 => Self::Field(Wire::get(d)?),
            2 => Self::Const(Wire::get(d)?),
            3 => Self::Select(Wire::get(d)?),
            4 => Self::Column(Wire::get(d)?),
            t => return Err(tag_err("DataNode", t)),
        })
    }
}

impl Wire for ListColNode {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Column(c) => {
                out.push(0);
                c.put(out);
            }
            Self::Group(g) => {
                out.push(1);
                g.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Column(Wire::get(d)?),
            1 => Self::Group(Wire::get(d)?),
            t => return Err(tag_err("ListColNode", t)),
        })
    }
}

impl Wire for MenuNode {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Item(i) => {
                out.push(0);
                i.put(out);
            }
            Self::Group {
                heading,
                help,
                items,
            } => {
                out.push(1);
                heading.put(out);
                help.put(out);
                items.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Item(Wire::get(d)?),
            1 => Self::Group {
                heading: Wire::get(d)?,
                help: Wire::get(d)?,
                items: Wire::get(d)?,
            },
            t => return Err(tag_err("MenuNode", t)),
        })
    }
}

impl Wire for Span {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Text(t) => {
                out.push(0);
                t.put(out);
            }
            Self::Var(v) => {
                out.push(1);
                v.put(out);
            }
            Self::Hp(n, inner) => {
                out.push(2);
                out.push(*n);
                inner.put(out);
            }
            Self::Pk(inner) => {
                out.push(3);
                inner.put(out);
            }
            Self::Pv(inner) => {
                out.push(4);
                inner.put(out);
            }
            Self::Rt(inner) => {
                out.push(5);
                inner.put(out);
            }
            Self::Link { perform, text } => {
                out.push(6);
                perform.put(out);
                text.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Text(Wire::get(d)?),
            1 => Self::Var(Wire::get(d)?),
            2 => Self::Hp(u8::get(d)?, Wire::get(d)?),
            3 => Self::Pk(Wire::get(d)?),
            4 => Self::Pv(Wire::get(d)?),
            5 => Self::Rt(Wire::get(d)?),
            6 => Self::Link {
                perform: Wire::get(d)?,
                text: Wire::get(d)?,
            },
            t => return Err(tag_err("Span", t)),
        })
    }
}

impl Wire for Block {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Self::Heading { level, text } => {
                out.push(0);
                out.push(*level);
                text.put(out);
            }
            Self::Para(s) => {
                out.push(1);
                s.put(out);
            }
            Self::ParaCont(s) => {
                out.push(2);
                s.put(out);
            }
            Self::Example(l) => {
                out.push(3);
                l.put(out);
            }
            Self::Lines(l) => {
                out.push(4);
                l.put(out);
            }
            Self::Figure(l) => {
                out.push(5);
                l.put(out);
            }
            Self::List {
                kind,
                compact,
                items,
            } => {
                out.push(6);
                kind.put(out);
                compact.put(out);
                items.put(out);
            }
            Self::DefList {
                compact,
                heads,
                items,
            } => {
                out.push(7);
                compact.put(out);
                heads.put(out);
                items.put(out);
            }
            Self::ParmList(items) => {
                out.push(8);
                items.put(out);
            }
            Self::Note(doc) => {
                out.push(9);
                doc.put(out);
            }
            Self::ImHelp(name) => {
                out.push(10);
                name.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(match u8::get(d)? {
            0 => Self::Heading {
                level: u8::get(d)?,
                text: Wire::get(d)?,
            },
            1 => Self::Para(Wire::get(d)?),
            2 => Self::ParaCont(Wire::get(d)?),
            3 => Self::Example(Wire::get(d)?),
            4 => Self::Lines(Wire::get(d)?),
            5 => Self::Figure(Wire::get(d)?),
            6 => Self::List {
                kind: Wire::get(d)?,
                compact: Wire::get(d)?,
                items: Wire::get(d)?,
            },
            7 => Self::DefList {
                compact: Wire::get(d)?,
                heads: Wire::get(d)?,
                items: Wire::get(d)?,
            },
            8 => Self::ParmList(Wire::get(d)?),
            9 => Self::Note(Wire::get(d)?),
            10 => Self::ImHelp(Wire::get(d)?),
            t => return Err(tag_err("Block", t)),
        })
    }
}
