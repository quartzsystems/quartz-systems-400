// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS commands in the service: the parameters out of the block,
//! the work in `ifscmd` over the job's IFS (its principal, its
//! connection's descriptors and current directory, its CCSID), and the
//! text forms of the displays for a job without a display.

use cmd::Value;
use cmd::block::Parms;
use ifs::client::IfsClient;
use ifscmd::Reply;
use ifscmd::cmds::{Subtree, admin, attr, auth, dirs, links, udfs, yes};
use stg::proto::CmdReply;

use super::stmfcmd::reply;
use crate::ipl::system::System;

/// A command's work against the job's IFS.
type Work<'a> = Box<dyn FnOnce(&dyn IfsClient) -> Reply + 'a>;

/// Run `name`, or `None` when it is not an IFS command.
#[allow(clippy::too_many_lines)] // one arm per command
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let p = |k: &str| parms.text(k);
    let int = |k: &str, d: i64| parms.int(k).unwrap_or(d);
    let sub = |k: &str| Subtree::of(&parms.name(k));
    let job_ccsid = crate::stgsvc::ifs::job_ccsid(system, system.current_job);
    let r: Work = match name {
        "CD" | "CHGCURDIR" => {
            let dir = p("DIR");
            Box::new(move |c| dirs::cd(c, &dir))
        }
        "MD" | "MKDIR" | "CRTDIR" => {
            let a = dirs::Md {
                dir: p("DIR"),
                dtaaut: or(parms.name("DTAAUT"), "*INDIR"),
                objaut: words(parms, "OBJAUT"),
                crtobjaud: or(parms.name("CRTOBJAUD"), "*SYSVAL"),
                crtobjscan: or(parms.name("CRTOBJSCAN"), "*PARENT"),
                rstdrnmunl: int("RSTDRNMUNL", 0) == 1,
            };
            Box::new(move |c| dirs::md(c, &a))
        }
        "RD" | "RMDIR" => {
            let (dir, s, rmv) = (p("DIR"), sub("SUBTREE"), yes(&parms.name("RMVLNK")));
            Box::new(move |c| dirs::rd(c, &dir, s, rmv))
        }
        "DEL" | "RMVLNK" => {
            let obj = p("OBJLNK");
            Box::new(move |c| links::del(c, &obj))
        }
        "REN" | "RNM" => {
            let (obj, new) = (p("OBJ"), p("NEWOBJ"));
            Box::new(move |c| links::ren(c, &obj, &new))
        }
        "MOV" => {
            let a = links::Mov {
                obj: p("OBJ"),
                todir: p("TODIR"),
                toobj: some(p("TOOBJ")),
                to_ccsid: to_ccsid(parms, job_ccsid),
            };
            Box::new(move |c| links::mov(c, &a))
        }
        "CPY" => {
            let a = links::Cpy {
                obj: p("OBJ"),
                todir: p("TODIR"),
                toobj: some(p("TOOBJ")),
                subtree: match int("SUBTREE", 0) {
                    1 => links::CpySubtree::None,
                    2 => links::CpySubtree::All,
                    _ => links::CpySubtree::NoDir,
                },
                replace: int("REPLACE", 0) == 1,
                keep_owner: int("OWNER", 0) == 1,
                to_ccsid: to_ccsid(parms, job_ccsid),
            };
            Box::new(move |c| links::cpy(c, &a))
        }
        "ADDLNK" => {
            let (obj, new) = (p("OBJ"), p("NEWLNK"));
            let hard = parms.name("LNKTYPE") == "*HARD";
            Box::new(move |c| links::addlnk(c, &obj, &new, hard))
        }
        "CHGAUT" => {
            let a = auth::Chgaut {
                obj: p("OBJ"),
                users: words(parms, "USER"),
                dtaaut: parms.name("DTAAUT"),
                objaut: words(parms, "OBJAUT"),
                autl: parms.specified("AUTL").then(|| parms.name("AUTL")),
                subtree: sub("SUBTREE"),
                symlnk: int("SYMLNK", 0) == 1,
            };
            Box::new(move |c| auth::chgaut(c, &a))
        }
        "CHGOWN" => {
            let (obj, own) = (p("OBJ"), parms.name("NEWOWN"));
            let (rvk, s, sym) = (int("RVKOLDAUT", 1) == 1, sub("SUBTREE"), int("SYMLNK", 0) == 1);
            Box::new(move |c| auth::chgown(c, &obj, &own, rvk, s, sym))
        }
        "CHGPGP" => {
            const DATA: [&str; 11] = [
                "*OLDPGP", "*PRIVATE", "*RWX", "*RX", "*RW", "*WX", "*R", "*W", "*X", "*EXCLUDE",
                "*NONE",
            ];
            const OBJ: [&str; 6] = ["*NONE", "*ALL", "*OBJEXIST", "*OBJMGT", "*OBJALTER", "*OBJREF"];
            let pick = |t: &[&str], n: i64| -> String {
                usize::try_from(n)
                    .ok()
                    .and_then(|i| t.get(i))
                    .map_or_else(String::new, |s| (*s).to_string())
            };
            let a = auth::Chgpgp {
                obj: p("OBJ"),
                newpgp: parms.name("NEWPGP"),
                dtaaut: pick(&DATA, int("DTAAUT", 0)),
                objaut: parms
                    .list("OBJAUT")
                    .into_iter()
                    .filter_map(|v| int_of(v).map(|n| pick(&OBJ, n)))
                    .collect(),
                rvkoldaut: int("RVKOLDAUT", 1) == 1,
                subtree: sub("SUBTREE"),
                symlnk: int("SYMLNK", 0) == 1,
            };
            Box::new(move |c| auth::chgpgp(c, &a))
        }
        "CHGATR" => {
            let (obj, text) = (p("OBJ"), p("TEXT"));
            let (atr, value) = (int("ATR", 0), int("VALUE", 0));
            let (s, sym) = (sub("SUBTREE"), yes(&parms.name("SYMLNK")));
            Box::new(move |c| attr::chgatr(c, &obj, atr, value, &text, s, sym))
        }
        "CHKOUT" | "CHKIN" => {
            let (obj, s) = (p("OBJ"), sub("SUBTREE"));
            let out = name == "CHKOUT";
            Box::new(move |c| attr::checkout(c, &obj, s, out))
        }
        "STRJRN" => {
            let Some(jrn) = journal_name(&p("JRN")) else {
                return Some(reply(Reply::fail(ifscmd::Msg::path("CPFA0DB", &p("JRN"))), ""));
            };
            let a = attr::StrJrn {
                objs: included(parms, "OBJ"),
                jrn,
                subtree: sub("SUBTREE"),
                inherit: yes(&parms.name("INHERIT")),
                both: parms.name("IMAGES") == "*BOTH",
                omit_open_close: parms.name("OMTJRNE") == "*OPNCLO",
            };
            Box::new(move |c| attr::strjrn(c, &a))
        }
        "ENDJRN" => {
            let (objs, s) = (included(parms, "OBJ"), sub("SUBTREE"));
            Box::new(move |c| attr::endjrn(c, &objs, s))
        }
        "CRTUDFS" => {
            let (u, aud, text) = (p("UDFS"), parms.name("CRTOBJAUD"), p("TEXT"));
            let mixed = parms.name("CASE") == "*MIXED";
            Box::new(move |c| udfs::crtudfs(c, &u, mixed, &aud, &text))
        }
        "DLTUDFS" => {
            let u = p("UDFS");
            Box::new(move |c| udfs::dltudfs(c, &u))
        }
        "DSPUDFS" => {
            let u = p("UDFS");
            Box::new(move |c| udfs::dspudfs(c, &u))
        }
        "MOUNT" | "ADDMFS" => {
            let (ty, mfs, over, opts) = (parms.name("TYPE"), p("MFS"), p("MNTOVRDIR"), p("OPTIONS"));
            Box::new(move |c| udfs::mount(c, &ty, &mfs, &over, &opts))
        }
        "UNMOUNT" | "RMVMFS" => {
            let (ty, mfs, over) = (parms.name("TYPE"), p("MFS"), p("MNTOVRDIR"));
            Box::new(move |c| udfs::unmount(c, &ty, &mfs, &over))
        }
        "RCLLNK" => Box::new(|c| admin::rcllnk(c, false)),
        "SAV" => {
            let objs = included(parms, "OBJ");
            let all = int("SUBTREE", -1) == -1;
            Box::new(move |c| admin::sav(c, &objs, all))
        }
        // The displays' text forms.
        "WRKLNK" | "DSPLNK" => {
            let (obj, ty) = (or(p("OBJ"), "*"), or(parms.name("OBJTYPE"), "*ALL"));
            Box::new(move |c| ifscmd::text::links_text(c, &obj, &ty))
        }
        "DSPAUT" | "WRKAUT" => {
            let obj = p("OBJ");
            Box::new(move |c| ifscmd::text::authority_text(c, &obj))
        }
        "DSPF" => {
            let f = p("STMF");
            Box::new(move |c| ifscmd::text::file_text(c, &f))
        }
        _ => return None,
    };
    let out = super::stmf::with_job(system, r);
    Some(reply(out, ""))
}

/// A value, or `dft` when it is empty.
fn or(v: String, dft: &str) -> String {
    if v.trim().is_empty() { dft.to_string() } else { v }
}

/// A path that was given, `None` when empty.
fn some(v: String) -> Option<String> {
    (!v.trim().is_empty()).then_some(v)
}

/// The words of a list parameter, upper case.
fn words(parms: &Parms, kwd: &str) -> Vec<String> {
    parms
        .list(kwd)
        .into_iter()
        .map(|v| v.text().map_or_else(|| v.source(), str::to_string).trim().to_ascii_uppercase())
        .filter(|w| !w.is_empty())
        .collect()
}

/// An integer value.
fn int_of(v: &Value) -> Option<i64> {
    match v {
        Value::Num(d) => d.mantissa_i128().and_then(|m| i64::try_from(m).ok()),
        Value::Char(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// The paths of an `OBJ` list of (name, `*INCLUDE`/`*OMIT`) groups that
/// are included; `*` and an empty list are the current directory.
fn included(parms: &Parms, kwd: &str) -> Vec<String> {
    let mut out = Vec::new();
    for v in parms.list(kwd) {
        let (path, inc) = match v {
            Value::Elems(e) => (
                e.first().and_then(Option::as_ref).map(Value::source).unwrap_or_default(),
                e.get(1)
                    .and_then(Option::as_ref)
                    .is_none_or(|x| !matches!(x.source().trim(), "O" | "*OMIT")),
            ),
            other => (other.text().map_or_else(|| other.source(), str::to_string), true),
        };
        let path = path.trim().trim_matches('\'').to_string();
        if inc {
            out.push(if path.is_empty() { ".".into() } else { path });
        }
    }
    if out.is_empty() {
        out.push(".".into());
    }
    out
}

/// `TOCCSID` with `DTAFMT(*TEXT)`: the CCSID the data is converted to.
fn to_ccsid(parms: &Parms, job: u16) -> Option<u16> {
    if parms.int("DTAFMT").unwrap_or(0) != 1 {
        return None;
    }
    match parms.int("TOCCSID").unwrap_or(0) {
        -2 => Some(819),
        -3 => Some(1252),
        -4 => Some(job),
        n if n > 0 => u16::try_from(n).ok(),
        _ => None,
    }
}

/// `LIB/JRN` of a journal's `/QSYS.LIB` path.
fn journal_name(path: &str) -> Option<String> {
    let comps: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let (first, rest) = comps.split_first()?;
    if !first.eq_ignore_ascii_case("QSYS.LIB") {
        return None;
    }
    match ifs::vfs::qsys::node_of(rest).ok()? {
        ifs::vfs::qsys::QsysNode::Obj(lib, name, ty) if ty == "*JRN" => Some(format!("{lib}/{name}")),
        _ => None,
    }
}
