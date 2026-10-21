// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped profiles, with IBM's names, classes and special
//! authorities (programs and tools look for them) and Quartz's text;
//! and the first IPL with security: profiles created, the existing
//! objects re-owned per IBM's conventions, the profile indexes built.
//! Values not settled against a machine are listed in
//! `docs/SEC_OPEN_QUESTIONS.md`.

use store::obj::header::{Authority, text};
use store::obj::{ObjectId, auth, ops::read_header, types};
use store::txn::Txn;

use crate::profile::index;
use crate::profile::objects;
use crate::profile::payload::UsrPrf;
use crate::profile::spcaut::{SpecialAuth as S, UserClass as C};
use crate::rec::text_of;
use crate::{Result, password};

/// One shipped profile.
#[derive(Debug, Clone, Copy)]
pub struct Shipped {
    /// Name.
    pub name: &'static str,
    /// User class.
    pub class: C,
    /// Special authorities.
    pub spcaut: S,
    /// Text, Quartz-written.
    pub text: &'static str,
}

const fn p(name: &'static str, class: C, spcaut: S, text: &'static str) -> Shipped {
    Shipped {
        name,
        class,
        spcaut,
        text,
    }
}

const JS: S = S(S::JOBCTL.0 | S::SAVSYS.0);
const INSTALL: S = S(S::ALLOBJ.0 | S::JOBCTL.0 | S::SAVSYS.0 | S::SECADM.0);

/// Every shipped profile. Only `QSECOFR` has a password.
pub static PROFILES: &[Shipped] = &[
    p(
        "QSYS",
        C::SecOfr,
        S::ALL,
        "Owner of the operating system's objects",
    ),
    p("QSECOFR", C::SecOfr, S::ALL, "Security officer"),
    p("QSYSOPR", C::SysOpr, JS, "System operator"),
    p("QPGMR", C::Pgmr, JS, "Programmer"),
    p("QUSER", C::User, S::NONE, "Workstation user"),
    p(
        "QSRV",
        C::Pgmr,
        S(JS.0 | S::SERVICE.0),
        "Service representative",
    ),
    p("QSRVBAS", C::User, S::NONE, "Basic service representative"),
    p(
        "QDFTOWN",
        C::User,
        S::NONE,
        "Owner of objects whose owner was deleted",
    ),
    p("QSPL", C::User, S::NONE, "Spooling"),
    p("QSPLJOB", C::User, S::NONE, "Spooling jobs"),
    p("QTCP", C::User, S(S::JOBCTL.0 | S::IOSYSCFG.0), "TCP/IP"),
    p("QDBSHR", C::User, S::NONE, "Database sharing"),
    p("QDBSHRDO", C::User, S::NONE, "Database sharing, data owner"),
    p("QDOC", C::User, S::NONE, "Documents"),
    p(
        "QLPAUTO",
        C::User,
        INSTALL,
        "Licensed program automatic install",
    ),
    p("QLPINSTALL", C::User, INSTALL, "Licensed program install"),
    p("QAUTPROF", C::User, S::NONE, "Authority profile"),
    p("QBRMS", C::User, S::NONE, "Backup and recovery"),
    p("QCLUMGT", C::User, S::NONE, "Cluster management"),
    p("QCLUSTER", C::User, S::NONE, "Clustering"),
    p("QCOLSRV", C::User, S::JOBCTL, "Collection services"),
    p("QDIRSRV", C::User, S::NONE, "Directory services"),
    p("QDLFM", C::User, S::NONE, "Data link file manager"),
    p(
        "QDSNX",
        C::User,
        S::NONE,
        "Distributed systems node executive",
    ),
    p("QEJBSVR", C::User, S::NONE, "Application server"),
    p("QFNC", C::User, S::NONE, "Finance"),
    p("QGATE", C::User, S::NONE, "Gateway"),
    p("QIPP", C::User, S::NONE, "Internet printing"),
    p("QMGTC", C::User, S::NONE, "Management central"),
    p("QMSF", C::User, S::NONE, "Mail server framework"),
    p("QNETSPLF", C::User, S::NONE, "Network spooled files"),
    p(
        "QNFSANON",
        C::User,
        S::NONE,
        "Network file system anonymous user",
    ),
    p("QNTP", C::User, S::NONE, "Network time"),
    p("QPEX", C::User, S::JOBCTL, "Performance explorer"),
    p("QPM400", C::User, S::NONE, "Performance management"),
    p("QRJE", C::User, S::NONE, "Remote job entry"),
    p("QSNADS", C::User, S::JOBCTL, "SNA distribution services"),
    p("QSOC", C::User, S::NONE, "Communications"),
    p("QSRVAGT", C::User, S::NONE, "Service agent"),
    p("QTCM", C::User, S::NONE, "Cluster resource services"),
    p("QTFTP", C::User, S::NONE, "Trivial file transfer"),
    p("QTMHHTP1", C::User, S::NONE, "Web server, CGI jobs"),
    p("QTMHHTTP", C::User, S::NONE, "Web server"),
    p("QTMPLPD", C::User, S::NONE, "Line printer daemon"),
    p("QTSTRQS", C::User, S::NONE, "Test request"),
    p("QUMB", C::User, S::NONE, "Ultimedia"),
    p("QYCMCIMOM", C::User, S::NONE, "CIM object manager"),
    p(
        "QYPSJSVR",
        C::User,
        S::NONE,
        "Management central Java server",
    ),
];

/// The shipped profile `name`, if it is one.
#[must_use]
pub fn shipped(name: &str) -> Option<&'static Shipped> {
    PROFILES.iter().find(|s| s.name == name)
}

/// The libraries the system owns.
pub const SYSTEM_LIBRARIES: &[&str] = &[
    "QSYS",
    "QGPL",
    "QUSRSYS",
    "QSPL",
    "QRECOVERY",
    "QHLPSYS",
    "QSYS2",
    "SYSIBM",
];

/// What [`install`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Profiles created.
    pub profiles: usize,
    /// Objects re-owned.
    pub reowned: usize,
    /// Index entries built.
    pub indexed: usize,
}

/// The profile a shipped entry starts as.
#[must_use]
pub fn profile_of(s: &Shipped, qsecurity: u8, pwdlvl: u8, now: u64) -> UsrPrf {
    let mut u = UsrPrf::new(s.name, qsecurity);
    u.usrcls = s.class;
    u.spcaut = s.spcaut;
    u.text = s.text.into();
    u.created = now;
    u.changed = now;
    u.ptylmt = 0;
    u.jobd = ("QDFTJOBD".into(), "QGPL".into());
    u.acgcde = "*SYS".into();
    u.atnpgm = ("*NONE".into(), String::new());
    u.locale = "*NONE".into();
    u.pwdchgblk = crate::profile::payload::NOMAX;
    if s.name == "QSECOFR" {
        u.pwd_none = false;
        u.pwdexp = true;
        u.hashes = password::hashes_for("QSECOFR", pwdlvl);
        u.pwd_changed = now;
    }
    u
}

/// Create the shipped profiles that are missing. The first time (no
/// `QSYS` profile yet) also re-own every object and build the indexes.
pub fn install(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    qsecurity: u8,
    pwdlvl: u8,
    now: u64,
) -> Result<Installed> {
    let first = !objects::exists(txn, qsys, "QSYS");
    let mut done = Installed::default();
    for s in PROFILES {
        if objects::exists(txn, qsys, s.name) {
            continue;
        }
        let u = profile_of(s, qsecurity, pwdlvl, now);
        objects::create(txn, qsys, &u, "QSYS", Authority::EXCLUDE, now)?;
        done.profiles += 1;
    }
    if first {
        done.reowned = reown(txn, qsys)?;
        done.indexed = index::rebuild(txn, qsys)?;
    }
    Ok(done)
}

/// Re-own the objects that predate security: everything in a system
/// library, and the system libraries, to `QSYS`, with IBM's public
/// authorities; a user's message queue in `QUSRSYS` to its user;
/// anything else whose owner is not a profile to `QDFTOWN`.
pub fn reown(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<usize> {
    let mut n = 0;
    for id in index::all_objects(txn, qsys)? {
        let (page, mut h) = read_header(txn, id)?;
        let lib = if id == qsys {
            "QSYS".to_string()
        } else {
            read_header(txn, h.library)?.1.name.to_string()
        };
        let name = h.name.to_string();
        let system = if id.type_code == types::LIB {
            SYSTEM_LIBRARIES.contains(&name.as_str())
        } else {
            SYSTEM_LIBRARIES.contains(&lib.as_str())
        };
        let owner = text_of(&h.owner);
        let new_owner =
            if id.type_code == types::MSGQ && lib == "QUSRSYS" && objects::exists(txn, qsys, &name)
            {
                name.clone()
            } else if system {
                "QSYS".into()
            } else if objects::exists(txn, qsys, &owner) {
                owner.clone()
            } else {
                "QDFTOWN".into()
            };
        h.owner = text(&new_owner)?;
        if system {
            h.public_authority = public_for(id.type_code, &name);
        }
        auth::save_authority(txn, page, &mut h)?;
        n += 1;
    }
    Ok(n)
}

/// IBM's `*PUBLIC` for a system object of a type.
#[must_use]
pub fn public_for(type_code: u16, name: &str) -> Authority {
    match type_code {
        types::LIB => match name {
            "QGPL" => Authority::CHANGE,
            "QSPL" | "QRECOVERY" => Authority::EXCLUDE,
            _ => Authority::USE,
        },
        types::USRPRF | types::SYSVAL | types::SPLF => Authority::EXCLUDE,
        types::MSGQ => Authority::CHANGE,
        _ => Authority::USE,
    }
}
