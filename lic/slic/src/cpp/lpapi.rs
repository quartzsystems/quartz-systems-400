// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The service's answers for the job's licensed program displays (GO
//! LICPGM, DSPSFWRSC, WRKLICINF, WRKSFWAGR) and for the product and
//! license APIs the job installs as natives (`runtime/sysapi/src/lp.rs`).

use licpgm::api::{self, ApiError};
use licpgm::{ProductStore, catalog, id};
use lpstore::{Products, SavfMedia};
use store::obj::{savf, types};

use super::direct::{in_store, lib_or};
use super::lpcmd::{agreement_rows, installed_rows, licinf_rows, sfwrsc_rows};
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::licpgm::{self as lp, system_info};
use crate::stgsvc::security::actor;

fn arg(args: &[String], i: usize) -> &str {
    args.get(i).map_or("", String::as_str)
}

/// `licpgm_panel`.
pub fn panel(system: &mut System, what: &str, args: &[String]) -> Result<Vec<Vec<String>>, String> {
    match what {
        "INSTALLED" => Ok(installed_rows(system)),
        "SFWRSC" => Ok(sfwrsc_rows(system)),
        "LICINF" => Ok(licinf_rows(system, args.first().map(String::as_str))),
        "AGREEMENTS" => Ok(agreement_rows(system)),
        "AGRTEXT" => {
            let text = catalog::agreement_text(arg(args, 0))
                .map_or_else(|| "This product has no software agreement.".into(), str::to_string);
            Ok(text.lines().map(|l| vec![l.to_string()]).collect())
        }
        "ACCEPT" | "DECLINE" => {
            let p = arg(args, 0)
                .parse()
                .map_err(|_| format!("CPF0C1F product {} is not valid", arg(args, 0)))?;
            let r = id::parse_release(arg(args, 1)).map_err(|_| "CPF0C1D release not valid".to_string())?;
            let user = actor().map_or_else(|| "QSYS".to_string(), |a| a.user.clone());
            let today = licpgm::Date::from_days(i64::try_from(now_unix() / 86_400).unwrap_or(0));
            lp::with(system, |s, _, _| {
                let mut b = s.agreements();
                if what == "ACCEPT" {
                    b.accept(&p, r, &user, today);
                } else {
                    b.decline(&p, r, &user, today);
                }
                s.save_agreements(&b).map_err(|e| e.to_string())
            })?;
            Ok(Vec::new())
        }
        "MEDIA" => {
            let streams = if arg(args, 0).contains('/') {
                let (lib, name) = arg(args, 0).split_once('/').unwrap_or(("*LIBL", arg(args, 0)));
                let (lib, name) = (lib.to_string(), name.to_string());
                vec![in_store(system, |txn, qsys, names| {
                    let id = wm::obj::find(txn, qsys, names, &lib_or(&lib), &name, types::SAVF)
                        .map_err(|_| format!("CPF9812 save file {lib}/{name} not found"))?;
                    savf::read(txn, id).map_err(|e| e.to_string())
                })?]
            } else {
                // An install device: every product on its media.
                super::lpcmd::media_streams(arg(args, 0), None)?
                    .into_iter()
                    .map(|(_, s)| s)
                    .collect()
            };
            let mut loads = Vec::new();
            for stream in streams {
                let media = SavfMedia::read(stream).map_err(|e| format!("CPF3D94 {e}"))?;
                loads.extend(licpgm::plan::Media::loads(&media));
            }
            Ok(loads
                .into_iter()
                .map(|m| {
                    let k = m.load.key;
                    let text = m
                        .definition
                        .option(k.option)
                        .map_or_else(|| m.load.text.clone(), |o| o.text.clone());
                    vec![
                        k.product.to_string(),
                        k.option.to_string(),
                        k.load_type.name().to_string(),
                        k.load_id.to_string(),
                        k.release.to_string(),
                        text,
                    ]
                })
                .collect())
        }
        _ => Err(format!("CPF3C21 {what} is not a licensed program display")),
    }
}

/// `licpgm_api`: one API over its parameters' bytes, as the job read
/// them (`runtime/sysapi/src/lp.rs` says which and how long).
pub fn call(system: &mut System, name: &str, parms: &[Vec<u8>]) -> Result<Vec<u8>, (String, Vec<u8>)> {
    let sys = system_info(system);
    let job = system
        .jobs
        .get(system.current_job)
        .map(|j| format!("{:06}/{}/{}", j.number, j.user, j.name))
        .unwrap_or_default();
    let today = licpgm::Date::from_days(i64::try_from(now_unix() / 86_400).unwrap_or(0));
    let r = lp::with(system, |s, cell, qsys| {
        let objs = Products::new(cell, qsys, lpstore::stamp(now_unix()));
        let cx = api::dispatch::CallCtx {
            sys: &sys,
            today,
            job: &job,
            curlib: "QGPL",
        };
        Ok(api::dispatch::call(s, &objs, &cx, name, parms))
    })
    .unwrap_or_else(|e| Err(ApiError::new("CPF3CF2").text(&e, 50)));
    r.map_err(|e| (e.id.to_string(), e.data))
}
