// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panels, print panels and their areas in source order.

use super::class::{BaseType, Shift};
use super::help::{Document, Span};
use super::keyl::Action;

/// A `:PANEL.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Panel {
    /// Name.
    pub name: String,
    /// The title text.
    pub title: Vec<Span>,
    /// `HELP`: the extended help module.
    pub help: Option<String>,
    /// `KEYL`.
    pub keyl: String,
    /// `ENTER`: the action of the Enter key.
    pub enter: Option<Action>,
    /// `DEPTH`: 24 or 27.
    pub depth: u32,
    /// `WIDTH`: 80 or 132.
    pub width: u32,
    /// `PANELID`: the variable shown as the panel identifier.
    pub panelid: Option<String>,
    /// `TOPSEP`.
    pub topsep: Separator,
    /// `BOTSEP`.
    pub botsep: Separator,
    /// `TOPINST`: instruction text under the title.
    pub topinst: Option<Vec<Span>>,
    /// `CSRVAR`: the variable that receives the cursor's field.
    pub csrvar: Option<String>,
    /// `CSRLST`: the variable that receives the cursor's list entry.
    pub csrlst: Option<String>,
    /// `HELPTITLE`: the title of the help panels.
    pub helptitle: Option<String>,
    /// `PRVPNL`: the panel `PRVPNL` shows.
    pub prvpnl: Option<String>,
    /// `NXTPNL`: the panel `NXTPNL` shows.
    pub nxtpnl: Option<String>,
    /// `ENBGUI`.
    pub enbgui: bool,
    /// `KEYLSHLF`.
    pub keylshlf: Option<String>,
    /// The areas in source order.
    pub areas: Vec<Area>,
    /// Source line.
    pub line: u32,
}

impl Panel {
    /// The panel's command line, if any.
    #[must_use]
    pub fn cmdline(&self) -> Option<&CmdLine> {
        self.areas.iter().find_map(|a| match a {
            Area::CmdLine(c) => Some(c),
            _ => None,
        })
    }
}

/// A `:PRTPNL.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrintPanel {
    /// Name.
    pub name: String,
    /// The title text.
    pub title: Vec<Span>,
    /// `HELP`.
    pub help: Option<String>,
    /// `WIDTH`.
    pub width: u32,
    /// `DEPTH`.
    pub depth: u32,
    /// `TOPSEP`.
    pub topsep: Separator,
    /// `BOTSEP`.
    pub botsep: Separator,
    /// `:PRTHEAD.` content.
    pub head: Option<Document>,
    /// `:PRTTRAIL.` content.
    pub trail: Option<Document>,
    /// The areas in source order.
    pub areas: Vec<Area>,
    /// Source line.
    pub line: u32,
}

/// One area of a panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Area {
    /// `:DATA.`.
    Data(DataArea),
    /// `:LIST.`.
    List(ListArea),
    /// `:MENU.`.
    Menu(MenuArea),
    /// `:INFO.`.
    Info(InfoArea),
    /// `:TEXT.`.
    Text(TextArea),
    /// `:CMDLINE.`.
    CmdLine(CmdLine),
    /// `:TOPINST.`.
    TopInst(Vec<Span>),
    /// `:BOTINST.`.
    BotInst(Vec<Span>),
}

impl Area {
    /// The area's `DEPTH`, when it has one.
    #[must_use]
    pub fn depth(&self) -> Option<Depth> {
        match self {
            Self::Data(a) => Some(a.depth),
            Self::List(a) => Some(a.depth),
            Self::Menu(a) => Some(a.depth),
            Self::Info(a) => Some(a.depth),
            Self::Text(a) => a.depth,
            Self::CmdLine(_) | Self::TopInst(_) | Self::BotInst(_) => None,
        }
    }
}

/// `DEPTH`: a number of lines or `*` for the rest of the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    /// That many lines.
    Lines(u32),
    /// `*`: whatever is left.
    Fill,
}

/// `TOPSEP`, `BOTSEP`, `GRPSEP`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Separator {
    /// `SPACE`: a blank line.
    #[default]
    Space,
    /// `RULE`: a line of dashes.
    Rule,
    /// `NONE`.
    None,
    /// `SYSNAM` (also written `SYSNAME`): a line with `System:` and the system name (panels).
    SysName,
    /// `DATETIME`: the system name on the title line, the date and
    /// time on the line under it (panels).
    DateTime,
}

/// `LAYOUT`: columns of a data or list area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// One column.
    #[default]
    One,
    /// Two columns.
    Two,
    /// Three columns.
    Three,
}

/// `SCROLL`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scroll {
    /// `YES`.
    Yes,
    /// `NO`.
    #[default]
    No,
}

/// `USAGE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Usage {
    /// `INPUT`.
    Input,
    /// `OUTPUT`.
    Output,
    /// `INOUT`.
    Inout,
}

impl Usage {
    /// The program can change the value.
    #[must_use]
    pub fn is_input(self) -> bool {
        matches!(self, Self::Input | Self::Inout)
    }
}

/// `DISPLAY`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Display {
    /// `YES`.
    #[default]
    Yes,
    /// `NO`.
    No,
}

/// `VARUPD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarUpd {
    /// `YES`: input fields are read before the action.
    Yes,
    /// `NO`.
    No,
}

/// `:DATA.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataArea {
    /// The heading text.
    pub title: Vec<Span>,
    /// `DEPTH`.
    pub depth: Depth,
    /// `LAYOUT`.
    pub layout: Layout,
    /// `BOTSEP`.
    pub botsep: Separator,
    /// `COMPACT`.
    pub compact: bool,
    /// `SCROLL`.
    pub scroll: Scroll,
    /// `TOPINST`.
    pub topinst: Option<Vec<Span>>,
    /// `HELP`.
    pub help: Option<String>,
    /// Items in order.
    pub items: Vec<DataNode>,
    /// Source line.
    pub line: u32,
}

/// One item of a data area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataNode {
    /// `:DATAGRP.`.
    Group(DataGroup),
    /// `:DATAI.`.
    Field(DataField),
    /// `:DATAC.`.
    Const(DataConst),
    /// `:DATASLT.`.
    Select(DataSelect),
    /// `:DATACOL.`: the following items start a column this wide.
    Column(u32),
}

/// `:DATAGRP.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataGroup {
    /// The heading.
    pub title: Vec<Span>,
    /// `GRPSEP`.
    pub grpsep: Separator,
    /// `HELP`.
    pub help: Option<String>,
    /// `COMPACT`.
    pub compact: bool,
    /// `NOGUI`.
    pub nogui: bool,
    /// Items in order.
    pub items: Vec<DataNode>,
    /// Source line.
    pub line: u32,
}

/// `:DATAI.`: a prompt and a field. The flags are the tag's own
/// attributes, one bool each.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct DataField {
    /// `VAR`.
    pub var: String,
    /// The prompt text.
    pub prompt: Vec<Span>,
    /// `HELP`.
    pub help: Option<String>,
    /// `USAGE`.
    pub usage: Usage,
    /// `NAME`: the name a program refers to the field by.
    pub name: Option<String>,
    /// `BASETYPE`: overrides the class for this field.
    pub basetype: Option<BaseType>,
    /// `AUTOSKIP`.
    pub autoskip: bool,
    /// `DISPLAY`.
    pub display: Display,
    /// `NOCHECK`.
    pub nocheck: bool,
    /// `REQUIRED`.
    pub required: bool,
    /// `MAXWIDTH`.
    pub maxwidth: Option<u32>,
    /// `MINWIDTH`.
    pub minwidth: Option<u32>,
    /// `SHIFT`.
    pub shift: Option<Shift>,
    /// `CSRLOC`: the cursor starts here.
    pub csrloc: bool,
    /// `NOPUTOVR`.
    pub noputovr: bool,
    /// `:DATAIX.` fields on the same line.
    pub extenders: Vec<DataExtender>,
    /// Source line.
    pub line: u32,
}

/// `:DATAIX.` with its `:DATAIXT.` text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataExtender {
    /// `VAR`.
    pub var: String,
    /// `USAGE`.
    pub usage: Usage,
    /// `NAME`.
    pub name: Option<String>,
    /// `HELP`.
    pub help: Option<String>,
    /// `MAXWIDTH`.
    pub maxwidth: Option<u32>,
    /// `MINWIDTH`.
    pub minwidth: Option<u32>,
    /// `DISPLAY`.
    pub display: Display,
    /// The text between the field and this one.
    pub text: Vec<Span>,
    /// Source line.
    pub line: u32,
}

/// `:DATAC.`: constant text in a data area.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataConst {
    /// The text.
    pub text: Vec<Span>,
    /// `NAME`.
    pub name: Option<String>,
    /// `HELP`.
    pub help: Option<String>,
    /// `DISPLAY`.
    pub display: Display,
    /// `WIDTH`.
    pub width: Option<u32>,
    /// Source line.
    pub line: u32,
}

/// `:DATASLT.`: a selection field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataSelect {
    /// `VAR`.
    pub var: String,
    /// The prompt text.
    pub prompt: Vec<Span>,
    /// `HELP`.
    pub help: Option<String>,
    /// `USAGE`.
    pub usage: Usage,
    /// `NAME`.
    pub name: Option<String>,
    /// `DISPLAY`.
    pub display: Display,
    /// `REQUIRED`.
    pub required: bool,
    /// `CSRLOC`.
    pub csrloc: bool,
    /// `SLTTYPE`: `SINGLE` or `MULTI`.
    pub slttype: Option<String>,
    /// `NOCHECK`.
    pub nocheck: bool,
    /// The choices.
    pub choices: Vec<DataChoice>,
    /// Source line.
    pub line: u32,
}

/// `:DATASLTC.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataChoice {
    /// `VALUE`: what the variable gets.
    pub value: String,
    /// `HELP`.
    pub help: Option<String>,
    /// `COND`.
    pub cond: Option<String>,
    /// `DISPLAY`.
    pub display: Display,
    /// The text.
    pub text: Vec<Span>,
    /// Source line.
    pub line: u32,
}

/// `:LIST.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListArea {
    /// The heading text.
    pub title: Vec<Span>,
    /// `DEPTH`.
    pub depth: Depth,
    /// `LISTDEF`.
    pub listdef: String,
    /// `VIEWS`: how many views the program expects.
    pub views: Option<u32>,
    /// `ACTOR`: `UIM` or `CALLER`.
    pub actor: Option<String>,
    /// `EXTACT`: extended action entry.
    pub extact: bool,
    /// `MAXHEAD`: lines of column headings.
    pub maxhead: Option<u32>,
    /// `PARMS`: the variable record passed to exit programs.
    pub parms: Option<String>,
    /// `SCROLL`.
    pub scroll: Scroll,
    /// `BOTINST`: instruction text under the list.
    pub botinst: Option<Vec<Span>>,
    /// `ENTER`.
    pub enter: Option<Action>,
    /// `LAYOUT`.
    pub layout: Layout,
    /// `SELECT`: `SINGLE`, `MULTI` or `NONE`.
    pub select: Option<String>,
    /// `ACTLEN`: the option column's width.
    pub actlen: Option<u32>,
    /// `COMPACT`: no blank row between the legends and the headings.
    pub compact: bool,
    /// `HELP`.
    pub help: Option<String>,
    /// `:LISTACT.` items.
    pub actions: Vec<ListAction>,
    /// `:LISTCOL.` and `:LISTGRP.` items in order.
    pub columns: Vec<ListColNode>,
    /// `:LISTVIEW.` items.
    pub list_views: Vec<ListView>,
    /// Source line.
    pub line: u32,
}

impl ListArea {
    /// Every column, grouped or not, in order.
    #[must_use]
    pub fn all_columns(&self) -> Vec<&ListColumn> {
        let mut out = Vec::new();
        for n in &self.columns {
            match n {
                ListColNode::Column(c) => out.push(c),
                ListColNode::Group(g) => out.extend(g.columns.iter()),
            }
        }
        out
    }
}

/// `:LISTACT.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListAction {
    /// `OPTION`: the number or letter typed in the option column.
    pub option: Option<String>,
    /// `ENTER`: the action.
    pub enter: Option<Action>,
    /// `HELP`.
    pub help: Option<String>,
    /// `CONFIRM`: the confirmation panel.
    pub confirm: Option<String>,
    /// `USREXIT`: the exit program called first.
    pub usrexit: Option<Action>,
    /// `ACTFOR`: `LIST` or `ALL`.
    pub actfor: Option<String>,
    /// `RETURN`: the number returned.
    pub ret: Option<u32>,
    /// `VARUPD`.
    pub varupd: Option<VarUpd>,
    /// `ENTERMSG`: the message after the action.
    pub entermsg: Option<String>,
    /// The text, `1=Select`.
    pub text: Vec<Span>,
    /// Source line.
    pub line: u32,
}

/// A column or a group of columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListColNode {
    /// `:LISTCOL.`.
    Column(ListColumn),
    /// `:LISTGRP.`.
    Group(ListGroup),
}

/// `:LISTGRP.`: a heading over several columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListGroup {
    /// The heading.
    pub heading: Vec<Span>,
    /// `COL`: the name of the group as a view column.
    pub col: Option<String>,
    /// `HELP`.
    pub help: Option<String>,
    /// `NOGUI`.
    pub nogui: bool,
    /// The columns.
    pub columns: Vec<ListColumn>,
    /// Source line.
    pub line: u32,
}

/// `:LISTCOL.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListColumn {
    /// `VAR`.
    pub var: String,
    /// `USAGE`.
    pub usage: Usage,
    /// `MAXWIDTH`.
    pub maxwidth: Option<u32>,
    /// `MINWIDTH`.
    pub minwidth: Option<u32>,
    /// `HELP`.
    pub help: Option<String>,
    /// `NAME`: the name views refer to; the variable's when absent.
    pub name: String,
    /// `DISPLAY`.
    pub display: Display,
    /// `DSPCOL`: the column it is shown at.
    pub dspcol: Option<u32>,
    /// `NOGUI`.
    pub nogui: bool,
    /// `AUTOSKIP`.
    pub autoskip: bool,
    /// The heading.
    pub heading: Vec<Span>,
    /// Source line.
    pub line: u32,
}

/// `:LISTVIEW.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListView {
    /// `COLS`: column names in order.
    pub cols: Vec<String>,
    /// `LAYOUT`.
    pub layout: Layout,
    /// Source line.
    pub line: u32,
}

/// `:MENU.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuArea {
    /// The heading text.
    pub title: Vec<Span>,
    /// `DEPTH`.
    pub depth: Depth,
    /// `SCROLL`.
    pub scroll: Scroll,
    /// `BOTSEP`.
    pub botsep: Separator,
    /// `TOPSEP`.
    pub topsep: Separator,
    /// `OPTIONS`: the variable that receives the option.
    pub options: Option<String>,
    /// `HELP`.
    pub help: Option<String>,
    /// Items and groups in order.
    pub items: Vec<MenuNode>,
    /// Source line.
    pub line: u32,
}

impl MenuArea {
    /// Every item, grouped or not, in order.
    #[must_use]
    pub fn all_items(&self) -> Vec<&MenuItem> {
        let mut out = Vec::new();
        for n in &self.items {
            match n {
                MenuNode::Item(i) => out.push(i),
                MenuNode::Group { items, .. } => out.extend(items.iter()),
            }
        }
        out
    }
}

/// A menu item or a group of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuNode {
    /// `:MENUI.`.
    Item(MenuItem),
    /// `:MENUGRP.`.
    Group {
        /// The heading.
        heading: Vec<Span>,
        /// `HELP`.
        help: Option<String>,
        /// The items.
        items: Vec<MenuItem>,
    },
}

/// `:MENUI.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    /// `OPTION`.
    pub option: MenuOption,
    /// `ACTION`.
    pub action: Action,
    /// `HELP`.
    pub help: Option<String>,
    /// `COND`.
    pub cond: Option<String>,
    /// `VARUPD`.
    pub varupd: Option<VarUpd>,
    /// The text.
    pub text: Vec<Span>,
    /// Source line.
    pub line: u32,
}

/// A menu option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuOption {
    /// 1 to 999.
    Number(u16),
    /// `*`: the command line's entry.
    Star,
}

/// `:INFO.`: scrollable text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InfoArea {
    /// `DEPTH`.
    pub depth: Depth,
    /// `SCROLL`.
    pub scroll: Scroll,
    /// `BOTSEP`.
    pub botsep: Separator,
    /// `HELP`.
    pub help: Option<String>,
    /// The content.
    pub body: Document,
    /// Source line.
    pub line: u32,
}

/// `:TEXT.`: fixed text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextArea {
    /// `DEPTH`.
    pub depth: Option<Depth>,
    /// `BOTSEP`.
    pub botsep: Separator,
    /// The content.
    pub body: Document,
    /// Source line.
    pub line: u32,
}

/// `:CMDLINE.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmdLine {
    /// `SIZE`.
    pub size: CmdLineSize,
    /// `NAME`: the name a program refers to the line by.
    pub name: Option<String>,
    /// The prompt, `Selection or command`.
    pub prompt: Vec<Span>,
    /// Source line.
    pub line: u32,
}

/// `SIZE` of a command line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CmdLineSize {
    /// `SHORT`: one line.
    Short,
    /// `LONG`: two lines.
    #[default]
    Long,
}
