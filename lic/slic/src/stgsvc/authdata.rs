// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panel data for the authority displays (request 270): an object's
//! authorities (`DSPOBJAUT`, `EDTOBJAUT`), the objects a list secures
//! (`DSPAUTLOBJ`) and the objects a user owns (`WRKOBJOWN`), from the
//! security layer rather than its text.

use sec::auth::bits::Auth;
use sec::profile::index::Kind;
use stg::proto::Sys2Data;
use store::obj::header::Authority as A;
use wm::cmd::Params;

use crate::ipl::system::System;

type Vars = Vec<(String, String)>;

/// The single authorities in the order the panels show them.
pub const BITS: [(&str, u32); 10] = [
    ("OPR", A::OBJOPR),
    ("MGT", A::OBJMGT),
    ("EXIST", A::OBJEXIST),
    ("ALTER", A::OBJALTER),
    ("REF", A::OBJREF),
    ("READ", A::READ),
    ("ADD", A::ADD),
    ("UPD", A::UPD),
    ("DLT", A::DLT),
    ("EXE", A::EXECUTE),
];

/// Where `DSPOBJAUT`'s detail view puts each `X`, from the start of
/// its `BITS` column.
const BIT_COLS: [usize; 10] = [1, 5, 10, 16, 21, 26, 30, 34, 38, 42];

/// The data of display `what`, or `None` when it is not one of these.
pub fn panel(system: &mut System, what: &str, args: &[String]) -> Option<Result<Sys2Data, String>> {
    let arg = |i: usize| {
        args.get(i)
            .map(|s| s.trim().to_ascii_uppercase())
            .unwrap_or_default()
    };
    Some(match what {
        "OBJAUT" => object_authorities(system, &arg(0), &arg(1), &arg(2)),
        "AUTLOBJ" => secured(system, &arg(0)),
        "OBJOWN" => owned(system, &arg(0)),
        _ => return None,
    })
}

fn pairs(p: &[(&str, String)]) -> Vars {
    p.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

/// The detail view's line of `X`s for `bits`.
fn detail(bits: Auth) -> String {
    let mut line = vec![' '; BIT_COLS[9] + 1];
    for ((_, m), &c) in BITS.iter().zip(&BIT_COLS) {
        if bits.0 & m != 0 {
            line[c] = 'X';
        }
    }
    line.into_iter().collect()
}

/// `OBJAUT LIB NAME TYPE`: the object's names, owner, group and list,
/// and in `AUTS` each user with the authority, the detail line
/// (`BITS`) and each single authority (`OPR` .. `EXE`).
fn object_authorities(system: &mut System, lib: &str, name: &str, ty: &str) -> Result<Sys2Data, String> {
    let lib = if lib.is_empty() { "*LIBL" } else { lib };
    let p = Params::parse(&format!("OBJ({lib}/{name}) OBJTYPE({ty})"));
    let a = crate::svc::security::in_store(system, |txn, env| {
        let id = crate::svc::security::object(txn, env.qsys, &p)?;
        sec::cmd::object::authorities(txn, env, id)
    })?;
    let rows = a
        .rows
        .iter()
        .map(|r| {
            let mut row = pairs(&[
                ("USER", r.user.clone()),
                ("GROUP", r.group.clone()),
                ("AUT", r.shown.clone()),
                ("BITS", detail(r.bits)),
            ]);
            for (v, m) in BITS {
                row.push((v.into(), if r.bits.0 & m != 0 { "X" } else { "" }.into()));
            }
            row
        })
        .collect();
    let vars = pairs(&[
        ("OBJ", a.name),
        ("OBJLIB", a.lib),
        ("OBJTYPE", a.ty),
        ("OWNER", a.owner),
        ("PGP", a.pgp),
        ("AUTL", a.autl),
    ]);
    Ok((vars, vec![("AUTS".into(), rows)]))
}

/// `AUTLOBJ LIST`: the list's owner and the objects it secures.
fn secured(system: &mut System, list: &str) -> Result<Sys2Data, String> {
    let (owner, objs) =
        crate::svc::security::in_store(system, |txn, env| sec::cmd::object::secured_objects(txn, env, list))?;
    let rows = objs
        .into_iter()
        .map(|o| {
            pairs(&[
                ("OBJ", o.name),
                ("OBJLIB", o.lib),
                ("OBJTYPE", o.ty),
                ("OBJOWNER", o.owner),
                ("OBJTEXT", o.text),
            ])
        })
        .collect();
    Ok((
        pairs(&[("AUTL", list.to_string()), ("OWNER", owner)]),
        vec![("OBJS".into(), rows)],
    ))
}

/// `OBJOWN USER`: the objects the user owns with its authority.
fn owned(system: &mut System, user: &str) -> Result<Sys2Data, String> {
    let (user, objs) = crate::svc::security::in_store(system, |txn, env| {
        sec::cmd::object::user_objects(txn, env, user, Kind::Owned)
    })?;
    let rows = objs
        .into_iter()
        .map(|o| {
            pairs(&[
                ("OPT", String::new()),
                ("OBJ", o.name),
                ("OBJLIB", o.lib),
                ("OBJTYPE", o.ty),
                ("OBJAUT", o.aut),
            ])
        })
        .collect();
    Ok((pairs(&[("USRPRF", user)]), vec![("OBJS".into(), rows)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_marks_each_authority_under_its_heading() {
        let heads = "Opr Mgt Exist Alter Ref  Read Add Upd Dlt Exe";
        let all = detail(Auth::ALL);
        for (i, &c) in BIT_COLS.iter().enumerate() {
            assert_eq!(&all[c..=c], "X", "{i}");
            assert_ne!(&heads[c..=c], " ", "{i}");
        }
        assert_eq!(detail(Auth::EXCLUDE).trim(), "");
    }
}
