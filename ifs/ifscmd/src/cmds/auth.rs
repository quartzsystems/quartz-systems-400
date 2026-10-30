// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGAUT`, `CHGOWN`, `CHGPGP`. IBM sends no completion message for
//! these (none was found in `QCPFMSG`'s `CPCA` range); a line of text
//! says what changed.

use ifs::auth::data::DataAuth;
use ifs::client::IfsClient;
use ifs::ops::{ChgAut, Who};

use super::dirs::objaut;
use super::{Subtree, each};
use crate::msg::{Msg, Reply};

/// `CHGAUT`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chgaut {
    /// `OBJ`.
    pub obj: String,
    /// `USER`: profiles, or `*PUBLIC`.
    pub users: Vec<String>,
    /// `DTAAUT`: `*SAME`, `*NONE`, `*AUTL` or a data authority.
    pub dtaaut: String,
    /// `OBJAUT`: `*SAME`, `*NONE`, `*ALL` or object authorities.
    pub objaut: Vec<String>,
    /// `AUTL`: `None` when not given; `*NONE` removes the list.
    pub autl: Option<String>,
    /// `SUBTREE`.
    pub subtree: Subtree,
    /// `SYMLNK(*YES)`: change the link, not what it names.
    pub symlnk: bool,
}

fn done(what: &str, n: usize) -> Reply {
    let s = if n == 1 { "" } else { "s" };
    Reply::text(format!("{what} changed for {n} object{s}.\n"))
}

/// `CHGAUT`. `DTAAUT(*NONE) OBJAUT(*NONE)` for a user removes the
/// user's private authority.
pub fn chgaut(c: &dyn IfsClient, a: &Chgaut) -> Reply {
    let dt = a.dtaaut.trim().to_ascii_uppercase();
    let (data, data_autl) = match dt.as_str() {
        "" | "*SAME" => (None, false),
        "*AUTL" => (None, true),
        w => match DataAuth::parse(w) {
            Some(d) => (Some(d), false),
            None => return Reply::fail(Msg::new("CPDA091")),
        },
    };
    let objects = match objaut(&a.objaut) {
        Ok(o) => o,
        Err(m) => return Reply::fail(m),
    };
    let autl = a.autl.as_ref().map(|l| {
        let l = l.trim().to_ascii_uppercase();
        (l != "*NONE").then_some(l)
    });
    let mut who = Vec::new();
    let mut remove = Vec::new();
    for u in &a.users {
        let u = u.trim().to_ascii_uppercase();
        if u.is_empty() {
            continue;
        }
        if u.contains('*') && u != "*PUBLIC" {
            return Reply::fail(Msg::path("CPFA091", &u));
        }
        if u == "*PUBLIC" {
            who.push(Who::Public);
        } else if dt == "*NONE" && objects.is_some_and(|o| o.0 == 0) {
            remove.push(u);
        } else {
            who.push(Who::User(u));
        }
    }
    if data_autl && who.iter().any(|w| *w != Who::Public) {
        return Reply::fail(Msg::new("CPDA091"));
    }
    let change = ChgAut {
        who,
        data,
        data_autl,
        objects,
        autl,
    };
    each(
        c,
        &a.obj,
        a.subtree,
        !a.symlnk,
        |p| {
            if !change.who.is_empty() || change.autl.is_some() {
                c.chgaut(p, &change)?;
            }
            for u in &remove {
                c.rvkaut(p, u)?;
            }
            Ok(())
        },
        |n| done("Authority", n),
    )
}

/// `CHGOWN`.
pub fn chgown(c: &dyn IfsClient, obj: &str, newown: &str, rvkoldaut: bool, subtree: Subtree, symlnk: bool) -> Reply {
    let newown = newown.trim().to_ascii_uppercase();
    each(
        c,
        obj,
        subtree,
        !symlnk,
        |p| c.chgown(p, &newown, rvkoldaut),
        |n| done("Owner", n),
    )
}

/// `CHGPGP`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chgpgp {
    /// `OBJ`.
    pub obj: String,
    /// `NEWPGP`, `*NONE` for none.
    pub newpgp: String,
    /// `DTAAUT`: `*OLDPGP`, `*PRIVATE` or a data authority.
    pub dtaaut: String,
    /// `OBJAUT`: `*NONE`, `*ALL` or object authorities (with a data
    /// authority given).
    pub objaut: Vec<String>,
    /// `RVKOLDAUT(*YES)`.
    pub rvkoldaut: bool,
    /// `SUBTREE`.
    pub subtree: Subtree,
    /// `SYMLNK(*YES)`.
    pub symlnk: bool,
}

/// `CHGPGP`: the group's authority is the one it had privately
/// (`*PRIVATE`), else the old group's (`*OLDPGP`), or the values given.
pub fn chgpgp(c: &dyn IfsClient, a: &Chgpgp) -> Reply {
    let group = a.newpgp.trim().to_ascii_uppercase();
    let group = (group != "*NONE" && !group.is_empty()).then_some(group);
    let dt = a.dtaaut.trim().to_ascii_uppercase();
    let explicit = match dt.as_str() {
        "" | "*OLDPGP" | "*PRIVATE" => None,
        w => match DataAuth::parse(w) {
            Some(d) => Some(d),
            None => return Reply::fail(Msg::new("CPDA091")),
        },
    };
    let objects = if explicit.is_some() {
        match objaut(&a.objaut) {
            Ok(o) => o,
            Err(m) => return Reply::fail(m),
        }
    } else {
        None
    };
    each(
        c,
        &a.obj,
        a.subtree,
        !a.symlnk,
        |p| {
            c.chgpgp(p, group.as_deref(), a.rvkoldaut)?;
            if let (Some(g), Some(d)) = (&group, explicit) {
                c.chgaut(
                    p,
                    &ChgAut {
                        who: vec![Who::User(g.clone())],
                        data: Some(d),
                        data_autl: false,
                        objects,
                        autl: None,
                    },
                )?;
            }
            Ok(())
        },
        |n| done("Primary group", n),
    )
}
