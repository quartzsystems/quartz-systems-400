// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DLTUSRPRF` and `ANZDFTPWD`.

use std::fmt::Write as _;

use store::obj::lock::JobHandle;
use store::obj::ops;
use store::txn::Txn;

use super::{Env, Keywords, one_of, usrprf};
use crate::auth::grant;
use crate::password;
use crate::profile::index::{self, Kind};
use crate::profile::spcaut::SpecialAuth;
use crate::profile::{groups, objects, shipped};
use crate::{Result, SecError};

/// `DLTUSRPRF USRPRF() [OWNOBJOPT(*NODLT|*DLT|*CHGOWN user)]
/// [PGPOPT(*NOCHG|*CHGPGP group|*CHGPGP *NONE)]`. The profile's private
/// authorities go with it; its owned objects and the objects it is the
/// primary group of are handled as the options say, or stop the delete.
pub fn dltusrprf(txn: &mut Txn<'_>, env: &Env<'_>, job: JobHandle, k: &Keywords) -> Result<String> {
    env.actor
        .require(SpecialAuth::SECADM, "delete user profiles")?;
    let name = k
        .upper("USRPRF")
        .ok_or_else(|| SecError::msg("CPD0071", "USRPRF is required."))?;
    let (pid, _) = objects::read(txn, env.qsys, &name)?;
    if shipped::shipped(&name).is_some() {
        return Err(SecError::msg(
            "CPF2245",
            format!("User profile {name} is shipped with the system and cannot be deleted."),
        ));
    }
    if name == env.actor.user {
        return Err(SecError::msg(
            "CPF2245",
            "You cannot delete your own user profile.",
        ));
    }
    let members = groups::members(txn, env.qsys, &name)?;
    if !members.is_empty() {
        return Err(SecError::msg(
            "CPF2257",
            format!(
                "User profile {name} is the group of {} and cannot be deleted.",
                members.join(" ")
            ),
        ));
    }
    let own = k.list("OWNOBJOPT").unwrap_or_else(|| vec!["*NODLT".into()]);
    let pgp = k.list("PGPOPT").unwrap_or_else(|| vec!["*NOCHG".into()]);
    one_of("OWNOBJOPT", &own[0], &["*NODLT", "*DLT", "*CHGOWN"])?;
    one_of("PGPOPT", &pgp[0], &["*NOCHG", "*CHGPGP"])?;
    let owned: Vec<_> = index::list(txn, env.qsys, &name, Kind::Owned)?
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| *id != pid)
        .collect();
    let grouped: Vec<_> = index::list(txn, env.qsys, &name, Kind::Group)?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    if !owned.is_empty() && own[0] == "*NODLT" {
        return Err(SecError::msg(
            "CPF2204",
            format!(
                "User profile {name} owns {} objects; OWNOBJOPT(*NODLT) keeps it.",
                owned.len()
            ),
        ));
    }
    if !grouped.is_empty() && pgp[0] == "*NOCHG" {
        return Err(SecError::msg(
            "CPF2204",
            format!(
                "User profile {name} is the primary group of {} objects; PGPOPT(*NOCHG) keeps it.",
                grouped.len()
            ),
        ));
    }
    let new_pgp = pgp.get(1).cloned().unwrap_or_else(|| "*NONE".into());
    for id in grouped {
        let g = (new_pgp != "*NONE").then_some(new_pgp.as_str());
        grant::set_group(txn, env.qsys, id, g, true)?;
    }
    match own[0].as_str() {
        "*DLT" => {
            for id in owned {
                ops::delete(txn, job, id)?;
            }
        }
        "*CHGOWN" => {
            let to = own
                .get(1)
                .ok_or_else(|| SecError::msg("CPD0071", "OWNOBJOPT(*CHGOWN) needs a user."))?;
            for id in owned {
                grant::set_owner(txn, env.qsys, id, to, true)?;
            }
        }
        _ => {}
    }
    for (id, _) in index::list(txn, env.qsys, &name, Kind::Private)? {
        grant::set_private(txn, env.qsys, id, &name, None)?;
    }
    ops::delete(txn, job, pid)?;
    crate::auth::cache::bump_epoch();
    Ok(format!("User profile {name} deleted."))
}

/// `ANZDFTPWD [ACTION(*NONE|*DISABLE|*PWDEXP)]`: the profiles whose
/// password is their own name, with what was done to each.
pub fn anzdftpwd(txn: &mut Txn<'_>, env: &Env<'_>, k: &Keywords) -> Result<String> {
    env.actor
        .require(SpecialAuth::ALLOBJ, "analyze default passwords")?;
    env.actor
        .require(SpecialAuth::SECADM, "analyze default passwords")?;
    let words = k.list("ACTION").unwrap_or_default();
    let disable = words.iter().any(|w| w == "*DISABLE");
    let expire = words.iter().any(|w| w == "*PWDEXP");
    for w in &words {
        one_of("ACTION", w, &["*NONE", "*DISABLE", "*PWDEXP"])?;
    }
    let pwdlvl = usrprf::policy(env).pwdlvl;
    let mut o = String::from(" User        Status     Password expired  Action\n");
    let mut found = 0;
    for n in objects::list(txn, env.qsys)? {
        let (id, mut p) = objects::read(txn, env.qsys, &n)?;
        if p.pwd_none || !password::matches(&p, &n, pwdlvl) {
            continue;
        }
        found += 1;
        let mut done = Vec::new();
        if disable && !p.disabled {
            p.disabled = true;
            done.push("*DISABLE");
        }
        if expire && !p.pwdexp {
            p.pwdexp = true;
            done.push("*PWDEXP");
        }
        if !done.is_empty() {
            p.changed = env.now;
            objects::write(txn, id, &p)?;
        }
        let _ = writeln!(
            o,
            " {n:<10}  {:<9}  {:<16}  {}",
            if p.disabled { "*DISABLED" } else { "*ENABLED" },
            if p.pwdexp { "*YES" } else { "*NO" },
            if done.is_empty() {
                "*NONE".to_string()
            } else {
                done.join(" ")
            }
        );
    }
    if disable || expire {
        crate::auth::cache::bump_epoch();
    }
    let _ = writeln!(o, " {found} profiles have a default password.");
    Ok(o)
}
