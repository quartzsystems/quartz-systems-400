// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The panel renderer: a compiled panel plus the dialog's values
//! become a [`Screen`] of texts, fields, the cursor and the paging
//! indicator. Pure: no data stream, no store, no I/O. The rules are
//! numbered R1 to R21 in `docs/UIM.md` §7 and referenced here by
//! number.

mod ctx;
mod data;
mod dialog;
mod doc;
mod help;
mod keys;
mod layout;
mod list;
mod menu;
mod screen;

pub use dialog::{Dialog, ListState, MapDialog};
pub use layout::options_prompt;
pub use screen::{Attr, Field, More, Screen, Target, Text};

use ctx::Ctx;

use crate::model::{Area, CmdLineSize, Depth, KeyList, Panel, PanelGroup, Separator, Shift, Usage};

/// The variable `TOPSEP=DATETIME` shows on the second line: the date,
/// time and zone as the program formats them.
pub const DATETIME_VAR: &str = "*DATETIME";

/// How the screen is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// The system name for `TOPSEP=SYSNAME`.
    pub system: String,
    /// Show the copyright on the message line while no message shows.
    pub show_copyright: bool,
    /// The copyright text.
    pub copyright: String,
    /// The device is 27 by 132.
    pub wide: bool,
    /// The list view, from 0.
    pub view: usize,
    /// `F24=More keys` was pressed: show the keys after it.
    pub more_keys: bool,
    /// How many times `F24=More keys` was pressed, for a key list with
    /// more than two sets (it wins over `more_keys` when not 0).
    pub key_set: usize,
    /// The message line: text and whether it is an error.
    pub message: Option<(String, bool)>,
    /// The page of a data, menu, information or help area, from 0.
    pub page: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            system: "QS400".into(),
            show_copyright: false,
            copyright: String::new(),
            wide: false,
            view: 0,
            more_keys: false,
            key_set: 0,
            message: None,
            page: 0,
        }
    }
}

/// Why a panel cannot be rendered.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RenderError {
    /// The group has no such panel.
    #[error("panel {0} is not in the group")]
    NoPanel(String),
    /// The group has no such help module.
    #[error("help module {0} is not in the group")]
    NoHelp(String),
    /// The panel's key list is missing.
    #[error("key list {0} is not in the group")]
    NoKeyList(String),
    /// A 132-column panel on an 80-column device.
    #[error("panel {0} needs a wide display")]
    TooWide(String),
}

/// Render a panel.
pub fn render(
    group: &PanelGroup,
    panel: &str,
    dialog: &dyn Dialog,
    opts: &Options,
) -> Result<Screen, RenderError> {
    let p = group
        .panel(panel)
        .ok_or_else(|| RenderError::NoPanel(panel.to_string()))?;
    let cols = p.width as usize;
    if cols > 80 && !opts.wide {
        return Err(RenderError::TooWide(p.name.clone()));
    }
    let keyl = group
        .key_list(&p.keyl)
        .ok_or_else(|| RenderError::NoKeyList(p.keyl.clone()))?;
    let ctx = Ctx {
        group,
        dialog,
        opts,
    };
    let mut s = Screen::new(p.depth as usize, cols);

    // R1, R2: the title block.
    let title = ctx.text(&p.title);
    s.put(
        1,
        layout::center(cols, title.chars().count()),
        Attr::HighIntensity,
        &title,
    );
    if let Some(id) = &p.panelid {
        s.put(1, 2, Attr::Blue, ctx.value(id).trim_end());
    }
    let top = match p.topsep {
        Separator::None => 2,
        Separator::Space => 3,
        Separator::Rule => {
            rule(&mut s, 2);
            3
        }
        Separator::SysName => {
            s.put(2, cols - 18, Attr::Normal, "System:");
            s.put(2, cols - 8, Attr::Normal, opts.system.trim_end());
            3
        }
        Separator::DateTime => {
            // The system name ends the title line; the date and time
            // (the `*DATETIME` variable) end the line under it.
            s.put(1, cols - 8, Attr::Normal, opts.system.trim_end());
            let stamp = ctx.value(DATETIME_VAR);
            s.put_right(2, cols - 1, Attr::Normal, stamp.trim_end());
            3
        }
    };

    let bottom = bottom_block(&ctx, p, keyl, &mut s);
    place_areas(&ctx, p, &mut s, top, bottom);

    // R18: the message line.
    if let Some((m, _)) = &opts.message {
        s.put(s.rows, 2, Attr::HighIntensity, m);
    } else if opts.show_copyright {
        s.put(s.rows, 2, Attr::HighIntensity, &opts.copyright.clone());
    }
    if s.cursor.is_none() {
        s.cursor = default_cursor(&s);
    }
    Ok(s)
}

/// Whether a named condition of the group holds for the dialog (an
/// imported one is the dialog's to answer).
#[must_use]
pub fn condition(group: &PanelGroup, dialog: &dyn Dialog, name: &str) -> bool {
    let opts = Options::default();
    let ctx = Ctx {
        group,
        dialog,
        opts: &opts,
    };
    ctx.cond(Some(name))
}

/// Render a help module as a full-screen help panel (R20).
pub fn render_help(
    group: &PanelGroup,
    help_module: &str,
    opts: &Options,
) -> Result<Screen, RenderError> {
    help::render(group, help_module, opts)
}

/// R3: the keys, the command line or the selection and instruction
/// rows above them; returns the first row the bottom block uses.
fn bottom_block(ctx: &Ctx, p: &Panel, keyl: &KeyList, s: &mut Screen) -> usize {
    // R6: a key row ends by column W-2 (dspsfwrsc wraps a row that would reach W-1).
    let key_rows = keys::rows(ctx, keyl, s.limit() - 1);
    let key_top = s.rows - keys::reserved(keyl, s.limit() - 1, key_rows.len());
    for (i, row) in key_rows.iter().enumerate() {
        for (col, text) in row {
            s.put(key_top + i, *col, Attr::Blue, text);
        }
    }
    let botinst = p.areas.iter().find_map(|a| match a {
        Area::BotInst(t) => Some(ctx.text(t)),
        _ => None,
    });
    let has_menu = p.areas.iter().any(|a| matches!(a, Area::Menu(_)));
    let mut bottom = key_top;
    if let Some(c) = p.cmdline() {
        // R11: the command line sits on the keys.
        let field_rows = match c.size {
            CmdLineSize::Short => 1,
            CmdLineSize::Long => 2,
        };
        let prompt_row = key_top - field_rows - 1;
        let prompt = match ctx.text(&c.prompt) {
            t if t.trim().is_empty() && has_menu => "Selection or command".to_string(),
            t if t.trim().is_empty() => "Parameters or command".to_string(),
            t => t,
        };
        s.put(prompt_row, 2, Attr::Normal, &prompt);
        s.put(prompt_row + 1, 2, Attr::Normal, "===>");
        let len = s.limit() + 1 - 7 + (field_rows - 1) * s.limit();
        s.field(Field {
            row: prompt_row + 1,
            col: 7,
            len,
            attr: Attr::Underline,
            // Without a name, the dialog may still fill the line
            // (`F9=Retrieve`) through the `*CMDLINE` variable.
            value: ctx.value(c.name.as_deref().unwrap_or("*CMDLINE")),
            target: Target::CmdLine,
            usage: Usage::Input,
            shift: Shift::None,
            required: false,
            autoskip: false,
        });
        bottom = prompt_row;
        if let Some(b) = &botinst {
            bottom -= 1;
            s.put(bottom, 2, Attr::Blue, b);
        }
    } else {
        // The bottom separator, then the selection or the instruction.
        match p.botsep {
            Separator::None => {}
            Separator::Rule => {
                bottom -= 1;
                rule(s, bottom);
            }
            Separator::Space | Separator::SysName | Separator::DateTime => bottom -= 1,
        }
        if has_menu {
            // R12: `Selection` and a field on the next row.
            bottom -= 2;
            let prompt = botinst.clone().unwrap_or_else(|| "Selection".to_string());
            s.put(bottom, 2, Attr::Normal, &prompt);
            let width = menu::selection_width(p);
            s.field(Field {
                row: bottom + 1,
                col: 7,
                len: width,
                attr: Attr::Underline,
                value: String::new(),
                target: Target::MenuSelection,
                usage: Usage::Input,
                shift: Shift::None,
                required: false,
                autoskip: false,
            });
        } else if let Some(b) = &botinst {
            bottom -= 1;
            s.put(bottom, 2, Attr::Blue, b);
        }
    }

    bottom
}

/// The areas in order over rows `top` to `bottom - 1`, `DEPTH='*'`
/// areas sharing what the fixed ones leave.
fn place_areas(ctx: &Ctx, p: &Panel, s: &mut Screen, top: usize, bottom: usize) {
    let areas: Vec<&Area> = p
        .areas
        .iter()
        .filter(|a| !matches!(a, Area::CmdLine(_) | Area::BotInst(_)))
        .collect();
    let available = bottom.saturating_sub(top);
    let mut fixed = 0;
    let mut fills = 0;
    let heights: Vec<Option<usize>> = areas
        .iter()
        .map(|a| match a {
            Area::TopInst(_) => Some(1),
            Area::Text(t) if t.depth.is_none() => {
                Some(doc::flow(ctx, &t.body, 2, s.limit(), 2).len())
            }
            _ => match a.depth() {
                Some(Depth::Lines(n)) => Some(n as usize),
                _ => None,
            },
        })
        .collect();
    for h in &heights {
        match h {
            Some(n) => fixed += n,
            None => fills += 1,
        }
    }
    let spare = available.saturating_sub(fixed);
    let (each, mut extra) = spare
        .checked_div(fills)
        .map_or((0, 0), |each| (each, spare % fills));
    // R14: one-column data areas share the colon column.
    let shared = areas
        .iter()
        .filter_map(|a| match a {
            Area::Data(d) => data::single_colon(ctx, d, s.limit()),
            _ => None,
        })
        .max();
    let mut row = top;
    for (area, h) in areas.iter().zip(heights) {
        let height = h.unwrap_or_else(|| {
            let e = usize::from(extra > 0);
            extra = extra.saturating_sub(1);
            each + e
        });
        if row + height > bottom {
            break;
        }
        match area {
            Area::TopInst(t) => s.put(row, 2, Attr::Blue, &ctx.text(t)),
            Area::Data(a) => data::render(ctx, a, s, row, height, shared),
            Area::List(a) => list::render(ctx, a, s, row, height),
            Area::Menu(a) => menu::render(ctx, a, s, row, height),
            Area::Info(a) => doc::render_area(ctx, &a.body, s, row, height, true),
            Area::Text(a) => doc::render_area(ctx, &a.body, s, row, height, false),
            Area::CmdLine(_) | Area::BotInst(_) => {}
        }
        row += height;
    }
}

/// A line of dashes across the usable width.
fn rule(s: &mut Screen, row: usize) {
    let dashes = "-".repeat(s.limit() - 1);
    s.put(row, 2, Attr::Normal, &dashes);
}

/// R10: `More...` or `Bottom` right-aligned to the last usable column.
pub(crate) fn indicator(s: &mut Screen, row: usize, bottom: bool) {
    let text = if bottom { "Bottom" } else { "More..." };
    let end = s.limit();
    s.put_right(row, end, Attr::HighIntensity, text);
    s.more = Some(More {
        row,
        col: end + 1 - text.len(),
        bottom,
    });
}

/// The cursor when no field asked for it: the first option field, else
/// the command line or selection, else the first field.
fn default_cursor(s: &Screen) -> Option<(usize, usize)> {
    let pick = |pred: &dyn Fn(&Target) -> bool| {
        s.fields
            .iter()
            .find(|f| pred(&f.target))
            .map(|f| (f.row, f.col))
    };
    pick(&|t| matches!(t, Target::ListOption { .. }))
        .or_else(|| pick(&|t| matches!(t, Target::CmdLine | Target::MenuSelection)))
        .or_else(|| pick(&|_| true))
}
