// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The profile and authority commands on the service console: thin
//! wrappers over `sec::cmd`, run as the console's job.

use std::fmt::Write as _;

use sec::cmd::{dltusrprf, object, usrprf};
use sec::profile::index::Kind;
use store::obj::lock::JobHandle;
use wm::cmd::Params;

use super::security::{actor, in_store, keywords, object as resolve};
use crate::ipl::system::System;

/// Help lines.
pub const HELP: &str = "\
whoami                        the console job's user, groups and special authorities
crtusrprf USRPRF(..) [PASSWORD(..)] [USRCLS(..)] [SPCAUT(..)] [GRPPRF(..)] ...
chgusrprf USRPRF(..) ...      | dltusrprf USRPRF(..) [OWNOBJOPT(*NODLT|*DLT|*CHGOWN U)] [PGPOPT(*NOCHG|*CHGPGP G)]
dspusrprf USRPRF(..) [TYPE(*BASIC|*GRPMBR|*OBJOWN|*OBJAUT|*OBJPGP)] | wrkusrprf | dspautusr [SEQ(*GRPPRF)]
anzdftpwd [ACTION(*NONE|*DISABLE|*PWDEXP)]
grtobjaut OBJ(LIB/NAME) OBJTYPE(*T) USER(..) AUT(..) [REPLACE(*YES)] | AUTL(..)
rvkobjaut OBJ(..) OBJTYPE(..) USER(..) AUT(..) | AUTL(..)
dspobjaut OBJ(..) OBJTYPE(..) [AUTFMT(*DETAIL)] | edtobjaut OBJ(..) OBJTYPE(..)
chgobjown OBJ(..) OBJTYPE(..) NEWOWN(..) [CUROWNAUT(*SAME)] | chgobjpgp OBJ(..) OBJTYPE(..) NEWPGP(..)
chkobj OBJ(..) OBJTYPE(..) [AUT(..)] | grtusraut USER(..) REFUSER(..) | wrkobjown [USRPRF(..)]
crtautl AUTL(..) [AUT(..)] [TEXT(..)] | addautle|chgautle AUTL(..) USER(..) AUT(..) | rmvautle AUTL(..) USER(..)
dspautl AUTL(..) | dspautlobj AUTL(..)
";

fn done(mut s: String) -> String {
    s.push('\n');
    s
}

/// The profile and authority commands; `None` when `cmd` is not one.
pub fn command(system: &mut System, cmd: &str, line: &str) -> Option<String> {
    let p = Params::parse(line);
    let k = keywords(&p);
    let user = || {
        p.upper("USRPRF")
            .or_else(|| p.pos.first().map(|s| s.to_ascii_uppercase()))
            .unwrap_or_else(|| "*CURRENT".into())
    };
    let job = JobHandle(u64::from(system.current_job));
    let r = match cmd {
        "whoami" => Ok(whoami()),
        "crtusrprf" => in_store(system, |txn, env| usrprf::crtusrprf(txn, env, &k)).map(done),
        "chgusrprf" => in_store(system, |txn, env| usrprf::chgusrprf(txn, env, &k)).map(done),
        "dltusrprf" => {
            in_store(system, |txn, env| dltusrprf::dltusrprf(txn, env, job, &k)).map(done)
        }
        "dspusrprf" => dspusrprf(
            system,
            &user(),
            p.upper("TYPE").as_deref().unwrap_or("*BASIC"),
        ),
        "wrkusrprf" => in_store(system, usrprf::list).map(|rows| {
            rows.iter().fold(
                String::from(" User        Class       Text\n"),
                |mut o, (n, c, t)| {
                    let _ = writeln!(o, " {n:<10}  {:<10}  {t}", c.name());
                    o
                },
            )
        }),
        "dspautusr" => {
            let by_group = p.upper("SEQ").as_deref() == Some("*GRPPRF");
            in_store(system, |txn, env| usrprf::dspautusr(txn, env, by_group))
        }
        "anzdftpwd" => in_store(system, |txn, env| dltusrprf::anzdftpwd(txn, env, &k)),
        "grtobjaut" => on_object(system, &p, |txn, env, id| {
            object::grtobjaut(txn, env, id, &k)
        })
        .map(done),
        "rvkobjaut" => on_object(system, &p, |txn, env, id| {
            object::rvkobjaut(txn, env, id, &k)
        })
        .map(done),
        // The serial console cannot edit a list in place: EDTOBJAUT shows it.
        "dspobjaut" | "edtobjaut" if p.upper("AUTFMT").as_deref() == Some("*DETAIL") => {
            on_object(system, &p, |txn, _, id| object::dspobjaut_detail(txn, id))
        }
        "dspobjaut" | "edtobjaut" => on_object(system, &p, object::dspobjaut),
        "chgobjown" => on_object(system, &p, |txn, env, id| {
            object::chgobjown(txn, env, id, &k)
        })
        .map(done),
        "chgobjpgp" => on_object(system, &p, |txn, env, id| {
            object::chgobjpgp(txn, env, id, &k)
        })
        .map(done),
        "chkobj" => {
            on_object(system, &p, |txn, env, id| object::chkobj(txn, env, id, &k)).map(done)
        }
        "grtusraut" => in_store(system, |txn, env| object::grtusraut(txn, env, &k)).map(done),
        "wrkobjown" => in_store(system, |txn, env| {
            object::objects_of(txn, env, &user(), Kind::Owned)
        }),
        "crtautl" => in_store(system, |txn, env| object::crtautl(txn, env, &k)).map(done),
        "addautle" => in_store(system, |txn, env| object::setautle(txn, env, &k, true)).map(done),
        "chgautle" => in_store(system, |txn, env| object::setautle(txn, env, &k, false)).map(done),
        "rmvautle" => in_store(system, |txn, env| object::rmvautle(txn, env, &k)).map(done),
        "dspautl" | "dspautlobj" => {
            let name = p
                .upper("AUTL")
                .or_else(|| p.pos.first().map(|s| s.to_ascii_uppercase()))
                .unwrap_or_default();
            if cmd == "dspautl" {
                in_store(system, |txn, env| object::dspautl(txn, env, &name))
            } else {
                in_store(system, |txn, env| object::dspautlobj(txn, env, &name))
            }
        }
        _ => return None,
    };
    Some(r.unwrap_or_else(|e| format!("{e}\n")))
}

/// Run `f` on the object `OBJ() OBJTYPE()` names.
fn on_object(
    system: &mut System,
    p: &Params,
    f: impl FnOnce(&mut store::Txn<'_>, &sec::cmd::Env<'_>, store::obj::ObjectId) -> sec::Result<String>,
) -> Result<String, String> {
    in_store(system, |txn, env| {
        let id = resolve(txn, env.qsys, p)?;
        f(txn, env, id)
    })
}

/// `DSPUSRPRF`: one of its displays.
fn dspusrprf(system: &mut System, user: &str, ty: &str) -> Result<String, String> {
    let now = crate::ipl::system::now_unix();
    in_store(system, |txn, env| match ty {
        "*BASIC" => {
            let p = usrprf::rtvusrprf(txn, env, user)?;
            Ok(usrprf::display_basic(&p, &usrprf::policy(env), now))
        }
        "*GRPMBR" => usrprf::display_members(txn, env, user),
        "*OBJOWN" => object::objects_of(txn, env, user, Kind::Owned),
        "*OBJAUT" => object::objects_of(txn, env, user, Kind::Private),
        "*OBJPGP" => object::objects_of(txn, env, user, Kind::Group),
        other => Err(sec::cmd::bad_value("TYPE", other)),
    })
}

/// `whoami`: the console job's user, groups, special authorities and
/// any adopted owners.
fn whoami() -> String {
    let me = actor();
    let mut o = format!("User . . . . . . . :   {}\n", me.user);
    let _ = writeln!(
        o,
        "Groups . . . . . . :   {}",
        if me.groups.is_empty() {
            "*NONE".into()
        } else {
            me.groups.join(" ")
        }
    );
    let _ = writeln!(
        o,
        "Special authority  :   {}",
        me.special().names().join(" ")
    );
    let _ = writeln!(o, "Security level . . :   {}", me.qsecurity);
    for a in &me.adopted {
        let _ = writeln!(
            o,
            "Adopted  . . . . . :   {} (program {})",
            a.owner, a.program
        );
    }
    o
}
