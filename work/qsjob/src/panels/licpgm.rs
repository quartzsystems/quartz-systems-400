// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! GO LICPGM's panels (`QSZLICPGM`): `QSLICPGM OPTION(n)` behind the
//! menu's options 1, 10 to 13 and 20 to 22, and the `WRKSFWAGR` and
//! `WRKLICINF` displays. The data comes from the service
//! (`licpgm_panel`); installs, deletes and saves run the commands.

use cmd::Resolved;
use mi::Job;
use stg::Client;

use super::certs::{exec, options, parm, show};
use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Choice, Response, UimDialog};

/// The driver of a command, if it is one of these.
pub fn run(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved, _out: &mut Outcome) -> Option<Result<String, Fail>> {
    let res = match r.name.as_str() {
        "QSLICPGM" => match parm(r, "OPTION").as_str() {
            "1" => install(job, host, surface, true),
            "10" => installed(host, surface),
            "11" => install(job, host, surface, false),
            "12" => select(job, host, surface, "LICPGM12"),
            "13" => select(job, host, surface, "LICPGM13"),
            o @ ("20" | "21" | "22") => languages(host, surface, o),
            o => Err(Fail::new("CPF9899", format!("option {o} is not a GO LICPGM panel"))),
        },
        "WRKSFWAGR" => agreements(host, surface),
        "WRKLICINF" => licenses(job, host, surface),
        _ => return None,
    };
    Some(res.map(|()| String::new()))
}

/// The service's rows, or its refusal as the command's escape.
fn rows(host: &Host, what: &str, args: &[String]) -> Result<Vec<Vec<String>>, Fail> {
    host.link
        .borrow_mut()
        .with(|l| l.licpgm_panel(what.into(), args.to_vec()))
        .map_err(|e| {
            let id = crate::command::jobsite::leading_msgid(&e).unwrap_or("CPF9899").to_string();
            Fail::new(&id, e)
        })
}

fn dialog(host: &Host) -> Result<UimDialog, Fail> {
    let mut d = crate::panels::dialog(host, "QSZLICPGM")?;
    d.copyright = false;
    Ok(d)
}

/// A list's rows from the service rows and the variables they fill.
fn fill(d: &mut UimDialog, list: &str, data: &[Vec<String>], vars: &[&str]) {
    d.clear_list(list);
    for r in data {
        let row = vars
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.is_empty())
            .map(|(i, v)| ((*v).to_string(), r.get(i).cloned().unwrap_or_default()))
            .collect();
        d.add_row(list, row);
    }
    d.list_mut(list).complete = true;
}

/// `INSTALLED` rows: product, option, status, text, release.
const INSTALLED: &[&str] = &["LPID", "LPOPT", "STATUS", "LPTEXT", "LPRLS"];

/// Option 10.
fn installed(host: &Host, surface: &mut Surface) -> Result<(), Fail> {
    let mut d = dialog(host)?;
    fill(&mut d, "INSTALLED", &rows(host, "INSTALLED", &[])?, INSTALLED);
    let mut panel = "LICPGM10";
    loop {
        match show(&mut d, surface, panel)? {
            Response::Return(11) => panel = if panel == "LICPGM10" { "LICPGM10R" } else { "LICPGM10" },
            Response::Return(19) => d.message = Some(("This system shows no trademarks.".into(), false)),
            Response::Exit | Response::Cancel | Response::Gone | Response::Return(_) => return Ok(()),
            _ => {}
        }
    }
}

/// Options 12 and 13: the installed options with 4=Delete or 1=Save.
fn select(job: &mut Job, host: &Host, surface: &mut Surface, panel: &str) -> Result<(), Fail> {
    let mut d = dialog(host)?;
    loop {
        fill(&mut d, "PROGRAMS", &rows(host, "INSTALLED", &[])?, &["LPID", "LPOPT", "STATUS", "LPTEXT", "LPRLS"]);
        match show(&mut d, surface, panel)? {
            Response::Options(choices) => {
                let mut none = |_: &mut Job, d: &mut UimDialog, c: &Choice| -> Result<(), Fail> {
                    d.message = Some((format!("Option {} is not valid here.", c.option), true));
                    Ok(())
                };
                options(job, host, &mut d, &choices, &mut none)?;
            }
            Response::Exit | Response::Cancel | Response::Gone => return Ok(()),
            _ => {}
        }
    }
}

/// The install options: the save file, what to restore. `None` when
/// the operator left.
fn install_options(host: &Host, surface: &mut Surface) -> Result<Option<(String, &'static str)>, Fail> {
    let mut d = dialog(host)?;
    d.set("INSOBJ", "1");
    d.set("INSIPL", "N");
    loop {
        match show(&mut d, surface, "LICINSOPT")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(None),
            Response::Return(_) => {
                let dev = d.get("INSDEV").trim().to_ascii_uppercase();
                let objs = match d.get("INSOBJ").trim() {
                    "1" => "*ALL",
                    "2" => "*PGM",
                    "3" => "*LNG",
                    _ => {
                        d.message = Some(("Type 1, 2 or 3 for the objects to install.".into(), true));
                        continue;
                    }
                };
                // A save file as LIBRARY/FILE, or an install device (OPT01).
                let device = !dev.contains('/')
                    && (1..=10).contains(&dev.len())
                    && dev.bytes().all(|b| b.is_ascii_alphanumeric());
                if dev.is_empty() || !(dev.contains('/') || device) {
                    d.message = Some((
                        "Type the install device, or the save file that holds the products as LIBRARY/FILE.".into(),
                        true,
                    ));
                    continue;
                }
                return Ok(Some((dev, objs)));
            }
            _ => {}
        }
    }
}

/// The software agreement of a product: whether it was accepted.
fn agreement(host: &Host, surface: &mut Surface, product: &str, rls: &str, text: &str) -> Result<bool, Fail> {
    let mut d = dialog(host)?;
    d.set("LPID", product);
    d.set("LPRLS", rls);
    d.set("LPTEXT", text);
    let lines = rows(host, "AGRTEXT", &[product.into(), rls.into()])?;
    fill(&mut d, "AGRTEXT", &lines, &["AGRLINE"]);
    loop {
        match show(&mut d, surface, "LICAGREE")? {
            Response::Return(14) => {
                rows(host, "ACCEPT", &[product.into(), rls.into()])?;
                return Ok(true);
            }
            Response::Return(15) | Response::Exit | Response::Cancel | Response::Gone => {
                rows(host, "DECLINE", &[product.into(), rls.into()])?;
                return Ok(false);
            }
            _ => {}
        }
    }
}

/// Options 11 and 1: the install options, the list of what the save
/// file holds (option 1 takes all of it), each product's agreement,
/// then `RSTLICPGM` per option.
fn install(job: &mut Job, host: &Host, surface: &mut Surface, all: bool) -> Result<(), Fail> {
    let Some((savf, objs)) = install_options(host, surface)? else {
        return Ok(());
    };
    // Media rows: product, option, type, load, release, text; one per
    // option (its code load).
    let media: Vec<Vec<String>> = rows(host, "MEDIA", std::slice::from_ref(&savf))?
        .into_iter()
        .filter(|r| r.get(2).is_some_and(|t| t == "*CODE"))
        .collect();
    let have = rows(host, "INSTALLED", &[])?;
    let status = |p: &str, o: &str| {
        have.iter()
            .find(|r| r[0] == p && r[1] == o)
            .map_or_else(String::new, |r| r[2].clone())
    };
    let mut picked: Vec<(String, String, String, String)> = Vec::new();
    if all {
        for r in &media {
            picked.push((r[0].clone(), r[1].clone(), r[4].clone(), r[5].clone()));
        }
    } else {
        let mut d = dialog(host)?;
        let list: Vec<Vec<String>> = media
            .iter()
            .map(|r| vec![r[0].clone(), r[1].clone(), status(&r[0], &r[1]), r[5].clone(), r[4].clone()])
            .collect();
        fill(&mut d, "PROGRAMS", &list, &["LPID", "LPOPT", "STATUS", "LPTEXT", "LPRLS"]);
        loop {
            match show(&mut d, surface, "LICPGM11")? {
                Response::Options(choices) => {
                    for c in &choices {
                        if c.option != "1" {
                            continue;
                        }
                        let cell = |v: &str| super::certs::cell(&d, &c.list, c.row, v);
                        picked.push((cell("LPID"), cell("LPOPT"), cell("LPRLS"), cell("LPTEXT")));
                    }
                    break;
                }
                Response::Exit | Response::Cancel | Response::Gone => return Ok(()),
                _ => {}
            }
        }
    }
    // Base options first, then the others, as RSTLICPGM needs.
    picked.sort_by_key(|(p, o, _, _)| (p.clone(), o != "*BASE", o.clone()));
    let mut accepted: Vec<String> = Vec::new();
    let mut declined: Vec<String> = Vec::new();
    for (p, o, rls, text) in &picked {
        if declined.contains(p) {
            continue;
        }
        if !accepted.contains(p) {
            if agreement(host, surface, p, rls, text)? {
                accepted.push(p.clone());
            } else {
                declined.push(p.clone());
                continue;
            }
        }
        let from = if savf.contains('/') {
            format!("DEV(*SAVF) SAVF({savf})")
        } else {
            format!("DEV({savf})")
        };
        let cmd = format!("RSTLICPGM LICPGM({p}) OPTION({o}) {from} RSTOBJ({objs})");
        if let Some((m, true)) = exec(job, host, &cmd) {
            return Err(Fail::new("CPF3D94", m));
        }
    }
    Ok(())
}

/// Options 20 to 22: QS/400 ships one language, so the lists are
/// the secondary language loads installed (none on a shipped system).
fn languages(host: &Host, surface: &mut Surface, option: &str) -> Result<(), Fail> {
    let mut d = dialog(host)?;
    d.set("PRILNG", "2924");
    d.set("PRILNGT", "English");
    d.clear_list("LANGS");
    d.list_mut("LANGS").complete = true;
    let panel = match option {
        "21" => "LICLNG21",
        "22" => "LICLNG22",
        _ => "LICLNG20",
    };
    loop {
        match show(&mut d, surface, panel)? {
            Response::Options(_) => d.message = Some(("No secondary language is available.".into(), true)),
            Response::Exit | Response::Cancel | Response::Gone | Response::Return(_) => return Ok(()),
            _ => {}
        }
    }
}

/// `WRKSFWAGR`: the agreements and 5=Display (with F14/F15).
fn agreements(host: &Host, surface: &mut Surface) -> Result<(), Fail> {
    let mut d = dialog(host)?;
    loop {
        // Rows: product, release, text, status.
        let data = rows(host, "AGREEMENTS", &[])?;
        let list: Vec<Vec<String>> = data
            .iter()
            .map(|r| vec![r[0].clone(), r[1].clone(), r[3].clone(), r[2].clone()])
            .collect();
        fill(&mut d, "AGRS", &list, &["LPID", "LPRLS", "AGRSTS", "LPTEXT"]);
        match show(&mut d, surface, "WRKSFWAGR")? {
            Response::Options(choices) => {
                for c in &choices {
                    if c.option == "5" {
                        let cell = |v: &str| super::certs::cell(&d, &c.list, c.row, v);
                        let (p, r, t) = (cell("LPID"), cell("LPRLS"), cell("LPTEXT"));
                        agreement(host, surface, &p, &r, &t)?;
                    }
                }
                d.list_mut("AGRS").options.clear();
            }
            Response::Exit | Response::Cancel | Response::Gone => return Ok(()),
            _ => {}
        }
    }
}

/// `WRKLICINF`: 2=Change, 5=Display license key.
fn licenses(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<(), Fail> {
    let mut d = dialog(host)?;
    loop {
        // Rows: product, term, feature, compliance, limit, count, ...
        let data = rows(host, "LICINF", &[])?;
        fill(&mut d, "LICS", &data, &["LPID", "LICTRM", "LICFTR", "LICCMP", "LICLMT", "LICCNT"]);
        match show(&mut d, surface, "WRKLICINF")? {
            Response::Options(choices) => {
                let mut none = |_: &mut Job, d: &mut UimDialog, c: &Choice| -> Result<(), Fail> {
                    d.message = Some((format!("Option {} is not valid here.", c.option), true));
                    Ok(())
                };
                options(job, host, &mut d, &choices, &mut none)?;
            }
            Response::Exit | Response::Cancel | Response::Gone => return Ok(()),
            _ => {}
        }
    }
}
