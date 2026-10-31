// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKHDWRSC` and `DSPHDWRSC` at a display (`QUIHDWRSC`): the table
//! comes from the hardware resource service through the storage
//! service's link, the rows from `hwrsrc::view`. Option 7 shows a
//! resource's detail, 5 (and 9 of storage) the configuration
//! descriptions that use it and the resources under it.

use hwrsrc::Table;
use hwrsrc::view::{self, TypeCol, TypeSpec};
use mi::Job;
use stg::Client;

use super::{cell, dialog, exec, fill, parm, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::Response;

type Rows = Vec<Vec<(String, String)>>;

/// The table, or the escape that ends the command.
fn table(host: &Host, refresh: bool) -> Result<Table, Fail> {
    host.link
        .borrow_mut()
        .with(|l| l.hw_table(refresh))
        .map_err(|e| Fail::new("CPF9898", e.trim_start_matches("CPF9898 ").to_string()))
}

/// The panel and list of a type's display.
fn panel_of(spec: &TypeSpec, work: bool) -> (&'static str, &'static str) {
    let list = match (spec.ty, spec.col) {
        ("*CSA", _) => "RSCCSA",
        (_, TypeCol::Type) => "RSCS",
        (_, TypeCol::TypeModel) => "RSCTMS",
    };
    if work {
        (spec.panel, list)
    } else if spec.col == TypeCol::Type {
        ("DSPTYPE", "RSCS")
    } else {
        ("DSPTM", "RSCTMS")
    }
}

/// The list's entries, the variables of `list`.
fn rows(rows: &[view::Row], list: &str) -> Rows {
    rows.iter()
        .map(|r| {
            let v = |k: &str, x: String| (k.to_string(), x);
            match list {
                "RSCS" => vec![
                    v("RSCNAME", r.indented()),
                    v("RSCTYPE", r.type_col.clone()),
                    v("RSCSTS", r.status.clone()),
                    v("RSCTEXT", r.text.clone()),
                ],
                "RSCCSA" => vec![
                    v("RSCNAM12", r.indented()),
                    v("RSCTM", r.type_col.clone()),
                    v("RSCSTS", r.status.clone()),
                    v("RSCTXT14", r.text.clone()),
                ],
                _ => vec![
                    v("RSCNAME", r.indented()),
                    v("RSCTM", r.type_col.clone()),
                    v("RSCSTS", r.status.clone()),
                    v("RSCTXT22", r.text.clone()),
                ],
            }
        })
        .collect()
}

/// The name variable of a list.
fn name_var(list: &str) -> &'static str {
    if list == "RSCCSA" { "RSCNAM12" } else { "RSCNAME" }
}

/// Run the display; `None` when the command is not one for a panel
/// (an outfile), so the service's processing program runs it.
pub fn run(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &cmd::Resolved,
    work: bool,
) -> Option<Result<String, Fail>> {
    if !work && parm(r, "OUTPUT") == "*OUTFILE" {
        return None;
    }
    let ty = parm(r, "TYPE");
    let Some(spec) = view::spec(&ty) else {
        return Some(Err(Fail::new("CPD0084", format!("{ty} is not a type of resource."))));
    };
    Some(list_loop(job, host, surface, spec, work))
}

fn list_loop(job: &mut Job, host: &Host, surface: &mut Surface, spec: &TypeSpec, work: bool) -> Result<String, Fail> {
    let mut d = dialog(host, "QUIHDWRSC")?;
    d.copyright = false;
    let (panel, list) = panel_of(spec, work);
    let mut refresh = false;
    loop {
        let t = table(host, refresh)?;
        refresh = false;
        d.set("RSCKIND", spec.word);
        fill(&mut d, list, rows(&view::list(&t, spec), list));
        match show(&mut d, surface, panel)? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Return(0) if !work => return Ok(String::new()),
            Response::Options(c) => {
                for ch in &c {
                    let name = cell(&d, &ch.list, ch.row, name_var(list));
                    let next = match (ch.option.as_str(), spec.ty) {
                        ("7", _) => detail(host, surface, &t, &name)?,
                        ("5", "*CMN" | "*LWS" | "*CRP") => cfgds(job, host, surface, &t, &name)?,
                        ("9", "*STG") => sub_list(job, host, surface, &name)?,
                        (o, _) => {
                            d.message = Some((format!("Option {o} is not valid for resource {name}."), true));
                            Next::Back
                        }
                    };
                    if next == Next::Exit {
                        return Ok(String::new());
                    }
                }
                d.list_mut(list).options.clear();
            }
            Response::Print => d.message = print(job, host, spec),
            Response::Return(5) => refresh = true,
            _ => {}
        }
    }
}

/// Where a second display leaves the operator.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Next {
    /// Back to the list.
    Back,
    /// F3: out of the command.
    Exit,
}

/// F6: the list printed by the service.
fn print(job: &mut Job, host: &Host, spec: &TypeSpec) -> Option<(String, bool)> {
    exec(job, host, &format!("DSPHDWRSC TYPE({}) OUTPUT(*PRINT)", spec.ty))
}

/// Option 7: the resource's detail.
fn detail(host: &Host, surface: &mut Surface, t: &Table, name: &str) -> Result<Next, Fail> {
    let mut d = dialog(host, "QUIHDWRSC")?;
    d.copyright = false;
    let mut t = t.clone();
    loop {
        let Some(x) = view::detail(&t, name) else {
            return Ok(Next::Back);
        };
        d.set("DRSC", &x.name);
        d.set("DTEXT", &x.text);
        d.set("DTM", &x.type_model);
        d.set("DSERIAL", &x.serial);
        d.set("DPART", &x.part);
        d.set("DLOC", &x.location);
        d.clear_list("DLINES");
        for l in &x.lines {
            d.add_row("DLINES", vec![("DLINE".into(), l.clone())]);
        }
        d.list_mut("DLINES").complete = true;
        match show(&mut d, surface, "DETAIL")? {
            Response::Exit | Response::Gone => return Ok(Next::Exit),
            Response::Return(5) => t = table(host, true)?,
            Response::Print => d.message = Some(("Print the list with F6 from the resource list.".into(), false)),
            _ => return Ok(Next::Back),
        }
    }
}

/// The configuration descriptions of `name`: a first row to create one,
/// then each line and device description that names it.
fn cfgd_rows(host: &Host, name: &str) -> (Rows, Vec<(String, String)>) {
    let found = host.link.borrow_mut().with(|l| l.hw_cfgds(name.to_string()));
    let mut rows: Rows = vec![vec![("CFGD".into(), String::new())]];
    for (n, _) in &found {
        rows.push(vec![("CFGD".into(), n.clone())]);
    }
    (rows, found)
}

/// Option 5: work with the configuration descriptions of a resource.
fn cfgds(job: &mut Job, host: &Host, surface: &mut Surface, t: &Table, name: &str) -> Result<Next, Fail> {
    let mut d = dialog(host, "QUIHDWRSC")?;
    d.copyright = false;
    let text = t.get(name).map(|r| r.kind.text().to_string()).unwrap_or_default();
    let kind = t.get(name).map(|r| r.kind);
    d.set("DRSC", name);
    d.set("DTEXT", &text);
    loop {
        let (rows, found) = cfgd_rows(host, name);
        fill(&mut d, "CFGDS", rows);
        match show(&mut d, surface, "CFGD")? {
            Response::Exit | Response::Gone => return Ok(Next::Exit),
            Response::Cancel => return Ok(Next::Back),
            Response::Options(c) => {
                for ch in &c {
                    let cfg = ch.row.checked_sub(1).and_then(|i| found.get(i));
                    let line = match (ch.option.as_str(), cfg) {
                        ("1", None) => match kind {
                            Some(hwrsrc::Kind::Port) => Some(format!("?CRTLINETH RSRCNAME({name})")),
                            Some(hwrsrc::Kind::Tape) => Some(format!("?CRTDEVTAP RSRCNAME({name})")),
                            Some(hwrsrc::Kind::Optical) => Some(format!("?CRTDEVOPT RSRCNAME({name})")),
                            Some(hwrsrc::Kind::TapeLibrary) => Some(format!("?CRTDEVMLB RSRCNAME({name})")),
                            _ => None,
                        },
                        ("5", Some((n, ty))) if ty == "*LIND" => Some(format!("WRKLIND LIND({n})")),
                        ("5", Some((n, _))) => Some(format!("WRKDEVD DEVD({n})")),
                        ("8", Some((n, ty))) if ty == "*LIND" => Some(format!("WRKCFGSTS CFGTYPE(*LIN) CFGD({n})")),
                        ("8", Some((n, _))) => Some(format!("WRKCFGSTS CFGTYPE(*DEV) CFGD({n})")),
                        _ => None,
                    };
                    d.message = match line {
                        Some(l) => exec(job, host, &l),
                        None => Some((format!("Option {} is not valid on this row.", ch.option), true)),
                    };
                }
                d.list_mut("CFGDS").options.clear();
            }
            Response::Print => d.message = Some(("There is nothing to print here.".into(), false)),
            _ => {}
        }
    }
}

/// Option 9 of storage: the resource and those it drives, each with
/// its options.
fn sub_list(job: &mut Job, host: &Host, surface: &mut Surface, name: &str) -> Result<Next, Fail> {
    let mut dlg = dialog(host, "QUIHDWRSC")?;
    dlg.copyright = false;
    let mut refresh = false;
    loop {
        let tbl = table(host, refresh)?;
        refresh = false;
        let Some(top) = tbl.get(name) else { return Ok(Next::Back) };
        dlg.set("RSCKIND", top.kind.text());
        let picked = tbl.tree(|x| x.name == top.name || x.logical == top.name, true, false);
        let list: Vec<view::Row> = picked
            .iter()
            .map(|tr| {
                let res = &tbl.resources[tr.index];
                view::Row {
                    name: res.name.clone(),
                    depth: tr.depth,
                    type_col: format!(" {}", view::type_model(res)),
                    status: res.status.text().into(),
                    text: res.kind.text().into(),
                }
            })
            .collect();
        fill(&mut dlg, "RSCTMS", rows(&list, "RSCTMS"));
        match show(&mut dlg, surface, "HWSUB")? {
            Response::Exit | Response::Gone => return Ok(Next::Exit),
            Response::Cancel => return Ok(Next::Back),
            Response::Options(c) => {
                for ch in &c {
                    let rsc = cell(&dlg, &ch.list, ch.row, "RSCNAME");
                    let next = match ch.option.as_str() {
                        "7" => detail(host, surface, &tbl, &rsc)?,
                        "5" => cfgds(job, host, surface, &tbl, &rsc)?,
                        o => {
                            dlg.message = Some((format!("Option {o} is not valid for resource {rsc}."), true));
                            Next::Back
                        }
                    };
                    if next == Next::Exit {
                        return Ok(Next::Exit);
                    }
                }
                dlg.list_mut("RSCTMS").options.clear();
            }
            Response::Return(5) => refresh = true,
            _ => {}
        }
    }
}

