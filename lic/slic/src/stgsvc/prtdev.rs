// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LAN printers and workstation customizing objects: `CRTDEVPRT` and
//! `CHGDEVPRT` for `DEVCLS(*LAN)` (the device, its printer section and
//! its output queue `QUSRSYS/<name>`), `CRTWSCST` (a source member
//! compiled into a `*WSCST`) and `RTVWSCST` (a model's source written
//! to a member).

use print::transform::Model;
use print::wscst::Wscst;
use store::obj::library::Library;
use store::obj::ops::{self, NameContext};
use store::obj::{JobHandle, ObjectId, types};
use store::txn::Txn;
use wm::cmd::Params;
use wm::obj::devd::{DevStatus, Devd};
use wm::obj::devprt::LanPrinter;

use super::spool::with_store;
use crate::ipl::system::{System, now_unix};

fn err(e: &wm::Error) -> String {
    format!("{} {e}", e.msgid().unwrap_or("CPD0084"))
}

fn owner(system: &System, me: u32) -> String {
    system
        .jobs
        .get(me)
        .map_or_else(|| "QSYS".into(), |j| j.user.clone())
}

fn stamp(owner: String) -> wm::obj::Stamp {
    wm::obj::Stamp {
        owner,
        system: "QS400".into(),
        now: now_unix(),
    }
}

/// A `TEXT()` value: quotes off, `*BLANK` blank.
fn text(p: &Params) -> Option<String> {
    p.kw("TEXT").map(|_| {
        let t = p.text("TEXT");
        if t.eq_ignore_ascii_case("*BLANK") || t.eq_ignore_ascii_case("*SAME") {
            String::new()
        } else {
            t.trim().to_string()
        }
    })
}

/// The output queue `QUSRSYS/<name>` of a LAN printer, made when
/// missing.
fn ensure_outq(txn: &mut Txn<'_>, qsys: ObjectId, name: &str, owner: &str) -> Result<(), String> {
    let lib = wm::obj::library(txn, qsys, "QUSRSYS").map_err(|e| err(&e))?;
    let n = qsname::Name::parse(name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
    let l = Library::load(txn, lib).map_err(|e| e.to_string())?;
    if l.lookup(txn, types::OUTQ, n).map_err(|e| e.to_string())?.is_some() {
        return Ok(());
    }
    let text = format!("Output queue of LAN printer {name}");
    let q = wm::obj::outq::OutQ::new(&text);
    wm::obj::create(txn, lib, n, types::OUTQ, &text, &stamp(owner.into()), &q.encode())
        .map(|_| ())
        .map_err(|e| err(&e))
}

fn find_device(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Result<Option<(ObjectId, Devd)>, String> {
    match wm::obj::find(txn, qsys, &NameContext::default(), "QSYS", name, types::DEVD) {
        Ok(id) => {
            let b = wm::obj::read(txn, id).map_err(|e| err(&e))?;
            Ok(Some((id, Devd::decode(&b).map_err(|e| err(&e))?)))
        }
        Err(_) => Ok(None),
    }
}

/// What an `*IP` LAN printer needs before it can print.
fn check(name: &str, l: &LanPrinter) -> Result<(), String> {
    if l.is_ip() && l.rmtlocname.is_empty() {
        return Err(format!(
            "CPD0084 device {name}: LANATTACH(*IP) needs RMTLOCNAME, the printer's host name or address"
        ));
    }
    if l.is_ip() && l.port == 0 && l.sysdrvpgm != "*IBMIPPDRV" {
        return Err(format!(
            "CPD0084 device {name}: LANATTACH(*IP) needs PORT, 9100 for most raw ports"
        ));
    }
    Ok(())
}

/// `CRTDEVPRT DEVD() DEVCLS(*LAN) TYPE() MODEL() LANATTACH(*IP) PORT()
/// RMTLOCNAME() ...` and `CHGDEVPRT DEVD() ...`.
pub fn crt_chg(system: &mut System, me: u32, cmd: &str, p: &Params) -> Result<String, String> {
    let name = p.pos_or(0, "DEVD")?.to_ascii_uppercase();
    let user = owner(system, me);
    let online = p.upper("ONLINE").filter(|v| v != "*SAME").map(|v| v == "*YES");
    let text = text(p);
    if cmd == "crtdevprt" {
        let devcls = p.upper("DEVCLS").unwrap_or_default();
        if devcls != "*LAN" {
            return Err(format!(
                "CPD0084 DEVCLS({devcls}): only *LAN printers are created here"
            ));
        }
        let devtype = p.upper("TYPE").unwrap_or_else(|| "3812".into());
        let model = p.upper("MODEL").unwrap_or_else(|| "1".into());
        let mut l = LanPrinter::default();
        l.apply(p).map_err(|e| err(&e))?;
        check(&name, &l)?;
        let mut d = Devd::lan_printer(&name, &devtype, &model, l);
        if let Some(o) = online {
            d.online = o;
            d.status = if o {
                DevStatus::VariedOn
            } else {
                DevStatus::VariedOff
            };
        }
        d.text = text.unwrap_or_default();
        with_store(system, |txn, qsys| {
            if find_device(txn, qsys, &name)?.is_some() {
                return Err(format!("CPF2602 device description {name} already exists"));
            }
            ensure_outq(txn, qsys, &name, &user)?;
            let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0084 DEVD({name}): {e}"))?;
            wm::obj::create(txn, qsys, n, types::DEVD, &d.text, &stamp(user.clone()), &d.encode())
                .map(|_| ())
                .map_err(|e| err(&e))
        })?;
        return Ok(format!("Description for device {name} created.\n"));
    }
    with_store(system, |txn, qsys| {
        let (id, mut d) = find_device(txn, qsys, &name)?
            .ok_or_else(|| format!("CPF2702 device description {name} not found"))?;
        let Some(l) = d.printer.as_mut() else {
            return Err(format!("CPF2702 device {name} is not a LAN printer"));
        };
        l.apply(p).map_err(|e| err(&e))?;
        check(&name, l)?;
        if let Some(o) = online {
            d.online = o;
        }
        if let Some(t) = text {
            d.text = t;
        }
        wm::obj::write(txn, id, &d.encode()).map_err(|e| err(&e))
    })?;
    Ok(format!("Description for device {name} changed.\n"))
}

/// `lib/name` with `*CURLIB` as `QGPL` and `*LIBL` kept.
fn qualified(v: &str, dft_lib: &str) -> (String, String) {
    let v = v.trim().trim_matches(['(', ')']).to_ascii_uppercase();
    let (lib, name) = match v.split_once('/') {
        Some((l, n)) => (l.to_string(), n.to_string()),
        None => match v.split_whitespace().collect::<Vec<_>>().as_slice() {
            [n, l] => ((*l).to_string(), (*n).to_string()),
            _ => (dft_lib.to_string(), v.clone()),
        },
    };
    let lib = if lib == "*CURLIB" { "QGPL".into() } else { lib };
    (lib, name)
}

/// `CRTWSCST WSCST(lib/name) [SRCFILE(lib/QTXTSRC)] [SRCMBR(*WSCST|name)]
/// [TEXT(*SRCMBRTXT|text)] [REPLACE(*YES|*NO)]`.
pub fn crtwscst(system: &mut System, me: u32, p: &Params) -> Result<String, String> {
    let (lib, name) = qualified(&p.pos_or(0, "WSCST")?, "*CURLIB");
    let lib = if lib == "*LIBL" { "QGPL".to_string() } else { lib };
    let (slib, sfile) = qualified(p.kw("SRCFILE").unwrap_or("QTXTSRC"), "*LIBL");
    let mbr = match p.upper("SRCMBR").as_deref() {
        None | Some("*WSCST") => name.clone(),
        Some(m) => m.to_string(),
    };
    let lines = super::pdm::read(system, &slib, &sfile, &mbr)?
        .ok_or_else(|| format!("CPF9815 member {mbr} not found in file {slib}/{sfile}"))?;
    let source: String = lines.iter().flat_map(|l| [l.trim_end(), "\n"]).collect();
    let mut w = Wscst::parse(&source).map_err(|e| format!("CPD0084 {slib}/{sfile} {mbr}: {e}"))?;
    w.text = match text(p) {
        None => w.name.clone(),
        Some(t) if t.eq_ignore_ascii_case("*SRCMBRTXT") => w.name.clone(),
        Some(t) => t,
    };
    let replace = !matches!(p.upper("REPLACE").as_deref(), Some("*NO"));
    let user = owner(system, me);
    with_store(system, |txn, qsys| {
        let l = wm::obj::library(txn, qsys, &lib).map_err(|e| err(&e))?;
        let n = qsname::Name::parse(&name).map_err(|e| format!("CPD0078 {name}: {e}"))?;
        let lo = Library::load(txn, l).map_err(|e| e.to_string())?;
        if let Some(old) = lo.lookup(txn, types::WSCST, n).map_err(|e| e.to_string())? {
            if !replace {
                return Err(format!("CPF5813 {lib}/{name} type *WSCST already exists"));
            }
            ops::delete(txn, JobHandle(u64::from(me)), old).map_err(|e| e.to_string())?;
        }
        wm::obj::create(txn, l, n, types::WSCST, &w.text, &stamp(user.clone()), &w.encode())
            .map(|_| ())
            .map_err(|e| err(&e))
    })?;
    Ok(format!("Workstation customizing object {name} created in library {lib}.\n"))
}

/// `RTVWSCST DEVTYPE(*TRANSFORM) MFRTYPMDL() SRCMBR() [SRCFILE()] [TEXT()]`:
/// the model's source into a member.
pub fn rtvwscst(system: &mut System, p: &Params) -> Result<String, String> {
    let devtype = p.upper("DEVTYPE").unwrap_or_else(|| "*TRANSFORM".into());
    if devtype != "*TRANSFORM" {
        return Err(format!(
            "CPD0084 DEVTYPE({devtype}): only *TRANSFORM sources are kept here"
        ));
    }
    let m = p.upper("MFRTYPMDL").unwrap_or_default();
    let model = Model::parse(&m)
        .ok_or_else(|| format!("CPD0084 MFRTYPMDL({m}): not a model the transform knows"))?;
    let mbr = p
        .upper("SRCMBR")
        .ok_or_else(|| "CPD0071 SRCMBR is needed".to_string())?;
    let (slib, sfile) = qualified(p.kw("SRCFILE").unwrap_or("QTXTSRC"), "*LIBL");
    let slib = if slib == "*LIBL" { "QGPL".to_string() } else { slib };
    let lines: Vec<String> = Wscst::source_for(model).lines().map(str::to_string).collect();
    super::pdm::write(system, &slib, &sfile, &mbr, &lines, "WSCST")?;
    Ok(format!(
        "Customizing source for {} written to member {mbr} of {slib}/{sfile}.\n",
        model.name()
    ))
}
