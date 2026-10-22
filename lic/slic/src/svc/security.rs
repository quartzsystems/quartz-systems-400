// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security commands on the service console: thin wrappers over
//! `sec::cmd`, run as the console's job. Reports with `OUTPUT(*PRINT)`
//! become spooled files through `QSYSPRT`.

use std::collections::HashMap;
use std::fmt::Write as _;

use sec::auth::principal::Principal;
use sec::cmd::{Env, Keywords};
use spool::printer::{Control, PageFormat, Printer};
use store::obj::ObjectId;
use store::obj::types;
use wm::cmd::Params;
use wm::obj::splf::{Schedule, SplfAttrs};

use crate::ipl::system::{System, now_unix};

/// Help lines.
pub const HELP_AUDIT: &str = "\
chgsecaud [QAUDCTL(..)] [QAUDLVL(..|*DFTSET)] [INLJRNRCV(..)]   audit journal and auditing values
dspsecaud                     auditing values and the audit journal
chgobjaud OBJ(LIB/NAME) OBJTYPE(*T) OBJAUD(*NONE|*USRPRF|*CHANGE|*ALL)
chgusraud USRPRF(..) [OBJAUD(..)] [AUDLVL(..)]
dspaudjrne [ENTTYP(..)] [USRPRF(..)] [FROMTIME(date)] [TOTIME(date)] [OUTPUT(*|*PRINT)]
cpyaudjrne ENTTYP(..) [OUTFILE(LIB/PREFIX)] [USRPRF(..)] [FROMTIME(..)] [TOTIME(..)]
prtadpobj USRPRF(..) | prtusrobj USRPRF(..) | prtpvtaut OBJTYPE(..) | prtpubaut OBJTYPE(..)
prtsysseca | prtjobdaut | prtsbsdaut | prtqaut
";

/// The principal of the job running the command; the system's when
/// none (the IPL's own console).
pub fn actor() -> Principal {
    crate::stgsvc::security::actor().unwrap_or_else(|| Principal {
        user: "QSYS".into(),
        spcaut: sec::profile::SpecialAuth::ALL.0,
        qsecurity: 40,
        ..Principal::default()
    })
}

/// Every system value's text, for a command's environment.
pub fn sysvals(system: &System) -> HashMap<String, String> {
    let mut m = HashMap::new();
    if let Some(v) = system.sysvals.as_ref() {
        for e in v.catalog().entries() {
            if let Ok(val) = v.get(&e.name) {
                m.insert(e.name.clone(), val.text());
            }
        }
    }
    m
}

/// The keywords of a command line.
pub fn keywords(p: &Params) -> Keywords {
    Keywords(p.kw.clone())
}

/// Run `f` with an environment and a transaction on ASP 1, committed
/// when it succeeds.
pub fn in_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut store::Txn<'_>, &Env<'_>) -> sec::Result<T>,
) -> Result<T, String> {
    let vals = sysvals(system);
    let get = |n: &str| vals.get(n).cloned().unwrap_or_default();
    let me = actor();
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    let env = Env {
        qsys,
        actor: &me,
        now: now_unix(),
        sysval: &get,
    };
    super::db::with_txn(&mut s.store, |txn| f(txn, &env).map_err(|e| e.to_string()))
}

/// Print `lines` as spooled file `file` of the running job.
pub fn print(system: &mut System, file: &str, lines: &[String]) -> Result<String, String> {
    let prtf = crate::stgsvc::prtf::resolve(system, "QSYS/QSYSPRT", &[]).unwrap_or_default();
    let fmt = PageFormat {
        len: prtf.page_len,
        width: prtf.page_width,
        lpi: prtf.lpi,
        cpi: prtf.cpi,
        ovrflw: prtf.ovrflw,
        fold: false,
    };
    let mut p = Printer::new(fmt);
    for l in lines {
        p.write_str(l, Control::line());
    }
    let me = system.current_job;
    let job = crate::stgsvc::msgs::sender(system, me);
    let attrs = SplfAttrs {
        name: file.into(),
        job: job.clone(),
        usrdta: file.into(),
        schedule: Schedule::FileEnd,
        copies: 1,
        outpty: 5,
        page_len: fmt.len,
        page_width: fmt.width,
        lpi: fmt.lpi,
        cpi: fmt.cpi,
        ovrflw: fmt.ovrflw,
        formtype: prtf.formtype.clone(),
        outq: crate::stgsvc::prtf::outq_of(system, &prtf, me),
        prtf: "QSYS/QSYSPRT".into(),
        ..SplfAttrs::default()
    };
    // The report is the job's; spooling it is the system's work.
    let id = sec::guard::as_system(|| crate::stgsvc::spool::create(system, attrs))?;
    let data = p.take();
    sec::guard::as_system(|| {
        crate::stgsvc::spool::append(system, id, &data, p.pages, p.records)?;
        crate::stgsvc::spool::close(system, id, false)
    })?;
    Ok(format!(
        "Report {file} printed: {} line(s), job {:06}/{}/{}.\n",
        lines.len(),
        job.2,
        job.1,
        job.0
    ))
}

/// Lines as console output, or a spooled file with `OUTPUT(*PRINT)`.
fn show(system: &mut System, p: &Params, file: &str, lines: &[String], print_default: bool) -> Result<String, String> {
    let printed = match p.upper("OUTPUT").as_deref() {
        Some("*PRINT") => true,
        Some("*") => false,
        _ => print_default,
    };
    if printed {
        print(system, file, lines)
    } else {
        Ok(lines.iter().fold(String::new(), |mut acc, l| {
            let _ = writeln!(acc, "{l}");
            acc
        }))
    }
}

/// `LIB/NAME` with `*LIBL` when no library is given.
fn qual(v: &str) -> (String, String) {
    match v.split_once('/') {
        Some((l, n)) => (l.to_ascii_uppercase(), n.to_ascii_uppercase()),
        None => ("*LIBL".into(), v.to_ascii_uppercase()),
    }
}

/// An object named by `OBJ(LIB/NAME) OBJTYPE(*T)`.
pub fn object(txn: &mut store::Txn<'_>, qsys: ObjectId, p: &Params) -> sec::Result<ObjectId> {
    let o = p
        .kw("OBJ")
        .map(str::to_string)
        .or_else(|| p.pos.first().cloned())
        .ok_or_else(|| sec::SecError::msg("CPD0071", "OBJ is required."))?;
    let t = p
        .upper("OBJTYPE")
        .or_else(|| p.pos.get(1).map(|s| s.to_ascii_uppercase()))
        .ok_or_else(|| sec::SecError::msg("CPD0071", "OBJTYPE is required."))?;
    let code = types::type_by_name(&t)
        .ok_or_else(|| sec::cmd::bad_value("OBJTYPE", &t))?
        .code;
    let (lib, name) = qual(&o);
    let names = super::ile::name_context(txn, qsys);
    wm::obj::find(txn, qsys, &names, &lib, &name, code)
        .map_err(|e| sec::SecError::msg("CPF9801", e.to_string()))
}

/// The audit commands; `None` when `cmd` is not one.
pub fn audit_command(system: &mut System, cmd: &str, line: &str) -> Option<String> {
    let p = Params::parse(line);
    let k = keywords(&p);
    let r = match cmd {
        "chgsecaud" => chgsecaud(system, &k),
        "dspsecaud" => in_store(system, sec::cmd::audit::dspsecaud),
        "chgobjaud" => {
            let v = p.upper("OBJAUD").unwrap_or_else(|| "*NONE".into());
            in_store(system, |txn, env| {
                let id = object(txn, env.qsys, &p)?;
                sec::cmd::audit::chgobjaud(txn, env, id, &v)
            })
            .map(|s| format!("{s}\n"))
        }
        "chgusraud" => in_store(system, |txn, env| sec::cmd::audit::chgusraud(txn, env, &k)).map(|s| format!("{s}\n")),
        "dspaudjrne" => {
            let lines = sec::cmd::audit::Selection::from_keywords(&k)
                .map_err(|e| e.to_string())
                .and_then(|sel| in_store(system, |txn, env| sec::cmd::audit::dspaudjrne(txn, env, &sel)));
            lines.and_then(|l| show(system, &p, "QPAUDENT", &l, false))
        }
        "cpyaudjrne" => cpyaudjrne(system, &p, &k),
        "prtadpobj" | "prtusrobj" => {
            let user = p.upper("USRPRF").unwrap_or_else(|| "*ALL".into());
            let lines = in_store(system, |txn, env| {
                if cmd == "prtadpobj" {
                    sec::cmd::audit::prtadpobj(txn, env, &user)
                } else {
                    sec::cmd::audit::prtusrobj(txn, env, &user)
                }
            });
            let file = if cmd == "prtadpobj" { "QPSECADP" } else { "QPUSROBJ" };
            lines.and_then(|l| show(system, &p, file, &l, true))
        }
        "prtpvtaut" | "prtpubaut" => {
            let t = p.upper("OBJTYPE").unwrap_or_else(|| "*FILE".into());
            let lines = in_store(system, |txn, env| {
                if cmd == "prtpvtaut" {
                    sec::cmd::audit::prtpvtaut(txn, env, &t)
                } else {
                    sec::cmd::audit::prtpubaut(txn, env, &t)
                }
            });
            let file = if cmd == "prtpvtaut" { "QPPVTAUT" } else { "QPPUBAUT" };
            lines.and_then(|l| show(system, &p, file, &l, true))
        }
        "prtsysseca" => {
            let lines = in_store(system, |_, env| {
                env.actor.require(
                    sec::profile::SpecialAuth::ALLOBJ.union(sec::profile::SpecialAuth::AUDIT),
                    "print system security attributes",
                )?;
                Ok(sec::cmd::audit::prtsysseca(env))
            });
            lines.and_then(|l| show(system, &p, "QPSYSSEC", &l, true))
        }
        "prtjobdaut" | "prtsbsdaut" | "prtqaut" => {
            let lines = wm_report(system, cmd);
            let file = match cmd {
                "prtjobdaut" => "QPJOBDAUT",
                "prtsbsdaut" => "QPSBSDAUT",
                _ => "QPQAUT",
            };
            lines.and_then(|l| show(system, &p, file, &l, true))
        }
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| format!("{e}\n")))
}

/// `CHGSECAUD`: the journal, then the values through `sysval`, which
/// checks and audits each change.
fn chgsecaud(system: &mut System, k: &Keywords) -> Result<String, String> {
    let (mut out, changes) = in_store(system, |txn, env| sec::cmd::audit::chgsecaud(txn, env, k))?;
    for (n, v) in changes {
        out.push_str(&super::reply(system, &format!("sysval {n} {v}")));
    }
    out.push_str("Security auditing changed.\n");
    Ok(out)
}

/// `CPYAUDJRNE ENTTYP() OUTFILE(LIB/PREFIX)`: one output file per type,
/// `PREFIX` and the type's code, in the model format.
fn cpyaudjrne(system: &mut System, p: &Params, k: &Keywords) -> Result<String, String> {
    let sel = sec::cmd::audit::Selection::from_keywords(k).map_err(|e| e.to_string())?;
    if sel.types.is_empty() {
        return Err("CPD0071 ENTTYP is required".into());
    }
    let (lib, prefix) = qual(&p.upper("OUTFILE").unwrap_or_else(|| "QTEMP/QAUDIT".into()));
    let lib = if lib == "*LIBL" { "QTEMP".to_string() } else { lib };
    let system_name = system.system_name();
    let entries = in_store(system, |txn, env| {
        env.actor.require(sec::profile::SpecialAuth::AUDIT, "copy audit entries")?;
        sec::cmd::audit::selected(txn, env.qsys, &sel)
    })?;
    let mut out = String::new();
    for kind in &sel.types {
        let name = sec::cmd::audit::outfile_name(&prefix, *kind);
        let fields: Vec<(qsname::Name, u32)> = sec::cmd::audit::outfile_fields(*kind)
            .into_iter()
            .map(|(n, l)| qsname::Name::parse(&n).map(|n| (n, l)).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        let rows: Vec<String> = entries
            .iter()
            .filter(|(_, ev)| ev.kind == *kind)
            .map(|(e, ev)| sec::cmd::audit::outfile_record(e, ev, &system_name))
            .collect();
        let n = rows.len();
        let job = store::obj::JobHandle(u64::from(system.current_job));
        let (st, root) = if lib == "QTEMP" {
            let t = system.temp.as_mut().ok_or("QTEMP is not available")?;
            (&mut t.store, t.qtemp)
        } else {
            let s = system.storage.as_mut().ok_or("object store not open")?;
            (&mut s.store, s.qsys.ok_or("object store not initialized")?)
        };
        super::db::with_txn(st, |txn| {
            let library = if lib == "QTEMP" {
                root
            } else {
                store::obj::ops::find_library(txn, root, qsname::Name::parse(&lib).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?
                    .id
            };
            let fname = qsname::Name::parse(&name).map_err(|e| e.to_string())?;
            let l = store::obj::library::Library::load(txn, library).map_err(|e| e.to_string())?;
            let file = match l.lookup(txn, types::FILE, fname).map_err(|e| e.to_string())? {
                Some(id) => id,
                None => db::file::create_pf_fields(txn, library, fname, &fields, &db::CreateFileOpts::default())
                    .map_err(|e| e.to_string())?,
            };
            db::util::put_lines(txn, job, file, qsname::MemberSpec::First, &rows, true)
                .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        let _ = writeln!(out, "{n} {} entries copied to {lib}/{name}.", kind.code());
    }
    Ok(out)
}

/// The reports whose data is work management's: job descriptions that
/// name a user, subsystem descriptions, and queues, with `*PUBLIC`.
fn wm_report(system: &mut System, cmd: &str) -> Result<Vec<String>, String> {
    in_store(system, |txn, env| {
        env.actor.require(
            sec::profile::SpecialAuth::ALLOBJ.union(sec::profile::SpecialAuth::AUDIT),
            "print this security report",
        )?;
        let want: &[u16] = match cmd {
            "prtjobdaut" => &[types::JOBD],
            "prtsbsdaut" => &[types::SBSD],
            _ => &[types::JOBQ, types::MSGQ, types::OUTQ],
        };
        let title = match cmd {
            "prtjobdaut" => " Job descriptions that name a user, *PUBLIC not *EXCLUDE",
            "prtsbsdaut" => " Subsystem descriptions and their routing programs",
            _ => " Queues whose *PUBLIC is not *EXCLUDE",
        };
        let mut out = vec![title.to_string()];
        for id in sec::profile::index::all_objects(txn, env.qsys)? {
            if !want.contains(&id.type_code) {
                continue;
            }
            let h = store::obj::ops::read_header(txn, id)?.1;
            let lib = store::obj::ops::read_header(txn, h.library)?.1.name.to_string();
            let public = sec::auth::bits::Auth::of(h.public_authority).display(false);
            let payload = wm::obj::read(txn, id).unwrap_or_default();
            let extra = match id.type_code {
                types::JOBD => match wm::obj::jobd::JobD::decode(&payload) {
                    Ok(d) if d.user != "*RQD" && h.public_authority.0 != 0 => d.user,
                    _ => continue,
                },
                types::SBSD => wm::obj::sbsd::Sbsd::decode(&payload)
                    .map(|d| {
                        d.rtges
                            .iter()
                            .map(|r| format!("{}/{}", r.pgm.1, r.pgm.0))
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default(),
                _ if h.public_authority.0 == 0 => continue,
                _ => String::new(),
            };
            out.push(format!(" {:<10} {:<10} {:<8} {:<9} {extra}", h.name, lib, types::type_by_code(id.type_code).map_or("", |t| t.name), public));
        }
        Ok(out)
    })
}
