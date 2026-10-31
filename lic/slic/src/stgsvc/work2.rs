// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panels batch 2's data (`panel2_data`): what `WRKSBMJOB`,
//! `WRKSHRPOOL`, `WRKSYSSTS`, `WRKDSKSTS`, `WRKSYSACT`, `WRKOUTQD`,
//! `DSPSPLF`, `WRKOBJLCK`, `WRKOBJPGP`, `DSPPFM`, `WRKJRNA`, `DSPJRN`,
//! `DSPJRNRCVA` and `DSPOVR` show, named as their panel groups' variables
//! (`ui/uim/pnlgrp/`). The job's library list, `QTEMP` and console state
//! are in place while these run.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use stg::JobInfo;
use stg::proto::PanelData;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use crate::ipl::system::System;

type Row = Vec<(String, String)>;

#[allow(clippy::needless_pass_by_value)] // numbers and strings alike
fn pair(k: &str, v: impl ToString) -> (String, String) {
    (k.to_string(), v.to_string())
}

/// `LIB/NAME` as `(lib, name)`, `*LIBL` when no library is given.
fn split(q: &str) -> (String, String) {
    let q = q.trim().to_ascii_uppercase();
    match q.split_once('/') {
        Some((l, n)) => (l.to_string(), n.to_string()),
        None => ("*LIBL".to_string(), q),
    }
}

/// The data of display `what`.
pub fn data(
    system: &mut System,
    me: &JobInfo,
    what: &str,
    args: &[String],
) -> Result<PanelData, String> {
    let arg = |i: usize| args.get(i).map_or("", String::as_str);
    match what {
        "SBMJOB" => Ok(sbmjobs(system, me)),
        "DSPSAVF" => {
            let (desc, rows) = crate::cpp::savf_listing(system, arg(0), arg(1))?;
            let row = |r: Vec<String>| {
                ["OBJ", "OBJTYPE", "SAVLIB", "OBJATR", "OBJSIZE", "OBJTEXT"]
                    .iter()
                    .zip(r)
                    .map(|(k, v)| ((*k).to_string(), v))
                    .collect()
            };
            Ok((desc, rows.into_iter().map(row).collect()))
        }
        "SHRPOOL" => Ok(shrpools(system)),
        "SYSSTS" => Ok(super::stats::syssts_panel(system, arg(0) == "*YES")),
        "DSKSTS" => Ok(super::stats::dsksts_panel(system, arg(0) == "*YES")),
        "SYSACT" => Ok(super::stats::sysact_panel(system)),
        "OUTQD" => outqd(system, arg(0)),
        "SPLF" => {
            super::splcmd::panel_lines(system, me.number, arg(0), arg(1), arg(2), arg(3) == "*HEX")
        }
        "OBJLCK" => objlck(system, arg(0), arg(1)),
        "PGP" => pgp(system, arg(0)),
        "PFM" => pfm(system, arg(0), arg(1)),
        "JRNA" => jrna(system, arg(0)),
        "JRNE" => jrne(system, arg(0)),
        "JRNRCVA" => jrnrcva(system, arg(0)),
        "OVR" => Ok(ovr(system)),
        _ => Err(format!("CPF9899 no display data called {what}")),
    }
}

/// `WRKSBMJOB SBMFROM(*USER)`: the jobs this user submitted that wait
/// on a queue, the user's batch jobs that run, and ended ones that left
/// spooled output.
fn sbmjobs(system: &mut System, me: &JobInfo) -> PanelData {
    let mut rows: Vec<Row> = Vec::new();
    let queues: BTreeSet<String> = system.queued.values().cloned().collect();
    for q in queues {
        let waiting = super::jobqs::with_jobq(system, &q, |jq, real| {
            let name = real.rsplit('/').next().unwrap_or(real).to_string();
            Ok((
                jq.jobs
                    .iter()
                    .filter(|j| j.submitter.1 == me.user)
                    .map(|j| {
                        (
                            j.name.clone(),
                            j.user.clone(),
                            j.number,
                            j.held,
                            name.clone(),
                        )
                    })
                    .collect::<Vec<_>>(),
                false,
            ))
        })
        .unwrap_or_default();
        for (name, user, number, held, queue) in waiting {
            rows.push(vec![
                pair("JOB", name),
                pair("USER", user),
                pair("NUMBER", format!("{number:06}")),
                pair("TYPE", "BATCH"),
                pair("STATUS", if held { "JOBQ    HLD" } else { "JOBQ" }),
                pair("FUNCTION", queue),
            ]);
        }
    }
    let active = system.jobs.all();
    let numbers: BTreeSet<u32> = active.iter().map(|j| j.number).collect();
    for j in active.iter().filter(|j| j.kind == 'B' && j.user == me.user) {
        rows.push(vec![
            pair("JOB", &j.name),
            pair("USER", &j.user),
            pair("NUMBER", format!("{:06}", j.number)),
            pair("TYPE", "BATCH"),
            pair("STATUS", "ACTIVE"),
            pair("FUNCTION", &j.function),
        ]);
    }
    let ended: BTreeSet<(String, String, u32)> = system
        .spool
        .files
        .values()
        .map(|e| e.attrs.job.clone())
        .filter(|(name, user, number)| {
            *user == me.user
                && !numbers.contains(number)
                && name != "QPRTJOB"
                && !name.starts_with("QPADEV")
        })
        .collect();
    for (name, user, number) in ended {
        rows.push(vec![
            pair("JOB", name),
            pair("USER", user),
            pair("NUMBER", format!("{number:06}")),
            pair("TYPE", "BATCH"),
            pair("STATUS", "OUTQ"),
            pair("FUNCTION", ""),
        ]);
    }
    (vec![pair("SBMFROM", "*USER")], rows)
}

/// Megabytes of `kb`, two decimals.
#[allow(clippy::cast_precision_loss)] // sizes far below 2^52
fn mb(kb: u64) -> String {
    format!("{:.2}", kb as f64 / 1024.0)
}

/// What a shared pool is for.
fn pool_text(name: &str) -> &'static str {
    match name {
        "*MACHINE" => "Machine pool",
        "*BASE" => "Base pool",
        "*INTERACT" => "Interactive pool",
        "*SPOOL" => "Spool pool",
        _ => "Shared pool",
    }
}

/// `WRKSHRPOOL`: the shared pools, defined or not.
fn shrpools(system: &mut System) -> PanelData {
    let main = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|t| wm::stats::mem_total_kb(&t))
        .unwrap_or(0);
    let rows = system
        .pools
        .info()
        .into_iter()
        .filter(|p| p.name.starts_with('*'))
        .map(|p| {
            let actlvl = if p.name == "*MACHINE" {
                "+++++".to_string()
            } else {
                p.actlvl.to_string()
            };
            vec![
                pair("POOL", &p.name),
                pair("SIZE", mb(p.size_kb)),
                pair("ACTLVL", actlvl),
                pair("ALLOC", mb(p.size_kb)),
                pair("POOLID", p.number),
                pair("PAGDFN", "*FIXED"),
                pair("PAGCUR", "*FIXED"),
                pair("POOLTEXT", pool_text(&p.name)),
            ]
        })
        .collect();
    (vec![pair("MAINSTG", mb(main))], rows)
}

/// `WRKOUTQD OUTQ(lib/name)`.
fn outqd(system: &mut System, q: &str) -> Result<PanelData, String> {
    let (lib, name) = split(q);
    let (real, oq, _) = super::spool::on_queue(system, &format!("{lib}/{name}"))?;
    let real_lib = real.split('/').next().unwrap_or("").to_string();
    let yn = |b: bool| if b { "*YES" } else { "*NO" };
    let none = |s: &str| {
        if s.trim().is_empty() {
            "*NONE".to_string()
        } else {
            s.to_string()
        }
    };
    let vars = vec![
        pair("OUTQ", &name),
        pair("OUTQLIB", real_lib),
        pair("WTRACT", if oq.writer.is_empty() { "N" } else { "Y" }),
        pair("WTRNAME", &oq.writer),
        pair("HELD", if oq.held { "Y" } else { "N" }),
        pair(
            "MAXPAGES",
            if oq.maxpages == 0 {
                "*NONE".to_string()
            } else {
                oq.maxpages.to_string()
            },
        ),
        pair(
            "AUTOSTR",
            if oq.autostrwtr == 0 {
                "*NONE".to_string()
            } else {
                oq.autostrwtr.to_string()
            },
        ),
        pair("DSPDTA", &oq.dspdta),
        pair("JOBSEP", "0"),
        pair("OPRCTL", yn(oq.oprctl)),
        pair(
            "SEQ",
            if oq.seq == wm::obj::outq::Seq::Fifo {
                "*FIFO"
            } else {
                "*JOBNBR"
            },
        ),
        pair("DTAQ", "*NONE"),
        pair("AUTCHK", &oq.autchk),
        pair("RMTSYS", none(&oq.rmtsys)),
        pair("USRDFNOPT", "*NONE"),
        pair("USRDFNOBJ", "*NONE"),
        pair("SPLFASP", "*SYSTEM"),
        pair("TEXT", &oq.text),
    ];
    Ok((vars, Vec::new()))
}

/// Run `f` in a transaction with the name context.
fn in_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId, &ops::NameContext) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| {
        let names = crate::svc::ile::name_context(txn, qsys);
        f(txn, qsys, &names)
    })
}

/// `WRKOBJLCK OBJ(lib/name) OBJTYPE(type)`: who holds the object.
fn objlck(system: &mut System, q: &str, ty: &str) -> Result<PanelData, String> {
    let (lib, name) = split(q);
    let ty = ty.trim().to_ascii_uppercase();
    let code = types::type_by_name(&ty)
        .map(|t| t.code)
        .ok_or_else(|| format!("CPD0084 object type {ty} is not valid"))?;
    let jobs = system.jobs.all();
    let (real, holders) = in_store(system, |txn, qsys, names| {
        let id = wm::obj::find(txn, qsys, names, &lib, &name, code)
            .map_err(|_| format!("CPF9801 {ty} {lib}/{name} not found"))?;
        let real = if code == types::LIB {
            "QSYS".to_string()
        } else {
            ile::store::library_name(txn, id).map_err(|e| e.to_string())?
        };
        Ok((real, txn.store().locks().holders(id)))
    })?;
    let rows = holders
        .into_iter()
        .map(|h| {
            let (jn, ju, nbr) = jobs
                .iter()
                .find(|j| j.handle == h.job.0)
                .map_or(("*UNKNOWN".to_string(), String::new(), 0), |j| {
                    (j.name.clone(), j.user.clone(), j.number)
                });
            vec![
                pair("JOB", jn),
                pair("USER", ju),
                pair("NUMBER", format!("{nbr:06}")),
                pair("LOCK", h.state.name()),
                pair("STATUS", " HELD"),
                pair("SCOPE", "*JOB"),
                pair("THREAD", ""),
            ]
        })
        .collect();
    let vars = vec![
        pair("OBJ", &name),
        pair("OBJLIB", real),
        pair("OBJTYPE", &ty),
        pair("ASPDEV", "*SYSBAS"),
    ];
    Ok((vars, rows))
}

/// `WRKOBJPGP PGP(name)`: the objects a group profile is the primary
/// group of.
fn pgp(system: &mut System, group: &str) -> Result<PanelData, String> {
    let group = group.trim().to_ascii_uppercase();
    let text = crate::svc::security::in_store(system, |txn, env| {
        sec::cmd::object::objects_of(txn, env, &group, sec::profile::index::Kind::Group)
    })?;
    let rows = text
        .lines()
        .skip(2)
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let at = |a: usize, b: usize| l.get(a..b.min(l.len())).unwrap_or("").trim().to_string();
            vec![
                pair("OBJ", at(1, 11)),
                pair("OBJLIB", at(13, 23)),
                pair("OBJTYPE", at(25, 35)),
            ]
        })
        .collect();
    Ok((vec![pair("PGP", &group)], rows))
}

/// The most records `DSPPFM` reads at once.
const PFM_MAX: usize = 9999;

/// `DSPPFM FILE(lib/name) MBR(name)`: each record's text and bytes in
/// hexadecimal.
fn pfm(system: &mut System, q: &str, mbr: &str) -> Result<PanelData, String> {
    let (lib, name) = split(q);
    let qual = format!("{lib}/{name}");
    let line = if mbr.trim().is_empty() {
        "dsppfm".to_string()
    } else {
        format!("dsppfm MBR({mbr})")
    };
    let spec = crate::svc::db::member_spec(&wm::cmd::Params::parse(&line), "MBR")?;
    let (store, ctx) = crate::svc::db::db(system)?;
    let (views, real_lib, member) = crate::svc::db::with_txn(store, |txn| {
        let id = crate::svc::db::file_id(txn, ctx, &qual)?;
        let desc = db::file::describe(txn, id).map_err(|e| crate::svc::db::db_err(&e))?;
        let member = desc
            .member(spec)
            .map(|m| m.name.to_string())
            .unwrap_or_default();
        let real_lib = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
        let views = db::dsp::dsppfm(txn, id, spec, 1, PFM_MAX, false)
            .map_err(|e| crate::svc::db::db_err(&e))?;
        Ok((views, real_lib, member))
    })?;
    let rows = views
        .into_iter()
        .map(|v| {
            let mut hex = String::with_capacity(v.bytes.len() * 2);
            for b in &v.bytes {
                let _ = write!(hex, "{b:02X}");
            }
            vec![pair("RRN", v.rrn), pair("TEXT", v.text()), pair("HEX", hex)]
        })
        .collect();
    Ok((
        vec![
            pair("FILE", name),
            pair("LIB", real_lib),
            pair("MBR", member),
        ],
        rows,
    ))
}

/// A Unix time as `MM/DD/YY HH:MM:SS`.
fn stamp(secs: u64) -> String {
    sec::cmd::civil(secs)
}

/// `WRKJRNA JRN(lib/name)`: the attributes, the receiver directory
/// (entries `*LIST`=`RCVS`) and the journaled objects (`OBJS`).
fn jrna(system: &mut System, q: &str) -> Result<PanelData, String> {
    let (lib, name) = split(q);
    in_store(system, |txn, qsys, names| {
        let id = wm::obj::find(txn, qsys, names, &lib, &name, jrn::JRN)
            .map_err(|_| format!("CPF9801 journal {name} in {lib} not found"))?;
        let journal = jrn::Journal::load(txn, id).map_err(|e| e.to_string())?;
        let real = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
        let attached = journal.attached(txn).ok();
        let (att_name, att_lib) = match &attached {
            Some(r) => {
                let (_, h) = ops::read_header(txn, r.id).map_err(|e| e.to_string())?;
                let l = ile::store::library_name(txn, r.id).map_err(|e| e.to_string())?;
                (h.name.to_string(), l)
            }
            None => ("*NONE".to_string(), String::new()),
        };
        let mut rows: Vec<Row> = Vec::new();
        for entry in journal.receivers(txn).map_err(|e| e.to_string())? {
            let (_, head) = ops::read_header(txn, entry.receiver).map_err(|e| e.to_string())?;
            let rlib = ile::store::library_name(txn, entry.receiver).map_err(|e| e.to_string())?;
            rows.push(vec![
                pair("*LIST", "RCVS"),
                pair("RCV", head.name.to_string()),
                pair("RCVLIB", rlib),
                pair("RCVSTS", entry.status.name()),
                pair("RCVATT", stamp(entry.attached)),
            ]);
        }
        let objects = journal.objects(txn).map_err(|e| e.to_string())?;
        let count = objects.len();
        for obj in objects {
            let (_, head) = ops::read_header(txn, obj.object).map_err(|e| e.to_string())?;
            let olib = ile::store::library_name(txn, obj.object).unwrap_or_default();
            let ty = types::type_by_code(obj.object_type).map_or_else(
                || format!("*{:04X}", obj.object_type),
                |t| t.name.to_string(),
            );
            rows.push(vec![
                pair("*LIST", "OBJS"),
                pair("OBJ", head.name.to_string()),
                pair("OBJLIB", olib),
                pair("OBJTYPE", ty),
                pair("IMAGES", if obj.both_images { "*BOTH" } else { "*AFTER" }),
                pair(
                    "OMTJRNE",
                    if obj.omit_open_close {
                        "*OPNCLO"
                    } else {
                        "*NONE"
                    },
                ),
            ]);
        }
        let vars = vec![
            pair("JRN", &name),
            pair("JRNLIB", real),
            pair("ATTRCV", att_name),
            pair("ATTRCVLIB", att_lib),
            pair("JRNTEXT", ccsid::decode_trimmed(&journal.header.text)),
            pair("ASP", "1"),
            pair("JRNSTATE", "*ACTIVE"),
            pair("MSGQ", "QSYSOPR"),
            pair("MSGQLIB", "*LIBL"),
            pair(
                "MNGRCV",
                if journal.attr.mngrcv_system {
                    "*SYSTEM"
                } else {
                    "*USER"
                },
            ),
            pair("DLTRCV", if journal.attr.dltrcv { "*YES" } else { "*NO" }),
            pair("OBJCOUNT", count),
        ];
        Ok((vars, rows))
    })
}

/// `DSPJRN JRN(lib/name)`: every entry of the receiver chain.
fn jrne(system: &mut System, q: &str) -> Result<PanelData, String> {
    let (lib, name) = split(q);
    let qual = format!("{lib}/{name}");
    let (store, ctx) = crate::svc::db::db(system)?;
    let (entries, real) = crate::svc::db::with_txn(store, |txn| {
        let id = crate::svc::db::typed_id(txn, ctx, &qual, jrn::JRN)?;
        let real = ile::store::library_name(txn, id).map_err(|e| e.to_string())?;
        let entries = jrn::dspjrn(txn, id, &jrn::Filter::default()).map_err(|e| e.to_string())?;
        Ok((entries, real))
    })?;
    let largest = entries.last().map_or(0, |e| e.seq);
    let rows = entries
        .into_iter()
        .map(|e| {
            let (obj, olib, mbr) = e.object.as_ref().map_or_else(
                || (String::new(), String::new(), String::new()),
                |o| {
                    (
                        o.object.to_string(),
                        o.library.to_string(),
                        o.member.map(|m| m.to_string()).unwrap_or_default(),
                    )
                },
            );
            let stamp = stamp(e.time);
            let time = stamp.split_whitespace().nth(1).unwrap_or("").to_string();
            vec![
                pair("SEQ", e.seq),
                pair("CODE", char::from(e.code)),
                pair("TYPE", String::from_utf8_lossy(&e.kind)),
                pair("OBJ", obj),
                pair("OBJLIB", olib),
                pair("MBR", mbr),
                pair("JOB", &e.job.name),
                pair("QJOB", e.job.qualified()),
                pair("TIME", time),
                pair("STAMP", stamp),
                pair("COUNT", e.count),
                pair("CYCLE", e.commit_cycle),
                pair("DATALEN", e.data.len()),
            ]
        })
        .collect();
    Ok((
        vec![
            pair("JRN", name),
            pair("JRNLIB", real),
            pair("LARGEST", format!("{largest:020}")),
        ],
        rows,
    ))
}

/// `DSPJRNRCVA JRNRCV(lib/name)`, from the receiver's attribute lines.
fn jrnrcva(system: &mut System, q: &str) -> Result<PanelData, String> {
    let (lib, name) = split(q);
    let qual = format!("{lib}/{name}");
    let (store, ctx) = crate::svc::db::db(system)?;
    let text = crate::svc::db::with_txn(store, |txn| {
        let id = crate::svc::db::typed_id(txn, ctx, &qual, jrn::JRNRCV)?;
        jrn::dspjrnrcva(txn, id).map_err(|e| e.to_string())
    })?;
    let mut vars = Vec::new();
    let mut libs = 0;
    for l in text.lines() {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let v = v.trim();
        let var = match k.trim() {
            "Journal receiver" => "RCV",
            "Library" => {
                libs += 1;
                if libs == 1 { "RCVLIB" } else { "JRNLIB" }
            }
            "Text" => "RCVTEXT",
            "Status" => "STATUS",
            "Journal" => "JRN",
            "Attached" => "ATTACHED",
            "Detached" => "DETACHED",
            "First sequence number" => "FIRSTSEQ",
            "Last sequence number" => "LASTSEQ",
            "Number of entries" => "ENTRIES",
            "Size (bytes)" => "SIZE",
            "Previous receiver" => "PRVRCV",
            "Next receiver" => "NXTRCV",
            _ => continue,
        };
        vars.push(pair(var, v));
    }
    if !vars.iter().any(|(k, _)| k == "JRNLIB") {
        let rcvlib = vars
            .iter()
            .find(|(k, _)| k == "RCVLIB")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        vars.push(pair("JRNLIB", rcvlib));
    }
    Ok((vars, Vec::new()))
}

/// `DSPOVR`: the job's database, printer and display file overrides.
fn ovr(system: &mut System) -> PanelData {
    let mut rows: Vec<Row> = Vec::new();
    if let Ok((_, ctx)) = crate::svc::db::db(system) {
        for o in ctx.session.overrides() {
            let mut what = Vec::new();
            if let Some(t) = &o.tofile {
                what.push(format!("TOFILE({}/{})", t.library, t.file));
            }
            if let Some(m) = &o.member {
                what.push(format!("MBR({m})"));
            }
            if let Some(s) = o.share {
                what.push(format!("SHARE({})", if s { "*YES" } else { "*NO" }));
            }
            if let Some(l) = o.lvlchk {
                what.push(format!("LVLCHK({})", if l { "*YES" } else { "*NO" }));
            }
            if o.secure {
                what.push("SECURE(*YES)".into());
            }
            if o.scope != db::odp::ovrdbf::DFTACTGRP {
                what.push(format!("OVRSCOPE({})", o.scope));
            }
            rows.push(vec![
                pair("FILE", o.file.to_string()),
                pair("TYPE", "DB"),
                pair("LEVEL", o.level),
                pair("KWDS", what.join(" ")),
            ]);
        }
    }
    let kinds = [
        ("PRT", &system.svc.prt_overrides),
        ("DSP", &system.svc.dsp_overrides),
    ];
    for (ty, list) in kinds {
        for o in list {
            let rest = o.line.split_once(' ').map_or("", |(_, r)| r).trim();
            let rest = rest
                .split_whitespace()
                .filter(|w| !w.to_ascii_uppercase().starts_with("FILE("))
                .collect::<Vec<_>>()
                .join(" ");
            rows.push(vec![
                pair("FILE", &o.file),
                pair("TYPE", ty),
                pair("LEVEL", o.level),
                pair("KWDS", rest),
            ]);
        }
    }
    (vec![pair("LVL", "*")], rows)
}
