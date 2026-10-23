// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statistics displays: `WRKSYSSTS` (`QGWCSSTS`), `WRKDSKSTS`
//! (`QGWCDSKS`), `WRKSYSACT` (`QGPYMON`), and `WRKSHRPOOL`
//! (`QGWCPOOL`). A shared pool's size and activity level typed over
//! either pool list change the pool (`CHGSHRPOOL`) on Enter.

use std::fmt::Write as _;

use cmd::Resolved;
use mi::Job;

use super::{Step, dialog, exec, fetch, load, others, parm, work};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

const POOLS: &str = "POOLS";

/// The date and time row of `TOPSEP=DATETIME`.
fn stamp(host: &Host, d: &mut UimDialog) {
    d.set("*DATETIME", &crate::panels::work::stamp(host));
}

/// Megabytes typed over a size, as kilobytes.
fn kb(text: &str) -> Option<u64> {
    let t = text.trim().replace(',', ".");
    let v: f64 = t.parse().ok()?;
    if !(0.0..1.0e9).contains(&v) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in range, checked above
    Some((v * 1024.0).round() as u64)
}

/// Keep each pool entry's size and activity level as shown, to see on
/// Enter what was typed over them.
fn remember(d: &mut UimDialog, act: &str) {
    for row in &mut d.list_mut(POOLS).rows {
        let get = |k: &str| {
            row.iter()
                .find(|(v, _)| v == k)
                .map(|(_, x)| x.clone())
                .unwrap_or_default()
        };
        let (size, level) = (get("SIZE"), get(act));
        row.push(("SIZE0".into(), size));
        row.push(("ACT0".into(), level));
    }
}

/// Run `CHGSHRPOOL` for each shared pool whose size or activity level
/// was typed over; whether anything was.
fn apply(job: &mut Job, host: &Host, d: &mut UimDialog, name: &str, act: &str) -> bool {
    let rows = d.list_mut(POOLS).rows.clone();
    let mut changed = false;
    for row in rows {
        let get = |k: &str| {
            row.iter()
                .find(|(v, _)| v == k)
                .map(|(_, x)| x.trim().to_string())
                .unwrap_or_default()
        };
        let pool = get(name);
        let mut parms = String::new();
        if get("SIZE") != get("SIZE0") {
            let Some(k) = kb(&get("SIZE")) else {
                d.message = Some((
                    format!("The size of {pool} is not a number of megabytes."),
                    true,
                ));
                return true;
            };
            let _ = write!(parms, " SIZE({k})");
        }
        if get(act) != get("ACT0") {
            let _ = write!(parms, " ACTLVL({})", get(act));
        }
        if parms.is_empty() {
            continue;
        }
        changed = true;
        if pool.starts_with('*') {
            exec(job, host, d, &format!("CHGSHRPOOL POOL({pool}){parms}"));
        } else {
            d.message = Some((
                format!("Pool {pool} is a subsystem's own pool; change it with CHGSBSD."),
                true,
            ));
        }
    }
    changed
}

/// `WRKSYSSTS [RESET(*YES)]`: F10 restarts the measurement, F11 shows
/// the transitions.
pub fn wrksyssts(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let mut d = dialog(host, "QGWCSSTS")?;
    let mut reset = parm(r, "RESET").is_some_and(|v| v == "*YES");
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let data = fetch(host, "SYSSTS", &[if reset { "*YES" } else { "*NO" }])?;
        reset = false;
        load(d, POOLS, data);
        remember(d, "MAXACT");
        stamp(host, d);
        Ok(())
    };
    let mut keys =
        |job: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
            Ok(match resp {
                Response::Return(1) => {
                    apply(job, host, d, "POOLNAME", "MAXACT");
                    Step::Reload
                }
                Response::Return(10) => {
                    // The next look measures from now.
                    fetch(host, "SYSSTS", &["*YES"])?;
                    Step::Reload
                }
                Response::Return(11) => {
                    d.view = 1 - d.view.min(1);
                    d.set("VIEW", &(d.view + 1).to_string());
                    Step::Stay
                }
                other => others(d, &other),
            })
        };
    work(
        job,
        host,
        surface,
        &mut d,
        "WRKSYSSTS",
        &mut reload,
        &mut keys,
    )
}

/// `WRKSHRPOOL`: the shared pools.
pub fn wrkshrpool(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QGWCPOOL")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        load(d, POOLS, fetch(host, "SHRPOOL", &[])?);
        remember(d, "ACTLVL");
        Ok(())
    };
    let mut keys =
        |job: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
            Ok(match resp {
                Response::Return(1) => {
                    apply(job, host, d, "POOL", "ACTLVL");
                    Step::Reload
                }
                Response::Return(11) => {
                    d.view = 1 - d.view.min(1);
                    Step::Stay
                }
                other => others(d, &other),
            })
        };
    work(
        job,
        host,
        surface,
        &mut d,
        "WRKSHRPOOL",
        &mut reload,
        &mut keys,
    )
}

/// `WRKDSKSTS [RESET(*YES)]`: F10 restarts the measurement.
pub fn wrkdsksts(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let mut d = dialog(host, "QGWCDSKS")?;
    let mut reset = parm(r, "RESET").is_some_and(|v| v == "*YES");
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let data = fetch(host, "DSKSTS", &[if reset { "*YES" } else { "*NO" }])?;
        reset = false;
        load(d, "UNITS", data);
        stamp(host, d);
        Ok(())
    };
    let mut keys =
        |_: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
            Ok(match resp {
                Response::Return(10) => {
                    fetch(host, "DSKSTS", &["*YES"])?;
                    Step::Reload
                }
                other => others(d, &other),
            })
        };
    work(
        job,
        host,
        surface,
        &mut d,
        "WRKDSKSTS",
        &mut reload,
        &mut keys,
    )
}

/// `WRKSYSACT`: F10 looks again.
pub fn wrksysact(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QGPYMON")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        load(d, "JOBS", fetch(host, "SYSACT", &[])?);
        stamp(host, d);
        Ok(())
    };
    let mut keys = |_: &mut Job, _: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
        Ok(match resp {
            Response::Return(10) => Step::Reload,
            other => others(d, &other),
        })
    };
    work(
        job,
        host,
        surface,
        &mut d,
        "WRKSYSACT",
        &mut reload,
        &mut keys,
    )
}
