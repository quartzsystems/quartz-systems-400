// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every API by name over its parameters' bytes, in IBM's parameter
//! order without the error code: what the job's natives send the
//! service (`runtime/sysapi/src/lp.rs`). Counts and lengths are
//! `Binary(4)` parameters as passed.

use crate::api::{self, ApiError};
use crate::model::Date;
use crate::store::{ObjectCheck, ProductStore};
use crate::system::SystemInfo;

fn p(parms: &[Vec<u8>], i: usize) -> &[u8] {
    parms.get(i).map_or(&[], Vec::as_slice)
}

fn bin4(parms: &[Vec<u8>], i: usize) -> i32 {
    api::bin4_at(p(parms, i), 0)
}

fn text(parms: &[Vec<u8>], i: usize, n: usize) -> String {
    api::char_at(p(parms, i), 0, n)
}

/// What a call needs besides its parameters.
pub struct CallCtx<'a> {
    /// The system.
    pub sys: &'a SystemInfo,
    /// Today.
    pub today: Date,
    /// The calling job, `number/user/name`, for `*JOB` license users.
    pub job: &'a str,
    /// The job's current library, for `*CURLIB`.
    pub curlib: &'a str,
}

/// Call API `name`. The result is the receiver (`QSZRTVPR`), the two
/// output fields (`QSZCHKTG`: validated and equivalent release, 6
/// each) or nothing.
pub fn call(
    store: &mut dyn ProductStore,
    objs: &dyn ObjectCheck,
    cx: &CallCtx<'_>,
    name: &str,
    parms: &[Vec<u8>],
) -> Result<Vec<u8>, ApiError> {
    let none = |r: Result<(), ApiError>| r.map(|()| Vec::new());
    match name {
        "QSZRTVPR" => {
            let fmt = text(parms, 3, 8);
            api::rtvpr::qszrtvpr(
                store,
                cx.sys,
                bin4(parms, 0),
                &text(parms, 1, 8),
                p(parms, 2),
                (!fmt.is_empty()).then_some(fmt.as_str()),
            )
        }
        "QSZCRTPD" => none(api::crtpd::qszcrtpd(
            store,
            p(parms, 0),
            p(parms, 1),
            p(parms, 2),
            bin4(parms, 3),
            p(parms, 4),
            bin4(parms, 5),
            p(parms, 6),
            p(parms, 7),
            cx.curlib,
            cx.today,
        )),
        "QSZCRTPL" => {
            let pl = api::crtpl::CrtPl {
                name: p(parms, 0),
                info: p(parms, 1),
                secondary_lib: p(parms, 2),
                principal: p(parms, 3),
                additional: p(parms, 4),
                n_additional: bin4(parms, 5),
                pre_exits: p(parms, 6),
                n_pre_exits: bin4(parms, 7),
                folders: p(parms, 8),
                n_folders: bin4(parms, 9),
                text: p(parms, 10),
                public_aut: p(parms, 11),
                n_directories: parms.get(12).map(|b| api::bin4_at(b, 0)),
            };
            none(api::crtpl::qszcrtpl(store, &pl))
        }
        "QSZPKGPO" => api::pkgpo::qszpkgpo(store, objs, p(parms, 0), p(parms, 1), p(parms, 2))
            .map(|_| Vec::new()),
        "QSZDLTPD" => none(api::crtpd::qszdltpd(store, p(parms, 0))),
        "QSZDLTPL" => none(api::crtpl::qszdltpl(store, p(parms, 0))),
        "QSZSPTPR" => none(api::sptpr::qszsptpr(
            store,
            p(parms, 0),
            bin4(parms, 1),
            &text(parms, 2, 8),
            bin4(parms, 3),
        )),
        "QSZCHKTG" => {
            let n = usize::try_from(bin4(parms, 2)).unwrap_or(0);
            let list: Vec<String> = (0..n)
                .map(|i| api::char_at(p(parms, 1), i * 6, 6))
                .collect();
            api::chktg::qszchktg(cx.sys, &text(parms, 0, 10), &list).map(|c| {
                let mut b = api::fixed(&c.validated, 6);
                b.extend(api::fixed(&c.equivalent, 6));
                b
            })
        }
        "QLZAREQ" | "QLZARLS" => {
            let defs: Vec<_> = store.definitions().into_iter().map(|(_, d)| d).collect();
            let mut repo = store.licenses();
            let (pf, uf) = (text(parms, 1, 8), text(parms, 3, 8));
            let r = if name == "QLZAREQ" {
                api::lic::qlzareq(
                    &mut repo,
                    &defs,
                    cx.sys,
                    cx.today,
                    p(parms, 0),
                    &pf,
                    p(parms, 2),
                    &uf,
                    cx.job,
                )
                .map(|_| Vec::new())
            } else {
                none(api::lic::qlzarls(
                    &mut repo,
                    &defs,
                    p(parms, 0),
                    &pf,
                    p(parms, 2),
                    &uf,
                    cx.job,
                ))
            };
            if r.is_ok() {
                store
                    .save_licenses(&repo)
                    .map_err(|e| ApiError::new("CPF9E15").text(&e.to_string(), 50))?;
            }
            r
        }
        _ => Err(ApiError::new("CPF3C21").text(name, 8)),
    }
}

/// The API names `call` answers.
pub const NAMES: &[&str] = &[
    "QSZRTVPR", "QSZCRTPD", "QSZCRTPL", "QSZPKGPO", "QSZDLTPD", "QSZDLTPL", "QSZSPTPR", "QSZCHKTG",
    "QLZAREQ", "QLZARLS",
];
