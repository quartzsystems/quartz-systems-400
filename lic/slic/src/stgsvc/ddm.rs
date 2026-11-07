// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DDM and DRDA in the service: the DDM server's password sign-on, DDM
//! files (`*FILE` objects of attribute `DDMF` holding
//! `ddm::objects::DdmFile`'s text) and server authentication entries
//! (`ADDSVRAUTE`), kept in `QSYS/QSYSVRAUT`, which only the service
//! reads.

use std::fmt::Write as _;

use hostsvr::common::rc;
use qsname::Name;
use stg::{HostRefused, HostSignedOn};
use store::obj::library::Library;
use store::obj::ops;
use store::obj::{JobHandle, ObjectId, types};
use store::txn::Txn;

use super::served::JobCtx;
use super::spool::with_store;
use crate::ipl::system::{System, now_unix};

/// The attribute of a DDM file.
pub const DDMF: &str = "DDMF";

/// Where server authentication entries are kept.
const SVRAUT: &str = "QSYSVRAUT";

/// `ddm_signon`: the password checked as a sign-on.
pub fn signon(
    system: &mut System,
    ctx: &mut JobCtx,
    user: &str,
    password: &str,
    swap: bool,
) -> Result<HostSignedOn, HostRefused> {
    let upper = user.trim().to_ascii_uppercase();
    if upper.is_empty() {
        return Err(HostRefused {
            rc: rc::USER_ID_LENGTH,
            msgid: "CPF1107".into(),
            text: "No user was given.".into(),
        });
    }
    let s = super::security::sign_on(system, &upper, password, "", false, None).map_err(|e| {
        HostRefused {
            rc: match e.msgid.as_str() {
                "CPF1394" | "CPF1110" | "CPF1220" => rc::USER_DISABLED,
                "CPF1118" => rc::PASSWORD_NONE,
                _ => rc::PASSWORD_INCORRECT,
            },
            msgid: e.msgid,
            text: e.text,
        }
    })?;
    if s.next == sec::signon::Next::ChangePassword {
        return Err(HostRefused {
            rc: rc::PASSWORD_EXPIRED,
            msgid: "CPF22E4".into(),
            text: "The password has expired.".into(),
        });
    }
    if swap {
        super::hostsvr::become_user(system, ctx, &s.user);
    }
    let t = now_unix();
    Ok(HostSignedOn {
        user: s.user,
        now: t,
        previous: s.previous,
        expires: 0,
        warning_days: 0,
    })
}

fn name(n: &str) -> Result<Name, String> {
    Name::parse(n.trim()).map_err(|e| format!("CPD0078 {n}: {e}"))
}

fn stamp(owner: String) -> wm::obj::Stamp {
    wm::obj::Stamp {
        owner,
        system: "QS400".into(),
        now: now_unix(),
    }
}

/// A `*FILE` named `n` in `lib`, with its attribute.
fn file_in(
    txn: &mut Txn<'_>,
    lib: ObjectId,
    n: Name,
) -> Result<Option<(ObjectId, String)>, String> {
    let l = Library::load(txn, lib).map_err(|e| e.to_string())?;
    let Some(id) = l.lookup(txn, types::FILE, n).map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let (_, h) = ops::read_header(txn, id).map_err(|e| e.to_string())?;
    Ok(Some((id, ccsid::decode_trimmed(&h.attribute))))
}

/// `ddmf_put`.
#[allow(clippy::too_many_arguments)] // the request's fields
pub fn put(
    system: &mut System,
    me: u32,
    lib: &str,
    file: &str,
    payload: &str,
    text: &str,
    create: bool,
    replace: bool,
) -> Result<String, String> {
    let lib = match lib.trim().to_ascii_uppercase().as_str() {
        "" | "*CURLIB" | "*LIBL" if create => "QGPL".to_string(),
        l => l.to_string(),
    };
    let owner = system
        .jobs
        .get(me)
        .map_or_else(|| "QSYS".into(), |j| j.user.clone());
    let n = name(file)?;
    let job = JobHandle(u64::from(me));
    with_store(system, |txn, qsys| {
        let l = if lib == "*LIBL" {
            let names = crate::svc::ile::name_context(txn, qsys);
            ops::resolve(txn, qsys, &names, qsname::LibSpec::Libl, n, types::FILE)
                .map(|r| r.library)
                .map_err(|_| format!("CPF9812 File {file} in library *LIBL not found."))?
        } else {
            wm::obj::library(txn, qsys, &lib)
                .map_err(|e| format!("CPF9810 Library {lib} not found: {e}"))?
        };
        let found = file_in(txn, l, n)?;
        match (create, found) {
            (true, Some((id, _))) if replace => {
                ops::delete(txn, job, id).map_err(|e| format!("CPF7302 {e}"))?;
            }
            (true, Some(_)) => {
                return Err(format!(
                    "CPF7302 File {file} in library {lib} exists already."
                ));
            }
            (false, None) => {
                return Err(format!("CPF9812 File {file} in library {lib} not found."));
            }
            (false, Some((_, a))) if a != DDMF => {
                return Err(format!(
                    "CPF7304 File {file} in library {lib} is not a DDM file."
                ));
            }
            (false, Some((id, _))) => {
                wm::obj::write(txn, id, payload.as_bytes()).map_err(|e| e.to_string())?;
                ops::change_text(txn, id, text, now_unix()).map_err(|e| e.to_string())?;
                let (_, h) = ops::read_header(txn, l).map_err(|e| e.to_string())?;
                return Ok(h.name.to_string().trim().to_string());
            }
            (true, None) => {}
        }
        wm::obj::create_attr(
            txn,
            l,
            (n, types::FILE, DDMF),
            text,
            &stamp(owner.clone()),
            payload.as_bytes(),
        )
        .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF7302")))?;
        let (_, h) = ops::read_header(txn, l).map_err(|e| e.to_string())?;
        Ok(h.name.to_string().trim().to_string())
    })
}

/// `ddmf_get`.
pub fn get(system: &mut System, lib: &str, file: &str) -> Result<(String, String), String> {
    let n = name(file)?;
    let lib = lib.trim().to_ascii_uppercase();
    with_store(system, |txn, qsys| {
        let names = crate::svc::ile::name_context(txn, qsys);
        let spec = match lib.as_str() {
            "" | "*LIBL" => qsname::LibSpec::Libl,
            "*CURLIB" => qsname::LibSpec::Curlib,
            l => qsname::LibSpec::Name(name(l)?),
        };
        let r = ops::resolve(txn, qsys, &names, spec, n, types::FILE)
            .map_err(|_| format!("CPF9812 File {file} in library {lib} not found."))?;
        let (_, h) = ops::read_header(txn, r.id).map_err(|e| e.to_string())?;
        if ccsid::decode_trimmed(&h.attribute) != DDMF {
            return Err(format!("CPF7304 File {file} is not a DDM file."));
        }
        let payload = wm::obj::read(txn, r.id).map_err(|e| e.to_string())?;
        let (_, lh) = ops::read_header(txn, r.library).map_err(|e| e.to_string())?;
        Ok((
            lh.name.to_string().trim().to_string(),
            String::from_utf8_lossy(&payload).into_owned(),
        ))
    })
}

/// One server authentication entry.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    user: String,
    server: String,
    usrid: String,
    password: String,
}

fn hex(s: &str) -> String {
    s.bytes().fold(String::new(), |mut o, b| {
        let _ = write!(o, "{b:02X}");
        o
    })
}

fn unhex(s: &str) -> String {
    let b: Vec<u8> = (0..s.len() / 2)
        .filter_map(|i| u8::from_str_radix(s.get(2 * i..2 * i + 2)?, 16).ok())
        .collect();
    String::from_utf8_lossy(&b).into_owned()
}

/// One line per entry: user, server, user ID and password, the last two
/// in hexadecimal.
fn encode(entries: &[Entry]) -> String {
    entries.iter().fold(String::new(), |mut s, e| {
        let _ = writeln!(
            s,
            "{} {} {} {}",
            e.user,
            hex(&e.server),
            hex(&e.usrid),
            hex(&e.password)
        );
        s
    })
}

fn decode(text: &str) -> Vec<Entry> {
    text.lines()
        .filter_map(|l| {
            let mut w = l.split(' ');
            Some(Entry {
                user: w.next()?.to_string(),
                server: unhex(w.next()?),
                usrid: unhex(w.next().unwrap_or("")),
                password: unhex(w.next().unwrap_or("")),
            })
        })
        .collect()
}

fn entries(system: &mut System) -> Vec<Entry> {
    sec::guard::as_system(|| {
        with_store(system, |txn, qsys| {
            let l = Library::load(txn, qsys).map_err(|e| e.to_string())?;
            let Some(id) = l
                .lookup(txn, types::USRSPC, name(SVRAUT)?)
                .map_err(|e| e.to_string())?
            else {
                return Ok(Vec::new());
            };
            let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
            Ok(decode(&String::from_utf8_lossy(&b)))
        })
    })
    .unwrap_or_default()
}

fn save(system: &mut System, all: &[Entry]) -> Result<(), String> {
    let text = encode(all);
    sec::guard::as_system(|| {
        with_store(system, |txn, qsys| {
            let n = name(SVRAUT)?;
            let l = Library::load(txn, qsys).map_err(|e| e.to_string())?;
            let id = if let Some(id) = l.lookup(txn, types::USRSPC, n).map_err(|e| e.to_string())? {
                id
            } else {
                let id = wm::obj::create(
                    txn,
                    qsys,
                    n,
                    types::USRSPC,
                    "Server authentication entries",
                    &stamp("QSYS".into()),
                    b"",
                )
                .map_err(|e| e.to_string())?;
                sec::auth::grant::set_public(txn, id, Some(sec::auth::bits::Auth::EXCLUDE))
                    .map_err(|e| e.to_string())?;
                id
            };
            wm::obj::write(txn, id, text.as_bytes()).map_err(|e| e.to_string())
        })
    })
}

/// The job's user.
fn current(ctx: &JobCtx) -> String {
    ctx.session.borrow().principal.user.clone()
}

/// `svraute`.
pub fn svraute(
    system: &mut System,
    ctx: &JobCtx,
    op: &str,
    user: &str,
    server: &str,
    usrid: Option<String>,
    password: Option<String>,
) -> Result<String, String> {
    let me = current(ctx);
    let user = match user.trim().to_ascii_uppercase().as_str() {
        "" | "*CURRENT" => me.clone(),
        u => u.to_string(),
    };
    if user != me {
        super::security::require(
            sec::profile::SpecialAuth::SECADM,
            "change another profile's server authentication entries",
        )?;
    }
    if super::hostsvr::read(system, &user).is_none() {
        return Err(format!("CPF2204 User profile {user} not found."));
    }
    let server = server.trim().to_string();
    // Server names other than the DDM's are kept as typed.
    let key = server.to_ascii_uppercase();
    let keep = super::security::sysval_text(system, "QRETSVRSEC").trim() == "1";
    let mut all = entries(system);
    let at = all
        .iter()
        .position(|e| e.user == user && e.server.to_ascii_uppercase() == key);
    let usrid = usrid.map(|u| {
        if u.eq_ignore_ascii_case("*USRPRF") {
            user.clone()
        } else {
            u
        }
    });
    let password = password.map(|p| {
        if p.eq_ignore_ascii_case("*NONE") || !keep {
            String::new()
        } else {
            p
        }
    });
    let done = match (op, at) {
        ("ADD", Some(_)) => {
            return Err(format!(
                "CPF2112 Server authentication entry {server} exists already for {user}."
            ));
        }
        ("ADD", None) => {
            all.push(Entry {
                user: user.clone(),
                server: server.clone(),
                usrid: usrid.unwrap_or_else(|| user.clone()),
                password: password.unwrap_or_default(),
            });
            "added"
        }
        ("CHG", Some(i)) => {
            if let Some(u) = usrid {
                all[i].usrid = u;
            }
            if let Some(p) = password {
                all[i].password = p;
            }
            "changed"
        }
        ("RMV", Some(i)) => {
            all.remove(i);
            "removed"
        }
        (_, None) => {
            return Err(format!(
                "CPF9801 No server authentication entry {server} is kept for {user}."
            ));
        }
        _ => return Err(format!("CPD0084 {op}")),
    };
    save(system, &all)?;
    Ok(format!(
        "Server authentication entry {server} {done} for {user}.\n"
    ))
}

/// `svraute_get`.
pub fn svraute_get(system: &mut System, ctx: &JobCtx, server: &str) -> Option<(String, String)> {
    let me = current(ctx);
    let key = server.trim().to_ascii_uppercase();
    entries(system)
        .into_iter()
        .find(|e| e.user == me && e.server.to_ascii_uppercase() == key)
        .map(|e| (e.usrid, e.password))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip() {
        let e = vec![Entry {
            user: "BOB".into(),
            server: "QDDMDRDASERVER".into(),
            usrid: "bob".into(),
            password: "a b'c".into(),
        }];
        assert_eq!(decode(&encode(&e)), e);
    }
}
