// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The certificate displays (`QSCERT`, Quartz extensions): `QSWRKCRT`
//! with its option 5 display, and `QSWRKAPPID`. The data comes from
//! the storage service (`cert_panel`), which opens the stores.

use cmd::{Resolved, Value};
use mi::Job;
use stg::Client;
use stg::proto::Sys2Data;

use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Choice, Response, UimDialog};

/// The driver of a command, if it is one of these.
pub fn run(job: &mut Job, host: &Host, surface: &mut Surface, r: &Resolved, _out: &mut Outcome) -> Option<Result<String, Fail>> {
    let res = match r.name.as_str() {
        "QSWRKCRT" => wrkcrt(job, host, surface, &parm(r, "KEYSTORE"), &parm(r, "PASSWORD")),
        "QSWRKAPPID" => wrkappid(job, host, surface, &parm(r, "APPID")),
        _ => return None,
    };
    Some(res.map(|()| String::new()))
}

/// A parameter's value as typed or defaulted, quotes taken off.
fn parm(r: &Resolved, kwd: &str) -> String {
    r.get(kwd)
        .and_then(|b| b.value.as_ref())
        .map(Value::source)
        .unwrap_or_default()
        .trim()
        .trim_matches('\'')
        .trim()
        .to_string()
}

/// The service's data, or its refusal as the command's escape.
fn fetch(host: &Host, what: &str, args: &[String]) -> Result<Sys2Data, Fail> {
    host.link
        .borrow_mut()
        .with(|l| l.cert_panel(what.into(), args.to_vec()))
        .map_err(|e| {
            let id = crate::command::jobsite::leading_msgid(&e)
                .unwrap_or("TCP1C17")
                .to_string();
            Fail::new(&id, e)
        })
}

fn dialog(host: &Host) -> Result<UimDialog, Fail> {
    let mut d = crate::panels::dialog(host, "QSCERT")?;
    d.copyright = false;
    Ok(d)
}

fn show(d: &mut UimDialog, surface: &mut Surface, panel: &str) -> Result<Response, Fail> {
    d.show(surface, panel)
        .map_err(|e| Fail::new("CPF6A12", format!("panel {panel}: {e}")))
}

/// The data into the dialog, lists kept at their page.
fn fill(d: &mut UimDialog, data: &Sys2Data) {
    for (k, v) in &data.0 {
        d.set(k, v);
    }
    for (name, rows) in &data.1 {
        let top = d.list_mut(name).top;
        d.clear_list(name);
        let n = rows.len();
        for r in rows {
            d.add_row(name, r.clone());
        }
        let l = d.list_mut(name);
        l.complete = true;
        l.top = if top < n { top } else { 0 };
    }
}

fn cell(d: &UimDialog, list: &str, row: usize, var: &str) -> String {
    d.row(list, row)
        .into_iter()
        .find(|(v, _)| v == var)
        .map(|(_, x)| x.trim().to_string())
        .unwrap_or_default()
}

/// Run a command from a panel: the line to show after it.
fn exec(job: &mut Job, host: &Host, text: &str) -> Option<(String, bool)> {
    let out = crate::command::run(job, host, text);
    if let Some(f) = &out.escape {
        if f.msgid == "CPF6801" {
            return None;
        }
        let line = f.text.lines().last().unwrap_or("").trim().to_string();
        let line = if line.is_empty() { format!("{} ended with {}.", out.name, f.msgid) } else { line };
        return Some((line, true));
    }
    out.text
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .map(|l| (l.to_string(), false))
}

/// Run the options typed: a `CMD` action with the entry's values, else
/// what `special` does with it.
fn options(
    job: &mut Job,
    host: &Host,
    d: &mut UimDialog,
    choices: &[Choice],
    special: &mut dyn FnMut(&mut Job, &mut UimDialog, &Choice) -> Result<(), Fail>,
) -> Result<(), Fail> {
    for c in choices {
        if let Some(uim::model::Action::Cmd(t)) = &c.action {
            let text = d.substitute(t, Some((&c.list, c.row)));
            if let Some(m) = exec(job, host, &text) {
                d.message = Some(m);
            }
        } else {
            special(job, d, c)?;
        }
    }
    for c in choices {
        d.list_mut(&c.list).options.clear();
    }
    Ok(())
}

/// `QSWRKCRT KEYSTORE() PASSWORD()`.
fn wrkcrt(job: &mut Job, host: &Host, surface: &mut Surface, store: &str, password: &str) -> Result<(), Fail> {
    let args = vec![store.to_string(), password.to_string()];
    let mut d = dialog(host)?;
    let mut last = String::new();
    loop {
        let data = fetch(host, "CRT", &args)?;
        fill(&mut d, &data);
        match show(&mut d, surface, "WRKCRT")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(()),
            Response::Options(choices) => {
                let mut five = |_: &mut Job, d: &mut UimDialog, c: &Choice| -> Result<(), Fail> {
                    if c.option != "5" {
                        d.message = Some((format!("Option {} is not valid here.", c.option), true));
                        return Ok(());
                    }
                    let label = cell(d, &c.list, c.row, "CRTLABEL");
                    let mut a = args.clone();
                    a.push(label);
                    display(host, surface, &a)
                };
                options(job, host, &mut d, &choices, &mut five)?;
            }
            Response::Return(6) => {
                let ks = d.get("KEYSTORE").trim().to_string();
                d.message = exec(job, host, &format!("?QSCRTCRT KEYSTORE('{ks}')"));
            }
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                d.message = exec(job, host, &text);
                d.set(CMDLINE_VAR, "");
            }
            Response::Retrieve => d.set(CMDLINE_VAR, &last),
            Response::Print => d.message = Some(("Print is not available in this release.".into(), true)),
            Response::Return(_) | Response::Menu(_) | Response::Panel(_) => {}
        }
    }
}

/// Option 5: one certificate until it is left.
fn display(host: &Host, surface: &mut Surface, args: &[String]) -> Result<(), Fail> {
    let data = fetch(host, "DSPCRT", args)?;
    let mut d = dialog(host)?;
    fill(&mut d, &data);
    loop {
        match show(&mut d, surface, "DSPCRT")? {
            Response::Print => d.message = Some(("Print is not available in this release.".into(), true)),
            Response::Return(r) if r != 0 => {}
            _ => return Ok(()),
        }
    }
}

/// `QSWRKAPPID APPID()`.
fn wrkappid(job: &mut Job, host: &Host, surface: &mut Surface, which: &str) -> Result<(), Fail> {
    let mut d = dialog(host)?;
    let mut last = String::new();
    loop {
        let mut data = fetch(host, "APPID", &[])?;
        for (_, rows) in &mut data.1 {
            rows.retain(|r| r.iter().any(|(k, v)| k == "APPID" && generic(which, v)));
        }
        fill(&mut d, &data);
        match show(&mut d, surface, "WRKAPPID")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(()),
            Response::Options(choices) => {
                let mut none = |_: &mut Job, d: &mut UimDialog, c: &Choice| -> Result<(), Fail> {
                    d.message = Some((format!("Option {} is not valid here.", c.option), true));
                    Ok(())
                };
                options(job, host, &mut d, &choices, &mut none)?;
            }
            Response::Return(6) => d.message = exec(job, host, "?QSADDAPPID"),
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                d.message = exec(job, host, &text);
                d.set(CMDLINE_VAR, "");
            }
            Response::Retrieve => d.set(CMDLINE_VAR, &last),
            Response::Print => d.message = Some(("Print is not available in this release.".into(), true)),
            Response::Return(_) | Response::Menu(_) | Response::Panel(_) => {}
        }
    }
}

/// `*ALL`, a generic `ABC*`, or one ID.
fn generic(pattern: &str, id: &str) -> bool {
    let p = pattern.trim();
    if p.is_empty() || p.eq_ignore_ascii_case("*ALL") {
        return true;
    }
    match p.strip_suffix('*') {
        Some(prefix) => id.to_ascii_uppercase().starts_with(&prefix.to_ascii_uppercase()),
        None => id.eq_ignore_ascii_case(p),
    }
}

#[cfg(test)]
mod tests {
    use super::generic;

    #[test]
    fn generic_ids() {
        assert!(generic("*ALL", "QIBM_X"));
        assert!(generic("QIBM_QZBS*", "QIBM_QZBS_SVR_FILE"));
        assert!(!generic("QIBM_QTV*", "QIBM_QZBS_SVR_FILE"));
    }
}
