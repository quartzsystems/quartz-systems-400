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
            let (lib, name) = arg(args, 0).split_once('/').unwrap_or(("*LIBL", arg(args, 0)));
            let (lib, name) = (lib.to_string(), name.to_string());
            let stream = in_store(system, |txn, qsys, names| {
                let id = wm::obj::find(txn, qsys, names, &lib_or(&lib), &name, types::SAVF)
                    .map_err(|_| format!("CPF9812 save file {lib}/{name} not found"))?;
                savf::read(txn, id).map_err(|e| e.to_string())
            })?;
            let media = SavfMedia::read(stream).map_err(|e| format!("CPF3D94 {e}"))?;
            Ok(licpgm::plan::Media::loads(&media)
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

fn bin4(b: &[u8]) -> i32 {
    api::bin4_at(b, 0)
}

fn p(parms: &[Vec<u8>], i: usize) -> &[u8] {
    parms.get(i).map_or(&[], Vec::as_slice)
}

fn text(parms: &[Vec<u8>], i: usize, n: usize) -> String {
    api::char_at(p(parms, i), 0, n)
}

fn fail(e: ApiError) -> (String, Vec<u8>) {
    (e.id.to_string(), e.data)
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
    let r: Result<Vec<u8>, ApiError> = lp::with(system, |s, cell, qsys| {
        let objs = Products::new(cell, qsys, lpstore::stamp(now_unix()));
        Ok(match name {
            "QSZRTVPR" => {
                let fmt = text(parms, 3, 8);
                api::rtvpr::qszrtvpr(
                    s,
                    &sys,
                    bin4(p(parms, 0)),
                    &text(parms, 1, 8),
                    p(parms, 2),
                    (!fmt.is_empty()).then_some(fmt.as_str()),
                )
            }
            "QSZCRTPD" => api::crtpd::qszcrtpd(
                s,
                p(parms, 0),
                p(parms, 1),
                p(parms, 2),
                bin4(p(parms, 3)),
                p(parms, 4),
                bin4(p(parms, 5)),
                p(parms, 6),
                p(parms, 7),
                "QGPL",
                today,
            )
            .map(|()| Vec::new()),
            "QSZCRTPL" => {
                let pl = api::crtpl::CrtPl {
                    name: p(parms, 0),
                    info: p(parms, 1),
                    secondary_lib: p(parms, 2),
                    principal: p(parms, 3),
                    additional: p(parms, 4),
                    n_additional: bin4(p(parms, 5)),
                    pre_exits: p(parms, 6),
                    n_pre_exits: bin4(p(parms, 7)),
                    folders: p(parms, 8),
                    n_folders: bin4(p(parms, 9)),
                    text: p(parms, 10),
                    public_aut: p(parms, 11),
                    n_directories: parms.get(12).map(|b| bin4(b)),
                };
                api::crtpl::qszcrtpl(s, &pl).map(|()| Vec::new())
            }
            "QSZPKGPO" => api::pkgpo::qszpkgpo(s, &objs, p(parms, 0), p(parms, 1), p(parms, 2)).map(|_| Vec::new()),
            "QSZDLTPD" => api::crtpd::qszdltpd(s, p(parms, 0)).map(|()| Vec::new()),
            "QSZDLTPL" => api::crtpl::qszdltpl(s, p(parms, 0)).map(|()| Vec::new()),
            "QSZSPTPR" => api::sptpr::qszsptpr(s, p(parms, 0), bin4(p(parms, 1)), &text(parms, 2, 8), bin4(p(parms, 3)))
                .map(|()| Vec::new()),
            "QSZCHKTG" => {
                let n = usize::try_from(bin4(p(parms, 2))).unwrap_or(0);
                let list: Vec<String> = (0..n).map(|i| api::char_at(p(parms, 1), i * 6, 6)).collect();
                api::chktg::qszchktg(&sys, &text(parms, 0, 10), &list).map(|c| {
                    let mut b = api::fixed(&c.validated, 6);
                    b.extend(api::fixed(&c.equivalent, 6));
                    b
                })
            }
            "QLZAREQ" | "QLZARLS" => {
                let defs: Vec<_> = s.definitions().into_iter().map(|(_, d)| d).collect();
                let mut repo = s.licenses();
                let r = if name == "QLZAREQ" {
                    api::lic::qlzareq(
                        &mut repo,
                        &defs,
                        &sys,
                        today,
                        p(parms, 0),
                        &text(parms, 1, 8),
                        p(parms, 2),
                        &text(parms, 3, 8),
                        &job,
                    )
                    .map(|_| Vec::new())
                } else {
                    api::lic::qlzarls(&mut repo, &defs, p(parms, 0), &text(parms, 1, 8), p(parms, 2), &text(parms, 3, 8), &job)
                        .map(|()| Vec::new())
                };
                if r.is_ok() {
                    s.save_licenses(&repo).map_err(|e| e.to_string())?;
                }
                r
            }
            _ => Err(ApiError::new("CPF3C21").text(name, 8)),
        })
    })
    .unwrap_or_else(|e| Err(ApiError::new("CPF3CF2").text(&e, 50)));
    r.map_err(fail)
}
