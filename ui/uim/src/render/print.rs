// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Print panels (`:PRTPNL.`, `QUIPRTP`): the title centered over the
//! print width, the `:PRTHEAD.` text, the areas laid out as on a
//! display (every list entry, no paging indicator) and the
//! `:PRTTRAIL.` text, as lines for a printer file.

use super::ctx::Ctx;
use super::screen::{Attr, Screen};
use super::{Dialog, Options, RenderError, doc, layout};
use crate::model::{Area, Panel, PanelGroup, Separator, Span};

/// The rows a print panel may take before it is cut.
const MAX_ROWS: usize = 2000;

/// A print panel as lines, trailing blanks cut.
pub fn render_print(
    group: &PanelGroup,
    name: &str,
    dialog: &dyn Dialog,
    opts: &Options,
) -> Result<Vec<String>, RenderError> {
    let pp = group
        .print_panels
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| RenderError::NoPanel(name.to_string()))?;
    let ctx = Ctx {
        group,
        dialog,
        opts,
        window: false,
    };
    let cols = (pp.width as usize).max(40);
    // Enough rows for every list entry.
    let entries: usize = pp
        .areas
        .iter()
        .filter_map(|a| match a {
            Area::List(l) => Some(ctx.list(&l.listdef).count() + 8),
            _ => None,
        })
        .sum();
    let rows = (pp.depth as usize + entries).min(MAX_ROWS);
    let mut s = Screen::new(rows, cols + 1);
    let title = ctx.text(&pp.title);
    let title = title.trim();
    s.put(
        1,
        layout::center(cols, title.chars().count()),
        Attr::HighIntensity,
        title,
    );
    let mut row = match pp.topsep {
        Separator::None => 2,
        _ => 3,
    };
    if let Some(h) = &pp.head {
        let lines = doc::flow(&ctx, h, 2, cols - 1, 2);
        doc::put_rows(&mut s, &lines, row);
        row += lines.len() + 1;
    }
    let trail = pp
        .trail
        .as_ref()
        .map(|t| doc::flow(&ctx, t, 2, cols - 1, 2))
        .unwrap_or_default();
    let bottom = rows.saturating_sub(trail.len());
    let panel = Panel {
        name: pp.name.clone(),
        title: vec![Span::Text(title.to_string())],
        help: None,
        keyl: String::new(),
        enter: None,
        depth: u32::try_from(rows).unwrap_or(u32::MAX),
        width: u32::try_from(cols).unwrap_or(132),
        panelid: None,
        topsep: pp.topsep,
        botsep: pp.botsep,
        topinst: None,
        csrvar: None,
        csrlst: None,
        helptitle: None,
        prvpnl: None,
        nxtpnl: None,
        enbgui: false,
        keylshlf: None,
        areas: pp.areas.clone(),
        line: pp.line,
    };
    super::place_areas(&ctx, &panel, &mut s, row, bottom);
    // No paging indicator on paper.
    if let Some(m) = s.more.take() {
        s.texts.retain(|t| !(t.row == m.row && t.col == m.col));
    }
    let mut out: Vec<String> = s
        .text_rows()
        .into_iter()
        .map(|r| r.trim_end().to_string())
        .collect();
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    if !trail.is_empty() {
        out.push(String::new());
        let mut t = Screen::new(trail.len(), cols + 1);
        doc::put_rows(&mut t, &trail, 1);
        out.extend(t.text_rows().into_iter().map(|r| r.trim_end().to_string()));
    }
    Ok(out)
}
