// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGDIRSVRA` (docs/DIRSRV.md section 2.2): the administrator of a
//! directory server instance and its password, or the password of its
//! update DN, written to the instance's `ibmslapd.conf` through
//! `dirsrv::config`. The password is kept as `{SSHA512}`. Needs
//! `*ALLOBJ` and `*IOSYSCFG` (`CPF222E`).

use cmd::Value;
use cmd::block::Parms;
use dirsrv::config::{ConfigError, SlapdConf, Var};
use ifscmd::{Msg, Reply};
use sec::profile::SpecialAuth;
use stg::proto::CmdReply;

use super::stmfcmd::reply;
use crate::ipl::system::System;
use crate::stgsvc::dirconf;

/// Run `name`, or `None` when it is not `CHGDIRSVRA`.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    (name == "CHGDIRSVRA").then(|| reply(chgdirsvra(system, parms), ""))
}

/// A configuration problem as its `GLD` message.
fn fail(e: &ConfigError) -> Reply {
    let m = e
        .vars()
        .into_iter()
        .fold(Msg::new(e.msgid()), |m, v| match v {
            Var::Chars(t, n) => m.chars(&t, n),
            Var::Vary(t) => m.vary(&t),
        });
    Reply::fail(m)
}

/// `CPF222E` naming the first special authority the user lacks.
fn need_authority() -> Option<Reply> {
    let p = crate::stgsvc::security::actor()?;
    [
        (SpecialAuth::ALLOBJ, "*ALLOBJ"),
        (SpecialAuth::IOSYSCFG, "*IOSYSCFG"),
    ]
    .into_iter()
    .find(|(a, _)| !p.has(*a))
    .map(|(_, n)| Reply::fail(Msg::new("CPF222E").chars(n, 10)))
}

/// `DN`'s two elements; `None` for `*SAME`.
fn dn_parm(parms: &Parms) -> Option<(String, String)> {
    let Some(Value::Elems(el)) = parms.get("DN") else {
        return None;
    };
    let text = |i: usize| match el.get(i).and_then(Option::as_ref) {
        Some(Value::Char(s)) => s.trim_end().to_string(),
        Some(v) => v.source(),
        None => String::new(),
    };
    let dn = text(0);
    (!dn.trim().eq_ignore_ascii_case("*SAME")).then(|| (dn, text(1)))
}

fn chgdirsvra(system: &mut System, parms: &Parms) -> Reply {
    if let Some(r) = need_authority() {
        return r;
    }
    let instance = parms.name("INSTANCE");
    let list = match dirconf::instances(system) {
        Ok(l) => l,
        Err(e) => return fail(&e),
    };
    let Some(inst) = list.find(&instance).cloned() else {
        return fail(&ConfigError::NoInstance(instance));
    };
    let done = Reply::done(Msg::new("GLD0123").chars(&inst.name, 8));
    let Some((dn, password)) = dn_parm(parms) else {
        return done;
    };
    if password.is_empty() {
        return fail(&ConfigError::NoPassword);
    }
    let path = inst.conf_path();
    let text = match dirconf::read(system, &path) {
        Ok(Some(t)) => t,
        Ok(None) => {
            return fail(&ConfigError::File {
                file: path,
                why: ifs::Errno::ENOENT.text().into(),
            });
        }
        Err(why) => return fail(&ConfigError::File { file: path, why }),
    };
    let mut conf = match SlapdConf::parse(&path, &text) {
        Ok(c) => c,
        Err(e) => return fail(&e),
    };
    let stored = dirsrv::pwd::hash(password.as_bytes());
    let r = match dn.trim().to_ascii_uppercase().as_str() {
        "*ADMINDN" => conf.set_admin(None, &stored),
        "*UPDATEDN" => conf.set_update_pw(&inst.name, &stored),
        _ => conf.set_admin(Some(&dn), &stored),
    };
    if let Err(e) = r {
        return fail(&e);
    }
    match dirconf::write(system, &path, conf.text()) {
        Ok(()) => {
            // A running instance takes the new password at once.
            if let Err(e) = crate::stgsvc::dirsrv::serve::config_changed(system, &inst.name) {
                eprintln!("qsstg: CHGDIRSVRA {}: {e}", inst.name);
            }
            done
        }
        Err(why) => fail(&ConfigError::File { file: path, why }),
    }
}
