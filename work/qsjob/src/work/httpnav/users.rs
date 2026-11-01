// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::users::Users` as the user, through the security
//! APIs and panel data the 5250 displays use: the profiles from
//! `QSYLAUTU` and `QSYRUSRI` (`sec_api Users`, `Profile`), a profile's
//! objects from `QSYLOBJA` (`Objects`), authorization lists from
//! `autl_panel_list` and `autl_panel` (`WRKAUTL`, `DSPAUTL`), the
//! objects a list secures from `QSYLATLO` (`ListObjects`, what
//! `DSPAUTLOBJ` shows); changes are commands (`run`).

use navigator::env::users::{AuthEntry, AuthList, ObjectAuthority, Profile, SPECIAL, SecuredObject, Users};
use qsweb::msg::Message;
use sec::api::{Answer, Query};
use sec::auth::bits::Auth;
use sec::profile::UsrPrf;
use sec::profile::payload::{NOMAX, SYSVAL};
use stg::Client;

use super::Link;
use crate::work::web::bridge::Main;

/// Profiles and authorization lists.
pub struct LiveUsers {
    /// The job's thread.
    pub link: Link,
}

/// `lib/name`, or the special value alone.
fn qual((name, lib): &(String, String)) -> String {
    if name.is_empty() {
        "*NONE".into()
    } else if name.starts_with('*') || lib.is_empty() {
        name.clone()
    } else {
        format!("{lib}/{name}")
    }
}

fn when(secs: u64) -> Option<u64> {
    (secs != 0).then_some(secs)
}

/// One profile as the pages show it; `is_group` decided by the caller.
fn profile(p: UsrPrf, is_group: bool) -> Profile {
    let have = p.spcaut.names();
    let special = SPECIAL.iter().filter(|s| have.contains(s)).map(|s| (*s).to_string()).collect();
    Profile {
        text: p.text.trim_end().to_string(),
        enabled: !p.disabled,
        class: p.usrcls.name().to_string(),
        last_signon: when(p.last_signon),
        group: if p.grpprf.starts_with('*') { String::new() } else { p.grpprf.clone() },
        supplemental: p.supgrpprf.iter().filter(|g| !g.starts_with('*')).cloned().collect(),
        gid: (p.gid != 0).then_some(p.gid),
        is_group,
        pwd_changed: p.pwd_changed,
        invalid_signons: p.signon_invalid,
        pwd_interval: match p.pwdexpitv {
            SYSVAL => "*SYSVAL".into(),
            NOMAX => "*NOMAX".into(),
            n => n.to_string(),
        },
        initial_program: qual(&p.inlpgm),
        initial_menu: qual(&p.inlmnu),
        current_library: p.curlib.clone(),
        limit_capabilities: p.lmtcpb.name().to_string(),
        jobd: qual(&p.jobd),
        outq: qual(&p.outq),
        msgq: qual(&p.msgq),
        home: p.homedir.clone(),
        special,
        language: p.langid.clone(),
        country: p.cntryid.clone(),
        ccsid: p.ccsid.to_string(),
        owned: u32::try_from(p.owned_count).unwrap_or(u32::MAX),
        name: p.name,
    }
}

/// Every profile the user may see, read with the security APIs.
fn profiles(m: &mut Main) -> Vec<Profile> {
    let q = Query::Users {
        select: "*ALL".into(),
        group: String::new(),
    };
    let Ok(Answer::Users { rows }) = m.sec(q) else {
        return Vec::new();
    };
    let all: Vec<UsrPrf> = rows
        .iter()
        .filter(|r| r.name != "*PUBLIC")
        .filter_map(|r| match m.sec(Query::Profile { user: r.name.clone() }) {
            Ok(Answer::Profile { data }) => UsrPrf::decode(&data).ok(),
            _ => None,
        })
        .collect();
    // A group: another profile names it, or it has a GID.
    let named = |n: &str| all.iter().any(|p| p.grpprf == n || p.supgrpprf.iter().any(|g| g == n));
    let groups: Vec<bool> = all.iter().map(|p| p.gid != 0 || named(&p.name)).collect();
    all.into_iter().zip(groups).map(|(p, g)| profile(p, g)).collect()
}

/// `autl_panel`'s rows as a list: the owner and text, then its users.
fn list(name: String, rows: &[Vec<String>], objects: u32) -> AuthList {
    let cell = |r: &Vec<String>, i: usize| r.get(i).map_or(String::new(), |s| s.trim().to_string());
    let head = rows.first();
    AuthList {
        name,
        owner: head.map(|r| cell(r, 0)).unwrap_or_default(),
        text: head.map(|r| cell(r, 1)).unwrap_or_default(),
        objects,
        entries: rows
            .iter()
            .skip(1)
            .map(|r| AuthEntry {
                user: cell(r, 0),
                authority: cell(r, 1),
                list_mgt: cell(r, 2) == "X",
            })
            .collect(),
    }
}

fn secured(m: &Main, name: &str) -> Vec<SecuredObject> {
    match m.sec(Query::ListObjects { list: name.to_string() }) {
        Ok(Answer::Objects { rows }) => rows
            .into_iter()
            .map(|r| SecuredObject {
                object: r.name,
                lib: r.lib,
                typ: r.ty,
                owner: r.owner,
            })
            .collect(),
        _ => Vec::new(),
    }
}

impl Users for LiveUsers {
    fn profiles(&self, user: &str) -> Vec<Profile> {
        self.link.read(user, profiles)
    }

    fn authorities(&self, user: &str, name: &str) -> Vec<ObjectAuthority> {
        let n = name.trim().to_ascii_uppercase();
        self.link.read(user, move |m| match m.sec(Query::Objects { user: n, kind: 0 }) {
            Ok(Answer::Objects { rows }) => rows
                .into_iter()
                .map(|r| ObjectAuthority {
                    authority: Auth(r.bits).display(r.ty == "*AUTL").to_string(),
                    object: r.name,
                    lib: r.lib,
                    typ: r.ty,
                    owner: r.owned,
                })
                .collect(),
            _ => Vec::new(),
        })
    }

    fn lists(&self, user: &str) -> Vec<AuthList> {
        self.link.read(user, |m| {
            let names = m.link.borrow_mut().with(Client::autl_panel_list).unwrap_or_default();
            names
                .into_iter()
                .map(|(n, _)| {
                    let q = n.clone();
                    let rows = m.link.borrow_mut().with(|l| l.autl_panel(q)).unwrap_or_default();
                    let objects = u32::try_from(secured(m, &n).len()).unwrap_or(0);
                    list(n, &rows, objects)
                })
                .collect()
        })
    }

    fn secured(&self, user: &str, name: &str) -> Vec<SecuredObject> {
        let n = name.trim().to_ascii_uppercase();
        self.link.read(user, move |m| secured(m, &n))
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.link.run(user, command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_rows() {
        let rows = vec![
            vec!["QSECOFR".to_string(), "Payroll".to_string()],
            vec!["QSECOFR".into(), "*ALL".into(), "X".into()],
            vec!["*PUBLIC".into(), "*EXCLUDE".into(), String::new()],
        ];
        let l = list("PAYROLL".into(), &rows, 2);
        assert_eq!((l.owner.as_str(), l.text.as_str(), l.objects), ("QSECOFR", "Payroll", 2));
        assert!(l.entries[0].list_mgt && !l.entries[1].list_mgt);
        assert_eq!(qual(&("QCMD".into(), "QSYS".into())), "QSYS/QCMD");
        assert_eq!(qual(&("*NONE".into(), String::new())), "*NONE");
    }
}
