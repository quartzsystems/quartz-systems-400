// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `:PANEL.` and `:PRTPNL.` with their areas: data, list, menu,
//! information, text and the command line.

use std::collections::BTreeSet;

use super::{Build, line_u32};
use crate::diag::ids;
use crate::model::{
    Area, CmdLine, CmdLineSize, DataArea, DataChoice, DataConst, DataExtender, DataField,
    DataGroup, DataNode, DataSelect, Depth, InfoArea, ListAction, ListArea, ListColNode,
    ListColumn, ListGroup, ListView, MenuArea, MenuItem, MenuNode, MenuOption, Panel, PrintPanel,
    Span, TextArea,
};
use crate::tree::Element;

impl Build {
    pub(super) fn panel(&mut self, el: &Element) -> Option<Panel> {
        let name = self.name(el, "NAME", "Panel")?;
        self.define("Panel", &name, el.line);
        let keyl = self.upper(el, "KEYL").unwrap_or_default();
        if !keyl.is_empty() {
            self.refer("Key list", &keyl, el.line);
        }
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        let panelid = self.upper(el, "PANELID");
        self.refer_opt("Variable", panelid.as_ref(), el.line);
        let csrvar = self.upper(el, "CSRVAR");
        self.refer_opt("Variable", csrvar.as_ref(), el.line);
        let csrlst = self.upper(el, "CSRLST");
        self.refer_opt("List", csrlst.as_ref(), el.line);
        let prvpnl = self.upper(el, "PRVPNL");
        self.refer_opt("Panel", prvpnl.as_ref(), el.line);
        let nxtpnl = self.upper(el, "NXTPNL");
        self.refer_opt("Panel", nxtpnl.as_ref(), el.line);
        let depth = self.u32(el, "DEPTH").unwrap_or(24);
        let width = self.u32(el, "WIDTH").unwrap_or(80);
        if !matches!((depth, width), (24, 80 | 132) | (27, 132)) {
            self.diags.push(ids::bad_panel_size(el.line, depth, width));
        }
        let title = self.leading_spans(el);
        let topinst = self.text(el, "TOPINST").map(|t| vec![Span::Text(t)]);
        let areas = self.areas(el, true);
        if areas.is_empty() {
            self.diags.push(ids::empty_panel(el.line, &name));
        }
        self.check_depths(el.line, depth, &areas);
        Some(Panel {
            name,
            title,
            help,
            keyl,
            enter: self.action(el, "ENTER"),
            depth,
            width,
            panelid,
            topsep: self.separator(el, "TOPSEP"),
            botsep: self.separator(el, "BOTSEP"),
            topinst,
            csrvar,
            csrlst,
            helptitle: self.text(el, "HELPTITLE"),
            prvpnl,
            nxtpnl,
            enbgui: self.flag(el, "ENBGUI"),
            keylshlf: self.upper(el, "KEYLSHLF"),
            areas,
            line: line_u32(el.line),
        })
    }

    pub(super) fn print_panel(&mut self, el: &Element) -> Option<PrintPanel> {
        let name = self.name(el, "NAME", "Print panel")?;
        self.define("Panel", &name, el.line);
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        let title = self.leading_spans(el);
        let head = el.named("PRTHEAD").next().map(|h| self.document(h));
        let trail = el.named("PRTTRAIL").next().map(|t| self.document(t));
        Some(PrintPanel {
            name,
            title,
            help,
            width: self.u32(el, "WIDTH").unwrap_or(132),
            depth: self.u32(el, "DEPTH").unwrap_or(66),
            topsep: self.separator(el, "TOPSEP"),
            botsep: self.separator(el, "BOTSEP"),
            head,
            trail,
            areas: self.areas(el, false),
            line: line_u32(el.line),
        })
    }

    /// The areas of a panel; menus and command lines only on display panels.
    fn areas(&mut self, el: &Element, display: bool) -> Vec<Area> {
        let mut areas = Vec::new();
        let mut has_cmdline = false;
        for c in el.elements() {
            let area = match c.name.as_str() {
                "DATA" => self.data_area(c).map(Area::Data),
                "LIST" => self.list_area(c).map(Area::List),
                "MENU" if display => self.menu_area(c).map(Area::Menu),
                "INFO" => self.info_area(c).map(Area::Info),
                "TEXT" => Some(Area::Text(TextArea {
                    depth: self.depth(c, "DEPTH"),
                    botsep: self.separator(c, "BOTSEP"),
                    body: self.document(c),
                    line: line_u32(c.line),
                })),
                "CMDLINE" if display => {
                    if has_cmdline {
                        self.diags.push(ids::second_cmdline(c.line));
                        continue;
                    }
                    has_cmdline = true;
                    Some(Area::CmdLine(CmdLine {
                        size: self.cmdline_size(c, "SIZE"),
                        name: self.upper(c, "NAME"),
                        prompt: self.spans(&c.children),
                        line: line_u32(c.line),
                    }))
                }
                "TOPINST" => Some(Area::TopInst(self.spans(&c.children))),
                "BOTINST" => Some(Area::BotInst(self.spans(&c.children))),
                _ => None,
            };
            areas.extend(area);
        }
        if display {
            for a in &areas {
                if let Area::Menu(m) = a {
                    for i in m.all_items() {
                        if i.option == MenuOption::Star && !has_cmdline {
                            self.diags.push(ids::star_without_cmdline(i.line as usize));
                        }
                    }
                }
            }
        }
        areas
    }

    /// Fixed depths plus one line per `*` area must fit under the title
    /// and above the key lines.
    fn check_depths(&mut self, line: usize, depth: u32, areas: &[Area]) {
        let mut total = 0u32;
        let mut reserved = 3u32;
        for a in areas {
            match a {
                Area::CmdLine(c) => {
                    reserved += match c.size {
                        CmdLineSize::Short => 1,
                        CmdLineSize::Long => 2,
                    };
                }
                Area::TopInst(_) | Area::BotInst(_) => reserved += 1,
                _ => match a.depth() {
                    Some(Depth::Lines(n)) => total += n,
                    Some(Depth::Fill) => total += 1,
                    None => {}
                },
            }
        }
        let available = depth.saturating_sub(reserved);
        if total > available {
            self.diags
                .push(ids::depth_exceeds_panel(line, total, available));
        }
    }

    fn data_area(&mut self, el: &Element) -> Option<DataArea> {
        let depth = self.depth(el, "DEPTH")?;
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        let title = self.leading_spans(el);
        let topinst = self.text(el, "TOPINST").map(|t| vec![Span::Text(t)]);
        Some(DataArea {
            title,
            depth,
            layout: self.layout(el, "LAYOUT"),
            botsep: self.separator(el, "BOTSEP"),
            compact: self.flag(el, "COMPACT"),
            scroll: self.scroll(el, "SCROLL"),
            topinst,
            help,
            items: self.data_items(el),
            line: line_u32(el.line),
        })
    }

    fn data_items(&mut self, el: &Element) -> Vec<DataNode> {
        let mut items: Vec<DataNode> = Vec::new();
        for c in el.elements() {
            match c.name.as_str() {
                "DATAGRP" => {
                    let help = self.upper(c, "HELP");
                    self.refer_opt("Help module", help.as_ref(), c.line);
                    let title = self.leading_spans(c);
                    let inner = self.data_items(c);
                    items.push(DataNode::Group(DataGroup {
                        title,
                        grpsep: self.separator(c, "GRPSEP"),
                        help,
                        compact: self.flag(c, "COMPACT"),
                        nogui: self.flag(c, "NOGUI"),
                        items: inner,
                        line: line_u32(c.line),
                    }));
                }
                "DATACOL" => {
                    if let Some(w) = self.u32(c, "WIDTH") {
                        items.push(DataNode::Column(w));
                    }
                }
                "DATAI" => {
                    if let Some(f) = self.data_field(c) {
                        items.push(DataNode::Field(f));
                    }
                }
                "DATAIX" => {
                    let Some(DataNode::Field(f)) = items.last_mut() else {
                        self.diags.push(ids::extender_without_item(c.line));
                        continue;
                    };
                    let (Some(var), Some(usage)) = (self.upper(c, "VAR"), self.usage(c, "USAGE"))
                    else {
                        continue;
                    };
                    let help = self.upper(c, "HELP");
                    let ext = DataExtender {
                        var,
                        usage,
                        name: self.upper(c, "NAME"),
                        help,
                        maxwidth: self.u32(c, "MAXWIDTH"),
                        minwidth: self.u32(c, "MINWIDTH"),
                        display: self.display(c, "DISPLAY"),
                        text: Vec::new(),
                        line: line_u32(c.line),
                    };
                    self.refer("Variable", &ext.var, c.line);
                    self.refer_opt("Help module", ext.help.as_ref(), c.line);
                    f.extenders.push(ext);
                }
                "DATAIXT" => {
                    let text = self.spans(&c.children);
                    if let Some(DataNode::Field(f)) = items.last_mut()
                        && let Some(x) = f.extenders.last_mut()
                    {
                        x.text = text;
                    } else {
                        self.diags.push(ids::extender_without_item(c.line));
                    }
                }
                "DATAC" => {
                    let help = self.upper(c, "HELP");
                    self.refer_opt("Help module", help.as_ref(), c.line);
                    items.push(DataNode::Const(DataConst {
                        text: self.spans(&c.children),
                        name: self.upper(c, "NAME"),
                        help,
                        display: self.display(c, "DISPLAY"),
                        width: self.u32(c, "WIDTH"),
                        line: line_u32(c.line),
                    }));
                }
                "DATASLT" => {
                    if let Some(s) = self.data_select(c) {
                        items.push(DataNode::Select(s));
                    }
                }
                _ => {}
            }
        }
        items
    }

    fn data_field(&mut self, el: &Element) -> Option<DataField> {
        let var = self.upper(el, "VAR")?;
        self.refer("Variable", &var, el.line);
        let usage = self.usage(el, "USAGE")?;
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        Some(DataField {
            var,
            prompt: self.spans(&el.children),
            help,
            usage,
            name: self.upper(el, "NAME"),
            basetype: self.basetype(el, "BASETYPE"),
            autoskip: self.flag(el, "AUTOSKIP"),
            display: self.display(el, "DISPLAY"),
            nocheck: self.flag(el, "NOCHECK"),
            required: self.flag(el, "REQUIRED"),
            maxwidth: self.u32(el, "MAXWIDTH"),
            minwidth: self.u32(el, "MINWIDTH"),
            shift: self.shift(el, "SHIFT"),
            csrloc: self.flag(el, "CSRLOC"),
            noputovr: self.flag(el, "NOPUTOVR"),
            extenders: Vec::new(),
            line: line_u32(el.line),
        })
    }

    fn data_select(&mut self, el: &Element) -> Option<DataSelect> {
        let var = self.upper(el, "VAR")?;
        self.refer("Variable", &var, el.line);
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        let prompt = self.leading_spans(el);
        let mut choices = Vec::new();
        for c in el.named("DATASLTC") {
            let Some(value) = self.text(c, "VALUE") else {
                continue;
            };
            let help = self.upper(c, "HELP");
            self.refer_opt("Help module", help.as_ref(), c.line);
            let cond = self.upper(c, "COND");
            self.refer_opt("Condition", cond.as_ref(), c.line);
            choices.push(DataChoice {
                value,
                help,
                cond,
                display: self.display(c, "DISPLAY"),
                text: self.spans(&c.children),
                line: line_u32(c.line),
            });
        }
        Some(DataSelect {
            var,
            prompt,
            help,
            usage: self
                .usage(el, "USAGE")
                .unwrap_or(crate::model::Usage::Inout),
            name: self.upper(el, "NAME"),
            display: self.display(el, "DISPLAY"),
            required: self.flag(el, "REQUIRED"),
            csrloc: self.flag(el, "CSRLOC"),
            slttype: self.choice(el, "SLTTYPE", &["SINGLE", "MULTI"]),
            nocheck: self.flag(el, "NOCHECK"),
            choices,
            line: line_u32(el.line),
        })
    }

    fn list_area(&mut self, el: &Element) -> Option<ListArea> {
        let depth = self.depth(el, "DEPTH")?;
        let listdef = self.upper(el, "LISTDEF")?;
        self.refer("List", &listdef, el.line);
        let parms = self.upper(el, "PARMS");
        self.refer_opt("Variable record", parms.as_ref(), el.line);
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        let title = self.leading_spans(el);
        let botinst = self.text(el, "BOTINST").map(|t| vec![Span::Text(t)]);
        let mut actions: Vec<ListAction> = Vec::new();
        let mut columns: Vec<ListColNode> = Vec::new();
        let mut list_views = Vec::new();
        let mut options = BTreeSet::new();
        for c in el.elements() {
            match c.name.as_str() {
                "LISTACT" => {
                    if let Some(a) = self.list_action(c, &mut options) {
                        actions.push(a);
                    }
                }
                "LISTCOL" => {
                    if let Some(col) = self.list_column(c) {
                        columns.push(ListColNode::Column(col));
                    }
                }
                "LISTGRP" => {
                    let help = self.upper(c, "HELP");
                    self.refer_opt("Help module", help.as_ref(), c.line);
                    let heading = self.leading_spans(c);
                    let cols = c
                        .named("LISTCOL")
                        .filter_map(|x| self.list_column(x))
                        .collect();
                    columns.push(ListColNode::Group(ListGroup {
                        heading,
                        col: self.upper(c, "COL"),
                        help,
                        nogui: self.flag(c, "NOGUI"),
                        columns: cols,
                        line: line_u32(c.line),
                    }));
                }
                "LISTVIEW" => {
                    let cols = self.names(c, "COLS");
                    list_views.push(ListView {
                        cols,
                        layout: self.layout(c, "LAYOUT"),
                        line: line_u32(c.line),
                    });
                }
                _ => {}
            }
        }
        let views = self.check_views(el, &columns, &list_views);
        Some(ListArea {
            title,
            depth,
            listdef,
            views,
            actor: self.choice(el, "ACTOR", &["UIM", "CALLER"]),
            extact: self.flag(el, "EXTACT"),
            maxhead: self.u32(el, "MAXHEAD"),
            parms,
            scroll: self.scroll(el, "SCROLL"),
            botinst,
            enter: self.action(el, "ENTER"),
            layout: self.layout(el, "LAYOUT"),
            select: self.choice(el, "SELECT", &["SINGLE", "MULTI", "NONE"]),
            actlen: self.u32(el, "ACTLEN"),
            help,
            actions,
            columns,
            list_views,
            line: line_u32(el.line),
        })
    }

    /// A list needs columns or views, views name known columns and
    /// `VIEWS` agrees with the `:LISTVIEW.` count; answers `VIEWS`.
    fn check_views(
        &mut self,
        el: &Element,
        columns: &[ListColNode],
        list_views: &[ListView],
    ) -> Option<u32> {
        if columns.is_empty() && list_views.is_empty() {
            self.diags.push(ids::list_without_columns(el.line));
        }
        let known: BTreeSet<String> = columns
            .iter()
            .flat_map(|n| match n {
                ListColNode::Column(c) => vec![c.name.clone()],
                ListColNode::Group(g) => g
                    .col
                    .iter()
                    .cloned()
                    .chain(g.columns.iter().map(|c| c.name.clone()))
                    .collect(),
            })
            .collect();
        for v in list_views {
            for col in &v.cols {
                if !known.contains(col) {
                    self.diags
                        .push(ids::view_column_unknown(v.line as usize, col));
                }
            }
        }
        let views = self.u32(el, "VIEWS");
        if let Some(n) = views
            && !list_views.is_empty()
            && list_views.len() != n as usize
        {
            self.diags
                .push(ids::views_mismatch(el.line, list_views.len(), n));
        }
        views
    }

    fn list_action(&mut self, el: &Element, options: &mut BTreeSet<String>) -> Option<ListAction> {
        let option = self.upper(el, "OPTION");
        let enter = self.action(el, "ENTER");
        if option.is_none() && el.attr("ENTER").is_none() {
            self.diags.push(ids::listact_without_option(el.line));
            return None;
        }
        if let Some(o) = &option {
            let numeric = o.parse::<u32>().is_ok_and(|n| (1..=99).contains(&n));
            let letter = o.len() == 1 && o.chars().all(|c| c.is_ascii_alphabetic());
            if !numeric && !letter {
                self.diags.push(ids::bad_list_option(el.line, o));
            } else if !options.insert(o.clone()) {
                self.diags.push(ids::duplicate_option(el.line, o));
            }
        }
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        let confirm = self.upper(el, "CONFIRM");
        self.refer_opt("Panel", confirm.as_ref(), el.line);
        Some(ListAction {
            option,
            enter,
            help,
            confirm,
            usrexit: self.action(el, "USREXIT"),
            actfor: self.choice(el, "ACTFOR", &["LIST", "ALL"]),
            ret: self.u32(el, "RETURN"),
            varupd: self.varupd(el, "VARUPD"),
            entermsg: self.upper(el, "ENTERMSG"),
            text: self.spans(&el.children),
            line: line_u32(el.line),
        })
    }

    fn list_column(&mut self, el: &Element) -> Option<ListColumn> {
        let var = self.upper(el, "VAR")?;
        self.refer("Variable", &var, el.line);
        let usage = self.usage(el, "USAGE")?;
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        let name = self.upper(el, "NAME").unwrap_or_else(|| var.clone());
        Some(ListColumn {
            var,
            usage,
            maxwidth: self.u32(el, "MAXWIDTH"),
            minwidth: self.u32(el, "MINWIDTH"),
            help,
            name,
            display: self.display(el, "DISPLAY"),
            dspcol: self.u32(el, "DSPCOL"),
            nogui: self.flag(el, "NOGUI"),
            autoskip: self.flag(el, "AUTOSKIP"),
            heading: self.spans(&el.children),
            line: line_u32(el.line),
        })
    }

    fn menu_area(&mut self, el: &Element) -> Option<MenuArea> {
        let depth = self.depth(el, "DEPTH")?;
        let options = self.upper(el, "OPTIONS");
        self.refer_opt("Variable", options.as_ref(), el.line);
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        let title = self.leading_spans(el);
        let mut used = BTreeSet::new();
        let mut items = Vec::new();
        for c in el.elements() {
            match c.name.as_str() {
                "MENUI" => {
                    if let Some(i) = self.menu_item(c, &mut used) {
                        items.push(MenuNode::Item(i));
                    }
                }
                "MENUGRP" => {
                    let help = self.upper(c, "HELP");
                    self.refer_opt("Help module", help.as_ref(), c.line);
                    let heading = self.leading_spans(c);
                    let inner = c
                        .named("MENUI")
                        .filter_map(|m| self.menu_item(m, &mut used))
                        .collect();
                    items.push(MenuNode::Group {
                        heading,
                        help,
                        items: inner,
                    });
                }
                _ => {}
            }
        }
        Some(MenuArea {
            title,
            depth,
            scroll: self.scroll(el, "SCROLL"),
            botsep: self.separator(el, "BOTSEP"),
            topsep: self.separator(el, "TOPSEP"),
            options,
            help,
            items,
            line: line_u32(el.line),
        })
    }

    fn menu_item(&mut self, el: &Element, used: &mut BTreeSet<String>) -> Option<MenuItem> {
        let text = self.upper(el, "OPTION")?;
        let option = if text == "*" {
            MenuOption::Star
        } else {
            match text.parse::<u16>() {
                Ok(n) if (1..=999).contains(&n) => MenuOption::Number(n),
                _ => {
                    self.diags.push(ids::bad_menu_option(el.line, &text));
                    return None;
                }
            }
        };
        let key = match option {
            MenuOption::Star => "*".to_string(),
            MenuOption::Number(n) => n.to_string(),
        };
        if !used.insert(key.clone()) {
            self.diags.push(ids::duplicate_option(el.line, &key));
        }
        let action = self.action(el, "ACTION")?;
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        let cond = self.upper(el, "COND");
        self.refer_opt("Condition", cond.as_ref(), el.line);
        Some(MenuItem {
            option,
            action,
            help,
            cond,
            varupd: self.varupd(el, "VARUPD"),
            text: self.spans(&el.children),
            line: line_u32(el.line),
        })
    }

    fn info_area(&mut self, el: &Element) -> Option<InfoArea> {
        let depth = self.depth(el, "DEPTH")?;
        let help = self.upper(el, "HELP");
        self.refer_opt("Help module", help.as_ref(), el.line);
        Some(InfoArea {
            depth,
            scroll: self.scroll(el, "SCROLL"),
            botsep: self.separator(el, "BOTSEP"),
            help,
            body: self.document(el),
            line: line_u32(el.line),
        })
    }
}
