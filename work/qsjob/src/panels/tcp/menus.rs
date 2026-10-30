// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TCP/IP menus: `CFGTCP` with its related tables menu,
//! `CFGTCPAPP` and the applications' menus, `CFGTCPPTP`.

use mi::Job;

use super::{Left, dialog, exec, lists, show};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{CMDLINE_VAR, Response, UimDialog};

/// What a `RETURN n` option of a menu does.
type Options<'a> = &'a mut dyn FnMut(&mut Job, &Host, &mut Surface, &mut UimDialog, u32) -> Result<Option<Left>, Fail>;

/// Show a menu panel until it is left; `on` runs its `RETURN` options.
fn menu(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    d: &mut UimDialog,
    panel: &str,
    on: Options<'_>,
) -> Result<Left, Fail> {
    let mut last = String::new();
    loop {
        let resp = show(d, surface, panel)?;
        match resp {
            Response::Exit => return Ok(Left::Exit),
            Response::Cancel => return Ok(Left::Cancel),
            Response::Gone => return Ok(Left::Gone),
            Response::Command(text) => {
                last = text.trim_start_matches('?').to_string();
                d.message = exec(job, host, &text);
                d.set(CMDLINE_VAR, "");
            }
            Response::Retrieve => d.set(CMDLINE_VAR, &last),
            Response::Return(4) => {
                let typed = d.get(CMDLINE_VAR).trim().to_string();
                if typed.is_empty() {
                    d.message = Some(("Type a command to prompt on the command line.".into(), true));
                } else {
                    last.clone_from(&typed);
                    d.message = exec(job, host, &format!("?{}", typed.trim_start_matches('?')));
                    d.set(CMDLINE_VAR, "");
                }
            }
            Response::Return(n) if n > 1 => match on(job, host, surface, d, n)? {
                Some(Left::Exit) => return Ok(Left::Exit),
                Some(Left::Gone) => return Ok(Left::Gone),
                Some(Left::Cancel) | None => {}
            },
            Response::Print => d.message = Some((super::NO_PRINT.into(), true)),
            Response::Return(_) | Response::Options(_) | Response::Menu(_) | Response::Panel(_) => {}
        }
    }
}

/// `CFGTCP`.
pub fn cfgtcp(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<Left, Fail> {
    let mut d = dialog(host, "QTOCCFG", false)?;
    menu(job, host, surface, &mut d, "CFGTCP", &mut |job, host, surface, d, n| {
        Ok(match n {
            101 => Some(lists::ifcs(job, host, surface)?),
            102 => Some(lists::rtes(job, host, surface)?),
            104 => Some(lists::ports(job, host, surface)?),
            110 => Some(lists::hosts(job, host, surface)?),
            121 => {
                let mut t = dialog(host, "QTOCCFG", false)?;
                Some(menu(job, host, surface, &mut t, "CFGTBL", &mut |_, _, _, _, _| Ok(None))?)
            }
            123 => {
                d.message = Some(("IP filter rules are not available in this release.".into(), true));
                None
            }
            _ => None,
        })
    })
}

/// `CFGTCPAPP` (`panel` CFGTCPAPP) or one application's menu.
pub fn app(job: &mut Job, host: &Host, surface: &mut Surface, panel: &str) -> Result<Left, Fail> {
    let copyright = matches!(panel, "CFGSNMP" | "CFGTELN" | "CFGSMTP");
    let mut d = dialog(host, "QTOCAPP", copyright)?;
    menu(job, host, surface, &mut d, panel, &mut |_, _, _, _, _| Ok(None))
}

/// `CFGTCPPTP`: point-to-point needs PPP lines and modems, which this
/// release does not have.
pub fn ptp(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<Left, Fail> {
    let mut d = dialog(host, "QTOCCFG", false)?;
    menu(job, host, surface, &mut d, "CFGPTP", &mut |_, _, _, d, _| {
        d.message = Some(("Point-to-point connections are not available in this release.".into(), true));
        Ok(None)
    })
}
