// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKOUTQD` (`QGSPDSPQD`) and the spooled file viewer `DSPSPLF`
//! (`QGSPDSPF`).

use cmd::Resolved;
use mi::Job;

use super::viewer::{self, Control, WIDTH};
use super::{dialog, fetch, load, parm, qual, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

/// `WRKOUTQD OUTQ()`: the description; F13 prompts `CHGOUTQ`.
pub fn wrkoutqd(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let q = qual(r, "OUTQ", "*LIBL");
    let mut d = dialog(host, "QGSPDSPQD")?;
    load(&mut d, "", fetch(host, "OUTQD", &[&q])?);
    loop {
        match show(&mut d, surface, "WRKOUTQD")? {
            Response::Return(5) => load(&mut d, "", fetch(host, "OUTQD", &[&q])?),
            Response::Return(13) => {
                let lib = d.get("OUTQLIB");
                let name = d.get("OUTQ");
                super::exec(
                    job,
                    host,
                    &mut d,
                    &format!("?CHGOUTQ OUTQ({}/{})", lib.trim(), name.trim()),
                );
                load(&mut d, "", fetch(host, "OUTQD", &[&q])?);
            }
            _ => return Ok(String::new()),
        }
    }
}

const LIST: &str = "LINES";

/// The spooled file's lines and where each starts: page and line.
struct Splf {
    lines: Vec<String>,
    pos: Vec<(u32, u32)>,
    widest: usize,
}

fn read(host: &Host, args: &[&str]) -> Result<(Splf, stg::proto::PanelData), Fail> {
    let data = fetch(host, "SPLF", args)?;
    let get = |row: &Vec<(String, String)>, k: &str| {
        row.iter()
            .find(|(v, _)| v == k)
            .map(|(_, x)| x.clone())
            .unwrap_or_default()
    };
    let lines: Vec<String> = data.1.iter().map(|r| get(r, "TEXT")).collect();
    let pos = data
        .1
        .iter()
        .map(|r| {
            (
                get(r, "PAGE").parse().unwrap_or(0),
                get(r, "LNO").parse().unwrap_or(0),
            )
        })
        .collect();
    let widest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    Ok((Splf { lines, pos, widest }, (data.0, Vec::new())))
}

/// The lines as shown: cut at the window, or folded into rows of the
/// window's width; with the line each row starts at.
fn rows(f: &Splf, col: usize, fold: bool) -> (Vec<String>, Vec<usize>) {
    let mut out = Vec::new();
    let mut from = Vec::new();
    for (i, l) in f.lines.iter().enumerate() {
        if fold {
            let chars: Vec<char> = l.chars().collect();
            if chars.is_empty() {
                out.push(String::new());
                from.push(i);
            }
            for c in chars.chunks(WIDTH) {
                out.push(c.iter().collect());
                from.push(i);
            }
        } else {
            out.push(viewer::window(l, col, WIDTH));
            from.push(i);
        }
    }
    (out, from)
}

/// `DSPSPLF FILE() JOB() SPLNBR()`: the viewer.
#[allow(clippy::too_many_lines)] // one arm per key and Control command
pub fn dspsplf(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let file = parm(r, "FILE").unwrap_or_default();
    let job = parm(r, "JOB").filter(|j| j != "*").unwrap_or_default();
    // The special values arrive as their numbers (`*ONLY` 0, `*LAST` -1).
    let nbr = match parm(r, "SPLNBR").unwrap_or_default().as_str() {
        "0" => "*ONLY".to_string(),
        "-1" => "*LAST".to_string(),
        n => n.to_string(),
    };
    let mut hex = parm(r, "VIEW").is_some_and(|v| v == "*HEX");
    let mut d = dialog(host, "QGSPDSPF")?;
    let fetch_for = |hex: bool| -> Result<(Splf, stg::proto::PanelData), Fail> {
        read(host, &[&file, &job, &nbr, if hex { "*HEX" } else { "" }])
    };
    let (mut f, vars) = fetch_for(hex)?;
    load(&mut d, LIST, vars);
    let mut col = 0usize;
    loop {
        let fold = d.get("FOLD") == "Y";
        let (shown, from) = rows(&f, if fold { 0 } else { col }, fold);
        let top = d.list_mut(LIST).top.min(shown.len().saturating_sub(1));
        super::fill(
            &mut d,
            LIST,
            shown
                .iter()
                .map(|l| vec![("LINE".to_string(), l.clone())])
                .collect(),
        );
        d.list_mut(LIST).top = top;
        let line = from.get(top).copied().unwrap_or(0);
        let (pg, ln) = f.pos.get(line).copied().unwrap_or((1, 1));
        d.set(
            "PAGELINE",
            &if hex {
                format!("{}", line + 1)
            } else {
                format!("{pg}/{ln}")
            },
        );
        let first = if fold { 1 } else { col + 1 };
        d.set("COLS", &format!("{first} - {}", first + WIDTH - 1));
        d.set("RULER", &viewer::ruler(if fold { 0 } else { col }, WIDTH));
        let resp = show(&mut d, surface, "DSPSPLF")?;
        let ctl = d.get("CTL");
        d.set("CTL", "");
        let rows_n = shown.len();
        let mut top = d.list_mut(LIST).top;
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Return(19) => {
                if fold || col == 0 {
                    d.message = Some(("Already at the left edge.".into(), false));
                } else {
                    col = col.saturating_sub(WIDTH);
                }
            }
            Response::Return(20) => {
                if fold || col + WIDTH >= f.widest {
                    d.message = Some(("The right end of the lines is shown.".into(), false));
                } else {
                    col += WIDTH;
                }
            }
            Response::Return(11) => {
                d.set("FOLD", if fold { "N" } else { "Y" });
                top = 0;
            }
            Response::Return(16) => {
                let what = d.get("FIND").trim().to_string();
                if what.is_empty() {
                    d.message = Some(("Type the text to find on the Find line.".into(), true));
                } else {
                    let hit = shown
                        .iter()
                        .enumerate()
                        .skip(top + 1)
                        .chain(shown.iter().enumerate().take(top + 1))
                        .find(|(_, l)| l.contains(&what));
                    match hit {
                        Some((i, _)) => top = i,
                        None => d.message = Some((format!("{what} was not found."), true)),
                    }
                }
            }
            Response::Return(14) => {
                d.message = Some((
                    "Find options are not available in this release.".into(),
                    true,
                ));
            }
            Response::Return(_) => match viewer::parse(&ctl) {
                Control::None => {}
                Control::Down(n) => top = (top + n.unwrap_or(16)).min(rows_n.saturating_sub(1)),
                Control::Up(n) => top = top.saturating_sub(n.unwrap_or(16)),
                Control::Line(n) => {
                    top = from
                        .iter()
                        .position(|i| *i + 1 >= n)
                        .unwrap_or(rows_n.saturating_sub(1));
                }
                Control::Page(p) => {
                    let p = u32::try_from(p).unwrap_or(u32::MAX);
                    match f.pos.iter().position(|(pg, _)| *pg == p) {
                        Some(l) => top = from.iter().position(|i| *i == l).unwrap_or(0),
                        None => d.message = Some((format!("The file has no page {p}."), true)),
                    }
                }
                Control::Column(c) => {
                    if c > f.widest.max(1) {
                        d.message = Some((format!("The lines are not {c} columns wide."), true));
                    } else {
                        col = c - 1;
                    }
                }
                Control::Top => top = 0,
                Control::Bottom => top = rows_n.saturating_sub(16),
                Control::Hex(h) => {
                    if h != hex {
                        hex = h;
                        let (nf, vars) = fetch_for(hex)?;
                        f = nf;
                        load(&mut d, LIST, vars);
                        top = 0;
                        col = 0;
                    }
                }
                Control::Bad(why) => {
                    d.set("CTL", &ctl);
                    d.message = Some((why, true));
                }
            },
            _ => {
                d.message = Some((
                    "That function is not available in this release.".into(),
                    true,
                ));
            }
        }
        d.list_mut(LIST).top = top;
    }
}
