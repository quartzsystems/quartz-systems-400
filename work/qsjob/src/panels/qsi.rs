// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The QSI server's displays (`QSQSICSL`, docs/QSID.md section 12):
//! `STRQSIPAIR`'s pairing password, shown once (F12 ends it), and
//! `WRKQSICSL`'s consoles with option 4 to remove one.

use cmd::Resolved;
use mi::Job;
use stg::qsi::{QsiOp, QsiReply};

use crate::command::qsicmd::{self, ask, fail};
use crate::command::{Fail, Outcome};
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};
use uim::model::Action;

/// The driver of a command of this module, if it is one.
pub fn run(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
    _out: &mut Outcome,
) -> Option<Result<String, Fail>> {
    Some(match r.name.as_str() {
        "STRQSIPAIR" => strqsipair(host, surface),
        "WRKQSICSL" => wrkqsicsl(job, host, surface),
        _ => return None,
    })
}

fn show(d: &mut UimDialog, surface: &mut Surface, panel: &str) -> Result<Response, Fail> {
    d.show(surface, panel)
        .map_err(|e| crate::panels::failed(format!("panel {panel}: {e}")))
}

fn set_fingerprint(d: &mut UimDialog, hex: &str) {
    d.set("FPRINT1", hex.get(..32).unwrap_or(hex));
    d.set("FPRINT2", hex.get(32..).unwrap_or(""));
}

/// `STRQSIPAIR`: a password made and shown; F12 ends it.
fn strqsipair(host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let fp = qsicmd::fingerprint(host)?;
    let (text, until, attempts) = match ask(host, QsiOp::StartPairing { by: String::new() }) {
        QsiReply::Password {
            text,
            until,
            attempts,
        } => (text, until, attempts),
        r => return Err(fail(r)),
    };
    let mut d = crate::panels::dialog(host, "QSQSICSL")?;
    set_fingerprint(&mut d, &fp);
    d.set("PASSWORD", &text);
    d.set("UNTIL", &qsicmd::when(host, until, true));
    d.set("ATTEMPTS", &attempts.to_string());
    let shown = show(&mut d, surface, "QSIPAIR")?;
    // The password leaves the display's memory with it.
    drop(d);
    if matches!(shown, Response::Cancel) {
        return match ask(host, QsiOp::EndPairing { by: String::new() }) {
            QsiReply::Done => Ok(format!(
                "{} Pairing password ended.\n",
                servers::msg::QSI_PASSWORD_ENDED
            )),
            r => Err(fail(r)),
        };
    }
    let t = qsicmd::when(host, until, true);
    let time = t.split_whitespace().last().unwrap_or("").to_string();
    Ok(format!(
        "{} Pairing password made: good for one pairing until {time}.\n",
        servers::msg::QSI_PASSWORD
    ))
}

/// `WRKQSICSL`: the consoles, option 4 removes one.
fn wrkqsicsl(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = crate::panels::dialog(host, "QSQSICSL")?;
    loop {
        let fp = qsicmd::fingerprint(host).unwrap_or_default();
        set_fingerprint(&mut d, &fp);
        let rows = qsicmd::consoles(host)?
            .into_iter()
            .map(|c| {
                vec![
                    ("CNAME".to_string(), c.name.clone()),
                    ("PAIRED".to_string(), qsicmd::when(host, c.paired, false)),
                    (
                        "LASTSES".to_string(),
                        qsicmd::when(host, c.last_contact, false),
                    ),
                    (
                        "FPSHORT".to_string(),
                        c.fingerprint.get(..16).unwrap_or("").to_string(),
                    ),
                    ("FPFULL".to_string(), c.fingerprint.clone()),
                ]
            })
            .collect::<Vec<_>>();
        d.clear_list("CONSOLES");
        for r in rows {
            d.add_row("CONSOLES", r);
        }
        d.list_mut("CONSOLES").complete = true;
        match show(&mut d, surface, "QSICSL")? {
            Response::Exit | Response::Cancel | Response::Gone => return Ok(String::new()),
            Response::Options(choices) => {
                for c in &choices {
                    if let Some(Action::Cmd(t)) = &c.action {
                        let text = crate::panels::option_command(&d, c, t);
                        if let Some(m) = crate::panels::exec(job, host, &text) {
                            d.send(m);
                        }
                    }
                }
                for c in &choices {
                    d.list_mut(&c.list).options.clear();
                }
            }
            _ => {}
        }
    }
}
