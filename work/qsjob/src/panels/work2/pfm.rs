// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The member viewer `DSPPFM` (`QUIDSPPFM`): characters, hexadecimal
//! side by side (F10) or under the characters (F11).

use cmd::Resolved;

use super::viewer::{self, Control, SIDE_BYTES, WIDTH};
use super::{dialog, fetch, load, parm, qual, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

const LIST: &str = "LINES";

/// One record: its number, text and bytes in hexadecimal.
struct Rec {
    rrn: String,
    text: String,
    hex: String,
}

/// The rows a mode shows from column `col`, with the record each row
/// belongs to; the end-of-data row last.
fn rows(recs: &[Rec], mode: &str, col: usize) -> (Vec<String>, Vec<usize>) {
    let mut out = Vec::new();
    let mut of = Vec::new();
    for (i, r) in recs.iter().enumerate() {
        match mode {
            "S" => {
                out.push(viewer::side(&r.hex, &r.text, col));
                of.push(i);
            }
            "V" => {
                for l in viewer::vertical(&r.hex, &r.text, col, WIDTH) {
                    out.push(l);
                    of.push(i);
                }
                out.push(String::new());
                of.push(i);
            }
            _ => {
                out.push(viewer::window(&r.text, col, WIDTH));
                of.push(i);
            }
        }
    }
    out.push(
        format!("{:^78}", "****** END OF DATA ******")
            .trim_end()
            .to_string(),
    );
    of.push(recs.len());
    (out, of)
}

/// How far F19 and F20 move in a mode.
fn step(mode: &str) -> usize {
    if mode == "S" { SIDE_BYTES } else { WIDTH }
}

/// `DSPPFM FILE() MBR() FROMRCD()`.
#[allow(clippy::too_many_lines)] // one arm per key and Control command
pub fn dsppfm(host: &Host, surface: &mut Surface, r: &Resolved) -> Result<String, Fail> {
    let file = qual(r, "FILE", "*LIBL");
    let mbr = parm(r, "MBR").filter(|m| m != "*FIRST").unwrap_or_default();
    let from: usize = parm(r, "FROMRCD").and_then(|n| n.parse().ok()).unwrap_or(1);
    let mut d = dialog(host, "QUIDSPPFM")?;
    let mut data = fetch(host, "PFM", &[&file, &mbr])?;
    let recs: Vec<Rec> = std::mem::take(&mut data.1)
        .into_iter()
        .map(|row| {
            let get = |k: &str| {
                row.iter()
                    .find(|(v, _)| v == k)
                    .map(|(_, x)| x.clone())
                    .unwrap_or_default()
            };
            Rec {
                rrn: get("RRN"),
                text: get("TEXT"),
                hex: get("HEX"),
            }
        })
        .collect();
    load(&mut d, LIST, data);
    let reclen = recs
        .iter()
        .map(|r| r.text.chars().count())
        .max()
        .unwrap_or(0);
    let mut col = 0usize;
    let mut top = recs
        .iter()
        .position(|r| r.rrn.parse::<usize>().is_ok_and(|n| n >= from))
        .unwrap_or(0);
    loop {
        let mode = d.get("MODE");
        let (shown, of) = rows(&recs, &mode, col);
        top = top.min(shown.len().saturating_sub(1));
        super::fill(
            &mut d,
            LIST,
            shown
                .iter()
                .map(|l| vec![("LINE".to_string(), l.clone())])
                .collect(),
        );
        d.list_mut(LIST).top = top;
        let rec = of.get(top).copied().unwrap_or(0);
        d.set(
            "RECORD",
            &recs
                .get(rec)
                .map_or_else(|| "*END".to_string(), |r| r.rrn.clone()),
        );
        d.set("COLUMN", &(col + 1).to_string());
        let width = if mode == "S" { SIDE_BYTES } else { WIDTH };
        d.set(
            "RULER",
            &if mode == "S" {
                viewer::side_ruler(col)
            } else {
                viewer::ruler(col, width.min(reclen.saturating_sub(col).max(1)))
            },
        );
        let resp = show(&mut d, surface, "DSPPFM")?;
        let ctl = d.get("CTL");
        d.set("CTL", "");
        top = d.list_mut(LIST).top;
        // The record at the top, whatever the mode shows next.
        let at = of.get(top).copied().unwrap_or(0);
        // Which rows hold which record does not depend on the column.
        let first_row = |mode: &str, rec: usize| {
            rows(&recs, mode, 0)
                .1
                .iter()
                .position(|i| *i >= rec)
                .unwrap_or(0)
        };
        match resp {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Return(19) => {
                if col == 0 {
                    d.message = Some(("Already at the left edge.".into(), false));
                } else {
                    col = col.saturating_sub(step(&mode));
                }
            }
            Response::Return(20) => {
                if col + step(&mode) >= reclen {
                    d.message = Some(("The end of the record is shown.".into(), false));
                } else {
                    col += step(&mode);
                }
            }
            Response::Return(10) => {
                let next = if mode == "C" { "S" } else { "C" };
                d.set("MODE", next);
                col = 0;
                top = first_row(next, at);
            }
            Response::Return(11) => {
                let next = if mode == "V" { "S" } else { "V" };
                d.set("MODE", next);
                col = 0;
                top = first_row(next, at);
            }
            Response::Return(_) => {
                let what = d.get("FIND").trim().to_string();
                if !what.is_empty() && ctl.trim().is_empty() {
                    match recs
                        .iter()
                        .skip(at + 1)
                        .position(|r| r.text.contains(&what))
                    {
                        Some(i) => top = first_row(&mode, at + 1 + i),
                        None => d.message = Some((format!("{what} was not found."), true)),
                    }
                }
                let last = shown.len().saturating_sub(1);
                match viewer::parse(&ctl) {
                    Control::None => {}
                    Control::Down(n) => {
                        top = first_row(&mode, (at + n.unwrap_or(1)).min(recs.len()));
                    }
                    Control::Up(n) => top = first_row(&mode, at.saturating_sub(n.unwrap_or(1))),
                    Control::Line(n) => {
                        let i = recs
                            .iter()
                            .position(|r| r.rrn.parse::<usize>().is_ok_and(|x| x >= n))
                            .unwrap_or(recs.len());
                        top = first_row(&mode, i);
                    }
                    Control::Column(c) => {
                        if c > reclen.max(1) {
                            d.message =
                                Some((format!("The records are not {c} columns long."), true));
                        } else {
                            col = c - 1;
                        }
                    }
                    Control::Top => top = 0,
                    Control::Bottom => top = last.saturating_sub(15),
                    Control::Hex(h) => {
                        let next = if h { "S" } else { "C" };
                        d.set("MODE", next);
                        top = first_row(next, at);
                    }
                    Control::Page(_) => {
                        d.set("CTL", &ctl);
                        d.message =
                            Some(("A member has no pages: type a record number.".into(), true));
                    }
                    Control::Bad(why) => {
                        d.set("CTL", &ctl);
                        d.message = Some((why, true));
                    }
                }
            }
            _ => {
                d.message = Some((
                    "That function is not available in this release.".into(),
                    true,
                ));
            }
        }
    }
}
